import type { CommandResult, IpcError } from "../../lib/types";

export interface CommandHistoryEntry {
  command: string;
  result: CommandResult;
  created_at: string;
}

export interface WorkbenchPageState {
  command: string;
  history: CommandHistoryEntry[];
  result: CommandResult | null;
  error: IpcError | null;
  loading: boolean;
}

export const initialWorkbenchPageState: WorkbenchPageState = {
  command: "",
  history: [],
  result: null,
  error: null,
  loading: false,
};

const workbenchErrorMessages: Record<string, string> = {
  INVALID_CONNECTION: "连接配置无效，请检查主机、端口和数据库。",
  CONNECTION_FAILED: "无法连接到 Redis 服务器，请检查网络和凭据。",
  AUTHENTICATION_FAILED: "Redis 身份验证失败，请检查用户名和密码。",
  UNSUPPORTED_DATA_TYPE: "当前 Redis 数据类型暂不支持。",
  COMMAND_FAILED: "Redis 操作失败，请稍后重试。",
  PERSISTENCE_FAILED: "本地连接保存失败，请稍后重试。",
  IPC_ERROR: "桌面端调用失败，请稍后重试。",
};

const fallbackWorkbenchError: IpcError = {
  code: "COMMAND_FAILED",
  message: workbenchErrorMessages.COMMAND_FAILED,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function isCommandReady(connectionId: string | null, command: string): boolean {
  return (
    typeof connectionId === "string" &&
    connectionId.trim().length > 0 &&
    normalizeCommand(command).length > 0
  );
}

export function normalizeCommand(command: string): string {
  return command.trim();
}

export function prependCommandHistory(
  history: CommandHistoryEntry[],
  entry: CommandHistoryEntry,
): CommandHistoryEntry[] {
  return [entry, ...history];
}

export function normalizeWorkbenchError(error: unknown): IpcError {
  if (isRecord(error) && typeof error.code === "string") {
    const safeMessage = Object.prototype.hasOwnProperty.call(
      workbenchErrorMessages,
      error.code,
    )
      ? workbenchErrorMessages[error.code]
      : undefined;
    if (!safeMessage) {
      return { ...fallbackWorkbenchError };
    }
    return {
      code: error.code,
      message: safeMessage,
    };
  }

  return { ...fallbackWorkbenchError };
}
