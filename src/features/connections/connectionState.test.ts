import { expect, it } from "vitest";
import { connectionFailureFeedback, parseSeedNodes } from "./connectionState";
import type { ConnectionProfile } from "../../lib/types";

const profile: ConnectionProfile = {
  id: "local", name: "本地 Redis", host: "::1", port: 6379, database: 0,
  username: null, has_password: true, tls: true, verify_server_cert: true,
  ca_certificate_name: null, client_certificate_name: null,
  has_ca_certificate: false, has_client_certificate: false,
};

it("未知错误不会把原始消息或未经识别的诊断写入连接提示", () => {
  const result = connectionFailureFeedback({
    code: "UNKNOWN_SECRET", message: "redis://:password@host", diagnostics: "GET private-key",
  }, profile, "测试连接失败");
  expect(result.message).toBe("测试连接失败");
  expect(result.details).toContain("连接目标：[::1]:6379");
  expect(JSON.stringify(result)).not.toMatch(/UNKNOWN_SECRET|password|private-key/);
});

it("Sentinel 失败提示包含发现节点、独立 TLS 和 SSH 地址，不包含凭据元数据", () => {
  const result = connectionFailureFeedback({ code: "AUTHENTICATION_FAILED" }, {
    ...profile,
    sentinel: { master_name: "primary", nodes: [{ host: "::1", port: 26379 }], tls: false, username: "private-user", has_password: true },
    ssh: { host: "bastion", port: 22, username: "private-ssh-user", auth_method: "agent", has_password: false, has_private_key: false, has_passphrase: false, has_identity_file: false, has_known_hosts_file: false },
  }, "测试连接失败");
  expect(result.details).toContain("连接目标：Sentinel · primary");
  expect(result.details).toContain("Sentinel 节点：[::1]:26379；TLS：未启用");
  expect(result.details).toContain("数据库：0；TLS：已启用（校验证书）");
  expect(result.details).toContain("SSH 隧道：bastion:22");
  expect(JSON.stringify(result)).not.toContain("private-");
});

it("Cluster 失败提示包含种子节点并显示固定数据库 0", () => {
  const result = connectionFailureFeedback({ code: "CLUSTER_TOPOLOGY_FAILED" }, {
    ...profile, database: 5, cluster: { nodes: [{ host: "::1", port: 7000 }, { host: "redis-b", port: 7001 }], read_from_replicas: false },
  }, "测试连接失败");
  expect(result.details).toContain("连接目标：Cluster · 2 个种子 · [::1]:7000、redis-b:7001");
  expect(result.details).toContain("数据库：0；TLS：已启用（校验证书）");
});

it("拒绝后端不接受的选项式主机名", () => {
  expect(parseSeedNodes("-host:7000")).toBeNull();
});

it.each([
  "",
  "host:0",
  "host:65536",
  "[garbage]:7000",
  "[1:2]:7000",
  "http://host:7000",
  "a:7000\nA:7000",
  "[::1]:7000\n[0:0:0:0:0:0:0:1]:7000",
  Array.from({ length: 33 }, (_, i) => `node-${i}:7000`).join("\n"),
])("拒绝无效或重复 Cluster seeds %s", (seeds) => {
  expect(parseSeedNodes(seeds)).toBeNull();
});
it("接受 32 个唯一种子及 IPv6 首节点", () => {
  const nodes = parseSeedNodes(
    [
      "[::1]:7000",
      ...Array.from({ length: 31 }, (_, i) => `node-${i}:7000`),
    ].join("\n"),
  );
  expect(nodes).toHaveLength(32);
  expect(nodes?.[0]).toEqual({ host: "::1", port: 7000 });
});
