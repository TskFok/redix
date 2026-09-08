import { useEffect, useRef, useState } from "react";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import { getCollectionPage, mutateCollection, type CollectionEntry, type CollectionKind, type CollectionMutation, type CollectionPage } from "./collectionApi";
import { browserErrorMessage } from "./browserState";

export interface CollectionDetailsProps {
  connectionId: string;
  keyName: string;
  kind: CollectionKind;
  disabled?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onChanged?: () => void;
}

export function CollectionDetails(props: CollectionDetailsProps) {
  return <CollectionDetailsSession key={JSON.stringify([props.connectionId, props.keyName, props.kind])} {...props} />;
}

function CollectionDetailsSession({ connectionId, keyName, kind, disabled = false, onBusyChange, onChanged }: CollectionDetailsProps) {
  const [page, setPage] = useState<CollectionPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState<string | null>(null);
  const [cursor, setCursor] = useState("0");
  const [history, setHistory] = useState<string[]>([]);
  const [pattern, setPattern] = useState("*");
  const [appliedPattern, setAppliedPattern] = useState("*");
  const [editing, setEditing] = useState<CollectionEntry | null>(null);
  const [entryName, setEntryName] = useState("");
  const [value, setValue] = useState("");
  const [score, setScore] = useState("0");
  const [prepend, setPrepend] = useState(false);
  const { confirm, confirmationDialog } = useConfirmDialog(JSON.stringify([
    connectionId, keyName, kind, disabled, busy, cursor, pattern, appliedPattern,
    editing?.id, entryName, value, score, prepend,
  ]));
  const confirmationRef = useRef(confirm);
  confirmationRef.current = confirm;
  const request = useRef(0);
  const mounted = useRef(false);
  const inFlight = useRef(false);
  const callbacks = useRef({ onBusyChange, onChanged });
  callbacks.current = { onBusyChange, onChanged };
  const blocked = busy || disabled;
  const current = (token: number) => mounted.current && request.current === token;

  const load = async (next: string, match: string, previous: string[]) => {
    if (inFlight.current) return;
    const token = ++request.current;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await getCollectionPage({ connection_id: connectionId, key: keyName, kind, cursor: next, count: 100, pattern: match });
      if (!current(token)) return;
      setPage(result);
      setCursor(next);
      setHistory(previous);
      setAppliedPattern(match);
      setEditing(null);
    } catch (caught) {
      if (current(token)) setError(browserErrorMessage(caught, "集合读取失败；单页超过 2,000 条、4 MiB 或含非 UTF-8 数据时会拒绝返回。可缩小匹配范围后重试。"));
    } finally {
      if (current(token)) { inFlight.current = false; setBusy(false); }
    }
  };

  useEffect(() => {
    mounted.current = true;
    inFlight.current = false;
    void load("0", "*", []);
    return () => { mounted.current = false; request.current += 1; callbacks.current.onBusyChange?.(false); };
    // Session is remounted whenever connection, key or kind changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => { callbacks.current.onBusyChange?.(busy); }, [busy]);

  const write = async (mutation: CollectionMutation, confirmation: string) => {
    if (blocked || inFlight.current) return;
    const previousRequest = request.current;
    const deleting = mutation.operation === "hash_delete" || mutation.operation === "set_remove" || mutation.operation === "zset_remove";
    const accepted = await confirm(confirmation, deleting ? undefined : {
      title: editing ? "确认保存修改" : "确认添加一项",
      confirmLabel: editing ? "确认保存" : "确认添加",
      danger: false,
    });
    if (!accepted || confirmationRef.current !== confirm || !current(previousRequest) || inFlight.current) return;
    const token = ++request.current;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      await mutateCollection({ connection_id: connectionId, key: keyName, mutation });
      if (!current(token)) return;
      setEditing(null); setEntryName(""); setValue(""); setPage(null);
      callbacks.current.onChanged?.();
      // A mutation can invalidate SCAN cursors or shift list indexes.
      const refreshed = await getCollectionPage({ connection_id: connectionId, key: keyName, kind, cursor: "0", count: 100, pattern: appliedPattern });
      if (!current(token)) return;
      setPage(refreshed); setCursor("0"); setHistory([]);
    } catch (caught) {
      if (current(token)) setError(browserErrorMessage(caught, "操作或刷新失败，请刷新确认结果；不会自动重试写入。删除最后一项后键会消失。"));
    } finally {
      if (current(token)) { inFlight.current = false; setBusy(false); }
    }
  };

  const save = () => {
    let mutation: CollectionMutation;
    if (kind === "hash") mutation = { operation: "hash_set", field: editing?.id ?? entryName, value };
    else if (kind === "set") mutation = { operation: "set_add", member: entryName };
    else if (kind === "zset") {
      if (!score.trim() || !Number.isFinite(Number(score))) { setError("分数必须是有限数值。"); return; }
      mutation = { operation: "zset_add", member: editing?.id ?? entryName, score: Number(score) };
    } else mutation = editing ? { operation: "list_set", index: editing.id, value } : { operation: "list_append", value, prepend };
    const fields = Object.values(mutation).filter((item): item is string => typeof item === "string");
    if (fields.some((item) => new TextEncoder().encode(item).length > 1024 * 1024)) { setError("单个字段或值不能超过 1 MiB。"); return; }
    void write(mutation, editing ? "确认保存此项修改？只更新这一项，其他数据保持不变。" : "确认添加这一项？Hash 同名字段或 ZSet 同名成员会被更新。");
  };

  const edit = (entry: CollectionEntry) => { setEditing(entry); setEntryName(entry.id); setValue(entry.value); setScore(String(entry.score ?? 0)); };
  const remove = (entry: CollectionEntry) => {
    const mutation: CollectionMutation = kind === "hash" ? { operation: "hash_delete", field: entry.id } : kind === "set" ? { operation: "set_remove", member: entry.id } : { operation: "zset_remove", member: entry.id };
    void write(mutation, "确认删除所选项？其他数据保持不变，删除最后一项后键会消失。");
  };
  const typeName = { hash: "Hash", list: "List", set: "Set", zset: "Sorted Set" }[kind];

  return <section className="module-details collection-details" aria-label={`${typeName} 分页详情`}>
    {confirmationDialog}
    <div className="module-details-heading"><h3>{typeName}</h3><span className="form-help">总项数：{page?.total ?? "—"} · 当前页：{page?.entries.length ?? 0} 项</span></div>
    <p className="form-help">{kind === "list" ? "按索引分页。其他客户端修改 List 时索引可能移动，编辑前请刷新。" : "SCAN 每次请求 100 项；数量是提示，最多接受 2,000 项 / 4 MiB。空页也可能有下一页。并发修改时可能重复或遗漏，请刷新重新扫描。"}</p>
    {kind !== "list" && <form className="module-toolbar collection-search" onSubmit={(event) => { event.preventDefault(); if (!blocked && new TextEncoder().encode(pattern).length <= 4096) void load("0", pattern, []); }}>
      <label className="field"><span>{kind === "hash" ? "字段匹配模式" : "成员匹配模式"}</span><input autoCapitalize="off" autoCorrect="off" aria-label={kind === "hash" ? "字段匹配模式" : "成员匹配模式"} value={pattern} maxLength={4096} disabled={blocked} onChange={(event) => setPattern(event.target.value)} /></label>
      <button type="submit" className="button button-primary" disabled={blocked}>搜索</button>
    </form>}
    {error && <Toast kind="error" message={error} onClose={() => setError(null)} resetKey={errorToken} />}
    {busy && <p role="status" className="loading-state">正在处理集合数据…</p>}
    <div className="module-table-wrap"><table className="module-table"><thead><tr>
      {(kind === "hash" || kind === "list") && <th scope="col">{kind === "hash" ? "字段" : "索引"}</th>}
      <th scope="col">{kind === "set" || kind === "zset" ? "成员" : "值"}</th>{kind === "zset" && <th scope="col">分数</th>}<th scope="col">操作</th>
    </tr></thead><tbody>{page?.entries.map((entry, index) => <tr key={`${index}:${entry.id}`}>
      {(kind === "hash" || kind === "list") && <td><code className="module-value">{entry.id}</code></td>}
      <td><code className="module-value">{entry.value}</code></td>{kind === "zset" && <td><code>{entry.score}</code></td>}
      <td><div className="module-row-actions">{kind !== "set" && <button type="button" className="button button-quiet" disabled={blocked} aria-label={`编辑 ${entry.id}`} onClick={() => edit(entry)}>编辑</button>}
        {kind !== "list" && <button type="button" className="button button-danger" disabled={blocked} aria-label={`删除 ${entry.id}`} onClick={() => remove(entry)}>删除</button>}</div></td>
    </tr>)}</tbody></table></div>
    {page && page.entries.length === 0 && <p className="module-empty-state">本页没有匹配项。</p>}
    <div className="module-row-actions">
      <button type="button" className="button button-secondary" disabled={blocked || cursor === "0"} onClick={() => void load("0", appliedPattern, [])}>首页</button>
      <button type="button" className="button button-secondary" disabled={blocked || history.length === 0} onClick={() => void load(history[history.length - 1], appliedPattern, history.slice(0, -1))}>上一页</button>
      <button type="button" className="button button-secondary" disabled={blocked || !page?.has_more} onClick={() => page && void load(page.next_cursor, appliedPattern, [...history, cursor].slice(-100))}>下一页</button>
      <button type="button" className="button button-secondary" disabled={blocked} onClick={() => void load("0", appliedPattern, [])}>刷新</button>
    </div>
    <form className="module-action-card" onSubmit={(event) => { event.preventDefault(); save(); }}>
      <div className="module-card-heading"><h4>{editing ? `编辑 ${kind === "list" ? "索引" : "项"} ${editing.id}` : "添加一项"}</h4></div>
      <div className={kind === "zset" ? "module-form-grid" : "module-tab-content"}>
        {kind !== "list" && <label className="field"><span>{kind === "hash" ? "字段名" : "成员"}</span><input autoCapitalize="off" autoCorrect="off" aria-label={kind === "hash" ? "字段名" : "成员"} value={entryName} disabled={blocked || editing !== null} onChange={(event) => setEntryName(event.target.value)} /></label>}
        {(kind === "hash" || kind === "list") && <label className="field"><span>值</span><textarea autoCapitalize="off" autoCorrect="off" aria-label="值" value={value} disabled={blocked} onChange={(event) => setValue(event.target.value)} /></label>}
        {kind === "zset" && <label className="field"><span>分数</span><input autoCapitalize="off" autoCorrect="off" aria-label="分数" value={score} disabled={blocked} onChange={(event) => setScore(event.target.value)} /></label>}
      </div>
      {kind === "list" && !editing && <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={prepend} disabled={blocked} onChange={(event) => setPrepend(event.target.checked)} />添加到头部（默认尾部）</label>}
      <div className="module-row-actions">
        <button type="submit" className="button button-primary" disabled={blocked}>{editing ? "保存此项" : "添加"}</button>
        {editing && <button type="button" className="button button-secondary" disabled={blocked} onClick={() => { setEditing(null); setEntryName(""); setValue(""); }}>取消编辑</button>}
      </div>
    </form>
  </section>;
}
export default CollectionDetails;
