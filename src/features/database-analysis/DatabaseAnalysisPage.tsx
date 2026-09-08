import { useEffect, useState } from "react";
import AnalysisHistory from "./AnalysisHistory";
import AnalysisRecommendations from "./AnalysisRecommendations";

import { useAnalysisTask } from "./useAnalysisTask";
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
      <AnalysisRecommendations report={report} />
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
  const job = useAnalysisTask(connectionId, activeDatabase);
  const [timeoutSeconds, setTimeoutSeconds] = useState(300);
  useEffect(() => {
    setState({ ...initialDatabaseAnalysisState, input: { ...DEFAULT_ANALYSIS_INPUT, connection_id: connectionId } });
    setTimeoutSeconds(300);
  }, [connectionId, activeDatabase]);
  useEffect(() => {
    if (job.task) setState((current) => ({ ...current, input: job.task!.analysis }));
  }, [job.task?.id]);

  const handleSubmit = () => {
    if (job.loading) return;
    const error = validateInput(state.input) ?? (!Number.isInteger(timeoutSeconds) || timeoutSeconds < 1 || timeoutSeconds > 900 ? "超时必须在 1 到 900 秒之间。" : null);
    if (error) { setState((current) => ({ ...current, error })); return; }
    setState((current) => ({ ...current, error: null }));
    void job.start(state.input, timeoutSeconds);
  };

  return (
    <section className="database-page" aria-labelledby="database-analysis-page-title" aria-busy={job.loading}>
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
            disabled={job.loading}
            onClick={handleSubmit}
          >
            开始分析
          </button>
        </div>
        <fieldset className="editor-fieldset" disabled={job.loading}>
        <legend>扫描选项</legend>
        <div className="form-grid">
          <label className="field">
            <span>匹配模式</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
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
              autoCapitalize="off"
              autoCorrect="off"
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
              autoCapitalize="off"
              autoCorrect="off"
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
          <label className="field"><span>任务超时（秒）</span><input autoCapitalize="off" autoCorrect="off" aria-label="任务超时（秒）" type="number" min={1} max={900} step={1} value={timeoutSeconds} onChange={(event) => setTimeoutSeconds(Number(event.target.value))} /></label>
        </div>
        </fieldset>
      </section>
      <section className="database-panel" aria-label="后台分析任务">
        <p>任务和结果仅保留在本次应用内存，最多 16 条、同时运行 2 项；切页后可返回恢复，重启应用后清除。只有点击“保存当前分析”才写入本机历史。</p>
        <p className="browser-helper">已扫描是 SCAN 返回的键次数，可能包含重复或已过期键；已处理是成功读取且仍存在的键次数。扫描上限不是全库总数，不表示完成百分比。沿用现有采样：单实例最后一页可能超过上限，Cluster 按主节点分配上限。</p>
        {job.pending ? <p role="status">正在恢复或启动后台分析…</p> : null}
        {job.task ? <>
          <p role="status">已扫描 {formatAnalysisNumber(job.task.progress.scanned)} · 已处理 {formatAnalysisNumber(job.task.progress.processed)} · 扫描上限 {formatAnalysisNumber(job.task.progress.max_keys)} · 已结束节点 {job.task.nodes_completed}/{job.task.nodes_total}</p>
          {job.task.status === "running" ? <><p>正在分析数据库…关闭连接或切换数据库会停止当前任务；取消后不会继续发送扫描请求，已经发出的只读请求可能仍在服务器执行。</p><button type="button" className="button button-secondary" disabled={job.task.cancel_requested} onClick={() => void job.cancel()}>{job.task.cancel_requested ? "正在取消…" : "取消分析"}</button></> : null}
          {job.task.status === "cancelled" ? <p role="status">分析已取消，未生成完成报告。</p> : null}
          {job.task.status === "timed_out" ? <p role="status">分析已超时，已停止后续扫描；可缩小范围或调整超时后重试。</p> : null}
          {job.task.status === "completed" ? <p role="status">后台分析已完成。</p> : null}
          {job.task.status === "partial_failure" ? <p role="status">后台分析部分完成，请结合失败节点查看报告。</p> : null}
        </> : null}
        <button type="button" className="button button-quiet" disabled={job.pending} onClick={() => void job.recover()}>重新读取任务</button>
      </section>
      {(state.error ?? job.error) ? <p className="inline-error" role="alert">{state.error ?? job.error}</p> : null}
      {job.report ? <AnalysisResults report={job.report} /> : null}
      <AnalysisHistory key={JSON.stringify([connectionId, activeDatabase])} connectionId={connectionId} database={activeDatabase} report={job.report} renderReport={(report) => <AnalysisResults report={report} />} />
    </section>
  );
}

export default DatabaseAnalysisPage;
