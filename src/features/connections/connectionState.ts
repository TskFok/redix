import type { ConnectionEndpoint, ConnectionProfile, SshAuthMethod } from "../../lib/types";

export interface ConnectionPageState {
  profiles: ConnectionProfile[];
  editingProfile: ConnectionProfile | null;
  testingId: string | null;
  saving: boolean;
  error: string | null;
  activeId: string | null;
  loading: boolean;
  openingId: string | null;
  deletingId: string | null;
}

export interface ConnectionFormValues {
  ssh_enabled: boolean;
  ssh_host: string;
  ssh_port: string;
  ssh_username: string;
  ssh_identity_file: string;
  ssh_known_hosts_file: string;
  ssh_auth_method: SshAuthMethod;
  ssh_password: string;
  ssh_private_key: string;
  ssh_passphrase: string;
  clear_ssh_secrets: boolean;
  topology: "standalone" | "sentinel" | "cluster";
  cluster_nodes: string;
  cluster_read_from_replicas: boolean;
  sentinel_master_name: string;
  sentinel_nodes: string;
  sentinel_username: string;
  sentinel_password: string;
  sentinel_tls: boolean;
  clear_sentinel_password: boolean;
  name: string;
  host: string;
  port: string;
  username: string;
  database: string;
  password: string;
  tls: boolean;
  verify_server_cert: boolean;
  ca_certificate_name: string;
  ca_certificate: string;
  client_certificate_name: string;
  client_certificate: string;
  client_key: string;
  clear_ca_certificate: boolean;
  clear_client_certificate: boolean;
}

export const initialConnectionPageState: ConnectionPageState = {
  profiles: [],
  editingProfile: null,
  testingId: null,
  saving: false,
  error: null,
  activeId: null,
  loading: true,
  openingId: null,
  deletingId: null,
};

export const savedButOpenFailedMessage = "已保存连接，但打开失败，请重试。";
export const connectionSwitchFailedMessage = "切换连接失败，已保留原连接。";
export const connectionCleanupFailedMessage = "切换连接失败，清理新连接失败，请重试。";

export function formValuesFromProfile(
  profile?: ConnectionProfile,
): ConnectionFormValues {
  return {
    ssh_enabled: Boolean(profile?.ssh),
    ssh_host: profile?.ssh?.host ?? "",
    ssh_port: String(profile?.ssh?.port ?? 22),
    ssh_username: profile?.ssh?.username ?? "",
    ssh_identity_file: "",
    ssh_known_hosts_file: "",
    ssh_auth_method: profile?.ssh?.auth_method ?? "agent",
    ssh_password: "",
    ssh_private_key: "",
    ssh_passphrase: "",
    clear_ssh_secrets: false,
    topology: profile?.cluster ? "cluster" : profile?.sentinel ? "sentinel" : "standalone",
    cluster_nodes: profile?.cluster?.nodes.map(formatEndpoint).join("\n") ?? "127.0.0.1:7000",
    cluster_read_from_replicas: profile?.cluster?.read_from_replicas ?? false,
    sentinel_master_name: profile?.sentinel?.master_name ?? "",
    sentinel_nodes: profile?.sentinel?.nodes.map((node) => `${node.host.includes(":") ? `[${node.host}]` : node.host}:${node.port}`).join("\n") ?? "127.0.0.1:26379",
    sentinel_username: profile?.sentinel?.username ?? "",
    sentinel_password: "",
    sentinel_tls: profile?.sentinel?.tls ?? false,
    clear_sentinel_password: false,
    name: profile?.name ?? "",
    host: profile?.host ?? "127.0.0.1",
    port: profile ? String(profile.port) : "6379",
    username: profile?.username ?? "",
    database: profile ? String(profile.database) : "0",
    password: "",
    tls: profile?.tls ?? false,
    verify_server_cert: profile?.verify_server_cert ?? true,
    ca_certificate_name: profile?.ca_certificate_name ?? "",
    ca_certificate: "",
    client_certificate_name: profile?.client_certificate_name ?? "",
    client_certificate: "",
    client_key: "",
    clear_ca_certificate: false,
    clear_client_certificate: false,
  };
}

export function formatEndpoint(node: ConnectionEndpoint): string {
  return `${node.host.includes(":") ? `[${node.host}]` : node.host}:${node.port}`;
}

