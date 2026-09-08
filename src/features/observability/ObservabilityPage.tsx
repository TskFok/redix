import Select from "../../components/Select";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { useTransientFeedback } from "../../components/useTransientFeedback";

import {
  clearSlowLogs,
  getSlowLogConfig,
  getSlowLogs,
  publishPubSub,
  startProfiler,
  startPubSub,
  stopProfiler,
  stopPubSub,
  updateSlowLogConfig,
} from "../../lib/tauri";
import type {
  ProfilerEvent,
  ProfilerSession,
  ProfilerStatusEvent,
  PubSubMessageEvent,
  PubSubSession,
  PubSubStatusEvent,
  SlowLogConfig,
  SlowLogEntry,
} from "../../lib/types";
import {
  appendProfilerEvent,
  appendPubSubMessage,
  formatProfilerCommand,
  formatProfilerTime,
  formatPubSubTime,
  formatSlowLogDuration,
  formatSlowLogTime,
  parsePubSubTopics,
  toUserFacingObservabilityError,
} from "./observabilityState";
import { buildProfilerExport, buildPubSubExport, buildSlowLogExport, downloadObservabilityExport, filterProfilerEvents, filterPubSubMessages, filterSlowLogs, type ObservabilityExport } from "./observabilityExport";
import { usePausedFeed } from "./usePausedFeed";
import "./observabilityExtras.css";

const PUBSUB_MESSAGE_EVENT = "redix://pubsub/message";
const PUBSUB_STATUS_EVENT = "redix://pubsub/status";
const PROFILER_EVENT = "redix://profiler/event";
const PROFILER_STATUS_EVENT = "redix://profiler/status";

type ObservabilityTab = "slowlog" | "pubsub" | "profiler";

interface ObservabilityPageProps {
  connectionId: string;
  isCluster?: boolean;
}

export function ObservabilityPage({ connectionId, isCluster }: ObservabilityPageProps) {
  return <ObservabilitySessionPage key={`${connectionId}:${isCluster}`} connectionId={connectionId} isCluster={isCluster} />;
}

