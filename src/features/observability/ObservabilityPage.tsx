import Select from "../../components/Select";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState, type ReactNode } from "react";
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
import "./observabilityStreams.css";

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
  const [publishFeedback, setPublishFeedback, publishFeedbackToken] = useTransientFeedback();
  const [profilerSession, setProfilerSession] = useState<ProfilerSession | null>(null);
  const [profilerStatus, setProfilerStatus] = useState("idle");
  const [profilerEvents, setProfilerEvents] = useState<ProfilerEvent[]>([]);
  const [profilerBusy, setProfilerBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState<string | null>(null);
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
      aria-label="运维观察"
      aria-busy={slowLogLoading || slowLogBusy || pubSubBusy || profilerBusy}
    >
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

      <div className="observability-page-content">
        {isCluster && <p className="observability-warning">Cluster Slow Log 节点作用域尚未支持；Pub/Sub 和 Profiler 同样不可用。可在命令工作台显式执行原生命令，其路由范围由驱动决定。</p>}

        {error ? <Toast kind="error" message={error} resetKey={errorToken} onClose={() => setError(null)} /> : null}

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
            publishFeedbackToken={publishFeedbackToken}
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
      </div>
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
  publishFeedbackToken: unknown;
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
  publishFeedbackToken,
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
    <div className="observability-content observability-streams pubsub-workspace">
      <div className="pubsub-setup-grid">
        <section className="observability-panel stream-setup-panel" aria-labelledby="pubsub-title">
          <div className="stream-panel-heading">
            <div className="stream-heading-copy">
              <h3 id="pubsub-title">消息订阅</h3>
              <p className="stream-subscription-summary" title={activeSession ? `会话 ${activeSession.session_id} · ${activeSession.topics.map((topic) => topic.name).join("、")}` : undefined}>
                {activeSession ? `已订阅 ${activeSession.topics.length} 个${pattern ? "模式" : "频道"} · ${activeSession.topics.map((topic) => topic.name).join("、")}` : "订阅频道，实时接收消息。"}
              </p>
            </div>
            <span className={`observability-status observability-status-${status}`} role="status">
              <span className="observability-status-dot" aria-hidden="true" />
              {activeSession ? "订阅中" : status === "stopped" ? "已停止" : "未启动"}
            </span>
          </div>
          <div className="stream-subscribe-form">
            <label className="field">
              <span>频道或模式 <span className="stream-field-hint">每行一个</span></span>
              <textarea
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                aria-label="频道或模式"
                value={topicText}
                onChange={(event) => onTopicTextChange(event.target.value)}
                disabled={busy || activeSession !== null}
                rows={2}
                placeholder={pattern ? "user:*\norder:*" : "events\nnotifications"}
              />
            </label>
            <div className="stream-form-footer">
              <label className="checkbox-field">
                <input
                  type="checkbox"
                  aria-label="按模式订阅"
                  checked={pattern}
                  onChange={(event) => onPatternChange(event.target.checked)}
                  disabled={busy || activeSession !== null}
                />
                <span>按模式订阅</span>
                <code className="stream-field-hint">user:*</code>
              </label>
              {activeSession ? (
                <button type="button" className="button button-danger" onClick={onStop} disabled={busy}>
                  <StreamIcon name="stop" />{busy ? "停止中…" : "停止订阅"}
                </button>
              ) : (
                <button type="button" className="button button-primary" onClick={onStart} disabled={busy}>
                  <StreamIcon name="play" />{busy ? "启动中…" : "开始订阅"}
                </button>
              )}
            </div>
          </div>
        </section>

        <section className="observability-panel stream-setup-panel" aria-labelledby="pubsub-publish-title">
          <div className="stream-panel-heading">
            <div className="stream-heading-copy">
              <h3 id="pubsub-publish-title">发送消息</h3>
              <p>向指定频道发布一条消息。</p>
            </div>
            <span className="stream-command-tag">PUBLISH</span>
          </div>
          <div className="stream-publish-form">
            <label className="field">
              <span>频道</span>
              <input
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                value={publishChannel}
                onChange={(event) => onPublishChannelChange(event.target.value)}
                disabled={busy}
                placeholder="例如 events"
              />
            </label>
            <div className="stream-publish-message-row">
              <label className="field">
                <span>消息</span>
                <input
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  value={publishMessage}
                  onChange={(event) => onPublishMessageChange(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                      event.preventDefault();
                      onPublish();
                    }
                  }}
                  disabled={busy}
                  placeholder="输入消息，按 Enter 发送"
                />
              </label>
              <button type="button" className="button button-secondary" onClick={onPublish} disabled={busy}>
                <StreamIcon name="send" />发送
              </button>
            </div>
          </div>
          {publishFeedback ? <Toast kind="success" message={publishFeedback} resetKey={publishFeedbackToken} /> : null}
        </section>
      </div>

      <section className="observability-panel stream-feed-panel" aria-labelledby="pubsub-message-list-title">
        <div className="stream-feed-heading">
          <div className="stream-feed-title">
            <h3 id="pubsub-message-list-title">消息流</h3>
            <span className="stream-count">{filteredMessages.length}</span>
          </div>
          <span className="stream-feed-order">最新在前</span>
        </div>
        <StreamToolbar
          query={query}
          onQueryChange={setQuery}
          searchLabel="筛选消息"
          placeholder="频道、模式或消息内容"
          paused={feed.paused}
          onToggle={feed.toggle}
          onClear={feed.clear}
          clearDisabled={messages.length === 0 && feed.displayed.length === 0}
          exportLabel="导出消息 JSON"
          exportDisabled={filteredMessages.length === 0}
          output={() => buildPubSubExport(feed.displayed, query)}
        />
        {feed.paused && <p className="stream-feed-notice" role="status"><StreamIcon name="pause" />已暂停显示；后台继续接收，恢复后显示最新缓存。</p>}
        {filteredMessages.length === 0 ? (
          <StreamEmptyState
            icon={query.trim() ? "search" : "messages"}
            title={query.trim() ? "没有匹配的消息" : activeSession ? "等待频道消息" : "尚未接收消息"}
            description={query.trim() ? "试试其他关键词，或清空筛选条件。" : activeSession ? "收到消息后将自动显示在这里，也可以在上方发送一条消息。" : "在上方填写频道并开始订阅，消息将实时显示在这里。"}
          />
        ) : (
          <div className="observability-table-wrap stream-table-wrap">
            <table className="observability-table stream-table pubsub-message-table" aria-label="频道消息">
              <colgroup><col className="stream-time-column" /><col className="stream-channel-column" /><col className="stream-pattern-column" /><col /></colgroup>
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
                    <td><span className="stream-pattern-tag">{message.pattern || "直接订阅"}</span></td>
                    <td><code className="observability-message">{message.message}</code></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="stream-feed-footer">
          <span>缓存 {messages.length.toLocaleString()} / 5,000 条 · 显示 {filteredMessages.length.toLocaleString()} 条</span>
          <span>仅导出筛选后的可见消息；可能含敏感数据，不会自动保存。</span>
        </div>
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
    <div className="observability-content observability-streams profiler-workspace">
      <section className="observability-panel stream-monitor-panel" aria-labelledby="profiler-title">
        <div className="stream-monitor-heading">
          <span className="stream-heading-icon"><StreamIcon name="activity" /></span>
          <div className="stream-heading-copy">
            <h3 id="profiler-title">实时命令监控</h3>
            <p>实时查看当前实例的命令、来源与数据库。</p>
          </div>
          <div className="stream-monitor-actions">
            <span className={`observability-status observability-status-${status}`} role="status">
              <span className="observability-status-dot" aria-hidden="true" />
              {running ? "监控中" : status === "stopped" ? "已停止" : "未启动"}
            </span>
            {running ? (
              <button type="button" className="button button-danger" onClick={onStop} disabled={busy}>
                <StreamIcon name="stop" />{busy ? "停止中…" : "停止监控"}
              </button>
            ) : (
              <button type="button" className="button button-primary" onClick={onStart} disabled={busy}>
                <StreamIcon name="play" />{busy ? "启动中…" : "开始监控"}
              </button>
            )}
          </div>
        </div>
        <div className="stream-monitor-notice" role="note">
          <StreamIcon name="info" />
          <p>MONITOR 会接收当前实例的全部命令，可能影响 Redis 性能；生产环境请谨慎使用。</p>
        </div>
        {activeSession ? <p className="stream-session-summary" title={activeSession.session_id}>当前会话 · {activeSession.session_id}</p> : null}
      </section>

      <section className="observability-panel stream-feed-panel" aria-labelledby="profiler-event-list-title">
        <div className="stream-feed-heading">
          <div className="stream-feed-title">
            <h3 id="profiler-event-list-title">命令流</h3>
            <span className="stream-count">{filteredEvents.length}</span>
          </div>
          <span className="stream-feed-order">最新在前</span>
        </div>
        <StreamToolbar
          query={query}
          onQueryChange={setQuery}
          searchLabel="筛选命令"
          placeholder="命令、来源或 DB 编号"
          paused={feed.paused}
          onToggle={feed.toggle}
          onClear={feed.clear}
          clearDisabled={events.length === 0 && feed.displayed.length === 0}
          exportLabel="导出 Profiler LOG"
          exportDisabled={filteredEvents.length === 0}
          output={() => buildProfilerExport(feed.displayed, query)}
        />
        {feed.paused && <p className="stream-feed-notice" role="status"><StreamIcon name="pause" />已暂停显示；后台继续接收，MONITOR 仍在运行。恢复后显示最新缓存。</p>}
        {filteredEvents.length === 0 ? (
          <StreamEmptyState
            icon={query.trim() ? "search" : "activity"}
            title={query.trim() ? "没有匹配的命令" : running ? "等待命令执行" : "尚未采集命令"}
            description={query.trim() ? "试试其他关键词，或清空筛选条件。" : running ? "当前实例收到命令后，执行记录将自动显示在这里。" : "点击「开始监控」，查看当前实例的实时命令。"}
          />
        ) : (
          <div className="observability-table-wrap stream-table-wrap">
            <table className="observability-table stream-table profiler-event-table" aria-label="实时命令">
              <colgroup><col className="stream-time-column" /><col className="stream-database-column" /><col className="stream-source-column" /><col /></colgroup>
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
                    <td><span className="stream-database-tag">DB{event.database}</span></td>
                    <td className="observability-mono">{event.source}</td>
                    <td><code className="observability-command">{formatProfilerCommand(event.args)}</code></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <div className="stream-feed-footer">
          <span>缓存 {events.length.toLocaleString()} / 10,000 条 · 显示 {filteredEvents.length.toLocaleString()} 条</span>
          <span>仅导出筛选后的可见命令；参数可能含敏感数据，不会自动保存。</span>
        </div>
      </section>
    </div>
  );
}

type StreamIconName = "play" | "stop" | "pause" | "send" | "search" | "download" | "clear" | "messages" | "activity" | "info";

function StreamIcon({ name }: { name: StreamIconName }) {
  const paths: Record<StreamIconName, ReactNode> = {
    play: <path d="m8 5 11 7-11 7Z" />,
    stop: <rect x="6" y="6" width="12" height="12" rx="1" />,
    pause: <><path d="M8 5v14M16 5v14" /></>,
    send: <><path d="m21 3-7 18-4-7-7-4Z" /><path d="m10 14 5-5" /></>,
    search: <><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4.5 4.5" /></>,
    download: <><path d="M12 3v12m-4-4 4 4 4-4M5 16v4h14v-4" /></>,
    clear: <><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5M14 11v5" /></>,
    messages: <><path d="M20 11a7 7 0 0 1-7 7H8l-5 3V8a5 5 0 0 1 5-5h7a5 5 0 0 1 5 5Z" /><path d="M7 8h9M7 12h6" /></>,
    activity: <path d="M3 12h4l3-8 4 16 3-8h4" />,
    info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7v.01" /></>,
  };
  return <svg className="stream-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">{paths[name]}</svg>;
}

interface StreamToolbarProps {
  query: string;
  onQueryChange: (value: string) => void;
  searchLabel: string;
  placeholder: string;
  paused: boolean;
  onToggle: () => void;
  onClear: () => void;
  clearDisabled: boolean;
  exportLabel: string;
  exportDisabled: boolean;
  output: () => ObservabilityExport;
}

function StreamToolbar({ query, onQueryChange, searchLabel, placeholder, paused, onToggle, onClear, clearDisabled, exportLabel, exportDisabled, output }: StreamToolbarProps) {
  return (
    <div className="stream-feed-toolbar">
      <label className="stream-search">
        <span className="stream-search-label">{searchLabel}</span>
        <span className="stream-search-input">
          <StreamIcon name="search" />
          <input autoCapitalize="off" autoCorrect="off" spellCheck={false} value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder={placeholder} />
        </span>
      </label>
      <div className="stream-feed-actions">
        <button type="button" className="button button-secondary button-compact" onClick={onToggle} aria-pressed={paused}>
          <StreamIcon name={paused ? "play" : "pause"} />{paused ? "恢复显示" : "暂停显示"}
        </button>
        <ExportButton label={exportLabel} disabled={exportDisabled} output={output} icon />
        <button type="button" className="button button-quiet button-compact" onClick={onClear} disabled={clearDisabled}>
          <StreamIcon name="clear" />清空视图
        </button>
      </div>
    </div>
  );
}

function StreamEmptyState({ icon, title, description }: { icon: StreamIconName; title: string; description: string }) {
  return (
    <div className="stream-empty-state">
      <span className="stream-empty-icon"><StreamIcon name={icon} /></span>
      <h4>{title}</h4>
      <p>{description}</p>
    </div>
  );
}

function createSessionId(prefix = "pubsub"): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function ExportButton({ label, disabled, output, icon = false }: { label: string; disabled: boolean; output: () => ObservabilityExport; icon?: boolean }) {
  const [error, setError, errorToken] = useFeedbackState(false);
  return <>
    <button type="button" className="button button-quiet button-compact" disabled={disabled} onClick={() => {
      setError(false);
      try { downloadObservabilityExport(output()); } catch { setError(true); }
    }}>{icon && <StreamIcon name="download" />}{label}</button>
    {error ? <Toast kind="error" message="无法下载文件，请重试。" resetKey={errorToken} onClose={() => setError(false)} /> : null}
  </>;
}

export default ObservabilityPage;
