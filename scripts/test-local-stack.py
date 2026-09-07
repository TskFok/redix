#!/usr/bin/env python3
"""用本次创建的 loopback Docker Redis 8 实例运行模块测试；不复用既有容器。"""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid


IMAGE = "redis:8.4.5"
OWNER_LABEL = "io.redix.test.stack.nonce"
DOCKER_HOST = "unix://" + str(Path.home() / ".docker/run/docker.sock")
HOST = "127.0.0.1"


def create_arguments(name, nonce, cidfile):
    return [
        "create", "--name", name, "--label", f"{OWNER_LABEL}={nonce}",
        "--cidfile", str(cidfile), "--publish", f"{HOST}::6379",
        "--tmpfs", "/data:rw,nosuid,nodev,size=256m",
        "--memory", "512m", "--memory-swap", "512m", "--cpus", "1",
        "--pids-limit", "256", IMAGE,
    ]


def test_environment(inherited, port):
    environment = {name: value for name, value in inherited.items() if not name.startswith("REDIX_TEST_REDIS")}
    url = f"redis://{HOST}:{port}"
    environment["REDIX_TEST_REDIS_URL"] = url
    environment["REDIX_TEST_REDIS_STACK_URL"] = url
    return environment


class Docker:
    def __init__(self, executable):
        self.prefix = [executable, "--host", DOCKER_HOST]

    def run(self, arguments, timeout=30):
        result = subprocess.run(self.prefix + arguments, capture_output=True, text=True,
                                timeout=timeout, start_new_session=True)
        if result.returncode != 0:
            raise RuntimeError(f"Docker {arguments[0]} 失败：{result.stderr.strip()[:1000]}")
        return result.stdout

    def inspect(self, container_id):
        results = json.loads(self.run(["container", "inspect", container_id]))
        if not isinstance(results, list) or len(results) != 1:
            raise RuntimeError("Docker 未返回唯一容器信息。")
        return results[0]


def assert_owned(info, container_id, nonce):
    if not re.fullmatch(r"[a-f0-9]{64}", container_id):
        raise RuntimeError("容器 ID 无效，拒绝操作。")
    if info.get("Id") != container_id or info.get("Config", {}).get("Labels", {}).get(OWNER_LABEL) != nonce:
        raise RuntimeError("容器 ID 或本次所有权标签不符，拒绝操作或清理。")


@contextmanager
def defer_interrupts():
    """等 create 捕获 ID 或完成清理后再响应 Ctrl-C，避免留下身份未知的容器。"""
    interrupted = False
    previous = signal.getsignal(signal.SIGINT)

    def interrupt(_signal, _frame):
        nonlocal interrupted
        interrupted = True

    signal.signal(signal.SIGINT, interrupt)
    try:
        yield
    finally:
        signal.signal(signal.SIGINT, previous)
    if interrupted:
        raise KeyboardInterrupt


def cleanup_container(docker, container_id, nonce):
    if not re.fullmatch(r"[a-f0-9]{64}", container_id):
        raise RuntimeError("未捕获有效容器 ID，拒绝按名称清理。")
    with defer_interrupts():
        info = docker.inspect(container_id)
        assert_owned(info, container_id, nonce)
        docker.run(["rm", "--force", "--volumes", container_id])


@contextmanager
def owned_container(docker, nonce):
    container_id = None
    with tempfile.TemporaryDirectory(prefix="redix-stack-cid-") as temporary:
        cidfile = Path(temporary) / "container-id"
        try:
            with defer_interrupts():
                output = docker.run(create_arguments("redix-stack-test-" + nonce, nonce, cidfile))
            container_id = output.strip()
            if not cidfile.is_file() or cidfile.read_text().strip() != container_id:
                raise RuntimeError("Docker 输出与捕获的容器 ID 文件不符。")
            assert_owned(docker.inspect(container_id), container_id, nonce)
            docker.run(["start", container_id])
            yield container_id
        finally:
            # A failed/timed-out create may already have written the exact ID.
            if cidfile.is_file():
                container_id = cidfile.read_text().strip()
            if container_id:
                active_error = sys.exc_info()[0] is not None
                try:
                    cleanup_container(docker, container_id, nonce)
                except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
                    print(f"本次容器未能安全清理：{error}", file=sys.stderr)
                    if not active_error:
                        raise


def mapped_port(info):
    bindings = info.get("NetworkSettings", {}).get("Ports", {}).get("6379/tcp")
    if not isinstance(bindings, list) or len(bindings) != 1 or bindings[0].get("HostIp") != HOST:
        raise RuntimeError("Redis 端口必须仅绑定单个 127.0.0.1 随机端口。")
    try:
        port = int(bindings[0]["HostPort"])
    except (KeyError, TypeError, ValueError) as error:
        raise RuntimeError("Docker 端口映射无效。") from error
    if not 1 <= port <= 65535:
        raise RuntimeError("Docker 端口映射超出范围。")
    return port


def parse_run_id(info):
    for line in info.splitlines():
        if line.startswith("run_id:"):
            run_id = line.partition(":")[2]
            if re.fullmatch(r"[a-f0-9]{40}", run_id):
                return run_id
    raise RuntimeError("INFO server 缺少有效 run_id。")