function ObservabilitySessionPage({ connectionId, isCluster = false }: ObservabilityPageProps) {
  const [tab, setTab] = useState<ObservabilityTab>("slowlog");
  const [slowLogs, setSlowLogs] = useState<SlowLogEntry[]>([]);
  const [slowLogConfig, setSlowLogConfig] = useState<SlowLogConfig | null>(null);
  const [slowLogCount, setSlowLogCount] = useState("50");
  const [slowLogMaxLen, setSlowLogMaxLen] = useState("");
  const [slowLogSlowerThan, setSlowLogSlowerThan] = useState("");
  const [slowLogLoading, setSlowLogLoading] = useState(true);
  const [slowLogBusy, setSlowLogBusy] = useState(false);
  const [topicText, setTopicText] = useState("events");
  const [topicPattern, setTopicPattern] = useState(false);
  const [pubSubSession, setPubSubSession] = useState<PubSubSession | null>(null);
  const [pubSubStatus, setPubSubStatus] = useState("idle");
  const [pubSubMessages, setPubSubMessages] = useState<PubSubMessageEvent[]>([]);
  const [pubSubBusy, setPubSubBusy] = useState(false);
  const [publishChannel, setPublishChannel] = useState("events");
  const [publishMessage, setPublishMessage] = useState("");
  const [publishFeedback, setPublishFeedback] = useTransientFeedback();
  const [profilerSession, setProfilerSession] = useState<ProfilerSession | null>(null);
  const [profilerStatus, setProfilerStatus] = useState("idle");
  const [profilerEvents, setProfilerEvents] = useState<ProfilerEvent[]>([]);
  const [profilerBusy, setProfilerBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sessionRef = useRef<PubSubSession | null>(null);
  const profilerSessionRef = useRef<ProfilerSession | null>(null);
  const requestRef = useRef(0);
  const activeRef = useRef(true);

  useEffect(() => {
    activeRef.current = true;
    return () => { activeRef.current = false; };
  }, []);

  useEffect(() => {
    if (isCluster) {
      requestRef.current += 1;
      setSlowLogConfig(null);
      setSlowLogs([]);
      setSlowLogLoading(false);
      setError(null);
      return;
    }

    let disposed = false;
    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    setSlowLogLoading(true);
    setError(null);

    void Promise.all([
      getSlowLogConfig(connectionId),
      getSlowLogs({
        connection_id: connectionId,
        count: Number(slowLogCount),
      }),
    ])
      .then(([config, logs]) => {
        if (disposed || requestRef.current !== requestId) {
          return;
        }
        setSlowLogConfig(config);
        setSlowLogMaxLen(String(config.slowlog_max_len));
        setSlowLogSlowerThan(String(config.slowlog_log_slower_than));
        setSlowLogs(logs);
      })
      .catch((reason: unknown) => {
        if (!disposed && requestRef.current === requestId) {
          setError(toUserFacingObservabilityError(reason));
        }
      })
      .finally(() => {
        if (!disposed && requestRef.current === requestId) {
          setSlowLogLoading(false);
        }
      });

    return () => {
      disposed = true;
      requestRef.current += 1;
    };
  }, [connectionId, isCluster, slowLogCount]);

  useEffect(() => {
    let disposed = false;
    let unlistenMessage: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;
    let unlistenProfiler: (() => void) | undefined;
    let unlistenProfilerStatus: (() => void) | undefined;

    void Promise.all([
      listen<PubSubMessageEvent>(PUBSUB_MESSAGE_EVENT, (event) => {
        const payload = event.payload;
        const session = sessionRef.current;
        if (
          payload.connection_id !== connectionId ||
          session === null ||
          payload.session_id !== session.session_id
        ) {
          return;
        }
        setPubSubMessages((current) => appendPubSubMessage(current, payload));
      }),
      listen<PubSubStatusEvent>(PUBSUB_STATUS_EVENT, (event) => {
        const payload = event.payload;
        const session = sessionRef.current;
        if (
          payload.connection_id !== connectionId ||
          (session !== null && payload.session_id !== session.session_id)
        ) {
          return;
        }
        setPubSubStatus(payload.state);
      }),
      listen<ProfilerEvent>(PROFILER_EVENT, (event) => {
        const payload = event.payload;
        const session = profilerSessionRef.current;
        if (
          payload.connection_id !== connectionId ||
          session === null ||
          payload.session_id !== session.session_id
        ) {
          return;
        }
        setProfilerEvents((current) => appendProfilerEvent(current, payload));
      }),
      listen<ProfilerStatusEvent>(PROFILER_STATUS_EVENT, (event) => {
        const payload = event.payload;
        const session = profilerSessionRef.current;
        if (
          payload.connection_id !== connectionId ||
          (session !== null && payload.session_id !== session.session_id)
        ) {
          return;
        }
        setProfilerStatus(payload.state);
      }),
    ])
      .then(([messageUnlisten, statusUnlisten, profilerUnlisten, profilerStatusUnlisten]) => {
        if (disposed) {
          messageUnlisten();
          statusUnlisten();
          profilerUnlisten();
          profilerStatusUnlisten();
          return;
        }
        unlistenMessage = messageUnlisten;
        unlistenStatus = statusUnlisten;
        unlistenProfiler = profilerUnlisten;
        unlistenProfilerStatus = profilerStatusUnlisten;
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setError(toUserFacingObservabilityError(reason));
        }
      });

    return () => {
      disposed = true;
      unlistenMessage?.();
      unlistenStatus?.();
      unlistenProfiler?.();
      unlistenProfilerStatus?.();
      const session = sessionRef.current;
      sessionRef.current = null;
      if (session) {
        void stopPubSub({
          connection_id: session.connection_id,
          session_id: session.session_id,
        }).catch(() => undefined);
      }
      const profiler = profilerSessionRef.current;
      profilerSessionRef.current = null;
      if (profiler) {
        void stopProfiler({
          connection_id: profiler.connection_id,
          session_id: profiler.session_id,
        }).catch(() => undefined);
      }
    };
  }, [connectionId]);

  const refreshSlowLogs = async () => {
    setSlowLogBusy(true);
    setError(null);
    try {
      const logs = await getSlowLogs({
        connection_id: connectionId,
        count: Number(slowLogCount),
      });
      setSlowLogs(logs);
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setSlowLogBusy(false);
    }
  };

  const handleClearSlowLogs = async () => {
    setSlowLogBusy(true);
    setError(null);
    try {
      await clearSlowLogs(connectionId);
      setSlowLogs([]);
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setSlowLogBusy(false);
    }
  };

  const handleSaveSlowLogConfig = async () => {
    const maxLen = Number.parseInt(slowLogMaxLen, 10);
    const slowerThan = Number.parseInt(slowLogSlowerThan, 10);
    if (!Number.isSafeInteger(maxLen) || maxLen < 0 || !Number.isSafeInteger(slowerThan)) {
      setError("Slow Log 配置必须是有效的整数。");
      return;
    }

    setSlowLogBusy(true);
    setError(null);
    try {
      const config = await updateSlowLogConfig({
        connection_id: connectionId,
        slowlog_max_len: maxLen,
        slowlog_log_slower_than: slowerThan,
      });
      setSlowLogConfig(config);
      setSlowLogMaxLen(String(config.slowlog_max_len));
      setSlowLogSlowerThan(String(config.slowlog_log_slower_than));
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setSlowLogBusy(false);
    }
  };

  const handleStartPubSub = async () => {
    const topics = parsePubSubTopics(topicText, topicPattern);
    if (topics.length === 0) {
      setError("至少输入一个频道或模式。");
      return;
    }

    setPubSubBusy(true);
    setError(null);
    setPublishFeedback(null);
    const sessionId = createSessionId();
    try {
      const session = await startPubSub({
        connection_id: connectionId,
        session_id: sessionId,
        topics,
      });
      if (!activeRef.current) {
        await stopPubSub({ connection_id: session.connection_id, session_id: session.session_id });
        return;
      }
      sessionRef.current = session;
      setPubSubSession(session);
      setPubSubStatus("running");
      setPubSubMessages([]);
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setPubSubBusy(false);
    }
  };

  const handleStopPubSub = async () => {
    const session = sessionRef.current;
    if (!session) {
      return;
    }

    setPubSubBusy(true);
    setError(null);
    try {
      await stopPubSub({
        connection_id: session.connection_id,
        session_id: session.session_id,
      });
      sessionRef.current = null;
      setPubSubSession(null);
      setPubSubStatus("stopped");
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setPubSubBusy(false);
    }
  };

  const handleStartProfiler = async () => {
    setProfilerBusy(true);
    setError(null);
    const sessionId = createSessionId("profiler");
    try {
      const session = await startProfiler({
        connection_id: connectionId,
        session_id: sessionId,
      });
      if (!activeRef.current) {
        await stopProfiler({ connection_id: session.connection_id, session_id: session.session_id });
        return;
      }
      profilerSessionRef.current = session;
      setProfilerSession(session);
      setProfilerStatus("running");
      setProfilerEvents([]);
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setProfilerBusy(false);
    }
  };

  const handleStopProfiler = async () => {
    const session = profilerSessionRef.current;
    if (!session) {
      return;
    }

    setProfilerBusy(true);
    setError(null);
    try {
      await stopProfiler({
        connection_id: session.connection_id,
        session_id: session.session_id,
      });
      profilerSessionRef.current = null;
      setProfilerSession(null);
      setProfilerStatus("stopped");
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setProfilerBusy(false);
    }
  };

  const handlePublish = async () => {
    if (!publishChannel.trim() || !publishMessage) {
      setError("频道和消息不能为空。");
      return;
    }

    setPubSubBusy(true);
    setError(null);
    setPublishFeedback(null);
    try {
      const receivers = await publishPubSub({
        connection_id: connectionId,
        channel: publishChannel.trim(),
        message: publishMessage,
      });
      setPublishFeedback(`消息已发送，当前有 ${receivers} 个订阅者。`);
      setPublishMessage("");
    } catch (reason) {
      setError(toUserFacingObservabilityError(reason));
    } finally {
      setPubSubBusy(false);
    }
  };

  return (
    <section
      className="observability-page"
      aria-labelledby="observability-page-title"
      aria-busy={slowLogLoading || slowLogBusy || pubSubBusy || profilerBusy}
    >
      <div className="page-heading observability-page-heading">
        <div>
          <p className="eyebrow">OPERATIONS / OBSERVABILITY</p>
          <h2 id="observability-page-title">运维观察</h2>
          <p className="page-description">
            用 Slow Log 定位慢命令、通过 Pub/Sub 观察频道消息，并用 Profiler 查看实时命令。操作针对当前连接。
          </p>
        </div>
        <span className="observability-scope">当前连接 · {connectionId}</span>
      </div>

      {isCluster && <p className="observability-warning">Cluster Slow Log 节点作用域尚未支持；Pub/Sub 和 Profiler 同样不可用。可在命令工作台显式执行原生命令，其路由范围由驱动决定。</p>}
      <div className="observability-tabs" role="tablist" aria-label="运维观察模块">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "slowlog"}
          className={`observability-tab${tab === "slowlog" ? " observability-tab-active" : ""}`}
          onClick={() => setTab("slowlog")}
          disabled={isCluster}
        >
          Slow Log
          <small>慢命令记录</small>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "pubsub"}
          className={`observability-tab${tab === "pubsub" ? " observability-tab-active" : ""}`}
          onClick={() => setTab("pubsub")}
          disabled={isCluster}
        >
          Pub/Sub
          <small>频道消息流</small>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "profiler"}
          className={`observability-tab${tab === "profiler" ? " observability-tab-active" : ""}`}
          onClick={() => setTab("profiler")}
          disabled={isCluster}
        >
          Profiler
          <small>实时命令监控</small>
        </button>
      </div>

      {error ? (
        <p className="inline-error" role="alert">
          {error}
        </p>
      ) : null}

      {isCluster ? (
        <p className="empty-state">Cluster 观察功能需要明确的节点作用域，当前尚未开放。</p>
      ) : tab === "slowlog" ? (
        <SlowLogPanel
          config={slowLogConfig}
          count={slowLogCount}
          logs={slowLogs}
          loading={slowLogLoading}
          busy={slowLogBusy}
          maxLen={slowLogMaxLen}
          slowerThan={slowLogSlowerThan}
          onCountChange={setSlowLogCount}
          onMaxLenChange={setSlowLogMaxLen}
          onSlowerThanChange={setSlowLogSlowerThan}
          onRefresh={() => void refreshSlowLogs()}
          onClear={() => void handleClearSlowLogs()}
          onSaveConfig={() => void handleSaveSlowLogConfig()}
        />
      ) : tab === "pubsub" ? (
        <PubSubPanel
          key={`pubsub-${pubSubSession?.session_id ?? "idle"}`}
          activeSession={pubSubSession}
          busy={pubSubBusy}
          messages={pubSubMessages}
          pattern={topicPattern}
          publishChannel={publishChannel}
          publishFeedback={publishFeedback}
          publishMessage={publishMessage}
          status={pubSubStatus}
          topicText={topicText}
          onClearMessages={() => setPubSubMessages([])}
          onPatternChange={setTopicPattern}
          onPublish={() => void handlePublish()}
          onPublishChannelChange={setPublishChannel}
          onPublishMessageChange={setPublishMessage}
          onStart={() => void handleStartPubSub()}
          onStop={() => void handleStopPubSub()}
          onTopicTextChange={setTopicText}
        />
      ) : (
        <ProfilerPanel
          key={`profiler-${profilerSession?.session_id ?? "idle"}`}
          activeSession={profilerSession}
          busy={profilerBusy}
          events={profilerEvents}
          status={profilerStatus}
          onClearEvents={() => setProfilerEvents([])}
          onStart={() => void handleStartProfiler()}
          onStop={() => void handleStopProfiler()}
        />
      )}
    </section>
  );
}

