import { useEffect, useRef, useState, type ReactNode } from "react";
import type { DatabaseAnalysisReport } from "../../lib/types";
import { deleteAnalysisHistory, getAnalysisHistory, listAnalysisHistory, saveAnalysisHistory,
  type AnalysisHistorySummary, type SavedAnalysis } from "./analysisHistoryApi";

interface Props {
  connectionId: string;
  database: number;
  report: DatabaseAnalysisReport | null;
  renderReport: (report: DatabaseAnalysisReport) => ReactNode;
}

function Comparison({ current, previous }: { current: DatabaseAnalysisReport; previous: DatabaseAnalysisReport }) {
  if (current.database !== previous.database || current.pattern !== previous.pattern || current.delimiter !== previous.delimiter || current.progress.max_keys !== previous.progress.max_keys) {
    return <p role="status">扫描参数不同，不能直接比较差值；请使用相同数据库、匹配模式、分隔符和扫描上限重新分析。</p>;
  }
  const metrics = [
    ["已观察键数", previous.total_keys.observed, current.total_keys.observed],
    ["采样内存（字节）", previous.total_memory.total, current.total_memory.total],
    ["获取内存的键数", previous.total_memory.observed, current.total_memory.observed],
  ] as const;
  return <div>
    <p>比较的是两次扫描观察值，扫描不是数据库快照；键过期和写入会影响结果。{current.progress.truncated || previous.progress.truncated ? "至少一次分析达到扫描上限，差值不能代表全库变化。" : ""}</p>
    <table className="database-table" aria-label="分析结果比较">
      <thead><tr><th>指标</th><th>历史</th><th>当前</th><th>变化</th></tr></thead>
      <tbody>{metrics.map(([label, before, after]) => <tr key={label}><th>{label}</th><td>{before.toLocaleString("zh-CN")}</td><td>{after.toLocaleString("zh-CN")}</td><td>{after - before > 0 ? "+" : ""}{(after - before).toLocaleString("zh-CN")}</td></tr>)}</tbody>
    </table>
  </div>;
}

export default function AnalysisHistory({ connectionId, database, report, renderReport }: Props) {
  const [items, setItems] = useState<AnalysisHistorySummary[]>([]);
  const [selected, setSelected] = useState<SavedAnalysis | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const generation = useRef(0);
  const scope = JSON.stringify([connectionId, database]);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;

  useEffect(() => {
    const token = ++generation.current;
    setItems([]); setSelected(null); setDeleteId(null); setError(null); setMessage(null); setBusy(true);
    void listAnalysisHistory({ connection_id: connectionId, database })
      .then((result) => { if (token === generation.current && scopeRef.current === scope) setItems(result); })
      .catch(() => { if (token === generation.current && scopeRef.current === scope) setError("读取分析历史失败，本机文件可能损坏或不可访问；原文件不会被覆盖。"); })
      .finally(() => { if (token === generation.current && scopeRef.current === scope) setBusy(false); });
    return () => { generation.current += 1; };
  }, [connectionId, database, scope]);

  const run = async (operation: (isCurrent: () => boolean) => Promise<void>) => {
    if (busy) return;
    const token = ++generation.current;
    const isCurrent = () => generation.current === token && scopeRef.current === scope;
    setBusy(true); setError(null); setMessage(null);
    try { await operation(isCurrent); }
    catch { if (isCurrent()) setError("历史记录操作失败，请检查本机文件权限或记录上限（每库 20 条、总计 50 条、单报告 256 KiB）。原有记录会保留。"); }
    finally { if (isCurrent()) setBusy(false); }
  };

  const save = () => {
    if (!report || report.database !== database) return;
    void run(async (isCurrent) => {
      const summary = await saveAnalysisHistory({ connection_id: connectionId, report });
      if (isCurrent()) { setItems((current) => [summary, ...current]); setMessage("分析报告已保存到本机。"); }
    });
  };

  return <section className="database-panel" aria-label="分析历史" aria-busy={busy}>
    <div className="database-panel-heading"><h3>分析历史</h3><button className="button button-secondary" disabled={busy || !report || report.database !== database} onClick={save}>保存当前分析</button></div>
    <p>报告可能包含键名与命名空间。仅点击保存后写入本机，不包含键值或连接凭据；每个数据库最多 20 条，全局最多 50 条。</p>
    {error && <p className="inline-error" role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    {busy && <p role="status">正在处理分析历史…</p>}
    {!busy && items.length === 0 && <p>当前数据库暂无已保存分析。</p>}
    {items.length > 0 && <div className="database-table-wrap"><table className="database-table" aria-label="已保存分析">
      <thead><tr><th>保存时间</th><th>匹配模式</th><th>键数</th><th>内存（字节）</th><th>操作</th></tr></thead>
      <tbody>{items.map((item) => <tr key={item.id}>
        <td>{new Date(item.saved_at).toLocaleString("zh-CN")}{item.truncated ? "（部分）" : ""}</td><td><code>{item.pattern}</code></td><td>{item.total_keys.toLocaleString("zh-CN")}</td><td>{item.total_memory.toLocaleString("zh-CN")}</td>
        <td><button className="button button-quiet" disabled={busy} onClick={() => void run(async (isCurrent) => {
          const result = await getAnalysisHistory({ connection_id: connectionId, database, id: item.id });
          if (isCurrent()) { setSelected(result); setDeleteId(null); }
        })}>查看历史报告</button><button className="button button-danger" disabled={busy} onClick={() => setDeleteId(item.id)}>删除历史报告</button></td>
      </tr>)}</tbody>
    </table></div>}
    {deleteId && <div role="group" aria-label="确认删除分析历史"><p>确认删除这份本机分析报告？Redis 数据不会被修改。</p>
      <button className="button button-danger" disabled={busy} onClick={() => void run(async (isCurrent) => {
        const id = deleteId;
        await deleteAnalysisHistory({ connection_id: connectionId, database, id });
        if (isCurrent()) {
          setItems((current) => current.filter((item) => item.id !== id));
          setSelected((current) => current?.id === id ? null : current); setDeleteId(null);
        }
      })}>确认删除报告</button><button className="button button-quiet" disabled={busy} onClick={() => setDeleteId(null)}>取消删除</button>
    </div>}
    {selected && <section aria-label="历史报告详情">
      <h4>历史报告 · {new Date(selected.saved_at).toLocaleString("zh-CN")}</h4>
      <button className="button button-quiet" onClick={() => setSelected(null)}>关闭历史报告</button>
      {report && <Comparison current={report} previous={selected.report} />}
      {renderReport(selected.report)}
    </section>}
  </section>;
}
