import { useEffect, useRef, useState } from "react";

import {
  getDatabaseOverview,
  getInstanceOverview,
  selectDatabase,
} from "../../lib/tauri";
import type { ConnectionProfile } from "../../lib/types";
import {
  databaseLoadFailedMessage,
  databaseSwitchFailedMessage,
  formatBytes,
  formatMetric,
  initialDatabasePageState,
  toUserFacingDatabaseError,
  type DatabasePageState,
} from "./databaseState";

interface DatabasePageProps {
  connectionId: string;
  activeDatabase: number;
  onProfileChanged: (profile: ConnectionProfile) => void;
}

export function DatabasePage({
  connectionId,
  activeDatabase,
  onProfileChanged,
}: DatabasePageProps) {
  const [state, setState] = useState<DatabasePageState>(() => ({
    ...initialDatabasePageState,
  }));
  const [reloadToken, setReloadToken] = useState(0);
  const requestRef = useRef(0);
  const mountedRef = useRef(false);

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
    setState((current) => ({
      ...initialDatabasePageState,
      requestId,
      loading: true,
      switching: current.switching,
    }));

    const overview = Promise.allSettled([
      getInstanceOverview(connectionId),
      getDatabaseOverview(connectionId),
    ]);

    void overview.then(([instanceResult, databaseResult]) => {
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }

      const instance = instanceResult.status === "fulfilled" ? instanceResult.value : null;
      const databases =
        databaseResult.status === "fulfilled" ? databaseResult.value : [];
      const failed = [instanceResult, databaseResult].find(
        (result) => result.status === "rejected",
      );
      setState((current) => ({
        ...current,
        loading: false,
        instance,
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
  }, [connectionId, reloadToken]);

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
      if (!mountedRef.current || requestRef.current !== requestId) {
        return;
      }
      setState((current) => ({ ...current, switching: false, switchError: null }));
      onProfileChanged(profile);
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
            查看当前 Standalone Redis 实例的只读指标和数据库键空间，不执行全库扫描。
          </p>
        </div>
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

      <div className="database-metric-grid" aria-label="实例指标">
        <MetricCard label="服务器版本" value={formatMetric(state.instance?.server_version)} />
        <MetricCard label="Redis 模式" value={formatMetric(state.instance?.redis_mode)} />
        <MetricCard
          label="已连接客户端"
          value={formatMetric(state.instance?.connected_clients)}
        />
        <MetricCard
          label="已用内存"
          value={formatBytes(state.instance?.used_memory_bytes)}
        />
        <MetricCard
          label="最大内存"
          value={formatBytes(state.instance?.max_memory_bytes)}
        />
        <MetricCard label="主从角色" value={formatMetric(state.instance?.role)} />
        <MetricCard
          label="命令处理数"
          value={formatMetric(state.instance?.total_commands_processed)}
        />
        <MetricCard
          label="命中 / 未命中"
          value={`${formatMetric(state.instance?.keyspace_hits)} / ${formatMetric(
            state.instance?.keyspace_misses,
          )}`}
        />
      </div>

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

      <section className="database-panel" aria-labelledby="module-list-title">
        <div className="database-panel-heading">
          <div>
            <p className="eyebrow">MODULES</p>
            <h3 id="module-list-title">已加载模块</h3>
          </div>
        </div>
        {state.instance?.modules.length ? (
          <ul className="database-module-list">
            {state.instance.modules.map((module) => (
              <li key={`${module.name}-${module.version ?? "unknown"}`}>
                <strong>{module.name}</strong>
                <span>{module.version ?? "版本不可用"}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="empty-state-compact">未检测到模块或模块信息不可用。</p>
        )}
      </section>
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

export default DatabasePage;
