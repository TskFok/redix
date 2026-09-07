import { useEffect, useRef, useState } from "react";
import AnalysisHistory from "./AnalysisHistory";

import { analyzeDatabase } from "../../lib/tauri";
import type {
  AnalysisKey,
  DatabaseAnalysisReport,
  ExpirationGroup,
  NamespaceSummary,
  TypeSummary,
} from "../../lib/types";
import {
  DEFAULT_ANALYSIS_INPUT,
  formatAnalysisNumber,
  formatCoverage,
  initialDatabaseAnalysisState,
  toUserFacingAnalysisError,
  type DatabaseAnalysisPageState,
} from "./databaseAnalysisState";

interface DatabaseAnalysisPageProps {
  connectionId: string;
  activeDatabase: number;
}

function validateInput(input: DatabaseAnalysisPageState["input"]): string | null {
  if (!input.connection_id) {
    return "当前连接不可用，请重新打开连接。";
  }
  if (input.pattern.length === 0 || input.pattern.length > 512) {
    return "匹配模式长度必须在 1 到 512 个字符之间。";
  }
  if (input.delimiter.length < 1 || input.delimiter.length > 8) {
    return "命名空间分隔符长度必须在 1 到 8 个字符之间。";
  }
  if (
    !Number.isInteger(input.max_keys) ||
    input.max_keys < 1000 ||
    input.max_keys > 1000000
  ) {
    return "扫描键数必须在 1000 到 1000000 之间。";
  }

  return null;
}

