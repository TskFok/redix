import { useEffect, useRef, useState } from "react";
import Toast from "../../components/Toast";
import Select from "../../components/Select";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import { getCollectionPage, getListEntry, mutateCollection, type CollectionEntry, type CollectionKind, type CollectionMutation, type CollectionOrder, type CollectionPage } from "./collectionApi";
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

function integerInRange(value: string, min: bigint, max: bigint): string | null {
  if (value.length > 20 || !/^-?\d+$/.test(value)) return null;
  const number = BigInt(value);
  return number >= min && number <= max ? number.toString() : null;
}

function fieldTtlLabel(ttl: number | null) {
  if (ttl === null || ttl === undefined) return "不可用";
  if (ttl === -1) return "不过期";
  if (ttl === -2) return "字段已过期";
  return `${ttl} ms`;
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
  const [order, setOrder] = useState<CollectionOrder>(kind === "zset" ? "score_asc" : "scan");
  const [ttlTarget, setTtlTarget] = useState<CollectionEntry | null>(null);
  const [fieldTtl, setFieldTtl] = useState("60000");
  const [listIndex, setListIndex] = useState("0");
  const [indexResult, setIndexResult] = useState<{ index: string; entry: CollectionEntry | null } | null>(null);
  const [trimCount, setTrimCount] = useState("1");
  const { confirm, confirmationDialog } = useConfirmDialog(JSON.stringify([
    connectionId, keyName, kind, disabled, busy, cursor, pattern, appliedPattern,
    editing?.id, entryName, value, score, prepend, order, ttlTarget?.id, fieldTtl, listIndex, trimCount,
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

  const pageInput = (next: string, match: string, nextOrder = order) => ({
    connection_id: connectionId, key: keyName, kind, cursor: next, count: 100, pattern: match,
    ...(kind === "zset" ? { order: nextOrder } : {}),
  });

  const load = async (next: string, match: string, previous: string[], nextOrder = order) => {
    if (inFlight.current) return;
    const token = ++request.current;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await getCollectionPage(pageInput(next, match, nextOrder));
      if (!current(token)) return;
      setPage(result);
      setCursor(next);
      setHistory(previous);
      setAppliedPattern(match);
      setOrder(nextOrder);
      setEditing(null);
      setTtlTarget(null);
      setIndexResult(null);
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

  const write = async (mutation: CollectionMutation, confirmation: string, options?: Parameters<typeof confirm>[1]) => {
    if (blocked || inFlight.current) return;
    const previousRequest = request.current;
    const deleting = mutation.operation === "hash_delete" || mutation.operation === "set_remove" || mutation.operation === "zset_remove" || mutation.operation === "list_trim";
    const accepted = await confirm(confirmation, options ?? (deleting ? undefined : {
      title: editing ? "确认保存修改" : "确认添加一项",
      confirmLabel: editing ? "确认保存" : "确认添加",
      danger: false,
    }));
    if (!accepted || confirmationRef.current !== confirm || !current(previousRequest) || inFlight.current) return;
    const token = ++request.current;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      await mutateCollection({ connection_id: connectionId, key: keyName, mutation });
      if (!current(token)) return;
      setEditing(null); setEntryName(""); setValue(""); setPage(null); setTtlTarget(null); setIndexResult(null);
      callbacks.current.onChanged?.();
      // A mutation can invalidate SCAN cursors or shift list indexes.
      const refreshed = await getCollectionPage(pageInput("0", appliedPattern)).catch((caught: unknown) => {
        if (caught && typeof caught === "object" && "code" in caught && caught.code === "KEY_NOT_FOUND") {
          return { entries: [], next_cursor: "0", has_more: false, total: "0", ttl_ms: -2, hash_field_ttl_supported: page?.hash_field_ttl_supported ?? null } satisfies CollectionPage;
        }
        throw caught;
      });
      if (!current(token)) return;
      setPage(refreshed); setCursor("0"); setHistory([]);
    } catch (caught) {
      if (current(token)) {
        const ttlUnavailable = kind === "hash" && caught && typeof caught === "object" && "code" in caught && caught.code === "UNSUPPORTED_FEATURE";
        setError(ttlUnavailable ? "字段 TTL 操作不可用，请确认 Redis 版本至少为 7.4，且账号拥有 HPTTL、HPEXPIRE、HPERSIST 和 EVAL 所需权限。" : browserErrorMessage(caught, "操作或刷新失败，请刷新确认结果；不会自动重试写入。删除最后一项后键会消失。"));
      }
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

  const edit = (entry: CollectionEntry) => { setTtlTarget(null); setEditing(entry); setEntryName(entry.id); setValue(entry.value); setScore(String(entry.score ?? 0)); };
  const remove = (entry: CollectionEntry) => {
    const mutation: CollectionMutation = kind === "hash" ? { operation: "hash_delete", field: entry.id } : kind === "set" ? { operation: "set_remove", member: entry.id } : { operation: "zset_remove", member: entry.id };
    void write(mutation, "确认删除所选项？其他数据保持不变，删除最后一项后键会消失。");
  };

  const updateFieldTtl = () => {
    if (!ttlTarget || !page?.hash_field_ttl_supported) return;
    const ttl = integerInRange(fieldTtl, 1n, 3153600000000n);
    if (ttl === null) { setError("TTL 必须是 1 到 3153600000000 之间的整数毫秒（最多 36500 天）。"); return; }
    void write({ operation: "hash_expire", field: ttlTarget.id, ttl_ms: ttl }, `确认将字段「${ttlTarget.id}」的 TTL 设置为 ${ttl} 毫秒？到期后该字段会自动删除。`, {
      title: "确认设置字段 TTL", confirmLabel: "确认设置", danger: false,
    });
  };

  const trimList = (fromHead: boolean) => {
    const count = integerInRange(trimCount, 1n, 9223372036854775806n);
    if (count === null) { setError("删除数量必须是 1 到 9223372036854775806 之间的整数。"); return; }
    void write({ operation: "list_trim", count, from_head: fromHead }, `确认从 List「${keyName}」的${fromHead ? "头部" : "尾部"}删除 ${count} 项？数量超过长度时将清空整个 List，键也会删除。此操作不可撤销。`);
  };

  const lookupIndex = async () => {
    if (blocked || inFlight.current) return;
    const index = integerInRange(listIndex, -9223372036854775808n, 9223372036854775807n);
    if (index === null) { setError("索引必须是 -9223372036854775808 到 9223372036854775807 之间的整数。"); return; }
    const token = ++request.current;
    inFlight.current = true;
    setBusy(true); setError(null); setIndexResult(null); setEditing(null);
    try {
      const entry = await getListEntry({ connection_id: connectionId, key: keyName, index });
      if (current(token)) setIndexResult({ index, entry });
    } catch (caught) {
      if (current(token)) setError(browserErrorMessage(caught, "索引查询失败，请重试。"));
    } finally {
      if (current(token)) { inFlight.current = false; setBusy(false); }
    }
  };
  const typeName = { hash: "Hash", list: "List", set: "Set", zset: "Sorted Set" }[kind];

  return <section className="module-details collection-details" aria-label={`${typeName} 分页详情`}>
    {confirmationDialog}
    <div className="module-details-heading"><h3>{typeName}</h3><span className="form-help">总项数：{page?.total ?? "—"} · 当前页：{page?.entries.length ?? 0} 项</span></div>
    <p className="form-help">{kind === "list" ? "按索引分页。其他客户端修改 List 时索引可能移动，编辑前请刷新。" : kind === "zset" && order !== "scan" ? "按分数顺序每页浏览 100 项；同分成员按字典顺序排列，降序时一并反转。并发修改可能使分页位置移动，请刷新查看最新结果。" : "SCAN 每次请求 100 项；数量是提示，最多接受 2,000 项 / 4 MiB。空页也可能有下一页。并发修改时可能重复或遗漏，请刷新重新扫描。"}</p>
    {kind === "hash" && page?.hash_field_ttl_supported === false && <p className="form-help">当前连接无法使用字段 TTL（需要 Redis 7.4+ 及相应命令权限）。</p>}
    {kind === "zset" && <label className="field"><span>浏览顺序</span><Select aria-label="浏览顺序" value={order} disabled={blocked} onChange={(event) => {
      const nextOrder = event.target.value as CollectionOrder;
      if (!blocked) void load("0", "*", [], nextOrder);
    }}><option value="score_asc">分数升序</option><option value="score_desc">分数降序</option><option value="scan">成员匹配扫描</option></Select></label>}
    {kind !== "list" && (kind !== "zset" || order === "scan") && <form className="module-toolbar collection-search" onSubmit={(event) => { event.preventDefault(); if (!blocked && new TextEncoder().encode(pattern).length <= 4096) void load("0", pattern, []); }}>
      <label className="field"><span>{kind === "hash" ? "字段匹配模式" : "成员匹配模式"}</span><input autoCapitalize="off" autoCorrect="off" aria-label={kind === "hash" ? "字段匹配模式" : "成员匹配模式"} value={pattern} maxLength={4096} disabled={blocked} onChange={(event) => setPattern(event.target.value)} /></label>
      <button type="submit" className="button button-primary" disabled={blocked}>搜索</button>
    </form>}
    {kind === "list" && <div className="module-action-card">
      <form className="module-toolbar collection-search" onSubmit={(event) => { event.preventDefault(); void lookupIndex(); }}>
        <label className="field"><span>查询索引</span><input aria-label="查询索引" value={listIndex} maxLength={20} disabled={blocked} autoComplete="off" onChange={(event) => { setListIndex(event.target.value); setIndexResult(null); }} /></label>
        <button type="submit" className="button button-secondary" disabled={blocked}>查询索引</button>
      </form>
      <p className="form-help">索引从 0 开始，-1 表示尾项，-2 表示倒数第二项。</p>
      {indexResult && <div className="module-tab-content" aria-label="索引查询结果">
        {indexResult.entry ? <>
          <span className="form-help">查询 {indexResult.index} · 绝对索引 {indexResult.entry.id}</span>
          <code className="module-value">{indexResult.entry.value}</code>
          <div className="module-row-actions"><button type="button" className="button button-quiet" disabled={blocked} aria-label={`编辑查询结果 ${indexResult.entry.id}`} onClick={() => indexResult.entry && edit(indexResult.entry)}>编辑查询结果</button></div>
        </> : <p className="module-empty-state">该索引不存在：{indexResult.index}。</p>}
      </div>}
    </div>}
    {error && <Toast kind="error" message={error} onClose={() => setError(null)} resetKey={errorToken} />}
    {busy && <p role="status" className="loading-state">正在处理集合数据…</p>}
    <div className="module-table-wrap"><table className={`module-table browser-action-table${kind === "hash" ? " collection-table-hash" : ""}`}><thead><tr>
      {(kind === "hash" || kind === "list") && <th scope="col">{kind === "hash" ? "字段" : "索引"}</th>}
      <th scope="col">{kind === "set" || kind === "zset" ? "成员" : "值"}</th>{kind === "zset" && <th scope="col">分数</th>}{kind === "hash" && <th scope="col">字段 TTL</th>}<th scope="col">操作</th>
    </tr></thead><tbody>{page?.entries.map((entry, index) => <tr key={`${index}:${entry.id}`}>
      {(kind === "hash" || kind === "list") && <td><code className="module-value">{entry.id}</code></td>}
      <td><code className="module-value">{entry.value}</code></td>{kind === "zset" && <td><code>{entry.score}</code></td>}
      {kind === "hash" && <td><code>{fieldTtlLabel(entry.ttl_ms)}</code></td>}
      <td><div className="module-row-actions">{kind !== "set" && <button type="button" className="button button-quiet" disabled={blocked} aria-label={`编辑 ${entry.id}`} onClick={() => edit(entry)}>编辑</button>}
        {kind === "hash" && <button type="button" className="button button-quiet" disabled={blocked || !page?.hash_field_ttl_supported || entry.ttl_ms === -2} aria-label={`设置字段 TTL ${entry.id}`} onClick={() => { setEditing(null); setTtlTarget(entry); setFieldTtl(entry.ttl_ms !== null && entry.ttl_ms > 0 ? String(entry.ttl_ms) : "60000"); }}>TTL</button>}
        {kind !== "list" && <button type="button" className="button button-danger" disabled={blocked} aria-label={`删除 ${entry.id}`} onClick={() => remove(entry)}>删除</button>}</div></td>
    </tr>)}</tbody></table></div>
    {page && page.entries.length === 0 && <p className="module-empty-state">{page.ttl_ms === -2 ? "键已不存在。" : "本页没有匹配项。"}</p>}
    <div className="module-row-actions">
      <button type="button" className="button button-secondary" disabled={blocked || cursor === "0"} onClick={() => void load("0", appliedPattern, [])}>首页</button>
      <button type="button" className="button button-secondary" disabled={blocked || history.length === 0} onClick={() => void load(history[history.length - 1], appliedPattern, history.slice(0, -1))}>上一页</button>
      <button type="button" className="button button-secondary" disabled={blocked || !page?.has_more} onClick={() => page && void load(page.next_cursor, appliedPattern, [...history, cursor].slice(-100))}>下一页</button>
      <button type="button" className="button button-secondary" disabled={blocked} onClick={() => void load("0", appliedPattern, [])}>刷新</button>
    </div>
    {kind === "hash" && ttlTarget && <form className="module-action-card" onSubmit={(event) => { event.preventDefault(); updateFieldTtl(); }}>
      <div className="module-card-heading"><h4>字段 TTL · {ttlTarget.id}</h4></div>
      <p className="form-help">当前字段 TTL：{fieldTtlLabel(ttlTarget.ttl_ms)}。字段到期后自动删除；整个键的过期时间仍然生效。</p>
      <label className="field"><span>字段 TTL（毫秒）</span><input aria-label="字段 TTL（毫秒）" inputMode="numeric" maxLength={16} value={fieldTtl} disabled={blocked} onChange={(event) => setFieldTtl(event.target.value)} /></label>
      <div className="module-row-actions">
        <button type="submit" className="button button-primary" disabled={blocked}>设置 TTL</button>
        <button type="button" className="button button-secondary" disabled={blocked || ttlTarget.ttl_ms === -1} onClick={() => void write({ operation: "hash_persist", field: ttlTarget.id }, `确认移除字段「${ttlTarget.id}」的 TTL？字段将不再单独过期。`, { title: "确认移除字段 TTL", confirmLabel: "确认移除", danger: false })}>移除 TTL</button>
        <button type="button" className="button button-secondary" disabled={blocked} onClick={() => setTtlTarget(null)}>取消 TTL 编辑</button>
      </div>
    </form>}
    {kind === "list" && <div className="module-action-card">
      <div className="module-card-heading"><h4>头尾批量删除</h4></div>
      <label className="field"><span>删除数量</span><input aria-label="删除数量" inputMode="numeric" maxLength={19} value={trimCount} disabled={blocked} onChange={(event) => setTrimCount(event.target.value)} /></label>
      <div className="module-row-actions">
        <button type="button" className="button button-danger" disabled={blocked || !page || page.total === "0"} onClick={() => trimList(true)}>从头部删除</button>
        <button type="button" className="button button-danger" disabled={blocked || !page || page.total === "0"} onClick={() => trimList(false)}>从尾部删除</button>
      </div>
    </div>}
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
