import Select from "../../components/Select";
import { useState } from "react";
import type { DatabaseAnalysisReport } from "../../lib/types";
import type { SavedAnalysis } from "./analysisHistoryApi";

export function analysisScope(report: DatabaseAnalysisReport): string {
  return JSON.stringify([report.database, report.pattern, report.delimiter, report.progress.max_keys,
    (report.node_results ?? []).map((node) => node.node_id).sort(), (report.failed_nodes ?? []).map((node) => node.node_id).sort()]);
}

export function groupAnalysisHistory(items: SavedAnalysis[]) {
  const groups = new Map<string, SavedAnalysis[]>();
  for (const item of [...items].sort((a, b) => a.saved_at - b.saved_at)) {
    if (!Number.isFinite(item.saved_at) || !Number.isFinite(item.report.total_keys.observed) || !Number.isFinite(item.report.total_memory.total)) continue;
    const scope = analysisScope(item.report);
    groups.set(scope, [...(groups.get(scope) ?? []), item]);
  }
  return [...groups.entries()].map(([scope, reports]) => ({ scope, reports }));
}

function TrendLine({ points, title, memory }: { points: SavedAnalysis[]; title: string; memory?: boolean }) {
  const values = points.map((item) => memory ? item.report.total_memory.total : item.report.total_keys.observed);
  const max = Math.max(1, ...values);
  const start = points[0].saved_at;
  const duration = points[points.length - 1].saved_at - start;
  const coordinates = points.map((point, index) => ({ x: 70 + (duration > 0 ? (point.saved_at - start) / duration : index / Math.max(1, points.length - 1)) * 500, y: 135 - values[index] / max * 110 }));
  return <figure style={{ margin: "12px 0" }}><figcaption>{title}{memory ? "（字节）" : ""}</figcaption>
    <svg viewBox="0 0 600 175" role="img" aria-label={title} style={{ display: "block", width: "100%", maxWidth: 720 }}>
      <title>{title}，{points.length} 份已保存报告</title>
      <path d="M70 20V135H575" fill="none" stroke="currentColor" opacity="0.4" />
      <text x="60" y="30" textAnchor="end" fontSize="11" fill="currentColor">{max.toLocaleString("zh-CN")}</text>
      <text x="60" y="139" textAnchor="end" fontSize="11" fill="currentColor">0</text>
      <polyline points={coordinates.map((point) => `${point.x},${point.y}`).join(" ")} fill="none" stroke="var(--color-primary)" strokeWidth="2" />
      {coordinates.map((point, index) => <circle key={points[index].id} cx={point.x} cy={point.y} r="4" fill="var(--color-primary)"><title>{new Date(points[index].saved_at).toLocaleString("zh-CN")}：{values[index].toLocaleString("zh-CN")}</title></circle>)}
      <text x="70" y="160" fontSize="11" fill="currentColor">{new Date(start).toLocaleString("zh-CN")}</text>
      <text x="570" y="160" textAnchor="end" fontSize="11" fill="currentColor">{new Date(points[points.length - 1].saved_at).toLocaleString("zh-CN")}</text>
    </svg>
  </figure>;
}

export default function AnalysisTrends({ items, current }: { items: SavedAnalysis[]; current: DatabaseAnalysisReport | null }) {
  const groups = groupAnalysisHistory(items);
  const [selected, setSelected] = useState(() => Math.max(0, groups.findIndex((group) => group.scope === (current ? analysisScope(current) : ""))));
  const group = groups[selected] ?? groups[0];
  const reports = group?.reports ?? [];
  return <section aria-label="本机历史趋势">
    <h4>本机历史趋势</h4>
    <p>横轴为保存时间。仅连接扫描参数及节点范围相同的报告；扫描不是快照，截断和内存读取覆盖变化会影响趋势，不能据此推断全库增长。</p>
    {groups.length > 0 ? <label className="field"><span>趋势扫描范围</span><Select aria-label="趋势扫描范围" value={selected} onChange={(event) => setSelected(Number(event.target.value))}>{groups.map((item, index) => {
      const report = item.reports[0].report;
      return <option key={item.scope} value={index}>DB {report.database} · {report.pattern} · 分隔符 {report.delimiter} · 上限 {report.progress.max_keys.toLocaleString("zh-CN")} · 成功节点 {(report.node_results ?? []).map((node) => node.node_id).join(", ") || "当前实例"} · 失败节点 {(report.failed_nodes ?? []).map((node) => node.node_id).join(", ") || "无"} · {item.reports.length} 份</option>;
    })}</Select></label> : null}
    {reports.length >= 2 ? <><TrendLine title="历史已观察键数趋势" points={reports} /><TrendLine title="历史采样内存趋势" points={reports} memory /></> : <p>至少需要两份相同扫描范围的已保存报告才能绘制趋势。</p>}
    {reports.length > 0 ? <div className="database-table-wrap"><table className="database-table" aria-label="趋势数据"><thead><tr><th>保存时间</th><th>已观察键数</th><th>采样内存（字节）</th><th>读取内存的键数</th><th>范围</th></tr></thead><tbody>{reports.map((item) => <tr key={item.id}><td>{new Date(item.saved_at).toLocaleString("zh-CN")}</td><td>{item.report.total_keys.observed.toLocaleString("zh-CN")}</td><td>{item.report.total_memory.total.toLocaleString("zh-CN")}</td><td>{item.report.total_memory.observed.toLocaleString("zh-CN")}</td><td>{item.report.progress.truncated ? "已截断" : "未达到扫描上限"}{item.report.failed_nodes?.length ? "，部分节点失败" : ""}</td></tr>)}</tbody></table></div> : null}
  </section>;
}
