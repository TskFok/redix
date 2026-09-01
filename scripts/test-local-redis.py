#!/usr/bin/env python3
"""在隔离的临时 Redis 上运行集成测试，不访问用户已配置的实例。"""
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time


def main():
    executable = shutil.which("redis-server")
    if executable is None:
        print("未找到 redis-server，请先安装后重试。", file=sys.stderr)
        return 1
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="redix-redis-test-") as directory:
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        with open(Path(directory) / "redis.log", "w+") as log:
            server = subprocess.Popen([
                executable, "--bind", "127.0.0.1", "--port", str(port),
                "--save", "", "--appendonly", "no", "--dir", directory,
            ], stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if server.poll() is not None:
                        raise RuntimeError("临时 Redis 启动失败；可能是端口不可用。")
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=0.2) as client:
                            client.sendall(b"*2\r\n$4\r\nINFO\r\n$6\r\nserver\r\n")
                            with client.makefile("rb") as reply:
                                header = reply.readline()
                                if header.startswith(b"$"):
                                    info = reply.read(int(header[1:]))
                                    if f"\nprocess_id:{server.pid}\r\n".encode() in info:
                                        break
                    except OSError:
                        pass
                    time.sleep(0.05)
                else:
                    raise RuntimeError("等待临时 Redis 超时。")
                env = os.environ.copy()
                # Do not inherit addresses that might point to a user's running database.
                for name in list(env):
                    if name.startswith("REDIX_TEST_REDIS"):
                        del env[name]
                env["REDIX_TEST_REDIS_URL"] = f"redis://127.0.0.1:{port}"
                print("运行隔离 Redis 测试；Redis Stack 模块测试仍单独跳过。", flush=True)
                return subprocess.run([
                    "cargo", "test", "--manifest-path", "src-tauri/Cargo.toml",
                    "--test", "redis_integration",
                    "--test", "collection_integration",
                    "--test", "stream_entries_integration",
                    "--", "--ignored", "--nocapture", "--test-threads=1",
                ], cwd=root, env=env).returncode
            finally:
                if server.poll() is None:
                    server.terminate()
                    try:
                        server.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        server.kill()
                        server.wait()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError) as error:
        print(f"本地 Redis 测试未完成：{error}", file=sys.stderr)
        sys.exit(1)