function formatBytes(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "不可用";
  }
  if (value < 1024) {
    return `${formatAnalysisNumber(value)} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  if (value < 1024 * 1024 * 1024) {
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function typesText(types: TypeSummary[]): string {
  return types.length > 0
    ? types.map((summary) => `${summary.type}: ${formatAnalysisNumber(summary.total)}`).join("，")
    : "无";
}

function SummaryCard({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <article className="database-metric-card">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}

function AnalysisTable({
  title,
  headers,
  children,
}: {
  title: string;
  headers: string[];
  children: React.ReactNode;
}) {
  return (
    <section className="database-panel" aria-label={title}>
      <div className="database-panel-heading">
        <h3>{title}</h3>
      </div>
      <div className="database-table-wrap">
        <table className="database-table">
          <thead>
            <tr>
              {headers.map((header) => (
                <th key={header} scope="col">
                  {header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>{children}</tbody>
        </table>
      </div>
    </section>
  );
}

function KeyRows({ keys }: { keys: AnalysisKey[] }) {
  if (keys.length === 0) {
    return (
      <tr>
        <td colSpan={5}>暂无数据</td>
      </tr>
    );
  }

  return keys.map((item) => (
    <tr key={`${item.key}-${item.key_type}`}>
      <th scope="row"><code>{item.key}</code></th>
      <td>{item.key_type}</td>
      <td>{formatAnalysisNumber(item.length)}</td>
      <td>{formatBytes(item.memory_bytes)}</td>
      <td>{item.ttl_seconds === -1 ? "永不过期" : formatAnalysisNumber(item.ttl_seconds)}</td>
    </tr>
  ));
}

function NamespaceRows({ namespaces }: { namespaces: NamespaceSummary[] }) {
  if (namespaces.length === 0) {
    return (
      <tr>
        <td colSpan={4}>暂无数据</td>
      </tr>
    );
  }

  return namespaces.map((item) => (
    <tr key={item.namespace}>
      <th scope="row">{item.namespace}</th>
      <td>{formatAnalysisNumber(item.keys)}</td>
      <td>{formatBytes(item.memory_bytes)}</td>
      <td>{typesText(item.types)}</td>
    </tr>
  ));
}

function ExpirationGroups({ groups, totalKeys }: { groups: ExpirationGroup[]; totalKeys: number }) {
  return (
    <section className="database-panel" aria-labelledby="expiration-groups-title">
      <div className="database-panel-heading">
        <h3 id="expiration-groups-title">过期时间分布</h3>
      </div>
      {groups.length > 0 ? (
        <ul className="database-module-list">
          {groups.map((group) => {
            const width = totalKeys > 0 ? Math.min((group.keys / totalKeys) * 100, 100) : 0;
            return (
              <li key={group.label}>
                <span>{group.label}</span>
                <span>{formatAnalysisNumber(group.keys)} 键 / {formatBytes(group.memory_bytes)}</span>
                <span
                  aria-label={`${group.label} 占比 ${formatCoverage(group.keys, totalKeys)}`}
                  style={{
                    display: "block",
                    width: `${width}%`,
                    minWidth: width > 0 ? "4px" : undefined,
                    height: "6px",
                    borderRadius: "999px",
                    background: "var(--color-primary)",
                  }}
                />
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="empty-state-compact">暂无数据</p>
      )}
    </section>
  );
}

function AnalysisResults({ report }: { report: DatabaseAnalysisReport }) {
  return (
    <>
      {(report.failed_nodes?.length ?? 0) > 0 && <p role="alert" className="inline-error">{report.failed_nodes.length} 个主节点分析失败，当前为部分结果。重新分析可重试。</p>}
      {(report.node_results?.length ?? 0) > 0 && <section className="database-panel" aria-label="节点分析范围"><p>仅汇总成功主节点的分析结果；可展开查看各节点范围。</p>{report.node_results.map((node) => <details key={node.node_id}><summary>{node.node_id} · {node.endpoint.host.includes(":") ? `[${node.endpoint.host}]` : node.endpoint.host}:{node.endpoint.port} · 已处理 {node.report.progress.processed} 个键{node.report.progress.truncated ? " · 已截断" : ""}</summary><AnalysisResults report={{ ...node.report, node_results: [], failed_nodes: [] }} /></details>)}</section>}
      {report.progress.truncated ? (
        <p className="inline-error" role="status">
          结果已达到扫描上限，可能不完整
        </p>
      ) : null}
      <section className="database-metric-grid" aria-label="分析摘要">
        <SummaryCard
          label="总键数"
          value={formatAnalysisNumber(report.total_keys.total)}
          detail={`已观察 ${formatAnalysisNumber(report.total_keys.observed)}`}
        />
        <SummaryCard
          label="总内存"
          value={formatBytes(report.total_memory.total)}
          detail={`已获取 ${formatAnalysisNumber(report.total_memory.observed)} 个键的内存`}
        />
        <SummaryCard
          label="内存覆盖率"
          value={formatCoverage(report.total_memory.observed, report.total_keys.observed)}
          detail="成功读取 MEMORY USAGE 的键占比"
        />
        <SummaryCard
          label="已处理键数"
          value={formatAnalysisNumber(report.progress.processed)}
          detail={`已扫描 ${formatAnalysisNumber(report.progress.scanned)}`}
        />
      </section>
      <AnalysisTable title="类型统计" headers={["类型", "键数", "内存"]}>
        {report.total_keys.types.length > 0 ? report.total_keys.types.map((summary) => (
          <tr key={summary.type}>
            <th scope="row">{summary.type}</th>
            <td>{formatAnalysisNumber(summary.total)}</td>
            <td>{formatBytes(report.total_memory.types.find((item) => item.type === summary.type)?.total)}</td>
          </tr>
        )) : (
          <tr><td colSpan={3}>暂无数据</td></tr>
        )}
      </AnalysisTable>
      <AnalysisTable title="命名空间（按键数）" headers={["命名空间", "键数", "内存", "类型"]}>
        <NamespaceRows namespaces={report.top_namespaces_by_keys} />
      </AnalysisTable>
      <AnalysisTable title="命名空间（按内存）" headers={["命名空间", "键数", "内存", "类型"]}>
        <NamespaceRows namespaces={report.top_namespaces_by_memory} />
      </AnalysisTable>
      <AnalysisTable title="Top Keys（按内存）" headers={["键名", "类型", "长度", "内存", "TTL"]}>
        <KeyRows keys={report.top_keys_by_memory} />
      </AnalysisTable>
      <AnalysisTable title="Top Keys（按长度）" headers={["键名", "类型", "长度", "内存", "TTL"]}>
        <KeyRows keys={report.top_keys_by_length} />
      </AnalysisTable>
      <ExpirationGroups groups={report.expiration_groups} totalKeys={report.total_keys.total} />
    </>
  );
}

export function DatabaseAnalysisPage({ connectionId, activeDatabase }: DatabaseAnalysisPageProps) {
  const [state, setState] = useState<DatabaseAnalysisPageState>(() => ({
    ...initialDatabaseAnalysisState,
    input: { ...DEFAULT_ANALYSIS_INPUT, connection_id: connectionId },
  }));
  const requestRef = useRef(0);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRef.current += 1;
    };
  }, []);

  useEffect(() => {
    requestRef.current += 1;
    setState({
      ...initialDatabaseAnalysisState,
      input: { ...DEFAULT_ANALYSIS_INPUT, connection_id: connectionId },
    });
  }, [connectionId, activeDatabase]);

  const handleSubmit = () => {
    if (state.loading) {
      return;
    }

    const error = validateInput(state.input);
    if (error) {
      setState((current) => ({ ...current, error }));
      return;
    }

    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    const input = { ...state.input, connection_id: connectionId };
    setState((current) => ({ ...current, input, loading: true, report: null, error: null }));

    void analyzeDatabase(input)
      .then((report) => {
        if (!mountedRef.current || requestRef.current !== requestId) {
          return;
        }
        setState((current) => ({ ...current, loading: false, report, error: null }));
      })
      .catch((caught: unknown) => {
        if (!mountedRef.current || requestRef.current !== requestId) {
          return;
        }
        setState((current) => ({
          ...current,
          loading: false,
          error: toUserFacingAnalysisError(caught),
        }));
      });
  };

  return (
    <section className="database-page" aria-labelledby="database-analysis-page-title" aria-busy={state.loading}>
      <div className="page-heading database-page-heading">
        <div>
          <p className="eyebrow">DATABASE ANALYSIS</p>
          <h2 id="database-analysis-page-title">数据库 {activeDatabase}</h2>
          <p className="page-description">按需扫描当前数据库并汇总键空间、内存与过期时间分布。</p>
        </div>
      </div>
      <section className="database-panel" aria-label="分析参数">
        <div className="database-panel-heading">
          <div>
            <h3>分析参数</h3>
            <p className="database-current">连接：{connectionId} · 数据库 {activeDatabase}</p>
          </div>
          <button
            type="button"
            className="button button-primary"
            disabled={state.loading}
            onClick={handleSubmit}
          >
            开始分析
          </button>
        </div>
        <div className="form-grid">
          <label className="field">
            <span>匹配模式</span>
            <input
              value={state.input.pattern}
              onChange={(event) =>
                setState((current) => ({
                  ...current,
                  input: { ...current.input, pattern: event.target.value },
                  error: null,
                }))
              }
            />
          </label>
          <label className="field">
            <span>命名空间分隔符</span>
            <input
              value={state.input.delimiter}
              onChange={(event) =>
                setState((current) => ({
                  ...current,
                  input: { ...current.input, delimiter: event.target.value },
                  error: null,
                }))
              }
            />
          </label>
          <label className="field">
            <span>最大扫描键数</span>
            <input
              aria-label="最大扫描键数"
              type="number"
              min={1000}
              max={1000000}
              step={1}
              value={state.input.max_keys}
              onChange={(event) =>
                setState((current) => ({
                  ...current,
                  input: { ...current.input, max_keys: Number(event.target.value) },
                  error: null,
                }))
              }
            />
          </label>
        </div>
      </section>
      {state.error ? <p className="inline-error" role="alert">{state.error}</p> : null}
      {state.loading ? <p className="empty-state-compact" role="status">正在分析数据库…</p> : null}
      {state.report ? <AnalysisResults report={state.report} /> : null}
      <AnalysisHistory key={JSON.stringify([connectionId, activeDatabase])} connectionId={connectionId} database={activeDatabase} report={state.report} renderReport={(report) => <AnalysisResults report={report} />} />
    </section>
  );
}

export default DatabaseAnalysisPage;
