import { useEffect, useRef, useState } from "react";
import { closeCliSession, executeCliCommand, openCliSession, type CliSessionInput } from "./cliApi";
import { appendCliTranscript, cliError, type CliTranscriptEntry } from "./cliState";
import "./cli.css";

interface CliPageProps { connectionId: string; database: number; isCluster?: boolean }

export default function CliPage({ connectionId, database, isCluster = false }: CliPageProps) {
  const [version, setVersion] = useState(0);
  return <CliSessionView key={`${connectionId}:${database}:${isCluster}:${version}`} connectionId={connectionId} database={database} isCluster={isCluster} onRestart={() => setVersion((current) => current + 1)} />;
}

function CliSessionView({ connectionId, database, isCluster = false, onRestart }: CliPageProps & { onRestart: () => void }) {
  const sessionRef = useRef<CliSessionInput | null>(null);
  const [status, setStatus] = useState<"opening" | "ready" | "closed">("opening");
  const [command, setCommand] = useState("");
  const [busy, setBusy] = useState(false);
  const [entries, setEntries] = useState<CliTranscriptEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(true);
  const inFlight = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    // StrictMode replays effects: each lifetime must own a distinct backend session.
    const session = { connection_id: connectionId, session_id: crypto.randomUUID() };
    sessionRef.current = session;
    let disposed = false;
    active.current = true;
    void openCliSession(session).then(() => {
      if (disposed || sessionRef.current !== session) { void closeCliSession(session).catch(() => undefined); return; }
      setStatus("ready");
    }).catch((reason) => {
      if (!disposed && sessionRef.current === session) { setStatus("closed"); setError(cliError(reason)); }
    });
    return () => {
      disposed = true;
      active.current = false;
      if (sessionRef.current === session) sessionRef.current = null;
      void closeCliSession(session).catch(() => undefined);
    };
  }, [connectionId]);

  useEffect(() => { if (status === "ready" && !busy) inputRef.current?.focus(); }, [status, busy]);

  const execute = async () => {
    const session = sessionRef.current;
    if (!session || status !== "ready" || inFlight.current || !command.trim()) return;
    const raw = command.trim();
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const reply = await executeCliCommand({ ...session, command: raw });
      if (!active.current || sessionRef.current !== session) return;
      const output = reply.error_code ? cliError({ code: reply.error_code })
        : typeof reply.result?.value === "string" ? reply.result.value
        : JSON.stringify(reply.result?.value, null, 2) ?? "null";
      setEntries((current) => appendCliTranscript(current, { command: raw, output, error: reply.error_code !== null }));
      setCommand("");
      if (reply.session_closed) setStatus("closed");
    } catch (reason) {
      if (!active.current || sessionRef.current !== session) return;
      const message = cliError(reason);
      setEntries((current) => appendCliTranscript(current, { command: raw, output: message, error: true }));
      if (message.startsWith("CONNECTION_FAILED") || message.startsWith("IPC_ERROR")) {
        setStatus("closed");
        void closeCliSession(session).catch(() => undefined);
      }
    } finally {
      inFlight.current = false;
      if (active.current) setBusy(false);
    }
  };

  const close = async () => {
    const session = sessionRef.current;
    sessionRef.current = null;
    setStatus("closed");
    if (!session) return;
    try { await closeCliSession(session); } catch (reason) { if (active.current) setError(cliError(reason)); }
  };

  return <section className="cli-page" aria-label="独立 CLI 工作区">
    <div className="page-heading">
      <div><p className="eyebrow">PERSISTENT REDIS SESSION</p><h2>CLI</h2>
        <p className="page-description">{isCluster
          ? "Cluster 普通命令按键槽路由；不支持 MULTI/EXEC、WATCH、SELECT 等连接状态命令。"
          : `独立持久连接 · 初始 DB${database} · MULTI/EXEC、WATCH 和 SELECT 状态仅在当前 CLI 会话保留。`}</p>
      </div>
      <div className="cli-actions">
        <button type="button" className="button button-quiet" disabled={entries.length === 0} onClick={() => setEntries([])}>清空终端</button>
        {status === "closed" ? <button type="button" className="button button-primary" onClick={onRestart}>重新连接 CLI</button>
          : <button type="button" className="button button-danger" onClick={() => void close()}>关闭 CLI</button>}
      </div>
    </div>
    <p className="panel-hint cli-note">不自动保存命令或输出；内容可能含敏感数据。最多保留 200 条 / 2 MiB，单次输出上限 256 KiB。{isCluster ? "离开页面会关闭当前路由会话。" : "离开页面会关闭会话并丢弃未提交事务。"}</p>
    <p className="panel-hint cli-note">每条命令最多等待 5 秒；超时会丢弃连接且不会自动重试。{isCluster ? "订阅与 MONITOR 在 Cluster 中不支持。" : "订阅与 MONITOR 请使用运维观察。"}</p>
    {error ? <p role="alert" className="feedback feedback-error">{error}</p> : null}
    <div className="cli-terminal" role="log" aria-label="CLI 输出" aria-live="polite">
      {entries.length === 0 ? <p>Redis CLI · {status === "opening" ? "连接中…" : status === "closed" ? "会话已关闭" : "连接就绪"}</p> : null}
      {entries.map((entry, index) => <div key={index} className={entry.error ? "cli-entry cli-entry-error" : "cli-entry"}>
        <div className="cli-command"><span aria-hidden="true">redis&gt; </span>{entry.command}</div><pre>{entry.output}</pre>
      </div>)}
    </div>
    <form className="cli-input" onSubmit={(event) => { event.preventDefault(); void execute(); }}>
      <label className="field"><span>CLI 命令</span><input autoCapitalize="off" autoCorrect="off" ref={inputRef} value={command} onChange={(event) => setCommand(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void execute(); } }} disabled={status !== "ready" || busy} spellCheck={false} autoComplete="off" placeholder="输入 Redis 命令，按 Enter 执行" /></label>
      <button type="submit" className="button button-primary" disabled={status !== "ready" || busy || !command.trim()}>{busy ? "执行中…" : "执行命令"}</button>
    </form>
  </section>;
}
