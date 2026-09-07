import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch, Mock

spec = importlib.util.spec_from_file_location("local_stack", Path(__file__).with_name("test-local-stack.py"))
stack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stack)

CONTAINER_ID = "a" * 64
NONCE = "b" * 32


class FakeDocker:
    def __init__(self, *, label=NONCE, create_error=None):
        self.calls = []
        self.label = label
        self.create_error = create_error

    def run(self, arguments, **_kwargs):
        self.calls.append(arguments)
        if arguments[0] == "create":
            Path(arguments[arguments.index("--cidfile") + 1]).write_text(CONTAINER_ID)
            if self.create_error:
                raise self.create_error
            return CONTAINER_ID + "\n"
        return ""

    def inspect(self, container_id):
        self.calls.append(["inspect", container_id])
        return {"Id": CONTAINER_ID, "Config": {"Labels": {stack.OWNER_LABEL: self.label}}}


class StackLauncherTests(unittest.TestCase):
    def test_container_has_loopback_random_port_and_tmpfs_without_bind_mounts(self):
        args = stack.create_arguments("redix-stack-test-" + NONCE, NONCE, "/tmp/id")
        self.assertEqual(args[args.index("--publish") + 1], "127.0.0.1::6379")
        self.assertEqual(args[args.index("--memory") + 1], "512m")
        self.assertEqual(args[args.index("--cpus") + 1], "1")
        self.assertTrue(args[args.index("--tmpfs") + 1].startswith("/data:"))
        self.assertNotIn("--mount", args)
        self.assertNotIn("--volume", args)
        self.assertNotIn("-v", args)
        self.assertNotIn("--entrypoint", args)
        self.assertEqual(args[-1], "redis:8.4.5")
        self.assertNotIn("redis-server", args)

    def test_old_redis_targets_are_removed_before_setting_only_this_instance(self):
        env = stack.test_environment({"PATH": "/bin", "REDIX_TEST_REDIS_URL": "production", "REDIX_TEST_REDIS_STACK_URL": "other", "REDIX_TEST_REDIS_CLUSTER_URLS": "cluster", "REDIX_TEST_REDIS_SECRET": "secret"}, 32123)
        self.assertEqual(env, {"PATH": "/bin", "REDIX_TEST_REDIS_URL": "redis://127.0.0.1:32123", "REDIX_TEST_REDIS_STACK_URL": "redis://127.0.0.1:32123"})

    def test_port_inspection_rejects_public_or_multiple_bindings(self):
        def info(bindings):
            return {"NetworkSettings": {"Ports": {"6379/tcp": bindings}}}
        self.assertEqual(stack.mapped_port(info([{"HostIp": "127.0.0.1", "HostPort": "32123"}])), 32123)
        for bindings in [[{"HostIp": "0.0.0.0", "HostPort": "32123"}], [{"HostIp": "::", "HostPort": "32123"}], [], [{"HostIp": "127.0.0.1", "HostPort": "32123"}] * 2]:
            with self.assertRaises(RuntimeError):
                stack.mapped_port(info(bindings))

    def test_cleanup_requires_exact_captured_id_and_ownership_label(self):
        docker = FakeDocker(label="another-run")
        with self.assertRaises(RuntimeError):
            stack.cleanup_container(docker, CONTAINER_ID, NONCE)
        self.assertFalse(any(call[0] == "rm" for call in docker.calls))
        with self.assertRaises(RuntimeError):
            stack.cleanup_container(docker, "--all", NONCE)
        docker = FakeDocker()
        stack.cleanup_container(docker, CONTAINER_ID, NONCE)
        self.assertEqual(docker.calls[-1], ["rm", "--force", "--volumes", CONTAINER_ID])

    def test_failure_and_interrupt_after_start_always_clean_owned_container(self):
        for error in [RuntimeError("tests failed"), KeyboardInterrupt()]:
            docker = FakeDocker()
            with self.assertRaises(type(error)):
                with stack.owned_container(docker, NONCE) as captured:
                    self.assertEqual(captured, CONTAINER_ID)
                    raise error
            self.assertEqual(docker.calls[-1], ["rm", "--force", "--volumes", CONTAINER_ID])

    def test_create_timeout_uses_captured_cidfile_for_guarded_cleanup(self):
        docker = FakeDocker(create_error=subprocess.TimeoutExpired("docker create", 30))
        with self.assertRaises(subprocess.TimeoutExpired):
            with stack.owned_container(docker, NONCE):
                self.fail("create should not finish")
        self.assertEqual(docker.calls[-1], ["rm", "--force", "--volumes", CONTAINER_ID])

    def test_missing_required_module_blocks_test_execution(self):
        complete = [["name", "search", "ver", 20800], ["name", "ReJSON", "ver", 20800], ["name", "timeseries", "ver", 11200]]
        stack.validate_modules(json.dumps(complete))
        with self.assertRaises(RuntimeError):
            stack.validate_modules(json.dumps(complete[:2]))

    def test_readiness_retries_connection_refusal_then_checks_container_run_id(self):
        docker = FakeDocker()
        info = docker.inspect(CONTAINER_ID)
        info.update({"State": {"Running": True}, "NetworkSettings": {"Ports": {"6379/tcp": [{"HostIp": "127.0.0.1", "HostPort": "32123"}]}}})
        docker.inspect = Mock(return_value=info)
        docker.run = Mock(side_effect=[RuntimeError("Redis is starting"), "run_id:" + "c" * 40])
        with patch.object(stack, "read_run_id", return_value="c" * 40), patch.object(stack.time, "sleep"):
            self.assertEqual(stack.wait_until_ready(docker, CONTAINER_ID, NONCE), (32123, "c" * 40))

    def test_wrong_port_identity_blocks_before_any_configuration_or_test(self):
        docker = FakeDocker()
        info = docker.inspect(CONTAINER_ID)
        info.update({"State": {"Running": True}, "NetworkSettings": {"Ports": {"6379/tcp": [{"HostIp": "127.0.0.1", "HostPort": "32123"}]}}})
        docker.inspect = Mock(return_value=info)
        docker.run = Mock(return_value="run_id:" + "c" * 40)
        with patch.object(stack, "read_run_id", return_value="d" * 40):
            with self.assertRaisesRegex(RuntimeError, "run_id"):
                stack.wait_until_ready(docker, CONTAINER_ID, NONCE)
        self.assertEqual(docker.run.call_args.args[0][-2:], ["INFO", "server"])

    def test_persistence_verification_must_pass_before_module_tests(self):
        docker = FakeDocker()
        docker.run = Mock(side_effect=["OK", json.dumps({"save": "", "appendonly": "yes"})])
        with self.assertRaisesRegex(RuntimeError, "持久化"):
            stack.prepare_modules(docker, CONTAINER_ID)

    def test_interrupted_cargo_terminates_only_its_created_process_group(self):
        child = Mock(pid=12345)
        child.wait.side_effect = [KeyboardInterrupt(), 130]
        with patch.object(stack.subprocess, "Popen", return_value=child) as start, patch.object(stack.os, "killpg") as kill:
            with self.assertRaises(KeyboardInterrupt):
                stack.run_test(["cargo", "test"], Path("/tmp"), {})
            self.assertTrue(start.call_args.kwargs["start_new_session"])
            kill.assert_called_once_with(12345, stack.signal.SIGTERM)


if __name__ == "__main__":
    unittest.main()
