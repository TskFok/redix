import Select from "../../components/Select";
import { useState } from "react";

export interface InstanceSample { at: number; memory: number | null; ops: number | null; clients: number | null }
export function appendInstanceSample(samples: InstanceSample[], sample: InstanceSample): InstanceSample[] {
  if (!Number.isSafeInteger(sample.at) || sample.at < 0 || sample.at > 8640000000000000 || (samples.length > 0 && sample.at <= samples[samples.length - 1].at)) return samples;
  const finite = (value: number | null) => value !== null && Number.isFinite(value) && value >= 0 ? value : null;
  return [...samples.slice(-119), { at: sample.at, memory: finite(sample.memory), ops: finite(sample.ops), clients: finite(sample.clients) }];
}
const metrics = { memory: { label: "已用内存", unit: "字节" }, ops: { label: "操作数", unit: "次 / 秒" }, clients: { label: "已连接客户端", unit: "个" } };
type Metric = keyof typeof metrics;

export default function InstanceTrends({ samples }: { samples: InstanceSample[] }) {
  const [metric, setMetric] = useState<Metric>("memory");
  const info = metrics[metric];
  const max = Math.max(1, ...samples.map((sample) => sample[metric] ?? 0));
  const first = samples[0]?.at ?? 0, last = samples.at(-1)?.at ?? 0;
  const groups: { x: number; y: number; value: number; at: number }[][] = [];
  let group: typeof groups[number] = [];
  for (const sample of samples) {
    const value = sample[metric];
    if (value === null) { if (group.length) groups.push(group); group = []; continue; }
    group.push({ x: 75 + (last === first ? 0.5 : (sample.at - first) / (last - first)) * 555, y: 150 - value / max * 120, value, at: sample.at });
  }
  if (group.length) groups.push(group);
  return <section className="database-panel" aria-label="实例指标趋势">
    <h3>实例指标趋势</h3>
    <p>保留当前页面最近 120 次采样。开启自动刷新后持续采集，隐藏窗口时暂停；切换连接或离开页面后清空。读取失败显示为间隙。</p>
    <label className="field"><span>趋势指标</span><Select value={metric} onChange={(event) => setMetric(event.target.value as Metric)}>{Object.entries(metrics).map(([key, value]) => <option key={key} value={key}>{value.label}（{value.unit}）</option>)}</Select></label>
    {samples.length > 0 ? <>
      <svg viewBox="0 0 700 195" role="img" aria-label={`${info.label}趋势`} style={{ width: "100%", maxWidth: 900, display: "block" }}>
        <title>{info.label}，单位{info.unit}，{samples.length} 次采样</title>
        <path d="M75 25V150H635" fill="none" stroke="currentColor" opacity="0.4" />
        <text x="65" y="35" textAnchor="end" fontSize="11" fill="currentColor">{max.toLocaleString("zh-CN", { notation: "compact", maximumSignificantDigits: 4 })}</text>
        <text x="65" y="154" textAnchor="end" fontSize="11" fill="currentColor">0</text>
        {groups.map((points, index) => <g key={index}><polyline points={points.map((point) => `${point.x},${point.y}`).join(" ")} fill="none" stroke="var(--color-primary)" strokeWidth="2" />{points.map((point) => <circle key={point.at} cx={point.x} cy={point.y} r="3" fill="var(--color-primary)"><title>{new Date(point.at).toLocaleString("zh-CN")}：{point.value.toLocaleString("zh-CN")} {info.unit}</title></circle>)}</g>)}
        <text x="75" y="178" fontSize="11" fill="currentColor">{new Date(first).toLocaleTimeString("zh-CN")}</text>
        <text x="630" y="178" textAnchor="end" fontSize="11" fill="currentColor">{new Date(last).toLocaleTimeString("zh-CN")}</text>
      </svg>
      <details><summary>查看采样数据（{samples.length} 次）</summary><div className="database-table-wrap" style={{ maxHeight: 280, overflow: "auto" }}><table className="database-table" aria-label="实例趋势采样数据"><thead><tr><th>采样时间</th><th>{info.label}（{info.unit}）</th></tr></thead><tbody>{samples.map((sample) => <tr key={sample.at}><td>{new Date(sample.at).toLocaleString("zh-CN")}</td><td>{sample[metric] === null ? "不可用" : sample[metric]?.toLocaleString("zh-CN")}</td></tr>)}</tbody></table></div></details>
    </> : <p>加载概览后开始记录采样。</p>}
  </section>;
}
