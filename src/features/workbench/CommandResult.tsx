import { useState } from "react";

import type {
  WorkbenchResultFormat,
  CommandExecutionItem,
  CommandResult as CommandResultValue,
  IpcError,
} from "../../lib/types";
import { RespTable, RespTree } from "./RespResult";

interface CommandResultProps {
  result: CommandResultValue | null;
  error: IpcError | null;
  batchResults?: CommandExecutionItem[];
  format?: WorkbenchResultFormat;
  onFormatChange?: (format: WorkbenchResultFormat) => void;
}

export function formatResult(value: unknown, format: WorkbenchResultFormat = "text"): string {
  if (typeof value === "string") {
    return format === "json" ? JSON.stringify(value, null, 2) : value;
  }
  if (value === null) {
    return "null";
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  try {
    return JSON.stringify(value, null, format === "raw" ? 0 : 2) ?? "undefined";
  } catch {
    return "无法显示结果。";
  }
}

export function CommandResult({
  result,
  error,
  batchResults = [],
  format = "text",
  onFormatChange,
}: CommandResultProps) {
  const [copyStatus, setCopyStatus] = useState<string | null>(null);
  const formattedResult = result ? formatResult(result.value, format) : null;
  const isLargeResult = Boolean(formattedResult && formattedResult.length > 2000);
  const hasBatchResults = batchResults.length > 0;
  const copyValue = hasBatchResults
    ? batchResults
        .map((item) =>
          item.result
            ? `${item.command}\n${formatResult(item.result.value, format)}`
            : `${item.command}\n错误：${item.error_code ?? "COMMAND_FAILED"}`,
        )
        .join("\n\n")
    : formattedResult;

  const handleCopy = async () => {
    if (!copyValue) {
      return;
    }
    try {
      await navigator.clipboard.writeText(copyValue);
      setCopyStatus("已复制");
    } catch {
      setCopyStatus("复制失败，请手动复制结果。");
    }
  };

  return (
    <section
      className="workbench-result-panel"
      role="group"
      aria-label="命令结果"
      aria-live="polite"
    >
      <div className="workbench-panel-heading">
        <div>
          <p className="eyebrow">RESULT</p>
          <h2>命令结果</h2>
        </div>
        <div className="command-result-actions">
          {result || hasBatchResults ? (
            <label className="command-result-format field">
              <span>结果格式</span>
              <select
                aria-label="结果格式"
                value={format}
                onChange={(event) =>
                  onFormatChange?.(event.target.value as WorkbenchResultFormat)
                }
              >
                <option value="raw">Raw</option>
                <option value="text">Text</option>
                <option value="json">JSON</option>
                <option value="tree">树形</option>
                <option value="table">表格</option>
              </select>
            </label>
          ) : null}
          {result || hasBatchResults ? (
            <button type="button" className="button button-quiet" onClick={() => void handleCopy()}>
              复制结果
            </button>
          ) : null}
          {result ? <span className="command-result-kind">{result.kind}</span> : null}
        </div>
      </div>

      {error ? (
        <div className="feedback feedback-error" role="alert">
          <strong>{error.code}</strong>
          <span>{error.message}</span>
        </div>
      ) : hasBatchResults ? (
        <ol className="command-batch-results" aria-label="批量命令结果">
          {batchResults.map((item, index) => (
            <li key={`${item.command}-${index}`}>
              <div className="command-batch-heading">
                <code>{item.command}</code>
                <span>{item.error_code ? `错误：${item.error_code}` : "成功"}</span>
              </div>
              {item.result ? (
                format === "tree" ? <RespTree key={JSON.stringify(item.result.value)} value={item.result.value} /> : format === "table" ? <RespTable key={JSON.stringify(item.result.value)} value={item.result.value} /> : <pre className="command-result-pre">
                  {formatResult(item.result.value, format)}
                </pre>
              ) : null}
            </li>
          ))}
        </ol>
      ) : result && formattedResult !== null ? (
        format === "tree" ? <RespTree key={formattedResult} value={result.value} /> : format === "table" ? <RespTable key={formattedResult} value={result.value} /> :
        <details className="command-result-details" open={!isLargeResult}>
          <summary className="command-result-summary">
            {isLargeResult ? "查看完整结果" : "返回值"}
          </summary>
          <pre className="command-result-pre">{formattedResult}</pre>
        </details>
      ) : (
        <p className="workbench-empty-result">执行命令后，Redis 返回值会显示在这里。</p>
      )}
      {copyStatus ? (
        <p className="feedback feedback-success" role="status">
          {copyStatus}
        </p>
      ) : null}
    </section>
  );
}

export default CommandResult;
