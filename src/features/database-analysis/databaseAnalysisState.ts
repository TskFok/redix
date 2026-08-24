import type { AnalyzeDatabaseInput, DatabaseAnalysisReport } from "../../lib/types";

export const DEFAULT_ANALYSIS_INPUT: AnalyzeDatabaseInput = {
  connection_id: "",
  pattern: "*",
  delimiter: ":",
  max_keys: 100000,
};

export interface DatabaseAnalysisPageState {
  input: AnalyzeDatabaseInput;
  loading: boolean;
  report: DatabaseAnalysisReport | null;
  error: string | null;
}

export const initialDatabaseAnalysisState: DatabaseAnalysisPageState = {
  input: { ...DEFAULT_ANALYSIS_INPUT },
  loading: false,
  report: null,
  error: null,
};

export function toUserFacingAnalysisError(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
  ) {
    const messages: Record<string, string> = {
      INVALID_INPUT: "分析参数无效。",
      CONNECTION_FAILED: "无法连接 Redis，请检查连接状态。",
      COMMAND_FAILED: "Redis 返回异常，无法完成分析。",
      IPC_ERROR: "调用数据库分析失败，请稍后重试。",
    };
    return messages[error.code] ?? "数据库分析失败，请稍后重试。";
  }

  return "数据库分析失败，请稍后重试。";
}

export function formatAnalysisNumber(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "不可用";
  }

  return new Intl.NumberFormat("zh-CN").format(value);
}

export function formatCoverage(observed: number, total: number): string {
  if (total <= 0) {
    return "0%";
  }

  return `${((observed / total) * 100).toFixed(1)}%`;
}
