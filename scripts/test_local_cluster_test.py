import importlib.util
import os
from pathlib import Path
import socket
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("test-local-cluster.py")
SPEC = importlib.util.spec_from_file_location("test_local_cluster", SCRIPT)
cluster = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cluster)


class FakeSocket:
    next_port = 17000

    def __init__(self, *_args, **_kwargs):
        self.closed = False
        self.port = None

    def setsockopt(self, *_args):
        pass

    def bind(self, address):
        self.assert_open()
        assert address == ("127.0.0.1", 0)
        self.port = FakeSocket.next_port
        FakeSocket.next_port += 1

    def listen(self):
        self.assert_open()

    def getsockname(self):
        self.assert_open()
        return "127.0.0.1", self.port

    def close(self):
        self.closed = True

    def assert_open(self):
        if self.closed:
            raise AssertionError("socket reservation was released too early")


class FakeChild:
    def __init__(self, running=True, wait_times_out=False):
        self.running = running
        self.wait_times_out = wait_times_out
        self.terminated = 0
        self.killed = 0
        self.waited = 0

    def poll(self):
        return None if self.running else 0

    def terminate(self):
        self.terminated += 1

    def wait(self, timeout=None):
        self.waited += 1
        if self.wait_times_out and timeout is not None and self.killed == 0:
            raise cluster.subprocess.TimeoutExpired("redis-server", timeout)
        self.running = False

    def kill(self):
        self.killed += 1


class ClusterLauncherSafetyTests(unittest.TestCase):
    def test_child_environment_drops_all_inherited_redis_test_addresses(self):
        inherited = {
            "PATH": os.environ.get("PATH", ""),
            "REDIX_TEST_REDIS_URL": "redis://user-instance:6379",
            "REDIX_TEST_REDIS_STACK_URL": "redis://user-stack:6379",
            "REDIX_TEST_REDIS_CLUSTER_URLS": "redis://user-cluster:7000",
        }

        result = cluster.test_environment(
            inherited,
            ["redis://127.0.0.1:17000", "redis://127.0.0.1:17002"],
        )

        self.assertEqual(result["PATH"], inherited["PATH"])
        self.assertEqual(
            result["REDIX_TEST_REDIS_CLUSTER_URLS"],
            "redis://127.0.0.1:17000,redis://127.0.0.1:17002",
        )
        self.assertNotIn("REDIX_TEST_REDIS_URL", result)
        self.assertNotIn("REDIX_TEST_REDIS_STACK_URL", result)

    def test_six_unique_ports_remain_reserved_until_context_exit(self):
        reservations = []

        def create_socket(*_args, **_kwargs):
            reservation = FakeSocket()
            reservations.append(reservation)
            return reservation

        with mock.patch.object(socket, "socket", side_effect=create_socket):
            with cluster.reserve_ports(6) as ports:
                self.assertEqual(len(ports), 6)
                self.assertEqual(len(set(ports)), 6)
                self.assertTrue(all(not reservation.closed for reservation in reservations))
            self.assertTrue(all(reservation.closed for reservation in reservations))

    def test_node_config_binds_only_loopback_and_uses_separate_bus_port(self):
        config = cluster.node_config(Path('node "one"'), 17000, 17001)

        self.assertIn("bind 127.0.0.1", config)
        self.assertIn("protected-mode yes", config)
        self.assertIn("port 17000", config)
        self.assertIn("cluster-port 17001", config)
        self.assertIn("cluster-enabled yes", config)
        self.assertIn("cluster-config-file nodes.conf", config)
        self.assertIn("appendonly no", config)
        self.assertIn('save ""', config)
        self.assertIn('dir "node \\"one\\""', config)

    def test_cleanup_terminates_only_the_children_it_is_given(self):
        stopped = FakeChild(running=True)
        killed = FakeChild(running=True, wait_times_out=True)
        already_exited = FakeChild(running=False)
        unrelated = FakeChild(running=True)

        cluster.stop_children([stopped, killed, already_exited])

        self.assertEqual((stopped.terminated, stopped.killed), (1, 0))
        self.assertEqual((killed.terminated, killed.killed), (1, 1))
        self.assertEqual((already_exited.terminated, already_exited.killed), (0, 0))
        self.assertEqual((unrelated.terminated, unrelated.killed), (0, 0))

    def test_pid_verification_rejects_a_listener_not_owned_by_its_child(self):
        child = FakeChild(running=True)
        child.pid = 4242
        with mock.patch.object(cluster, "read_process_id", return_value=9999):
            with self.assertRaisesRegex(RuntimeError, "PID"):
                cluster.wait_for_owned_servers([(17000, child)], timeout=0.01)

    def test_owned_children_are_stopped_on_failure_timeout_and_interrupt(self):
        for error in (
            RuntimeError("failed"),
            cluster.subprocess.TimeoutExpired("cargo test", 180),
            KeyboardInterrupt(),
        ):
            child = FakeChild(running=True)
            with self.subTest(error=type(error).__name__):
                with mock.patch.object(cluster, "stop_children") as stop:
                    with self.assertRaises(type(error)):
                        with cluster.owned_children() as children:
                            children.append(child)
                            raise error
                    stop.assert_called_once_with([child])


if __name__ == "__main__":
    unittest.main()
