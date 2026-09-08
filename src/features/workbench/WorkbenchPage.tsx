import { useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  executeCommand,
  executeCommands,
  getCommandCatalog,
  listCommandHistory,
  saveCommandHistory,
  deleteCommandHistory,
  clearCommandHistory,
} from "../../lib/tauri";
import type {
  CommandDefinition,
  CommandDisplayFormat,
  CommandExecutionItem,
  CommandHistoryEntry,
} from "../../lib/types";
import CommandInput from "./CommandInput";
import CommandResult from "./CommandResult";
import CommandSuggestions, { CommandParameterHelp, currentCommandDefinition, currentCommandLine, filterCommandCatalog } from "./CommandSuggestions";
import {
  initialWorkbenchPageState,
  isCommandReady,
  normalizeCommand,
  normalizeCommandList,
  normalizeWorkbenchError,
  type WorkbenchPageState,
} from "./workbenchState";
import {
  filterHistoryEntry,
  historyEntriesFromExecution,
  prependHistoryEntries,
} from "./workbenchHistory";
import "./workbenchExtras.css";

interface WorkbenchPageProps {
  connectionId: string | null;
  defaultFormat?: CommandDisplayFormat;
  defaultContinueOnError?: boolean;
  initialCommand?: string;
  onCommandConsumed?: () => void;
}

