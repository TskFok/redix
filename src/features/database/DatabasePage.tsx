import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";

import {
  getDatabaseOverview,
  getInstanceDetails,
  selectDatabase,
} from "../../lib/tauri";
import type { ConnectionProfile } from "../../lib/types";
import { useAutoRefresh } from "../browser/useAutoRefresh";
import InstanceTrends, { appendInstanceSample, type InstanceSample } from "./InstanceTrends";
import {
  databaseLoadFailedMessage,
  databaseSwitchFailedMessage,
  formatBoolean,
  formatBytes,
  formatMetric,
  formatPercentage,
  formatTimestamp,
  initialDatabasePageState,
  toUserFacingDatabaseError,
  type DatabasePageState,
} from "./databaseState";

interface DatabasePageProps {
  connectionId: string;
  activeDatabase: number;
  onProfileChanged: (profile: ConnectionProfile) => void;
  isCluster?: boolean;
  onOpenTopology?: () => void;
}

export function DatabasePage({
  connectionId,
  activeDatabase,
  onProfileChanged,
  isCluster = false,
  onOpenTopology,
}: DatabasePageProps) {
  const [state, setState] = useState<DatabasePageState>(() => ({
    ...initialDatabasePageState,
  }));
  const [reloadToken, setReloadToken] = useState(0);
  const [refreshSeconds, setRefreshSeconds] = useState(0);
  const [trend, setTrend] = useState<{ connectionId: string; samples: InstanceSample[] }>({ connectionId, samples: [] });
  const previousConnection = useRef(connectionId);
  const requestRef = useRef(0);
  const mountedRef = useRef(false);
  useAutoRefresh(refreshSeconds, state.loading || state.switching, () => setReloadToken((current) => current + 1));

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
    const sameConnection = previousConnection.current === connectionId;
    previousConnection.current = connectionId;
    if (!sameConnection) setTrend({ connectionId, samples: [] });
    setState((current) => ({
      ...(sameConnection ? current : initialDatabasePageState),
      requestId,
      loading: true,
      switching: sameConnection ? current.switching : false,
    }));

    const overview = Promise.allSettled([
      isCluster ? Promise.resolve(null) : getInstanceDetails(connectionId),
      getDatabaseOverview(connectionId),
    ]);

    void overview.then(([instanceResult, databaseResult]) => {
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }

      const details = instanceResult.status === "fulfilled" ? instanceResult.value : null;
      if (!isCluster) {
        setTrend((current) => ({ connectionId, samples: appendInstanceSample(current.connectionId === connectionId ? current.samples : [], {
          at: Date.now(), memory: details?.overview.used_memory_bytes ?? null,
          ops: details?.stats.instantaneous_ops_per_sec ?? null,
          clients: details?.overview.connected_clients ?? null,
        }) }));
      }
      const databases =
        databaseResult.status === "fulfilled" ? databaseResult.value : [];
      const failed = [instanceResult, databaseResult].find(
        (result) => result.status === "rejected",
      );
      setState((current) => ({
        ...current,
        loading: false,
        details,
        databases,
        error: failed
          ? toUserFacingDatabaseError(
              failed.status === "rejected" ? failed.reason : undefined,
              databaseLoadFailedMessage,
            )
          : null,
        requestId,
      }));
    });

    return () => {
      requestRef.current += 1;
    };
  }, [connectionId, reloadToken, isCluster]);

  const handleSelectDatabase = async (database: number) => {
    if (state.switching || database === activeDatabase) {
      return;
    }

    const requestId = requestRef.current;
    setState((current) => ({
      ...current,
      switching: true,
      switchError: null,
    }));
    try {
      const profile = await selectDatabase({
        connection_id: connectionId,
        database,
      });
      // The connection changed even if this page was left while awaiting it.
      onProfileChanged(profile);
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({ ...current, switching: false, switchError: null }));
    } catch (error) {
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({
        ...current,
        switching: false,
        switchError: databaseSwitchFailedMessage,
      }));
    }
  };

  return (
    <section
      className="database-page"
      aria-labelledby="database-page-title"
      aria-busy={state.loading || state.switching}
    >
      <div className="page-heading database-page-heading">
        <div>
          <p className="eyebrow">DATABASE OVERVIEW</p>
          <h2 id="database-page-title">数据库概览</h2>
          <p className="page-description">
            {isCluster ? "DB 0 键数和过期键由全部主节点聚合；节点或字段不可用时显示不可用。" : "查看当前 Redis 实例的只读指标和数据库键空间，不执行全库扫描。"}
          </p>
        </div>
        <label className="field"><span>概览自动刷新</span><Select aria-label="概览自动刷新" value={refreshSeconds} onChange={(event) => setRefreshSeconds(Number(event.target.value))}><option value={0}>关闭</option>{[2, 5, 10, 30].map((seconds) => <option key={seconds} value={seconds}>{seconds} 秒</option>)}</Select></label>
        <button
          type="button"
          className="button button-secondary"
          onClick={() => setReloadToken((current) => current + 1)}
          disabled={state.loading || state.switching}
        >
          {state.loading ? "刷新中…" : "刷新概览"}
        </button>
      </div>

      {state.error ? (
        <p className="inline-error" role="alert">
          {state.error}
        </p>
      ) : null}
      {state.switchError ? (
        <p className="inline-error" role="alert">
          {state.switchError}
        </p>
      ) : null}

      {isCluster && <section className="database-panel"><p>Cluster 仅支持 DB 0，逐节点实例详情请在拓扑查看。</p><button type="button" className="button button-secondary" onClick={onOpenTopology}>查看 Cluster 拓扑</button></section>}
      {!isCluster && <><div className="database-metric-grid" aria-label="实例指标">
        <MetricCard
          label="服务器版本"
          value={formatMetric(state.details?.overview.server_version)}
        />
        <MetricCard label="Redis 模式" value={formatMetric(state.details?.overview.redis_mode)} />
        <MetricCard
          label="已连接客户端"
          value={formatMetric(state.details?.overview.connected_clients)}
        />
        <MetricCard
          label="已用内存"
          value={formatBytes(state.details?.overview.used_memory_bytes)}
        />
        <MetricCard
          label="最大内存"
          value={formatBytes(state.details?.overview.max_memory_bytes)}
        />
        <MetricCard label="主从角色" value={formatMetric(state.details?.overview.role)} />
        <MetricCard
          label="命令处理数"
          value={formatMetric(state.details?.overview.total_commands_processed)}
        />
        <MetricCard
          label="命中 / 未命中"
          value={`${formatMetric(state.details?.overview.keyspace_hits)} / ${formatMetric(
            state.details?.overview.keyspace_misses,
          )}`}
        />
      </div>

      <InstanceTrends samples={trend.connectionId === connectionId ? trend.samples : []} />

      <InfoPanel
        id="client-details"
        title="客户端"
        items={[
          ["已连接客户端", formatMetric(state.details?.clients.connected_clients)],
          ["阻塞客户端", formatMetric(state.details?.clients.blocked_clients)],
          ["跟踪客户端", formatMetric(state.details?.clients.tracking_clients)],
          ["最大客户端数", formatMetric(state.details?.clients.max_clients)],
        ]}
      />

      <InfoPanel
        id="memory-details"
        title="内存"
        items={[
          ["已用内存", formatBytes(state.details?.memory.used_memory_bytes)],
          ["内存峰值", formatBytes(state.details?.memory.used_memory_peak_bytes)],
          ["RSS 内存", formatBytes(state.details?.memory.used_memory_rss_bytes)],
          ["内存碎片率", formatPercentage(state.details?.memory.mem_fragmentation_ratio)],
          ["分配器活跃内存", formatBytes(state.details?.memory.allocator_active_bytes)],
          ["分配器驻留内存", formatBytes(state.details?.memory.allocator_resident_bytes)],
        ]}
      />

      <InfoPanel
        id="stats-details"
        title="统计"
        items={[
          ["即时操作数 / 秒", formatMetric(state.details?.stats.instantaneous_ops_per_sec)],
          ["过期键数", formatMetric(state.details?.stats.expired_keys)],
          ["淘汰键数", formatMetric(state.details?.stats.evicted_keys)],
          ["命中率", formatPercentage(state.details?.stats.hit_rate)],
        ]}
      />

      <InfoPanel
        id="persistence-details"
        title="持久化"
        items={[
          ["正在加载", formatBoolean(state.details?.persistence.loading)],
          ["RDB 最近保存时间", formatTimestamp(state.details?.persistence.rdb_last_save_time)],
          [
            "自上次保存后的变更数",
            formatMetric(state.details?.persistence.rdb_changes_since_last_save),
          ],
          ["AOF 已启用", formatBoolean(state.details?.persistence.aof_enabled)],
          [
            "AOF 重写进行中",
            formatBoolean(state.details?.persistence.aof_rewrite_in_progress),
          ],
        ]}
      />

      <InfoPanel
        id="replication-details"
        title="复制"
        items={[
          ["角色", formatMetric(state.details?.replication.role)],
          ["已连接副本", formatMetric(state.details?.replication.connected_replicas)],
          ["主节点连接状态", formatMetric(state.details?.replication.master_link_status)],
          ["主节点复制偏移量", formatMetric(state.details?.replication.master_repl_offset)],
        ]}
      />

      <section className="database-panel" aria-labelledby="command-stats-title">
        <div className="database-panel-heading">
          <div>
            <p className="eyebrow">INFO</p>
            <h3 id="command-stats-title">命令统计</h3>
          </div>
        </div>
        {state.details?.command_stats.length ? (
          <div className="database-table-wrap">
            <table className="database-table">
              <thead>
                <tr>
                  <th scope="col">命令</th>
                  <th scope="col">调用次数</th>
                  <th scope="col">总耗时</th>
                  <th scope="col">平均耗时</th>
                  <th scope="col">拒绝次数</th>
                  <th scope="col">失败次数</th>
                </tr>
              </thead>
              <tbody>
                {state.details.command_stats.map((command) => (
                  <tr key={command.command}>
                    <th scope="row">{command.command}</th>
                    <td>{formatMetric(command.calls)}</td>
                    <td>{formatMetric(command.usec)}</td>
                    <td>{formatMetric(command.usec_per_call)}</td>
                    <td>{formatMetric(command.rejected_calls)}</td>
                    <td>{formatMetric(command.failed_calls)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="empty-state-compact">未检测到命令统计或命令统计不可用。</p>
        )}
      </section>

      </>}
      <section className="database-panel" aria-labelledby="database-list-title">
        <div className="database-panel-heading">
          <div>
            <p className="eyebrow">KEYSPACE</p>
            <h3 id="database-list-title">数据库列表</h3>
          </div>
          <span className="database-current">当前数据库：{activeDatabase}</span>
        </div>
        {state.databases.length === 0 ? (
          <p className="empty-state-compact">当前没有可用的数据库键空间统计。</p>
        ) : (
          <div className="database-table-wrap">
            <table className="database-table">
              <thead>
                <tr>
                  <th scope="col">数据库</th>
                  <th scope="col">键数</th>
                  <th scope="col">过期键</th>
                  <th scope="col">平均 TTL</th>
                  <th scope="col">操作</th>
                </tr>
              </thead>
              <tbody>
                {state.databases.map((database) => {
                  const isCurrent = database.database === activeDatabase;
                  return (
                    <tr key={database.database}>
                      <th scope="row">数据库 {database.database}</th>
                      <td>{formatMetric(database.key_count)}</td>
                      <td>{formatMetric(database.expires)}</td>
                      <td>
                        {database.avg_ttl_ms === null || database.avg_ttl_ms === undefined
                          ? "不可用"
                          : `${database.avg_ttl_ms} ms`}
                      </td>
                      <td>
                        {isCurrent ? (
                          <span className="database-current-badge">当前</span>
                        ) : (
                          <button
                            type="button"
                            className="button button-secondary button-compact"
                            onClick={() => void handleSelectDatabase(database.database)}
                            disabled={state.switching}
                          >
                            {state.switching
                              ? "切换中…"
                              : `切换到数据库 ${database.database}`}
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {!isCluster && <section className="database-panel" aria-labelledby="module-list-title">
        <div className="database-panel-heading">
          <div>
            <p className="eyebrow">MODULES</p>
            <h3 id="module-list-title">已加载模块</h3>
          </div>
        </div>
        {state.details?.overview.modules.length ? (
          <ul className="database-module-list">
            {state.details.overview.modules.map((module) => (
              <li key={`${module.name}-${module.version ?? "unknown"}`}>
                <strong>{module.name}</strong>
                <span>{module.version ?? "版本不可用"}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="empty-state-compact">未检测到模块或模块信息不可用。</p>
        )}
      </section>}
    </section>
  );
}

function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <article className="database-metric-card">
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function InfoPanel({
  id,
  title,
  items,
}: {
  id: string;
  title: string;
  items: Array<[string, string]>;
}) {
  return (
    <section className="database-panel" aria-labelledby={id}>
      <div className="database-panel-heading">
        <div>
          <p className="eyebrow">INFO</p>
          <h3 id={id}>{title}</h3>
        </div>
      </div>
      <ul className="database-module-list">
        {items.map(([label, value]) => (
          <li key={label}>
            <strong>{label}</strong>
            <span>{value}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

export default DatabasePage;