interface SlowLogPanelProps {
  config: SlowLogConfig | null;
  count: string;
  logs: SlowLogEntry[];
  loading: boolean;
  busy: boolean;
  maxLen: string;
  slowerThan: string;
  onCountChange: (value: string) => void;
  onMaxLenChange: (value: string) => void;
  onSlowerThanChange: (value: string) => void;
  onRefresh: () => void;
  onClear: () => void;
  onSaveConfig: () => void;
}

function SlowLogPanel({
  config,
  count,
  logs,
  loading,
  busy,
  maxLen,
  slowerThan,
  onCountChange,
  onMaxLenChange,
  onSlowerThanChange,
  onRefresh,
  onClear,
  onSaveConfig,
}: SlowLogPanelProps) {
  const [query, setQuery] = useState("");
  const filteredLogs = filterSlowLogs(logs, query);
  return (
    <div className="observability-content">
      <section className="observability-panel" aria-labelledby="slow-log-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">SLOWLOG GET</p>
            <h3 id="slow-log-title">慢命令记录</h3>
          </div>
          <div className="observability-toolbar">
            <ExportButton label="导出 Slow Log CSV" disabled={loading || filteredLogs.length === 0} output={() => buildSlowLogExport(logs, "csv", query)} />
            <ExportButton label="导出 Slow Log JSON" disabled={loading || filteredLogs.length === 0} output={() => buildSlowLogExport(logs, "json", query)} />
            <label className="observability-inline-field">
              <span>读取数量</span>
              <Select
                aria-label="读取数量"
                value={count}
                onChange={(event) => onCountChange(event.target.value)}
                disabled={busy}
              >
                <option value="20">20 条</option>
                <option value="50">50 条</option>
                <option value="100">100 条</option>
                <option value="-1">全部</option>
              </Select>
            </label>
            <button type="button" className="button button-secondary button-compact" onClick={onRefresh} disabled={busy}>
              {loading || busy ? "刷新中…" : "刷新"}
            </button>
            <button type="button" className="button button-danger button-compact" onClick={onClear} disabled={busy}>
              清空记录
            </button>
          </div>
        </div>

        <p className="panel-hint observability-panel-hint">
          Redis 服务器维护环形慢日志；清空后只会影响当前实例的 Slow Log。
        </p>
        <label className="field"><span>筛选慢日志</span><input autoCapitalize="off" autoCorrect="off" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="命令、来源或客户端" /></label>
        <p className="panel-hint">仅导出当前筛选结果。命令参数可能包含敏感数据，请妥善保管导出文件。</p>

        {loading ? (
          <p className="loading-state" role="status">
            正在读取 Slow Log…
          </p>
        ) : filteredLogs.length === 0 ? (
          <p className="empty-state-compact">{logs.length === 0 ? "当前没有慢命令记录。" : "没有匹配的慢命令记录。"}</p>
        ) : (
          <div className="observability-table-wrap">
            <table className="observability-table">
              <thead>
                <tr>
                  <th scope="col">时间</th>
                  <th scope="col">耗时</th>
                  <th scope="col">命令</th>
                  <th scope="col">来源</th>
                  <th scope="col">客户端</th>
                </tr>
              </thead>
              <tbody>
                {filteredLogs.map((entry) => (
                  <tr key={entry.id}>
                    <td>{formatSlowLogTime(entry.time)}</td>
                    <td className="observability-mono">{formatSlowLogDuration(entry.duration_us)}</td>
                    <td>
                      <code className="observability-command">{entry.args.join(" ")}</code>
                    </td>
                    <td className="observability-mono">{entry.source || "不可用"}</td>
                    <td>{entry.client || "不可用"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="observability-panel" aria-labelledby="slow-log-config-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">CONFIG SET</p>
            <h3 id="slow-log-config-title">采集配置</h3>
          </div>
          <span className="observability-config-summary">
            {config ? `当前保留 ${config.slowlog_max_len} 条` : "配置不可用"}
          </span>
        </div>
        <div className="observability-config-grid">
          <label className="field">
            <span>slowlog-max-len</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="number"
              min="0"
              step="1"
              value={maxLen}
              onChange={(event) => onMaxLenChange(event.target.value)}
              disabled={busy}
            />
            <small>慢日志最多保留的记录数量。</small>
          </label>
          <label className="field">
            <span>slowlog-log-slower-than</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="number"
              step="1"
              value={slowerThan}
              onChange={(event) => onSlowerThanChange(event.target.value)}
              disabled={busy}
            />
            <small>微秒阈值；0 表示记录所有命令，负数表示禁用。</small>
          </label>
        </div>
        <div className="observability-panel-actions">
          <span className="observability-form-note">修改会立即写入 Redis 配置。</span>
          <button type="button" className="button button-primary" onClick={onSaveConfig} disabled={busy || loading}>
            保存配置
          </button>
        </div>
      </section>
    </div>
  );
}

interface PubSubPanelProps {
  activeSession: PubSubSession | null;
  busy: boolean;
  messages: PubSubMessageEvent[];
  pattern: boolean;
  publishChannel: string;
  publishFeedback: string | null;
  publishMessage: string;
  status: string;
  topicText: string;
  onClearMessages: () => void;
  onPatternChange: (value: boolean) => void;
  onPublish: () => void;
  onPublishChannelChange: (value: string) => void;
  onPublishMessageChange: (value: string) => void;
  onStart: () => void;
  onStop: () => void;
  onTopicTextChange: (value: string) => void;
}

function PubSubPanel({
  activeSession,
  busy,
  messages,
  pattern,
  publishChannel,
  publishFeedback,
  publishMessage,
  status,
  topicText,
  onClearMessages,
  onPatternChange,
  onPublish,
  onPublishChannelChange,
  onPublishMessageChange,
  onStart,
  onStop,
  onTopicTextChange,
}: PubSubPanelProps) {
  const [query, setQuery] = useState("");
  const feed = usePausedFeed(messages, onClearMessages);
  const filteredMessages = filterPubSubMessages(feed.displayed, query);
  return (
    <div className="observability-content">
      <section className="observability-panel" aria-labelledby="pubsub-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">SUBSCRIBE / PSUBSCRIBE</p>
            <h3 id="pubsub-title">消息订阅</h3>
          </div>
          <span className={`observability-status observability-status-${status}`} role="status">
            <span className="observability-status-dot" aria-hidden="true" />
            {activeSession ? "订阅中" : status === "stopped" ? "已停止" : "未启动"}
          </span>
        </div>
        <div className="pubsub-subscribe-grid">
          <label className="field">
            <span>频道或模式（每行一个）</span>
            <textarea
              autoCapitalize="off"
              autoCorrect="off"
              aria-label="频道或模式"
              value={topicText}
              onChange={(event) => onTopicTextChange(event.target.value)}
              disabled={busy || activeSession !== null}
              rows={4}
            />
            <small>模式订阅使用 PSUBSCRIBE，例如 user:*。</small>
          </label>
          <div className="pubsub-subscribe-actions">
            <label className="checkbox-field">
              <input
                autoCapitalize="off"
                autoCorrect="off"
                type="checkbox"
                checked={pattern}
                onChange={(event) => onPatternChange(event.target.checked)}
                disabled={busy || activeSession !== null}
              />
              <span>按模式订阅</span>
            </label>
            {activeSession ? (
              <button type="button" className="button button-danger" onClick={onStop} disabled={busy}>
                {busy ? "停止中…" : "停止订阅"}
              </button>
            ) : (
              <button type="button" className="button button-primary" onClick={onStart} disabled={busy}>
                {busy ? "启动中…" : "开始订阅"}
              </button>
            )}
          </div>
        </div>
        {activeSession ? (
          <p className="observability-session-note">
            会话 {activeSession.session_id} · {activeSession.topics.length} 个订阅项
          </p>
        ) : null}
      </section>

      <section className="observability-panel" aria-labelledby="pubsub-publish-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">PUBLISH</p>
            <h3 id="pubsub-publish-title">发送消息</h3>
          </div>
        </div>
        <div className="pubsub-publish-grid">
          <label className="field">
            <span>频道</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              value={publishChannel}
              onChange={(event) => onPublishChannelChange(event.target.value)}
              disabled={busy}
            />
          </label>
          <label className="field pubsub-message-field">
            <span>消息</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              value={publishMessage}
              onChange={(event) => onPublishMessageChange(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  onPublish();
                }
              }}
              disabled={busy}
            />
          </label>
          <button type="button" className="button button-secondary" onClick={onPublish} disabled={busy}>
            发送
          </button>
        </div>
        {publishFeedback ? (
          <p className="feedback feedback-success" role="status">
            {publishFeedback}
          </p>
        ) : null}
      </section>

      <section className="observability-panel" aria-labelledby="pubsub-message-list-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">LIVE MESSAGES</p>
            <h3 id="pubsub-message-list-title">消息流</h3>
          </div>
          <div className="observability-toolbar">
            <span className="observability-config-summary">{messages.length} / 5000 条 · 显示 {filteredMessages.length} 条</span>
            <button type="button" className="button button-quiet button-compact" onClick={feed.toggle}>{feed.paused ? "恢复显示" : "暂停显示"}</button>
            <ExportButton label="导出消息 JSON" disabled={filteredMessages.length === 0} output={() => buildPubSubExport(feed.displayed, query)} />
            <button type="button" className="button button-quiet button-compact" onClick={feed.clear} disabled={messages.length === 0 && feed.displayed.length === 0}>
              清空视图
            </button>
          </div>
        </div>
        <label className="field"><span>筛选消息</span><input autoCapitalize="off" autoCorrect="off" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="频道、模式或消息内容" /></label>
        <p className="panel-hint">{feed.paused ? "已暂停显示；后台继续接收，恢复后显示最新缓存。" : "缓存最多保留最新 5000 条消息。"} 仅导出当前筛选的可见快照，消息可能含敏感数据；不会自动保存。</p>
        {filteredMessages.length === 0 ? (
          <p className="empty-state-compact">启动订阅后，收到的频道消息会显示在这里。</p>
        ) : (
          <div className="observability-table-wrap pubsub-message-table-wrap">
            <table className="observability-table pubsub-message-table">
              <thead>
                <tr>
                  <th scope="col">时间</th>
                  <th scope="col">频道</th>
                  <th scope="col">匹配模式</th>
                  <th scope="col">消息</th>
                </tr>
              </thead>
              <tbody>
                {[...filteredMessages].reverse().map((message, index) => (
                  <tr key={`${message.received_at_ms}-${message.channel}-${index}`}>
                    <td className="observability-mono">{formatPubSubTime(message.received_at_ms)}</td>
                    <td className="observability-mono">{message.channel}</td>
                    <td>{message.pattern || "直接订阅"}</td>
                    <td>
                      <code className="observability-message">{message.message}</code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

interface ProfilerPanelProps {
  activeSession: ProfilerSession | null;
  busy: boolean;
  events: ProfilerEvent[];
  status: string;
  onClearEvents: () => void;
  onStart: () => void;
  onStop: () => void;
}

function ProfilerPanel({
  activeSession,
  busy,
  events,
  status,
  onClearEvents,
  onStart,
  onStop,
}: ProfilerPanelProps) {
  const running = activeSession !== null && status === "running";
  const [query, setQuery] = useState("");
  const feed = usePausedFeed(events, onClearEvents);
  const filteredEvents = filterProfilerEvents(feed.displayed, query);

  return (
    <div className="observability-content">
      <section className="observability-panel" aria-labelledby="profiler-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">MONITOR</p>
            <h3 id="profiler-title">实时命令监控</h3>
          </div>
          <span className={`observability-status observability-status-${status}`} role="status">
            <span className="observability-status-dot" aria-hidden="true" />
            {running ? "监控中" : status === "stopped" ? "已停止" : "未启动"}
          </span>
        </div>
        <p className="observability-warning" role="note">
          MONITOR 会接收当前实例的全部命令，可能影响 Redis 性能；生产环境请谨慎使用。
        </p>
        <div className="profiler-controls">
          <div>
            <p className="panel-hint observability-panel-hint">
              Profiler 只在当前页面保留实时事件，不会自动写入日志文件或持久化历史。命令参数可能包含敏感数据；仅在点击导出后保存本地文件。
            </p>
            {activeSession ? (
              <p className="observability-session-note">会话 {activeSession.session_id}</p>
            ) : null}
          </div>
          {running ? (
            <button type="button" className="button button-danger" onClick={onStop} disabled={busy}>
              {busy ? "停止中…" : "停止监控"}
            </button>
          ) : (
            <button type="button" className="button button-primary" onClick={onStart} disabled={busy}>
              {busy ? "启动中…" : "开始监控"}
            </button>
          )}
        </div>
      </section>

      <section className="observability-panel" aria-labelledby="profiler-event-list-title">
        <div className="observability-panel-heading">
          <div>
            <p className="eyebrow">LIVE COMMANDS</p>
            <h3 id="profiler-event-list-title">命令流</h3>
          </div>
          <div className="observability-toolbar">
            <span className="observability-config-summary">{events.length} / 10000 条 · 显示 {filteredEvents.length} 条</span>
            <button type="button" className="button button-quiet button-compact" onClick={feed.toggle}>{feed.paused ? "恢复显示" : "暂停显示"}</button>
            <ExportButton label="导出 Profiler LOG" disabled={filteredEvents.length === 0} output={() => buildProfilerExport(feed.displayed, query)} />
            <button
              type="button"
              className="button button-quiet button-compact"
              onClick={feed.clear}
              disabled={events.length === 0 && feed.displayed.length === 0}
            >
              清空视图
            </button>
          </div>
        </div>
        <label className="field"><span>筛选命令</span><input autoCapitalize="off" autoCorrect="off" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="命令、来源或 DB 编号" /></label>
        <p className="panel-hint">{feed.paused ? "已暂停显示；后台继续接收，MONITOR 仍在运行。恢复后显示最新缓存。" : "缓存最多保留最新 10000 条命令。"} 导出仅包含当前筛选的可见快照。</p>
        {filteredEvents.length === 0 ? (
          <p className="empty-state-compact">开始监控后，当前实例收到的命令会显示在这里。</p>
        ) : (
          <div className="observability-table-wrap profiler-event-table-wrap">
            <table className="observability-table profiler-event-table">
              <thead>
                <tr>
                  <th scope="col">时间</th>
                  <th scope="col">数据库</th>
                  <th scope="col">来源</th>
                  <th scope="col">命令</th>
                </tr>
              </thead>
              <tbody>
                {[...filteredEvents].reverse().map((event, index) => (
                  <tr key={`${event.received_at_ms}-${event.session_id}-${index}`}>
                    <td className="observability-mono">{formatProfilerTime(event.time)}</td>
                    <td className="observability-mono">DB{event.database}</td>
                    <td className="observability-mono">{event.source}</td>
                    <td>
                      <code className="observability-command">
                        {formatProfilerCommand(event.args)}
                      </code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

function createSessionId(prefix = "pubsub"): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function ExportButton({ label, disabled, output }: { label: string; disabled: boolean; output: () => ObservabilityExport }) {
  const [error, setError] = useState(false);
  return <>
    <button type="button" className="button button-quiet button-compact" disabled={disabled} onClick={() => {
      setError(false);
      try { downloadObservabilityExport(output()); } catch { setError(true); }
    }}>{label}</button>
    {error ? <span role="alert">无法下载文件，请重试。</span> : null}
  </>;
}

export default ObservabilityPage;