export function WorkbenchPage({
  connectionId,
  defaultFormat = "text",
  defaultContinueOnError = false,
  initialCommand,
  onCommandConsumed,
}: WorkbenchPageProps) {
  const [state, setState] = useState<WorkbenchPageState>(() => ({
    ...initialWorkbenchPageState,
    format: defaultFormat,
    continueOnError: defaultContinueOnError,
  }));
  const [suggestionIndex, setSuggestionIndex] = useState(-1);
  const [cursor, setCursor] = useState(0);
  const [historyBusy, setHistoryBusy] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = useRef<number | null>(null);
  const historyQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  const mountedRef = useRef(false);
  const requestRef = useRef(0);

  const normalizedConnectionId = connectionId?.trim() ?? "";
  const hasConnection = normalizedConnectionId.length > 0;
  const suggestions = filterCommandCatalog(state.command, state.catalog, cursor);
  const commandHelp = currentCommandDefinition(state.command, cursor, state.catalog);

  useLayoutEffect(() => {
    const position = pendingSelectionRef.current;
    if (position === null) return;
    pendingSelectionRef.current = null;
    inputRef.current?.focus();
    inputRef.current?.setSelectionRange(position, position);
  }, [state.command]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRef.current += 1;
    };
  }, []);

  useEffect(() => {
    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    setSuggestionIndex(-1);
    setCursor(0);
    setHistoryBusy(false);
    setState({
      ...initialWorkbenchPageState,
      format: defaultFormat,
      continueOnError: defaultContinueOnError,
      catalogLoading: true,
    });

    void getCommandCatalog()
      .then((catalog) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({ ...current, catalog, catalogLoading: false }));
        }
      })
      .catch((caught) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({
            ...current,
            catalogLoading: false,
            error: normalizeWorkbenchError(caught),
          }));
        }
      });

    if (normalizedConnectionId.length === 0) {
      return;
    }

    void listCommandHistory(normalizedConnectionId)
      .then((history) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({ ...current, history }));
        }
      })
      .catch((caught) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({
            ...current,
            error: normalizeWorkbenchError(caught),
          }));
        }
      });
  }, [normalizedConnectionId]);

  useEffect(() => {
    if (initialCommand === undefined) {
      return;
    }
    setSuggestionIndex(-1);
    setCursor(initialCommand.length);
    setState((current) => ({
      ...current,
      command: initialCommand,
      commands: normalizeCommandList(initialCommand),
      error: null,
    }));
    onCommandConsumed?.();
  }, [initialCommand, onCommandConsumed]);

  const handleSuggestionSelect = (suggestion: CommandDefinition) => {
    const line = currentCommandLine(state.command, cursor);
    const indentation = line.text.match(/^\s*/)?.[0] ?? "";
    const trailing = line.text.endsWith("\r") ? "\r" : "";
    const command = state.command.slice(0, line.start) + indentation + suggestion.name + trailing + state.command.slice(line.end);
    const nextCursor = line.start + indentation.length + suggestion.name.length;
    pendingSelectionRef.current = nextCursor;
    setCursor(nextCursor);
    setSuggestionIndex(-1);
    setState((current) => ({
      ...current,
      command,
      commands: normalizeCommandList(command),
      error: null,
    }));
  };

  const handleCommandKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (suggestions.length === 0) {
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSuggestionIndex((current) => (current + 1) % suggestions.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSuggestionIndex((current) =>
        current <= 0 ? suggestions.length - 1 : current - 1,
      );
    } else if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && suggestionIndex >= 0) {
      event.preventDefault();
      handleSuggestionSelect(suggestions[suggestionIndex]);
    }
  };

  const persistHistory = async (
    activeConnectionId: string,
    history: WorkbenchPageState["history"],
    requestId: number,
  ) => {
    try {
      const pending = historyQueueRef.current.catch(() => undefined).then(() =>
        saveCommandHistory({
          connection_id: activeConnectionId,
          entries: history.filter((entry) => filterHistoryEntry(entry.command)),
        }),
      );
      historyQueueRef.current = pending;
      await pending;
    } catch (caught) {
      if (mountedRef.current && requestRef.current === requestId) {
        setState((current) => ({
          ...current,
          error: normalizeWorkbenchError(caught),
        }));
      }
    }
  };

  const handleExecute = async () => {
    if (
      normalizedConnectionId.length === 0 ||
      !isCommandReady(connectionId, state.command) ||
      state.loading
      || historyBusy
    ) {
      return;
    }

    const activeConnectionId = normalizedConnectionId;
    const command = normalizeCommand(state.command);
    const commands = normalizeCommandList(command);
    if (commands.length === 0) {
      return;
    }
    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    setState((current) => ({
      ...current,
      command,
      commands,
      loading: true,
      result: null,
      batchResults: [],
      error: null,
    }));

    try {
      let items: CommandExecutionItem[];
      let singleResult = null;
      if (commands.length === 1) {
        singleResult = await executeCommand({
          connection_id: activeConnectionId,
          command: commands[0],
        });
        items = [{ command: commands[0], result: singleResult, error_code: null }];
      } else {
        items = await executeCommands({
          connection_id: activeConnectionId,
          commands,
          continue_on_error: state.continueOnError,
        });
      }

      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      const newHistory = historyEntriesFromExecution(activeConnectionId, items);
      const nextHistory = prependHistoryEntries(state.history, newHistory);
      setState((current) => ({
        ...current,
        result: singleResult,
        resultCommand: singleResult ? commands[0] : "",
        batchResults: singleResult ? [] : items,
        history: nextHistory,
        error: null,
        loading: false,
      }));
      void persistHistory(activeConnectionId, nextHistory, requestId);
    } catch (caught) {
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({
        ...current,
        result: null,
        batchResults: [],
        error: normalizeWorkbenchError(caught),
        loading: false,
      }));
    }
  };

  const handleHistorySelect = (command: string) => {
    setCursor(command.length);
    setState((current) => ({
      ...current,
      command,
      commands: normalizeCommandList(command),
      error: null,
    }));
  };

  const handleCommandChange = (command: string) => {
    setCursor(command.length);
    setSuggestionIndex(-1);
    setState((current) => ({
      ...current,
      command,
      commands: normalizeCommandList(command),
      error: null,
    }));
  };

  const handleDeleteHistory = async (entry?: CommandHistoryEntry) => {
    if (!hasConnection || historyBusy || state.loading) return;
    const requestId = requestRef.current;
    const activeConnectionId = normalizedConnectionId;
    setHistoryBusy(true);
    setState((current) => ({ ...current, error: null }));
    const pending = historyQueueRef.current.catch(() => undefined).then(() => entry
      ? deleteCommandHistory({ connection_id: activeConnectionId, command: entry.command, created_at: entry.created_at })
      : clearCommandHistory({ connection_id: activeConnectionId }));
    historyQueueRef.current = pending;
    try {
      await pending;
      if (mountedRef.current && requestRef.current === requestId) {
        setState((current) => ({ ...current, history: entry
          ? current.history.filter((item) => item.command !== entry.command || item.created_at !== entry.created_at)
          : [] }));
      }
    } catch (caught) {
      if (mountedRef.current && requestRef.current === requestId) {
        setState((current) => ({ ...current, error: normalizeWorkbenchError(caught) }));
      }
    } finally {
      if (mountedRef.current && requestRef.current === requestId) setHistoryBusy(false);
    }
  };

  const canExecute = isCommandReady(connectionId, state.command) && !historyBusy;

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
            执行单条或多条 Redis 命令，查看结构化结果；命令目录和历史只使用本地资源。
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
              <span className="panel-hint">
                {state.catalogLoading ? "加载本地目录…" : "支持多行批量执行"}
              </span>
            </div>
            <CommandInput
              inputRef={inputRef}
              onCursorChange={(position) => { if (position !== cursor) { setCursor(position); setSuggestionIndex(-1); } }}
              value={state.command}
              canExecute={canExecute}
              loading={state.loading}
              onChange={handleCommandChange}
              onKeyDown={handleCommandKeyDown}
              onSubmit={() => void handleExecute()}
            />
            <CommandSuggestions
              suggestions={suggestions}
              activeIndex={suggestionIndex}
              onSelect={handleSuggestionSelect}
            />
            <CommandParameterHelp command={commandHelp} />
            <label className="command-continue field">
              <span>
                <input
                  autoCapitalize="off"
                  autoCorrect="off"
                  type="checkbox"
                  checked={state.continueOnError}
                  onChange={(event) =>
                    setState((current) => ({
                      ...current,
                      continueOnError: event.target.checked,
                    }))
                  }
                  disabled={state.loading}
                />
                批量命令遇错后继续
              </span>
            </label>
          </section>

          <section className="workbench-history-panel" aria-labelledby="command-history-title">
            <div className="workbench-panel-heading">
              <div>
                <p className="eyebrow">HISTORY</p>
                <h2 id="command-history-title">命令历史</h2>
              </div>
              <span className="panel-hint">按连接保存，不保存敏感命令</span>
              <button type="button" className="button button-quiet" disabled={historyBusy || state.loading || state.history.length === 0} onClick={() => void handleDeleteHistory()}>清空历史</button>
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
                    <button type="button" className="button button-quiet" disabled={historyBusy || state.loading} aria-label={`删除历史 ${entry.command}`} onClick={() => void handleDeleteHistory(entry)}>删除</button>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="workbench-empty-result">执行过的命令会按最近顺序显示。</p>
            )}
          </section>
        </div>

        <CommandResult
          command={state.resultCommand}
          result={state.result}
          batchResults={state.batchResults}
          format={state.format}
          onFormatChange={(format) => setState((current) => ({ ...current, format }))}
          error={state.error}
        />
      </div>
    </section>
  );
}

export default WorkbenchPage;
