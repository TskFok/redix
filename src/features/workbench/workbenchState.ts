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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function isCommandReady(connectionId: string | null, command: string): boolean {
  return connectionId !== null && command.trim().length > 0;
}

export function prependCommandHistory(
  history: CommandHistoryEntry[],
  entry: CommandHistoryEntry,
): CommandHistoryEntry[] {
  return [entry, ...history];
}

export function normalizeWorkbenchError(error: unknown): IpcError {
  if (
    isRecord(error) &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return {
      code: error.code,
      message: error.message,
    };
  }

  return {
    code: "COMMAND_FAILED",
    message: "命令执行失败，请稍后重试。",
  };
}
