import Select from "../../components/Select";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useEffect, useRef, useState } from "react";
import { aggregateSearch, type AggregateFunction, type AggregateReducer, type SearchAggregateInput, type SearchAggregateResult } from "./aggregateApi";
import "./aggregatePanel.css";

interface Props { connectionId: string; index: string | null; enabled?: boolean }
interface Draft { query: string; load: string; groups: string; reducers: AggregateReducer[]; sort: string; direction: "asc" | "desc" }
const PAGE_SIZE = 50;
const initialDraft = (): Draft => ({ query: "*", load: "", groups: "", reducers: [{ function: "count", field: null, alias: "count" }], sort: "", direction: "asc" });
const bytes = (value: string) => new TextEncoder().encode(value).length;
const fieldValid = (value: string) => value.length > 0 && bytes(value) <= 256 && !/[\s@*,\u0000-\u001f\u007f-\u009f]/u.test(value);
const splitFields = (value: string) => value.split(",").map((field) => field.trim()).filter(Boolean);

function makeInput(draft: Draft, connectionId: string, index: string, offset: number): SearchAggregateInput | null {
  const load = splitFields(draft.load);
  const groups = splitFields(draft.groups);
  const reducers = draft.reducers.map((reducer) => ({ ...reducer, alias: reducer.alias.trim(), field: reducer.function === "count" ? null : reducer.field?.trim() ?? "" }));
  const loaded = new Set([...load, ...groups, ...reducers.flatMap((reducer) => reducer.field === null ? [] : [reducer.field])]);
  const grouped = groups.length > 0 || reducers.length > 0;
  const outputs = grouped ? [...groups, ...reducers.map((reducer) => reducer.alias)] : load;
  const sort = draft.sort.trim();
  if (!draft.query.trim() || bytes(draft.query) > 4096 || load.length > 16 || groups.length > 8 || reducers.length > 8 || loaded.size > 16 || loaded.size > 0 && [...loaded].some((field) => !fieldValid(field)) || new Set(load).size !== load.length || new Set(groups).size !== groups.length || new Set(outputs).size !== outputs.length || outputs.length === 0 || reducers.some((reducer) => !fieldValid(reducer.alias)) || sort && !outputs.includes(sort)) return null;
  return { connection_id: connectionId, index, query: draft.query, load_fields: load, group_by: groups, reducers, sort_by: sort ? [{ field: sort, direction: draft.direction }] : [], offset, limit: PAGE_SIZE };
}

function errorMessage(error: unknown): string {
  const code = error && typeof error === "object" && "code" in error ? error.code : null;
  if (code === "UNSUPPORTED_FEATURE") return "当前连接不支持聚合查询，需要 RedisSearch 2.0 或更新版本。";
  if (code === "INVALID_INPUT") return "聚合参数无效，请检查字段、别名和数量限制。";
  return "聚合查询失败，请检查索引、字段类型、查询语句及响应大小后重试。";
}

