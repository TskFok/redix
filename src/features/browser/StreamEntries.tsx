import Select from "../../components/Select";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useEffect, useRef, useState } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import { useTransientFeedback } from "../../components/useTransientFeedback";
import { addStreamEntry, deleteStreamEntries, getStreamEntries } from "./streamEntriesApi";
import type { StreamEntriesPage, StreamEntryField } from "./streamEntriesApi";
import { browserErrorMessage } from "./browserState";

interface StreamEntriesProps {
  connectionId: string;
  streamKey: string;
  disabled?: boolean;
  onChanged?: () => void;
  onBusyChange?: (busy: boolean) => void;
}

const U64_MAX = 18446744073709551615n;
function parseId(value: string): [bigint, bigint] | null {
  if (!/^\d+-\d+$/.test(value) || value.length > 41) return null;
  const [ms, sequence] = value.split("-").map(BigInt);
  return ms <= U64_MAX && sequence <= U64_MAX ? [ms, sequence] : null;
}
function validRange(start: string, end: string): boolean {
  const first = start === "-" ? [0n, 0n] : parseId(start);
  const last = end === "+" ? [U64_MAX, U64_MAX] : parseId(end);
  return !!first && !!last && (first[0] < last[0] || (first[0] === last[0] && first[1] <= last[1]));
}
function parseFields(raw: string): StreamEntryField[] | null {
  try {
    const fields: unknown = JSON.parse(raw);
    if (!Array.isArray(fields) || fields.length === 0 || fields.length > 500) return null;
    if (!fields.every((pair) => Array.isArray(pair) && pair.length === 2 && pair.every((value) => typeof value === "string"))) return null;
    const result = fields.map(([field, value]: string[]) => ({ field, value }));
    if (result.reduce((bytes, pair) => bytes + new TextEncoder().encode(pair.field + pair.value).length, 0) > 4 * 1024 * 1024) return null;
    return result;
  } catch { return null; }
}

// A fresh instance per connection/key also invalidates an A → B → A round trip.
export function StreamEntries(props: StreamEntriesProps) {
  return <StreamEntriesScope key={JSON.stringify([props.connectionId, props.streamKey])} {...props} />;
}

