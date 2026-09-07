#!/usr/bin/env python3
"""在三个隔离的 loopback Redis 子进程上运行 Cluster 集成测试。"""

from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time


HOST = "127.0.0.1"


@contextmanager
def reserve_ports(count):
    reservations = []
    try:
        for _ in range(count):
            reservation = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            reservation.bind((HOST, 0))
            reservation.listen()
            reservations.append(reservation)
        yield [reservation.getsockname()[1] for reservation in reservations]
    finally:
        for reservation in reservations:
            reservation.close()


def node_config(directory, client_port, bus_port):
    escaped_directory = str(directory).replace("\\", "\\\\").replace('"', '\\"')
    return "\n".join(
        [
            f"bind {HOST}",
            "protected-mode yes",
            f"port {client_port}",
            f"cluster-port {bus_port}",
            "cluster-enabled yes",
            "cluster-config-file nodes.conf",
            "cluster-node-timeout 5000",
            f"cluster-announce-ip {HOST}",
            f"cluster-announce-port {client_port}",
            f"cluster-announce-bus-port {bus_port}",
            'save ""',
            "appendonly no",
            "daemonize no",
            f'dir "{escaped_directory}"',
            "",
        ]
    )


def test_environment(inherited, seed_urls):
    environment = {
        name: value
        for name, value in inherited.items()
        if not name.startswith("REDIX_TEST_REDIS")
    }
    environment["REDIX_TEST_REDIS_CLUSTER_URLS"] = ",".join(seed_urls)
    return environment


def read_process_id(port, timeout=0.2):
    with socket.create_connection((HOST, port), timeout=timeout) as client:
        client.sendall(b"*2\r\n$4\r\nINFO\r\n$6\r\nserver\r\n")
        with client.makefile("rb") as reply:
            header = reply.readline()
            if not header.startswith(b"$"):
                raise RuntimeError("INFO server 未返回 bulk string。")
            size = int(header[1:].strip())
            if size < 0 or size > 4 * 1024 * 1024:
                raise RuntimeError("INFO server 响应长度无效。")
            info = reply.read(size).decode("utf-8", errors="strict")
    for line in info.splitlines():
        if line.startswith("process_id:"):
            return int(line.split(":", 1)[1])
    raise RuntimeError("INFO server 缺少 process_id。")


def wait_for_owned_servers(nodes, timeout=10):
    pending = {port: child for port, child in nodes}
    deadline = time.monotonic() + timeout
    while pending and time.monotonic() < deadline:
        for port, child in list(pending.items()):
            if child.poll() is not None:
                raise RuntimeError(f"临时 Redis 子进程 {child.pid} 启动失败。")
            try:
                actual_pid = read_process_id(port)
            except (OSError, UnicodeError, ValueError):
                continue
            if actual_pid != child.pid:
                raise RuntimeError(
                    f"端口 {port} 的 Redis PID {actual_pid} 不属于子进程 {child.pid}。"
                )
            del pending[port]
        if pending:
            time.sleep(0.05)
    if pending:
        raise RuntimeError("等待临时 Redis Cluster 节点超时。")


def stop_children(children):
    for child in children:
        if child.poll() is None:
            child.terminate()
    for child in children:
        if child.poll() is None:
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


@contextmanager
def owned_children():
    children = []
    try:
        yield children
    finally:
        stop_children(children)


def main():
    redis_server = shutil.which("redis-server")
    redis_cli = shutil.which("redis-cli")
    if redis_server is None or redis_cli is None:
        print("未找到 redis-server 或 redis-cli，请先安装后重试。", file=sys.stderr)
        return 1

    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="redix-cluster-test-") as temporary:
        with owned_children() as children:
            temporary_root = Path(temporary)
            with reserve_ports(6) as ports:
                nodes = []
                for index in range(3):
                    directory = temporary_root / f"node-{index + 1}"
                    directory.mkdir()
                    client_port = ports[index * 2]
                    bus_port = ports[index * 2 + 1]
                    config = directory / "redis.conf"
                    config.write_text(
                        node_config(directory, client_port, bus_port), encoding="utf-8"
                    )
                    nodes.append((directory, config, client_port))

            for directory, config, client_port in nodes:
                with (directory / "redis.log").open("wb") as log:
                    child = subprocess.Popen(
                        [redis_server, str(config)],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                    )
                children.append(child)

            wait_for_owned_servers(
                [(node[2], child) for node, child in zip(nodes, children)]
            )
            addresses = [f"{HOST}:{node[2]}" for node in nodes]
            cluster = subprocess.run(
                [
                    redis_cli,
                    "--cluster",
                    "create",
                    *addresses,
                    "--cluster-replicas",
                    "0",
                    "--cluster-yes",
                ],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=30,
            )
            if cluster.returncode != 0:
                raise RuntimeError(
                    "创建临时 Redis Cluster 失败："
                    + (cluster.stderr.strip() or cluster.stdout.strip())
                )

            # Cluster creation must not have replaced any listener with an unrelated process.
            wait_for_owned_servers(
                [(node[2], child) for node, child in zip(nodes, children)]
            )
            seed_urls = [f"redis://{address}" for address in addresses]
            environment = test_environment(os.environ, seed_urls)
            print("运行隔离 Redis Cluster 测试（3 primary，随机 client/bus 端口）。", flush=True)
            return subprocess.run(
                [
                    "cargo",
                    "test",
                    "--manifest-path",
                    "src-tauri/Cargo.toml",
                    "--test",
                    "cluster_integration",
                    "--test",
                    "analysis_tasks_integration",
                    "cluster_",
                    "--",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ],
                cwd=root,
                env=environment,
                timeout=180,
            ).returncode


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("本地 Redis Cluster 测试已中断。", file=sys.stderr)
        sys.exit(130)
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"本地 Redis Cluster 测试未完成：{error}", file=sys.stderr)
        sys.exit(1)
