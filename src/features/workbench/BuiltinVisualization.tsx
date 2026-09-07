import { useMemo, useState } from "react";
import { parseVisualization, type PlotPoint, type PlotSeries } from "./visualizationData";

const colors = ["#4689f4", "#db7435", "#21a58b", "#b169d8", "#dc6685", "#809833"];
const numberLabel = (value: number) => value.toLocaleString("zh-CN", {
  maximumSignificantDigits: 5,
  notation: Math.abs(value) >= 1e7 || (value !== 0 && Math.abs(value) < 1e-4) ? "scientific" : "standard",
});
const timeLabel = (value: number) => new Date(value).toISOString();

function chartScale(values: number[], start: number, length: number) {
  const min = Math.min(...values), max = Math.max(...values);
  // Normalize before subtraction, so opposite finite extremes cannot overflow.
  const magnitude = Math.max(Math.abs(min), Math.abs(max), 1);
  const low = min / magnitude, span = max / magnitude - low;
  return (value: number) => start + (span === 0 ? 0.5 : (value / magnitude - low) / span) * length;
}

function Plot({ series, geo }: { series: PlotSeries[]; geo: boolean }) {
  const points = series.flatMap((item) => item.points);
  if (!points.length) return <p>当前范围没有可绘制的数据。</p>;
  const xs = points.map((point) => point.x), ys = points.map((point) => point.y);
  const x = chartScale(geo ? [-180, 180] : xs, 82, 575);
  const y = chartScale(geo ? [-90, 90] : ys, 260, -230);
  const label = geo ? "地理坐标图" : "时间序列图";
  const xMin = geo ? -180 : Math.min(...xs), xMax = geo ? 180 : Math.max(...xs);
  const yMin = geo ? -90 : Math.min(...ys), yMax = geo ? 90 : Math.max(...ys);
  const tooltip = (point: PlotPoint) => `${geo ? numberLabel(point.x) : timeLabel(point.x)}，${numberLabel(point.y)}`;
  return <svg className="builtin-plot" viewBox="0 0 720 325" role="img" aria-label={label}>
    <title>{label}，{points.length} 个数据点；精确值可在下方数据表查看。</title>
    {[0, 0.5, 1].map((fraction) => <path key={fraction} d={`M82 ${30 + fraction * 230}H657`} stroke="currentColor" opacity="0.15" />)}
    <path d="M82 25V260H665" fill="none" stroke="currentColor" opacity="0.5" />
    <text x="72" y="34" textAnchor="end">{numberLabel(yMax)}</text>
    <text x="72" y="264" textAnchor="end">{numberLabel(yMin)}</text>
    <text x="82" y="285">{geo ? `${xMin}°` : timeLabel(xMin)}</text>
    <text x="657" y="302" textAnchor="end">{geo ? `${xMax}°` : timeLabel(xMax)}</text>
    <text x="365" y="322" textAnchor="middle">{geo ? "经度（横轴） / 纬度（纵轴），单位：度" : "UTC 时间（横轴） / 样本值（纵轴）"}</text>
    {series.map((item, index) => <g key={`${item.name}-${index}`} fill={colors[index % colors.length]}>
      {!geo ? <polyline points={item.points.map((point) => `${x(point.x)},${y(point.y)}`).join(" ")} fill="none" stroke={colors[index % colors.length]} strokeWidth="2" /> : null}
      {item.points.map((point, pointIndex) => <circle key={pointIndex} cx={x(point.x)} cy={y(point.y)} r={geo ? 4 : 2.5}><title>{item.name}：{tooltip(point)}</title></circle>)}
    </g>)}
  </svg>;
}

export default function BuiltinVisualization({ command, value }: { command: string; value: unknown }) {
  const data = useMemo(() => parseVisualization(command, value), [command, value]);
  const [opened, setOpened] = useState(false);
  const [selected, setSelected] = useState("all");
  const [page, setPage] = useState(0);
  if (!data) return null;
  const series: PlotSeries[] = "error" in data ? [] : data.kind === "timeseries" ? data.series : [{ name: "位置", points: data.points }];
  const visible = selected === "all" ? series : series.filter((_, index) => String(index) === selected);
  const rows = "error" in data ? [] : data.kind === "geo" ? data.points : visible.flatMap((item) => item.points.map((point) => ({ ...point, name: item.name })));
  const geo = !("error" in data) && data.kind === "geo";
  return <details className="builtin-visualization" open={opened} onToggle={(event) => setOpened(event.currentTarget.open)}>
    <summary>本地可视化</summary>
    {opened ? "error" in data ? <p role="status">{data.error}</p> : <>
      <p>{geo ? "经纬度散点图，使用完整地理坐标范围，无地图底图。" : "按时间排序的样本图，单位由原始数据定义。"}显示 {rows.length} 个数据点。名称最多展示 256 字符，完整名称见原始结果。</p>
      {series.length > 1 ? <label className="field"><span>显示序列</span><select value={selected} onChange={(event) => { setSelected(event.target.value); setPage(0); }}><option value="all">全部序列</option>{series.map((item, index) => <option key={index} value={index}>{item.name}</option>)}</select></label> : null}
      <Plot series={visible} geo={geo} />
      {!geo ? <ul className="builtin-legend">{visible.map((item, index) => <li key={index}><span style={{ background: colors[index % colors.length] }} />{item.name}</li>)}</ul> : null}
      {data.kind === "geo" && data.missing.length ? <p>缺少坐标的成员：{data.missing.join("、")}</p> : null}
      <div className="builtin-data"><table aria-label={geo ? "地理坐标数据" : "时间序列数据"}><thead><tr><th>{geo ? "成员" : "序列"}</th><th>{geo ? "经度" : "UTC 时间 / 毫秒时间戳"}</th><th>{geo ? "纬度" : "值"}</th></tr></thead><tbody>{rows.slice(page * 100, (page + 1) * 100).map((row, index) => <tr key={index}><th scope="row">{row.name}</th><td>{geo ? row.x : `${timeLabel(row.x)} / ${row.x}`}</td><td>{row.y}</td></tr>)}</tbody></table></div>
      {rows.length > 100 ? <div className="command-result-actions"><button type="button" className="button button-quiet" disabled={page === 0} onClick={() => setPage(page - 1)}>上一页数据</button><span>第 {page + 1} / {Math.ceil(rows.length / 100)} 页</span><button type="button" className="button button-quiet" disabled={(page + 1) * 100 >= rows.length} onClick={() => setPage(page + 1)}>下一页数据</button></div> : null}
    </> : null}
  </details>;
}
