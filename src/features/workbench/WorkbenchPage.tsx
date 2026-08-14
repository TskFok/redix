import { useEffect, useRef, useState } from "react";

import { executeCommand } from "../../lib/tauri";
import CommandInput from "./CommandInput";
import CommandResult from "./CommandResult";
import {
  initialWorkbenchPageState,
  isCommandReady,
  normalizeCommand,
  normalizeWorkbenchError,
  prependCommandHistory,
  type WorkbenchPageState,
} from "./workbenchState";

interface WorkbenchPageProps {
  connectionId: string | null;
}

export function WorkbenchPage({ connectionId }: WorkbenchPageProps) {
  const [state, setState] = useState<WorkbenchPageState>(() => ({
    ...initialWorkbenchPageState,
  }));
  const mountedRef = useRef(false);
  const requestRef = useRef(0);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRef.current += 1;
    };
  }, []);

  useEffect(() => {
    requestRef.current += 1;
    setState({ ...initialWorkbenchPageState });
  }, [connectionId]);

  const handleExecute = async () => {
    if (
      normalizedConnectionId.length === 0 ||
      !isCommandReady(connectionId, state.command) ||
      state.loading
    ) {
      return;
    }

    const activeConnectionId = normalizedConnectionId;
    const command = normalizeCommand(state.command);
    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    setState((current) => ({
      ...current,
      command,
      loading: true,
      error: null,
    }));

    try {
      const result = await executeCommand({
        connection_id: activeConnectionId,
        command,
      });
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({
        ...current,
        result,
        error: null,
        loading: false,
        history: prependCommandHistory(current.history, {
          command,
          result,
          created_at: new Date().toISOString(),
        }),
      }));
    } catch (caught) {
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({
        ...current,
        result: null,
        error: normalizeWorkbenchError(caught),
        loading: false,
      }));
    }
  };

  const handleHistorySelect = (command: string) => {
    setState((current) => ({
      ...current,
      command,
      error: null,
    }));
  };

  const normalizedConnectionId = connectionId?.trim() ?? "";
  const hasConnection = normalizedConnectionId.length > 0;
  const canExecute = isCommandReady(connectionId, state.command);

  return (
    <section
      className="workbench-page"
      aria-labelledby="workbench-page-title"
      aria-busy={state.loading}
    >
      <div className="page-heading workbench-page-heading">
        <div>
          <p className="eyebrow">COMMAND WORKSPACE</p>
          <h2 id="workbench-page-title">Workbench</h2>
          <p className="page-description">
            直接执行 Redis 命令并查看结构化返回值，命令会通过安全的桌面端 IPC 执行。
          </p>
        </div>
        <span className="workbench-connection-id">
          {hasConnection ? `连接：${normalizedConnectionId}` : "未连接"}
        </span>
      </div>

      {!hasConnection ? (
        <p className="feedback feedback-error" role="status" aria-live="polite">
          请先连接 Redis，再执行命令。
        </p>
      ) : null}
      {state.loading ? (
        <p className="loading-state" role="status" aria-live="polite">
          执行中…
        </p>
      ) : null}

      <div className="workbench-layout">
        <div className="workbench-command-column">
          <section className="workbench-command-panel" aria-labelledby="command-input-title">
            <div className="workbench-panel-heading">
              <div>
                <p className="eyebrow">COMMAND</p>
                <h2 id="command-input-title">输入命令</h2>
              </div>
              <span className="panel-hint">仅发送当前输入</span>
            </div>
            <CommandInput
              value={state.command}
              canExecute={canExecute}
              loading={state.loading}
              onChange={(command) =>
                setState((current) => ({ ...current, command, error: null }))
              }
              onSubmit={() => void handleExecute()}
            />
          </section>

          <section className="workbench-history-panel" aria-labelledby="command-history-title">
            <div className="workbench-panel-heading">
              <div>
                <p className="eyebrow">HISTORY</p>
                <h2 id="command-history-title">命令历史</h2>
              </div>
              <span className="panel-hint">仅保存在当前会话</span>
            </div>
            {state.history.length > 0 ? (
              <ol className="workbench-history-list" aria-label="命令历史">
                {state.history.map((entry, index) => (
                  <li key={`${entry.created_at}-${index}`}>
                    <button
                      type="button"
                      className="workbench-history-button"
                      onClick={() => handleHistorySelect(entry.command)}
                      aria-label={`回填命令 ${entry.command}`}
                    >
                      <code>{entry.command}</code>
                      <time dateTime={entry.created_at}>
                        {new Date(entry.created_at).toLocaleTimeString("zh-CN", {
                          hour: "2-digit",
                          minute: "2-digit",
                          second: "2-digit",
                        })}
                      </time>
                    </button>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="workbench-empty-result">执行过的命令会按最近顺序显示。</p>
            )}
          </section>
        </div>

        <CommandResult result={state.result} error={state.error} />
      </div>
    </section>
  );
}

export default WorkbenchPage;
