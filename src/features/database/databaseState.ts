import type { DatabaseOverview, InstanceDetails } from "../../lib/types";

export interface DatabasePageState {
  loading: boolean;
  switching: boolean;
  details: InstanceDetails | null;
  databases: DatabaseOverview[];
  error: string | null;
  switchError: string | null;
  requestId: number;
}

export const initialDatabasePageState: DatabasePageState = {
  loading: true,
  switching: false,
  details: null,
  databases: [],
  error: null,
  switchError: null,
  requestId: 0,
};

export const databaseLoadFailedMessage = "加载数据库概览失败，请稍后重试。";
export const databaseSwitchFailedMessage = "数据库切换失败，请稍后重试。";

export function formatMetric(value: number | string | null | undefined): string {
  if (value === null || value === undefined || value === "") {
    return "不可用";
  }

  return String(value);
}

export function formatBytes(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "不可用";
  }
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  if (value < 1024 * 1024 * 1024) {
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function formatBoolean(value: boolean | null | undefined): string {
  if (value === null || value === undefined) {
    return "不可用";
  }

  return value ? "是" : "否";
}

export function formatPercentage(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "不可用";
  }

  return `${(value * 100).toFixed(1)}%`;
}

export function formatTimestamp(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "不可用";
  }

  return new Date(value * 1_000).toLocaleString("zh-CN", {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function toUserFacingDatabaseError(
  error: unknown,
  fallback = databaseLoadFailedMessage,
): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
  ) {
    const messages: Record<string, string> = {
      CONNECTION_FAILED: "无法连接到 Redis 服务器，请检查连接状态。",
      AUTHENTICATION_FAILED: "Redis 身份验证失败，请检查凭据。",
      INVALID_CONNECTION: "连接配置无效，请重新连接。",
      PERSISTENCE_FAILED: "本地连接保存失败，请稍后重试。",
      COMMAND_FAILED: "Redis 操作失败，请稍后重试。",
      IPC_ERROR: "桌面端调用失败，请稍后重试。",
    };
    return messages[error.code] ?? fallback;
  }

  return fallback;
}