function StreamEntriesScope({ connectionId, streamKey, disabled = false, onChanged, onBusyChange }: StreamEntriesProps) {
  const [query, setQuery] = useState({ start: "-", end: "+", reverse: false, count: 100, cursors: [null] as (string | null)[] });
  const [start, setStart] = useState("-");
  const [end, setEnd] = useState("+");
  const [page, setPage] = useState<StreamEntriesPage | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [entryId, setEntryId] = useState("*");
  const [fieldsJson, setFieldsJson] = useState('[["field","value"]]');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState<string | null>(null);
  const [notice, setNotice, noticeToken] = useTransientFeedback();
  const [refresh, setRefresh] = useState(0);
  const { confirm, confirmationDialog } = useConfirmDialog(
    JSON.stringify([connectionId, streamKey, disabled, loading, query, refresh, selected, entryId, fieldsJson]),
  );
  const generation = useRef(0);
  const mutation = useRef(false);
  const busyCallback = useRef(onBusyChange);
  busyCallback.current = onBusyChange;
  useEffect(() => { busyCallback.current?.(busy || loading); }, [busy, loading]);
  useEffect(() => () => { generation.current += 1; busyCallback.current?.(false); }, []);

  useEffect(() => {
    let current = true;
    setLoading(true); setError(null); setPage(null); setSelected([]);
    void getStreamEntries({ connection_id: connectionId, key: streamKey, start: query.start, end: query.end,
      cursor: query.cursors[query.cursors.length - 1], count: query.count, reverse: query.reverse })
      .then((result) => { if (current) setPage(result); })
      .catch((reason: unknown) => { if (current) setError(browserErrorMessage(reason, "Stream 消息操作失败，请重试。")); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [connectionId, streamKey, query, refresh]);

  const unavailable = disabled || busy || loading;
  function applyRange() {
    if (!validRange(start, end)) { setError("消息范围无效：请输入完整 ID，并确保起点不大于终点。"); return; }
    setNotice(null); setQuery((previous) => ({ ...previous, start, end, cursors: [null] }));
  }
  async function mutate(kind: "add" | "delete") {
    if (unavailable || mutation.current) return;
    const fields = kind === "add" ? parseFields(fieldsJson) : null;
    if (kind === "add" && (!fields || (entryId !== "*" && (!parseId(entryId) || parseId(entryId)?.every((part) => part === 0n))))) {
      setError("消息 ID 或字段无效：ID 使用 * 或完整的非零 ID；字段为 1–500 组字符串键值对 JSON，总大小不超过 4 MiB。"); return;
    }
    if (kind === "delete" && selected.length === 0) return;
    const token = generation.current;
    const accepted = await confirm(
      kind === "add" ? `确认向 Stream「${streamKey}」添加消息？` : `确认删除 Stream「${streamKey}」选中的 ${selected.length} 条消息？此操作不可撤销，已有 Pending 引用不会被自动确认。`,
      kind === "add" ? { title: "确认添加消息", confirmLabel: "确认添加", danger: false } : undefined,
    );
    if (!accepted || token !== generation.current || mutation.current) return;
    mutation.current = true; setBusy(true); setError(null); setNotice(null);
    try {
      const result = kind === "add"
        ? await addStreamEntry({ connection_id: connectionId, key: streamKey, id: entryId, fields: fields! })
        : await deleteStreamEntries({ connection_id: connectionId, key: streamKey, ids: selected });
      if (token !== generation.current) return;
      setNotice(kind === "add" ? `已添加消息 ${result}` : `已删除 ${result} 条消息`);
      setSelected([]);
      // An append may be outside the current range/page; preserve the user's view.
      setRefresh((value) => value + 1);
      onChanged?.();
    } catch (reason) {
      if (token === generation.current) setError(browserErrorMessage(reason, "Stream 消息操作失败，请重试。"));
    } finally {
      if (token === generation.current) { mutation.current = false; setBusy(false); }
    }
  }

  return <section className="stream-entries" aria-label="Stream 消息" aria-busy={busy || loading}>
    {confirmationDialog}
    <div className="stream-section-heading">
      <div>
        <h3>Stream 消息</h3>
        <p className="stream-hint">按消息 ID 浏览、添加与删除，保留现有 TTL 和消费组。</p>
      </div>
      <button type="button" className="button button-secondary" disabled={unavailable} onClick={() => { setNotice(null); setRefresh((value) => value + 1); }}>刷新消息</button>
    </div>

    <div className="stream-message-filters">
      <div className="stream-range-fields">
        <label className="field"><span>起点 ID</span><input autoCapitalize="off" autoCorrect="off" aria-label="消息范围起点" value={start} disabled={unavailable} onChange={(event) => setStart(event.target.value)} /></label>
        <label className="field"><span>终点 ID</span><input autoCapitalize="off" autoCorrect="off" aria-label="消息范围终点" value={end} disabled={unavailable} onChange={(event) => setEnd(event.target.value)} /></label>
        <button type="button" className="button button-secondary" disabled={unavailable} onClick={applyRange}>应用消息范围</button>
      </div>
      <div className="stream-view-fields">
        <label className="field"><span>排序</span><Select aria-label="消息排序" value={query.reverse ? "desc" : "asc"} disabled={unavailable} onChange={(event) => setQuery({ ...query, reverse: event.target.value === "desc", cursors: [null] })}><option value="asc">ID 升序</option><option value="desc">ID 降序</option></Select></label>
        <label className="field"><span>每页条数</span><Select aria-label="每页消息数" value={query.count} disabled={unavailable} onChange={(event) => setQuery({ ...query, count: Number(event.target.value), cursors: [null] })}>{[50, 100, 250, 500].map((count) => <option key={count} value={count}>{count}</option>)}</Select></label>
      </div>
    </div>
    {error ? <Toast kind="error" message={error} onClose={() => setError(null)} resetKey={errorToken} /> : null}
    {notice ? <Toast kind="success" message={notice} onClose={() => setNotice(null)} resetKey={noticeToken} /> : null}

    <div className="stream-message-list">
      <div className="stream-table-wrap">
        <table className="stream-groups-table stream-messages-table" aria-label="Stream 消息列表">
          <thead><tr><th className="stream-check-column" aria-label="选择" /><th className="stream-id-column">消息 ID</th><th>字段与值</th></tr></thead>
          <tbody>
            {page?.entries.map((entry) => <tr key={entry.id} data-selected={selected.includes(entry.id)}>
              <td><input autoCapitalize="off" autoCorrect="off" type="checkbox" aria-label={`选择消息 ${entry.id}`} disabled={unavailable} checked={selected.includes(entry.id)} onChange={() => setSelected((previous) => previous.includes(entry.id) ? previous.filter((id) => id !== entry.id) : [...previous, entry.id])} /></td>
              <td><code>{entry.id}</code></td>
              <td><dl className="stream-entry-fields">{entry.fields.map(({ field, value }, index) => <div key={index}><dt>{field || <span className="stream-hint">空字段</span>}</dt><dd>{value || <span className="stream-hint">空字符串</span>}</dd></div>)}</dl></td>
            </tr>)}
            {loading || !page?.entries.length ? <tr><td colSpan={3}><p className="stream-empty-state">{loading ? "正在加载消息…" : error ? "消息未能加载，请刷新重试。" : "当前范围没有消息。"}</p></td></tr> : null}
          </tbody>
        </table>
      </div>
      <div className="stream-message-footer">
        <div className="stream-action-row">
          <span className="stream-selection-count">已选 {selected.length} 条</span>
          <button type="button" className="button button-danger" disabled={unavailable || selected.length === 0} onClick={() => void mutate("delete")}>删除选中消息</button>
        </div>
        <div className="stream-pagination">
          <span>第 {query.cursors.length} 页 · 本页 {page?.entries.length ?? 0} 条</span>
          <div className="stream-action-row">
            <button type="button" className="button button-secondary" aria-label="上一页消息" disabled={unavailable || query.cursors.length === 1} onClick={() => setQuery({ ...query, cursors: query.cursors.slice(0, -1) })}>上一页</button>
            <button type="button" className="button button-secondary" aria-label="下一页消息" disabled={unavailable || !page?.has_more || !page.next_cursor} onClick={() => { if (page?.next_cursor) setQuery({ ...query, cursors: [...query.cursors, page.next_cursor] }); }}>下一页</button>
          </div>
        </div>
      </div>
    </div>

    <section className="stream-composer" aria-label="新增 Stream 消息">
      <div className="stream-section-heading">
        <div><h4>新增消息</h4><p className="stream-hint">使用 * 自动生成 ID，字段按字符串键值对填写。</p></div>
      </div>
      <div className="stream-composer-fields">
        <label className="field"><span>新增消息 ID</span><input autoCapitalize="off" autoCorrect="off" aria-label="新增消息 ID" value={entryId} disabled={unavailable} onChange={(event) => setEntryId(event.target.value)} /><small className="stream-hint">自动 ID：* 或完整的 毫秒-序号</small></label>
        <label className="field"><span>字段 JSON：[["字段", "值"]]</span><textarea autoCapitalize="off" autoCorrect="off" aria-label="消息字段 JSON" spellCheck={false} rows={4} value={fieldsJson} disabled={unavailable} onChange={(event) => setFieldsJson(event.target.value)} /></label>
      </div>
      <div className="stream-composer-footer"><span className="stream-hint">最多 500 组字段，总大小不超过 4 MiB。</span><button type="button" className="button button-primary" disabled={unavailable} onClick={() => void mutate("add")}>添加消息</button></div>
    </section>
    <p className="stream-hint">分页结果会随数据变化；新增消息可能位于当前范围或分页之外。</p>
  </section>;
}

export default StreamEntries;