function AggregateEditor({ connectionId, index, enabled = true }: Props) {
  const [draft, setDraft] = useState<Draft>(initialDraft);
  const [result, setResult] = useState<SearchAggregateResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState<string | null>(null);
  const [retryOffset, setRetryOffset] = useState<number | null>(null);
  const sequence = useRef(0);
  const pending = useRef(false);
  useEffect(() => () => { sequence.current += 1; pending.current = false; }, []);

  function change(patch: Partial<Draft>) {
    sequence.current += 1;
    pending.current = false;
    setDraft((current) => ({ ...current, ...patch }));
    setResult(null);
    setBusy(false);
    setError(null);
    setRetryOffset(null);
  }
  function changeReducer(position: number, patch: Partial<AggregateReducer>) {
    change({ reducers: draft.reducers.map((reducer, index) => index === position ? { ...reducer, ...patch } : reducer) });
  }
  async function run(offset: number) {
    if (!enabled || !index || pending.current) return;
    const input = makeInput(draft, connectionId, index, offset);
    if (!input) { setError("聚合参数无效：使用不带 @ 的字段名；加载最多 16 个字段、分组最多 8 个字段、别名必须唯一，排序字段必须在输出中。"); setRetryOffset(null); return; }
    const token = ++sequence.current;
    pending.current = true;
    setBusy(true);
    setError(null);
    setRetryOffset(null);
    try {
      const next = await aggregateSearch(input);
      if (sequence.current === token) setResult(next);
    } catch (error) {
      if (sequence.current === token) { setError(errorMessage(error)); setRetryOffset(offset); }
    } finally {
      if (sequence.current === token) { pending.current = false; setBusy(false); }
    }
  }
  const disabled = !enabled || !index;
  return <section className="database-panel aggregate-panel" aria-labelledby="aggregate-title">
    <div className="database-panel-heading"><div><p className="eyebrow">FT.AGGREGATE</p><h3 id="aggregate-title">聚合查询</h3></div><span className="panel-hint">只读 · 每页 {PAGE_SIZE} 行</span></div>
    <p className="browser-helper">选择字段、分组与聚合函数。字段名不带 @，多个字段用逗号分隔；分组和聚合所需字段会自动加载，不支持 LOAD * 或任意管道。</p>
    <div className="aggregate-fields">
      <label className="field aggregate-query"><span>聚合查询语句</span><input autoCapitalize="off" autoCorrect="off" value={draft.query} disabled={disabled} maxLength={4096} spellCheck={false} onChange={(event) => change({ query: event.target.value })} /></label>
      <label className="field"><span>加载字段</span><input autoCapitalize="off" autoCorrect="off" value={draft.load} disabled={disabled} maxLength={4096} placeholder="price, category（可选）" onChange={(event) => change({ load: event.target.value })} /></label>
      <label className="field"><span>分组字段</span><input autoCapitalize="off" autoCorrect="off" value={draft.groups} disabled={disabled} maxLength={2048} placeholder="category（留空为整体聚合）" onChange={(event) => change({ groups: event.target.value })} /></label>
    </div>
    <div className="aggregate-reducers">
      {draft.reducers.map((reducer, position) => <div className="aggregate-reducer" key={position}>
        <label className="field"><span>聚合函数 {position + 1}</span><Select value={reducer.function} disabled={disabled} onChange={(event) => changeReducer(position, { function: event.target.value as AggregateFunction, field: event.target.value === "count" ? null : reducer.field ?? "" })}>{["count", "sum", "min", "max", "avg"].map((value) => <option key={value} value={value}>{value.toUpperCase()}</option>)}</Select></label>
        <label className="field"><span>聚合字段 {position + 1}</span><input autoCapitalize="off" autoCorrect="off" value={reducer.field ?? ""} disabled={disabled || reducer.function === "count"} maxLength={256} placeholder={reducer.function === "count" ? "COUNT 无需字段" : "数值字段"} onChange={(event) => changeReducer(position, { field: event.target.value })} /></label>
        <label className="field"><span>聚合别名 {position + 1}</span><input autoCapitalize="off" autoCorrect="off" value={reducer.alias} disabled={disabled} maxLength={256} onChange={(event) => changeReducer(position, { alias: event.target.value })} /></label>
        <button type="button" className="button button-secondary" aria-label={`删除聚合 ${position + 1}`} disabled={disabled} onClick={() => change({ reducers: draft.reducers.filter((_value, index) => index !== position) })}>删除</button>
      </div>)}
      <button type="button" className="button button-secondary" disabled={disabled || draft.reducers.length >= 8} onClick={() => change({ reducers: [...draft.reducers, { function: "count", field: null, alias: `count_${draft.reducers.length + 1}` }] })}>添加聚合函数</button>
    </div>
    <div className="aggregate-fields">
      <label className="field"><span>排序字段</span><input autoCapitalize="off" autoCorrect="off" value={draft.sort} disabled={disabled} maxLength={256} placeholder="输出分组字段或聚合别名（可选）" onChange={(event) => change({ sort: event.target.value })} /></label>
      <label className="field"><span>排序方向</span><Select value={draft.direction} disabled={disabled || !draft.sort.trim()} onChange={(event) => change({ direction: event.target.value as "asc" | "desc" })}><option value="asc">升序</option><option value="desc">降序</option></Select></label>
    </div>
    <p className="browser-helper">最多 16 个加载字段、8 个分组、8 个聚合函数；响应最多 2 MiB、32 列、64 KiB/单元格。LIMIT 分页不是快照，数据变化或排序值相同时可能重复或遗漏；偏移上限 10,000。请求最多等待 5 秒，服务器仍可能继续执行。</p>
    <div className="aggregate-actions"><button type="button" className="button button-primary" disabled={disabled || busy} onClick={() => void run(0)}>{busy ? "聚合查询中…" : "运行聚合"}</button>{retryOffset !== null ? <button type="button" className="button button-secondary" disabled={disabled || busy} onClick={() => void run(retryOffset)}>重试聚合</button> : null}</div>
    {error ? <Toast kind="error" message={error} resetKey={errorToken} onClose={() => setError(null)} /> : null}
    {result ? <div className="aggregate-results" aria-busy={busy}>
      <p className="browser-helper">偏移 {result.offset} · 本页 {result.rows.length} 行{result.offset + result.rows.length > 10_000 ? " · 已达分页偏移上限" : ""}</p>
      {result.rows.length ? <div className="search-document-table-wrap"><table className="data-table"><thead><tr><th scope="col">行</th>{result.columns.map((column) => <th scope="col" key={column}>{column}</th>)}</tr></thead><tbody>{result.rows.map((row, position) => <tr key={position}><td>{result.offset + position + 1}</td>{result.columns.map((column) => { const cell = row.fields.find((field) => field.name === column); return <td className="search-document-value" key={column}><pre>{!cell ? "—" : typeof cell.value === "string" ? cell.value : JSON.stringify(cell.value)}</pre></td>; })}</tr>)}</tbody></table></div> : <p>当前页没有聚合结果。</p>}
      <div className="aggregate-actions"><button type="button" className="button button-secondary" disabled={disabled || busy || result.offset === 0} onClick={() => void run(Math.max(0, result.offset - PAGE_SIZE))}>聚合上一页</button><button type="button" className="button button-secondary" disabled={disabled || busy || result.next_offset === null} onClick={() => { if (result.next_offset !== null) void run(result.next_offset); }}>聚合下一页</button></div>
    </div> : null}
  </section>;
}

export function AggregatePanel(props: Props) {
  // Remount on identity or availability changes before any stale state can render.
  return <AggregateEditor key={JSON.stringify([props.connectionId, props.index, props.enabled ?? true])} {...props} />;
}
