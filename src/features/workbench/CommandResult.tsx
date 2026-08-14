import type { CommandResult as CommandResultValue, IpcError } from "../../lib/types";

interface CommandResultProps {
  result: CommandResultValue | null;
  error: IpcError | null;
}

function formatResult(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  if (value === null) {
    return "null";
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  try {
    return JSON.stringify(value, null, 2) ?? "undefined";
  } catch {
    return "无法显示结果。";
  }
}

export function CommandResult({ result, error }: CommandResultProps) {
  const formattedResult = result ? formatResult(result.value) : null;
  const isLargeResult = Boolean(formattedResult && formattedResult.length > 2000);

  return (
    <section className="workbench-result-panel" role="group" aria-label="命令结果">
      <div className="workbench-panel-heading">
        <div>
          <p className="eyebrow">RESULT</p>
          <h2>命令结果</h2>
        </div>
        {result ? <span className="command-result-kind">{result.kind}</span> : null}
      </div>

      {error ? (
        <div className="feedback feedback-error" role="alert">
          <strong>{error.code}</strong>
          <span>{error.message}</span>
        </div>
      ) : result && formattedResult !== null ? (
        <details className="command-result-details" open={!isLargeResult}>
          <summary className="command-result-summary">
            {isLargeResult ? "查看完整结果" : "返回值"}
          </summary>
          <pre className="command-result-pre">{formattedResult}</pre>
        </details>
      ) : (
        <p className="workbench-empty-result">执行命令后，Redis 返回值会显示在这里。</p>
      )}
    </section>
  );
}

export default CommandResult;