def read_run_id(port):
    with socket.create_connection((HOST, port), timeout=2) as client:
        client.sendall(b"*2\r\n$4\r\nINFO\r\n$6\r\nserver\r\n")
        with client.makefile("rb") as response:
            header = response.readline(100)
            if not header.startswith(b"$"):
                raise RuntimeError("映射端口未返回 Redis INFO bulk string。")
            size = int(header[1:].strip())
            if not 0 < size <= 1024 * 1024:
                raise RuntimeError("Redis INFO 响应长度无效。")
            info = response.read(size)
            if len(info) != size or response.read(2) != b"\r\n":
                raise RuntimeError("Redis INFO 响应不完整。")
            return parse_run_id(info.decode("utf-8", errors="strict"))


def wait_until_ready(docker, container_id, nonce, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        info = docker.inspect(container_id)
        assert_owned(info, container_id, nonce)
        if not info.get("State", {}).get("Running"):
            raise RuntimeError("本次 Redis 容器未运行，请检查官方镜像默认启动配置。")
        port = mapped_port(info)
        try:
            inside = docker.run(["exec", container_id, "redis-cli", "--raw", "INFO", "server"], timeout=3)
            expected = parse_run_id(inside)
            actual = read_run_id(port)
        except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired):
            time.sleep(0.1)
            continue
        if actual != expected:
            raise RuntimeError("映射端口 Redis run_id 与本次容器不符，拒绝执行测试。")
        return port, expected
    raise RuntimeError("等待本次 Redis 容器就绪超时。")


def pairs(value):
    if isinstance(value, dict):
        return value
    if not isinstance(value, list) or len(value) % 2:
        raise RuntimeError("Redis 返回的属性结构无效。")
    return dict(zip(value[::2], value[1::2]))


def validate_modules(payload):
    modules = json.loads(payload)
    if not isinstance(modules, list):
        raise RuntimeError("MODULE LIST 未返回模块列表。")
    names = {str(pairs(module).get("name", "")).lower() for module in modules}
    missing = {"search", "rejson", "timeseries"} - names
    if missing:
        raise RuntimeError("官方镜像缺少所需模块：" + ", ".join(sorted(missing)) + "；保留默认 entrypoint/Cmd 后重试。")


def prepare_modules(docker, container_id):
    prefix = ["exec", container_id, "redis-cli"]
    # Preserve the image's default entrypoint/Cmd and module-loading configuration.
    # Until this point only identity checks have run and /data is ephemeral tmpfs.
    result = docker.run(prefix + ["--raw", "CONFIG", "SET", "save", "", "appendonly", "no"])
    if result.strip() != "OK":
        raise RuntimeError("无法关闭隔离实例持久化。")
    config = pairs(json.loads(docker.run(prefix + ["--json", "CONFIG", "GET", "save", "appendonly"])))
    if config.get("save") != "" or config.get("appendonly") != "no":
        raise RuntimeError("隔离实例持久化状态未通过校验。")
    validate_modules(docker.run(prefix + ["--json", "MODULE", "LIST"]))
    commands = json.loads(docker.run(prefix + ["--json", "COMMAND", "INFO", "FT.SEARCH", "JSON.GET", "TS.INFO"]))
    if not isinstance(commands, list) or len(commands) != 3 or any(command is None for command in commands):
        raise RuntimeError("Search、JSON 或 TimeSeries 命令不可用。")


def test_commands():
    common = ["cargo", "test", "--offline", "--manifest-path", "src-tauri/Cargo.toml"]
    options = ["--", "--ignored", "--nocapture", "--test-threads=1"]
    return [
        common + ["--test", "redis_integration"] + options,
        common + ["--lib", "redis::search::tests::vector_indexes_execute_real_flat_and_hnsw_knn_queries"] + options + ["--exact"],
        common + ["--lib", "redis::search::tests::typed_vector_query_executes_real_knn_search"] + options + ["--exact"],
    ]


def run_test(command, root, environment):
    child = subprocess.Popen(command, cwd=root, env=environment, start_new_session=True)
    try:
        return child.wait(timeout=300)
    except BaseException:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=5)
        raise


def main():
    executable = shutil.which("docker")
    if executable is None:
        raise RuntimeError("未找到 Docker CLI，请先安装并启动 Docker。")
    docker = Docker(executable)
    try:
        docker.run(["image", "inspect", IMAGE])
    except RuntimeError as error:
        raise RuntimeError(f"官方镜像 {IMAGE} 不可用或 Docker 未就绪。请先运行 docker --host {DOCKER_HOST} pull {IMAGE}；脚本不会自动拉取或修改凭据。") from error
    root = Path(__file__).resolve().parent.parent
    nonce = uuid.uuid4().hex
    with owned_container(docker, nonce) as container_id:
        port, run_id = wait_until_ready(docker, container_id, nonce)
        prepare_modules(docker, container_id)
        environment = test_environment(os.environ, port)
        print("运行隔离 Redis 8 模块测试（随机 loopback 端口、512 MiB、无持久化）。", flush=True)
        for command in test_commands():
            # Revalidate identity before each test binary, including after compilation delays.
            info = docker.inspect(container_id)
            assert_owned(info, container_id, nonce)
            if mapped_port(info) != port or read_run_id(port) != run_id:
                raise RuntimeError("测试前实例身份发生变化，已停止。")
            result = run_test(command, root, environment)
            if result != 0:
                return result
        return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("Redis 模块测试已中断。", file=sys.stderr)
        sys.exit(130)
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Redis 模块测试未完成：{error}", file=sys.stderr)
        sys.exit(1)
