import type { AppSettings, IpcError } from "../../lib/types";

export const DEFAULT_APP_SETTINGS: AppSettings = {
  version: 1,
  theme: "system",
  result_format: "raw",
  scan_count: 100,
  continue_on_error: false,
};

export function validateAppSettings(settings: AppSettings): string | null {
  if (!Number.isInteger(settings.scan_count) || settings.scan_count < 10 || settings.scan_count > 10000) {
    return "扫描数量必须在 10 到 10000 之间。";
  }
  if (!["system", "light", "dark"].includes(settings.theme)) {
    return "主题设置无效。";
  }
  if (!["raw", "text", "json"].includes(settings.result_format)) {
    return "结果格式设置无效。";
  }
  return null;
}

export function normalizeSettingsError(error: unknown): IpcError {
  const code =
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string"
      ? error.code
      : "PERSISTENCE_FAILED";
  return {
    code,
    message:
      code === "INVALID_CONNECTION"
        ? "设置无效，请检查选项。"
        : "设置保存失败，请稍后重试。",
  };
}
