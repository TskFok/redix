import { useEffect, useRef, useState } from "react";
import {
  claimStreamPendingAdvanced,
  getStreamPendingPage,
  updateStreamGroupId,
  type StreamPendingPage,
} from "./streamAdvancedApi";

interface Props {
  connectionId: string;
  streamKey: string;
  group: string;
  lastDeliveredId: string;
  disabled?: boolean;
  onChanged(): void | Promise<void>;
  onBusyChange?(busy: boolean): void;
}
interface Filter { start: string; end: string; consumer: string; count: number }
const defaultFilter: Filter = { start: "-", end: "+", consumer: "", count: 100 };
const emptyPage: StreamPendingPage = { entries: [], next_cursor: null, has_more: false };
const maxId = 18446744073709551615n;

function validId(value: string, incomplete = false) {
  if (!(incomplete ? /^\d+(?:-\d+)?$/ : /^\d+-\d+$/).test(value)) return false;
  return value.split("-").every(part => BigInt(part) <= maxId);
}
function parseInteger(value: string, signed = false): number | null {
  if (!(signed ? /^-?\d+$/ : /^\d+$/).test(value)) return null;
  const result = Number(value);
  return Number.isSafeInteger(result) ? result : null;
}

export default function StreamAdvancedPanel({ connectionId, streamKey, group, lastDeliveredId, disabled = false, onChanged, onBusyChange }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [filter, setFilter] = useState<Filter>(defaultFilter);
  const [applied, setApplied] = useState<Filter>(defaultFilter);
  const [page, setPage] = useState<StreamPendingPage>(emptyPage);
  const [history, setHistory] = useState<(string | null)[]>([null]);
  const [selected, setSelected] = useState<string[]>([]);
  const [groupId, setGroupId] = useState(lastDeliveredId);
  const [consumer, setConsumer] = useState("");
  const [minIdle, setMinIdle] = useState("0");
  const [timeMode, setTimeMode] = useState("default");
  const [deliveryTime, setDeliveryTime] = useState("0");
  const [retryCount, setRetryCount] = useState("");
  const [force, setForce] = useState(false);
  const [explicitIds, setExplicitIds] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const busyRef = useRef(false);
  const tokenRef = useRef(0);
  const mounted = useRef(false);
  const scope = JSON.stringify([connectionId, streamKey, group]);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;
  const target = { connection_id: connectionId, key: streamKey, group };

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; tokenRef.current++; };
  }, []);
  useEffect(() => {
    tokenRef.current++;
    busyRef.current = false;
    setBusy(false); setExpanded(false); setFilter(defaultFilter); setApplied(defaultFilter);
    setPage(emptyPage); setHistory([null]); setSelected([]); setGroupId(lastDeliveredId);
    setConsumer(""); setMinIdle("0"); setTimeMode("default"); setDeliveryTime("0");
    setRetryCount(""); setForce(false); setExplicitIds(""); setError(null); setMessage(null);
  }, [scope]);

  const perform = async (action: (isCurrent: () => boolean) => Promise<void>, fallback: string) => {
    if (busyRef.current || disabled) return;
    const token = ++tokenRef.current;
    const isCurrent = () => mounted.current && scopeRef.current === scope && tokenRef.current === token;
    busyRef.current = true; setBusy(true); onBusyChange?.(true); setError(null); setMessage(null);
    try { await action(isCurrent); }
    catch { if (isCurrent()) setError(fallback); }
    finally { if (isCurrent()) { busyRef.current = false; setBusy(false); onBusyChange?.(false); } }
  };
  const pageInput = (cursor: string | null, query: Filter) => ({
    ...target, start: query.start, end: query.end, consumer: query.consumer || null, count: query.count, cursor,
  });
  const load = (query: Filter, nextHistory: (string | null)[]) => void perform(async isCurrent => {
    const result = await getStreamPendingPage(pageInput(nextHistory[nextHistory.length - 1], query));
    if (isCurrent()) { setPage(result); setSelected([]); setHistory(nextHistory); setApplied(query); }
  }, "读取 Pending 范围失败，请检查范围、Group 和访问权限后重试。");
  const applyFilter = () => {
    const query = { ...filter, start: filter.start.trim(), end: filter.end.trim(), consumer: filter.consumer.trim() };
    if ((query.start !== "-" && !validId(query.start)) || (query.end !== "+" && !validId(query.end)) || query.count < 1 || query.count > 500 || !Number.isInteger(query.count) || query.consumer.length > 256) {
      setError("请输入有效的起止 ID（毫秒-序号）、消费者及 1 到 500 的每页数量。"); return;
    }
    load(query, [null]);
  };
  const updateId = () => {
    const id = groupId.trim();
    if (id !== "$" && !validId(id, true)) { setError("Group ID 必须是 $、非负整数或毫秒-序号。"); return; }
    void perform(async isCurrent => {
      await updateStreamGroupId({ ...target, last_delivered_id: id });
      if (!isCurrent()) return;
      setMessage(`Group 最后投递 ID 已更新为 ${id}。`);
      await onChanged();
    }, "更新 Group ID 失败，请检查 ID、Group 和访问权限后重试。");
  };
  const claim = () => {
    const idle = parseInteger(minIdle);
    const time = timeMode === "default" ? null : parseInteger(deliveryTime, timeMode === "time");
    const retry = retryCount.trim() === "" ? null : parseInteger(retryCount);
    const ids = [...new Set([...selected, ...explicitIds.split(/[\s,]+/).filter(Boolean)])];
    if (!consumer.trim() || consumer.length > 256 || idle === null || (timeMode !== "default" && time === null) || (retryCount.trim() !== "" && retry === null) || ids.length === 0 || ids.length > 500 || ids.some(id => !validId(id))) {
      setError("请检查目标消费者、消息 ID 和整数选项；一次最多转移 500 条，TIME 可为带符号的 Unix 毫秒。"); return;
    }
    void perform(async isCurrent => {
      const affected = await claimStreamPendingAdvanced({ ...target, consumer: consumer.trim(), min_idle_ms: idle,
        entries: ids, idle_ms: timeMode === "idle" ? time : null, time_ms: timeMode === "time" ? time : null,
        retry_count: retry, force });
      if (!isCurrent()) return;
      setSelected([]); setExplicitIds("");
      setMessage(`已转移 ${affected.length} / ${ids.length} 条；未满足条件或已被删除的消息不会返回。`);
      try {
        const result = await getStreamPendingPage(pageInput(history[history.length - 1], applied));
        if (!isCurrent()) return;
        setPage(result);
        await onChanged();
      } catch { if (isCurrent()) setError("转移已完成，但刷新 Pending 失败，请重新应用范围查看结果。"); }
    }, "高级 Claim 失败，请检查选项、Group 和访问权限后重试。");
  };
  const locked = busy || disabled;

  return <section className="stream-group-subpanel" aria-label="Stream 高级操作" aria-busy={busy}>
    <button type="button" className="button button-secondary" disabled={locked} aria-expanded={expanded} onClick={() => {
      setExpanded(!expanded);
      if (!expanded) load(applied, history);
    }}>{expanded ? "收起 Stream 高级操作" : "展开 Stream 高级操作"}</button>
    {expanded && <>
      <div className="stream-group-create">
        <label className="field"><span>Group 最后投递 ID</span><input value={groupId} disabled={locked} onChange={event => setGroupId(event.target.value)} /></label>
        <button type="button" className="button button-secondary" disabled={locked} onClick={updateId}>更新 Group ID</button>
      </div>
      <p className="panel-hint">SETID 调整下一次读取新消息的位置，可能使消息重新投递或跳过尚未读取的消息；现有 Pending 和 Stream 内容会保留。</p>
      <div className="form-grid">
        <label className="field"><span>Pending 起始 ID</span><input value={filter.start} disabled={locked} onChange={event => setFilter(current => ({...current,start:event.target.value}))} /></label>
        <label className="field"><span>Pending 结束 ID</span><input value={filter.end} disabled={locked} onChange={event => setFilter(current => ({...current,end:event.target.value}))} /></label>
        <label className="field"><span>Pending 消费者过滤</span><input value={filter.consumer} disabled={locked} placeholder="留空查看所有消费者" onChange={event => setFilter(current => ({...current,consumer:event.target.value}))} /></label>
        <label className="field"><span>Pending 每页数量</span><input type="number" min={1} max={500} value={filter.count} disabled={locked} onChange={event => setFilter(current => ({...current,count:Number(event.target.value)}))} /></label>
      </div>
      <button type="button" className="button button-secondary" disabled={locked} onClick={applyFilter}>应用 Pending 范围</button>
      <p className="panel-hint">当前范围 {applied.start} 至 {applied.end} · {applied.consumer || "全部消费者"} · 第 {history.length} 页</p>
      <div className="stream-table-wrap"><table className="stream-groups-table" aria-label="Pending 范围结果">
        <thead><tr><th>选择</th><th>ID</th><th>消费者</th><th>空闲（毫秒）</th><th>投递次数</th></tr></thead>
        <tbody>{page.entries.map(entry => <tr key={entry.id}>
          <td><input type="checkbox" aria-label={`高级选择 Pending ${entry.id}`} disabled={locked} checked={selected.includes(entry.id)} onChange={() => setSelected(current => current.includes(entry.id) ? current.filter(id => id !== entry.id) : [...current,entry.id])}/></td>
          <td><code>{entry.id}</code></td><td>{entry.consumer}</td><td>{entry.idle_ms}</td><td>{entry.deliveries}</td>
        </tr>)}</tbody>
      </table></div>
      {page.entries.length === 0 && <p>当前范围没有 Pending 消息。</p>}
      <div className="card-actions">
        <button type="button" className="button button-quiet" disabled={locked || history.length < 2} onClick={() => load(applied, history.slice(0,-1))}>Pending 上一页</button>
        <button type="button" className="button button-quiet" disabled={locked || !page.has_more || !page.next_cursor} onClick={() => load(applied, [...history,page.next_cursor])}>Pending 下一页</button>
      </div>
      <h4>高级 Claim</h4>
      <div className="form-grid">
        <label className="field"><span>高级转移目标消费者</span><input value={consumer} disabled={locked} onChange={event => setConsumer(event.target.value)} /></label>
        <label className="field"><span>最小空闲时间（毫秒）</span><input inputMode="numeric" value={minIdle} disabled={locked} onChange={event => setMinIdle(event.target.value)} /></label>
        <label className="field"><span>投递时间设置</span><select value={timeMode} disabled={locked} onChange={event => setTimeMode(event.target.value)}><option value="default">使用当前时间</option><option value="idle">IDLE 相对空闲时间</option><option value="time">TIME Unix 时间</option></select></label>
        {timeMode !== "default" && <label className="field"><span>{timeMode === "idle" ? "IDLE 空闲毫秒" : "TIME Unix 毫秒"}</span><input value={deliveryTime} disabled={locked} onChange={event => setDeliveryTime(event.target.value)} /></label>}
        <label className="field"><span>RETRYCOUNT 投递次数</span><input value={retryCount} disabled={locked} placeholder="留空保留当前次数" onChange={event => setRetryCount(event.target.value)} /></label>
      </div>
      <label className="field"><span>补充消息 ID</span><textarea value={explicitIds} disabled={locked} placeholder="可填写未在当前页的消息 ID，以空格、逗号或换行分隔" onChange={event => setExplicitIds(event.target.value)} /></label>
      <label className="field settings-checkbox"><span><input type="checkbox" checked={force} disabled={locked} onChange={event => setForce(event.target.checked)} />FORCE 创建 Pending 记录</span></label>
      <p className="panel-hint">Claim 会改变消息所属消费者与空闲时间。{force ? "启用 FORCE 后，消息即使不在 Pending 中，只要仍存在于 Stream，也会为其创建 Pending 记录。" : "仅转移满足最小空闲时间的现有 Pending 消息。"}</p>
      <button type="button" className="button button-primary" disabled={locked || (!selected.length && !explicitIds.trim())} onClick={claim}>执行高级 Claim</button>
      {error && <p role="alert" className="feedback feedback-error">{error}</p>}
      {message && <p role="status">{message}</p>}
    </>}
  </section>;
}
