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
  name: string;
  host: string;
  port: string;
  username: string;
  database: string;
  password: string;
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

export function formValuesFromProfile(
  profile?: ConnectionProfile,
): ConnectionFormValues {
  return {
    name: profile?.name ?? "",
    host: profile?.host ?? "127.0.0.1",
    port: profile ? String(profile.port) : "6379",
    username: profile?.username ?? "",
    database: profile ? String(profile.database) : "0",
    password: "",
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