export function connectionAddress(profile: ConnectionProfile): string {
  if (profile.cluster) return `Cluster · ${profile.cluster.nodes.length} 个种子 · ${profile.cluster.nodes.map(formatEndpoint).join("、")}`;
  if (profile.sentinel) return `Sentinel · ${profile.sentinel.master_name}`;
  return formatEndpoint(profile);
}

export function parseSeedNodes(text: string): ConnectionEndpoint[] | null {
  const lines = text.split(/[\n,]+/).map((line) => line.trim()).filter(Boolean);
  if (!lines.length || lines.length > 32) return null;
  const nodes: ConnectionEndpoint[] = [];
  const seen = new Set<string>();
  for (const line of lines) {
    const match = /^(?:\[([0-9a-fA-F:.]+)\]|([a-zA-Z0-9_.-]+)):(\d+)$/.exec(line);
    if (!match) return null;
    const host = match[1] || match[2];
    if (host.startsWith("-")) return null;
    const port = Number(match[3]);
    if (port < 1 || port > 65535 || !Number.isInteger(port)) return null;
    let normalized = host.toLowerCase();
    if (match[1]) {
      try { normalized = new URL(`http://[${host}]`).hostname; } catch { return null; }
    }
    const key = `${normalized}:${port}`;
    if (seen.has(key)) return null;
    seen.add(key); nodes.push({ host, port });
  }
  return nodes;
}

export function replaceProfile(
  profiles: ConnectionProfile[],
  profile: ConnectionProfile,
): ConnectionProfile[] {
  const existingIndex = profiles.findIndex((item) => item.id === profile.id);
  if (existingIndex === -1) {
    return [...profiles, profile];
  }

  return profiles.map((item) => (item.id === profile.id ? profile : item));
}

const errorMessages: Record<string, string> = {
  INVALID_CONNECTION: "连接配置无效，请检查主机、端口和数据库。",
  CONNECTION_FAILED: "无法连接到 Redis 服务器，请检查网络和凭据。",
  SSH_TUNNEL_FAILED: "SSH 隧道建立失败，请检查 ssh-agent、私钥和 known_hosts；应用不会自动信任未知主机。",
  AUTHENTICATION_FAILED: "Redis 身份验证失败，请检查用户名和密码。",
  CLUSTER_TOPOLOGY_FAILED: "Redis 集群拓扑发现失败，请检查种子节点和集群配置。",
  CLUSTER_NODE_UNAVAILABLE: "Redis 集群节点不可用，请检查节点地址和网络。",
  PERSISTENCE_FAILED: "本地连接保存失败，请稍后重试。",
  INVALID_INPUT: "输入参数无效，请检查连接配置。",
  UNSUPPORTED_DATA_TYPE: "当前 Redis 数据类型暂不支持。",
  COMMAND_FAILED: "Redis 操作失败，请稍后重试。",
  IPC_ERROR: "桌面端调用失败，请稍后重试。",
};

export function toUserFacingError(error: unknown, fallback = "操作失败，请稍后重试。") {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
  ) {
    return errorMessages[error.code] ?? fallback;
  }

  return fallback;
}

export interface ConnectionFailureFeedback {
  message: string;
  details: string[];
}

export function connectionFailureFeedback(
  error: unknown,
  profile: ConnectionProfile,
  fallback: string,
  summary?: string,
): ConnectionFailureFeedback {
  const message = toUserFacingError(error, fallback);
  const details: string[] = summary ? [message] : [];
  if (typeof error === "object" && error !== null && "code" in error &&
      typeof error.code === "string" && Object.hasOwn(errorMessages, error.code)) {
    details.push(`错误码：${error.code}`);
    // diagnostics 仅由后端按已知失败类型生成；任意 message/cause 不进入界面。
    if ("diagnostics" in error && typeof error.diagnostics === "string" && error.diagnostics.trim()) {
      details.push(`失败原因：${error.diagnostics.trim()}`);
    }
  }
  details.push(`连接目标：${connectionAddress(profile)}`);
  if (profile.sentinel) {
    details.push(`Sentinel 节点：${profile.sentinel.nodes.map(formatEndpoint).join("、")}；TLS：${profile.sentinel.tls ? "已启用" : "未启用"}`);
  }
  details.push(`数据库：${profile.cluster ? 0 : profile.database}；TLS：${profile.tls ? (profile.verify_server_cert ? "已启用（校验证书）" : "已启用（不校验证书）") : "未启用"}`);
  if (profile.ssh) details.push(`SSH 隧道：${formatEndpoint(profile.ssh)}`);
  return { message: summary ?? message, details };
}
