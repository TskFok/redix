import type { ConnectionProfile } from "../../lib/types";

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
  topology: "standalone" | "sentinel";
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
    topology: profile?.sentinel ? "sentinel" : "standalone",
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
  PERSISTENCE_FAILED: "本地连接保存失败，请稍后重试。",
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
