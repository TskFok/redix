import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";

import {
  aggregateArray,
  appendArrayElements,
  deleteArrayElements,
  deleteArrayRange,
  getArrayRange,
  getArraySummary,
  searchArray,
  setArrayElement,
} from "../../lib/tauri";
import type {
  ArrayAggregateOperation,
  ArraySummary,
} from "../../lib/types";
import { browserErrorMessage } from "./browserState";
import {
  applyArrayAggregate,
  applyArrayRange,
  applyArraySearch,
  createArrayDetailState,
  isCurrentArrayRequest,
  type ArrayDetailState,
  type ArrayRequestToken,
} from "./arrayState";

interface ArrayDetailsProps {
  connectionId: string;
  keyName: string;
  initialSummary?: ArraySummary;
  disabled?: boolean;
  onBusyChange?: (busy: boolean) => void;
}

type ArrayTab = "view" | "search" | "aggregate";
type SearchCriteria = "EXACT" | "GLOB" | "PREFIX" | "SUFFIX";

const ARRAY_PAGE_SIZE = 500;

function isIndex(value: string): boolean {
  return /^\d{1,20}$/.test(value.trim());
}

function formatJson(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

function errorMessage(error: unknown, fallback: string): string {
  return browserErrorMessage(error, fallback);
}

function ArraySummaryCards({ summary }: { summary: ArraySummary | null }) {
  const cards = [
    ["长度", summary?.length ?? "—"],
    ["已占用槽位", summary?.count ?? "—"],
    ["下一个索引", summary?.next_index ?? "—"],
  ];
  return (
    <div className="module-summary-grid" aria-label="Array 摘要">
      {cards.map(([label, value]) => (
        <div className="module-summary-card" key={label}>
          <span>{label}</span>
          <strong>{value}</strong>
        </div>
      ))}
    </div>
  );
}

export function ArrayDetails({
  connectionId,
  keyName,
  initialSummary,
  disabled = false,
  onBusyChange,
}: ArrayDetailsProps) {
  const [state, setState] = useState<ArrayDetailState>(() =>
    createArrayDetailState(initialSummary ?? null),
  );
  const [busy, setBusy] = useState(false);
  const [editingIndex, setEditingIndex] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState("");
  const [writeIndex, setWriteIndex] = useState("");
  const [writeValue, setWriteValue] = useState("");
  const [appendText, setAppendText] = useState("");
  const [searchCriteria, setSearchCriteria] = useState<SearchCriteria>("EXACT");
  const [searchValue, setSearchValue] = useState("");
  const [searchStart, setSearchStart] = useState("");
  const [searchEnd, setSearchEnd] = useState("");
  const [searchNoCase, setSearchNoCase] = useState(false);
  const [searchWithValues, setSearchWithValues] = useState(true);
  const [searchLimit, setSearchLimit] = useState("100");
  const [aggregateOperation, setAggregateOperation] =
    useState<ArrayAggregateOperation>("SUM");
  const [aggregateStart, setAggregateStart] = useState("");
  const [aggregateEnd, setAggregateEnd] = useState("");
  const [aggregateMatch, setAggregateMatch] = useState("");
  const [aggregateLimit, setAggregateLimit] = useState("100");
  const requestRef = useRef<ArrayRequestToken | null>(null);
  const requestIdRef = useRef(0);

  const beginRequest = (tab: ArrayTab = state.activeTab): ArrayRequestToken => {
    const token: ArrayRequestToken = {
      connectionId,
      key: keyName,
      requestId: requestIdRef.current + 1,
    };
    requestIdRef.current = token.requestId;
    requestRef.current = token;
    setBusy(true);
    setState((current) => ({ ...current, activeTab: tab, loading: true, error: null }));
    return token;
  };

  const isCurrent = (token: ArrayRequestToken) =>
    isCurrentArrayRequest(requestRef.current, token);

  const finishRequest = (token: ArrayRequestToken) => {
    if (isCurrent(token)) {
      setBusy(false);
      setState((current) => ({ ...current, loading: false }));
    }
  };

  const loadViewData = async (
    token: ArrayRequestToken,
    start: string,
    end: string,
  ) => {
    const [summary, range] = await Promise.all([
      getArraySummary({ connection_id: token.connectionId, key: token.key }),
      getArrayRange({
        connection_id: token.connectionId,
        key: token.key,
        start,
        end,
      }),
    ]);
    if (!isCurrent(token)) {
      return;
    }
    setState((current) => applyArrayRange({ ...current, summary }, range));
  };

  const runRange = async (start = state.rangeStart, end = state.rangeEnd) => {
    const normalizedStart = start.trim();
    const normalizedEnd = end.trim();
    if (!isIndex(normalizedStart) || !isIndex(normalizedEnd)) {
      setState((current) => ({
        ...current,
        error: "范围索引必须是 0 到 20 位的非负整数。",
      }));
      return;
    }
    setState((current) => ({
      ...current,
      rangeStart: normalizedStart,
      rangeEnd: normalizedEnd,
    }));
    const token = beginRequest("view");
    try {
      await loadViewData(token, normalizedStart, normalizedEnd);
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: errorMessage(caught, "读取 Array 范围失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  useEffect(() => {
    setState(createArrayDetailState(initialSummary ?? null));
    const token: ArrayRequestToken = {
      connectionId,
      key: keyName,
      requestId: requestIdRef.current + 1,
    };
    requestIdRef.current = token.requestId;
    requestRef.current = token;
    setBusy(true);
    setState((current) => ({ ...current, loading: true }));
    void loadViewData(token, "0", "499")
      .catch((caught) => {
        if (isCurrent(token)) {
          setState((current) => ({
            ...current,
            loading: false,
            error: errorMessage(caught, "读取 Array 失败，请稍后重试。"),
          }));
        }
      })
      .finally(() => finishRequest(token));
    return () => {
      requestIdRef.current += 1;
      requestRef.current = null;
    };
    // The initial summary is a seed only. A key/connection change starts a new request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId, keyName]);

  useEffect(() => {
    onBusyChange?.(busy);
  }, [busy, onBusyChange]);

  const runMutation = async (operation: () => Promise<unknown>) => {
    const token = beginRequest("view");
    try {
      await operation();
      await loadViewData(token, state.rangeStart, state.rangeEnd);
      if (isCurrent(token)) {
        setEditingIndex(null);
        setWriteIndex("");
        setWriteValue("");
        setAppendText("");
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: errorMessage(caught, "修改 Array 失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleWrite = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const index = writeIndex.trim();
    if (!isIndex(index) || writeValue.length === 0) {
      setState((current) => ({ ...current, error: "索引必须为非负整数，值不能为空。" }));
      return;
    }
    await runMutation(() =>
      setArrayElement({
        connection_id: connectionId,
        key: keyName,
        index,
        value: writeValue,
      }),
    );
  };

  const handleAppend = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const values = appendText.split("\n").map((value) => value.trim()).filter(Boolean);
    if (values.length === 0) {
      setState((current) => ({ ...current, error: "至少填写一个要追加的值。" }));
      return;
    }
    await runMutation(() =>
      appendArrayElements({ connection_id: connectionId, key: keyName, values }),
    );
  };

  const handleSaveCell = async (index: string) => {
    if (editingValue.length === 0) {
      setState((current) => ({ ...current, error: "值不能为空；删除槽位请使用删除操作。" }));
      return;
    }
    await runMutation(() =>
      setArrayElement({
        connection_id: connectionId,
        key: keyName,
        index,
        value: editingValue,
      }),
    );
  };

  const handleDeleteCell = async (index: string) => {
    if (!window.confirm(`确定删除 Array 索引“${index}”吗？`)) {
      return;
    }
    await runMutation(() =>
      deleteArrayElements({ connection_id: connectionId, key: keyName, indices: [index] }),
    );
  };

  const handleDeleteRange = async () => {
    if (!window.confirm(`确定删除索引 ${state.rangeStart} 到 ${state.rangeEnd} 吗？`)) {
      return;
    }
    await runMutation(() =>
      deleteArrayRange({
        connection_id: connectionId,
        key: keyName,
        start: state.rangeStart,
        end: state.rangeEnd,
      }),
    );
  };

  const handleSearch = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = searchValue.trim();
    const limit = Number(searchLimit);
    if (value === "" || !Number.isInteger(limit) || limit < 1 || limit > ARRAY_PAGE_SIZE) {
      setState((current) => ({
        ...current,
        error: "搜索值不能为空，返回数量必须是 1 到 500。",
      }));
      return;
    }
    const token = beginRequest("search");
    try {
      const result = await searchArray({
        connection_id: connectionId,
        key: keyName,
        start: searchStart.trim() || null,
        end: searchEnd.trim() || null,
        predicates: [{ criteria: searchCriteria, value }],
        combinator: null,
        nocase: searchNoCase,
        with_values: searchWithValues,
        limit,
      });
      if (isCurrent(token)) {
        setState((current) => applyArraySearch(current, result));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: errorMessage(caught, "搜索 Array 失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleAggregate = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (aggregateOperation === "MATCH" && aggregateMatch.trim() === "") {
      setState((current) => ({ ...current, error: "MATCH 操作需要填写匹配值。" }));
      return;
    }
    const limit = Number(aggregateLimit);
    if (!Number.isInteger(limit) || limit < 1 || limit > ARRAY_PAGE_SIZE) {
      setState((current) => ({ ...current, error: "聚合范围上限必须是 1 到 500。" }));
      return;
    }
    const token = beginRequest("aggregate");
    try {
      const result = await aggregateArray({
        connection_id: connectionId,
        key: keyName,
        operation: aggregateOperation,
        start: aggregateStart.trim() || null,
        end: aggregateEnd.trim() || null,
        values: aggregateOperation === "MATCH" ? [aggregateMatch] : [],
        limit,
      });
      if (isCurrent(token)) {
        setState((current) => applyArrayAggregate(current, result));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: errorMessage(caught, "聚合 Array 失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const displayState = state.loading && state.cells.length === 0 ? "正在读取 Array…" : null;

  return (
    <section className="module-details array-details" aria-labelledby="array-details-title" aria-busy={busy || disabled}>
      <div className="module-details-heading">
        <div>
          <p className="eyebrow">REDIS MODULE</p>
          <h3 id="array-details-title">Array</h3>
        </div>
        <span className="module-detail-badge">AR*</span>
      </div>
      <ArraySummaryCards summary={state.summary} />
      <div className="module-tabs" role="tablist" aria-label="Array 视图">
        {([
          ["view", "浏览"],
          ["search", "搜索"],
          ["aggregate", "聚合"],
        ] as const).map(([tab, label]) => (
          <button
            key={tab}
            type="button"
            role="tab"
            aria-selected={state.activeTab === tab}
            className={`module-tab${state.activeTab === tab ? " module-tab-active" : ""}`}
            onClick={() => setState((current) => ({ ...current, activeTab: tab }))}
            disabled={busy || disabled}
          >
            {label}
          </button>
        ))}
      </div>

      {state.error ? <p className="feedback feedback-error" role="alert">{state.error}</p> : null}
      {displayState ? <p className="loading-state" role="status">{displayState}</p> : null}

      {state.activeTab === "view" ? (
        <div className="module-tab-content">
          <form className="module-toolbar" onSubmit={(event) => { event.preventDefault(); void runRange(); }}>
            <label className="field">
              <span>起始索引</span>
              <input autoCapitalize="off" autoCorrect="off" value={state.rangeStart} onChange={(event) => setState((current) => ({ ...current, rangeStart: event.target.value }))} disabled={busy || disabled} inputMode="numeric" />
            </label>
            <label className="field">
              <span>结束索引</span>
              <input autoCapitalize="off" autoCorrect="off" value={state.rangeEnd} onChange={(event) => setState((current) => ({ ...current, rangeEnd: event.target.value }))} disabled={busy || disabled} inputMode="numeric" />
            </label>
            <button type="submit" className="button button-primary" disabled={busy || disabled}>读取范围</button>
            <button type="button" className="button button-danger" onClick={() => void handleDeleteRange()} disabled={busy || disabled}>删除范围</button>
          </form>

          <div className="module-action-grid">
            <form className="module-action-card" onSubmit={(event) => void handleWrite(event)}>
              <div className="module-card-heading"><h4>写入元素</h4><span>ARSET</span></div>
              <div className="module-form-grid">
                <label className="field"><span>索引</span><input autoCapitalize="off" autoCorrect="off" value={writeIndex} onChange={(event) => setWriteIndex(event.target.value)} disabled={busy || disabled} inputMode="numeric" /></label>
                <label className="field"><span>值</span><input autoCapitalize="off" autoCorrect="off" value={writeValue} onChange={(event) => setWriteValue(event.target.value)} disabled={busy || disabled} /></label>
              </div>
              <button type="submit" className="button button-secondary" disabled={busy || disabled}>写入</button>
            </form>
            <form className="module-action-card" onSubmit={(event) => void handleAppend(event)}>
              <div className="module-card-heading"><h4>追加元素</h4><span>ARSET</span></div>
              <label className="field"><span>每行一个值</span><textarea autoCapitalize="off" autoCorrect="off" value={appendText} onChange={(event) => setAppendText(event.target.value)} disabled={busy || disabled} rows={2} /></label>
              <button type="submit" className="button button-secondary" disabled={busy || disabled}>追加</button>
            </form>
          </div>

          {state.cells.length > 0 ? (
            <div className="module-table-wrap">
              <table className="module-table">
                <caption>Array 索引 {state.rangeStart} 至 {state.rangeEnd}</caption>
                <thead><tr><th scope="col">索引</th><th scope="col">值</th><th scope="col">操作</th></tr></thead>
                <tbody>
                  {state.cells.map((cell) => {
                    const editing = editingIndex === cell.index;
                    return (
                      <tr key={cell.index}>
                        <td><code>{cell.index}</code></td>
                        <td>
                          {editing ? (
                            <input autoCapitalize="off" autoCorrect="off" aria-label={`编辑索引 ${cell.index}`} value={editingValue} onChange={(event) => setEditingValue(event.target.value)} disabled={busy || disabled} />
                          ) : cell.value === null ? (
                            <span className="module-empty-cell">空槽位</span>
                          ) : (
                            <code className="module-value" title={cell.value}>{cell.value}</code>
                          )}
                        </td>
                        <td>
                          {cell.value !== null ? (
                            <div className="module-row-actions">
                              {editing ? (
                                <>
                                  <button type="button" className="button button-quiet button-compact" onClick={() => void handleSaveCell(cell.index)} disabled={busy || disabled}>保存</button>
                                  <button type="button" className="button button-quiet button-compact" onClick={() => setEditingIndex(null)} disabled={busy || disabled}>取消</button>
                                </>
                              ) : (
                                <button type="button" className="button button-quiet button-compact" onClick={() => { setEditingIndex(cell.index); setEditingValue(cell.value ?? ""); }} disabled={busy || disabled}>编辑</button>
                              )}
                              <button type="button" className="button button-danger button-compact" onClick={() => void handleDeleteCell(cell.index)} disabled={busy || disabled}>删除</button>
                            </div>
                          ) : <span className="module-muted">不可操作</span>}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : !state.loading ? <p className="module-empty-state">当前范围没有元素。</p> : null}
        </div>
      ) : null}

      {state.activeTab === "search" ? (
        <div className="module-tab-content">
          <form className="module-query-form" onSubmit={(event) => void handleSearch(event)}>
            <div className="module-form-grid module-form-grid-wide">
              <label className="field"><span>匹配方式</span><Select value={searchCriteria} onChange={(event) => setSearchCriteria(event.target.value as SearchCriteria)} disabled={busy || disabled}><option value="EXACT">精确</option><option value="GLOB">Glob</option><option value="PREFIX">前缀</option><option value="SUFFIX">后缀</option></Select></label>
              <label className="field"><span>匹配值</span><input autoCapitalize="off" autoCorrect="off" value={searchValue} onChange={(event) => setSearchValue(event.target.value)} disabled={busy || disabled} /></label>
              <label className="field"><span>返回数量</span><input autoCapitalize="off" autoCorrect="off" type="number" min="1" max={ARRAY_PAGE_SIZE} value={searchLimit} onChange={(event) => setSearchLimit(event.target.value)} disabled={busy || disabled} /></label>
            </div>
            <div className="module-form-grid">
              <label className="field"><span>起始索引（可选）</span><input autoCapitalize="off" autoCorrect="off" value={searchStart} onChange={(event) => setSearchStart(event.target.value)} disabled={busy || disabled} /></label>
              <label className="field"><span>结束索引（可选）</span><input autoCapitalize="off" autoCorrect="off" value={searchEnd} onChange={(event) => setSearchEnd(event.target.value)} disabled={busy || disabled} /></label>
            </div>
            <div className="module-checkbox-row">
              <label><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={searchWithValues} onChange={(event) => setSearchWithValues(event.target.checked)} disabled={busy || disabled} />返回值</label>
              <label><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={searchNoCase} onChange={(event) => setSearchNoCase(event.target.checked)} disabled={busy || disabled} />忽略大小写</label>
              <button type="submit" className="button button-primary" disabled={busy || disabled}>执行搜索</button>
            </div>
          </form>
          {state.search ? (
            <div className="module-table-wrap">
              <table className="module-table"><caption>搜索结果：{state.search.total} 项</caption><thead><tr><th scope="col">索引</th><th scope="col">值</th></tr></thead><tbody>{state.search.elements.map((element) => <tr key={element.index}><td><code>{element.index}</code></td><td><code className="module-value">{searchWithValues ? element.value || "空值" : "未读取"}</code></td></tr>)}</tbody></table>
            </div>
          ) : <p className="module-empty-state">输入条件后搜索 Array 元素。</p>}
        </div>
      ) : null}

      {state.activeTab === "aggregate" ? (
        <div className="module-tab-content">
          <form className="module-query-form" onSubmit={(event) => void handleAggregate(event)}>
            <div className="module-form-grid module-form-grid-wide">
              <label className="field"><span>聚合操作</span><Select value={aggregateOperation} onChange={(event) => setAggregateOperation(event.target.value as ArrayAggregateOperation)} disabled={busy || disabled}><option value="SUM">SUM</option><option value="MIN">MIN</option><option value="MAX">MAX</option><option value="AND">AND</option><option value="OR">OR</option><option value="XOR">XOR</option><option value="MATCH">MATCH</option><option value="USED">USED</option></Select></label>
              <label className="field"><span>上限</span><input autoCapitalize="off" autoCorrect="off" type="number" min="1" max={ARRAY_PAGE_SIZE} value={aggregateLimit} onChange={(event) => setAggregateLimit(event.target.value)} disabled={busy || disabled} /></label>
              {aggregateOperation === "MATCH" ? <label className="field"><span>匹配值</span><input autoCapitalize="off" autoCorrect="off" value={aggregateMatch} onChange={(event) => setAggregateMatch(event.target.value)} disabled={busy || disabled} /></label> : null}
            </div>
            <div className="module-form-grid">
              <label className="field"><span>起始索引（可选）</span><input autoCapitalize="off" autoCorrect="off" value={aggregateStart} onChange={(event) => setAggregateStart(event.target.value)} disabled={busy || disabled} /></label>
              <label className="field"><span>结束索引（可选）</span><input autoCapitalize="off" autoCorrect="off" value={aggregateEnd} onChange={(event) => setAggregateEnd(event.target.value)} disabled={busy || disabled} /></label>
            </div>
            <button type="submit" className="button button-primary" disabled={busy || disabled}>执行聚合</button>
          </form>
          {state.aggregate ? <pre className="module-result" aria-label="Array 聚合结果">{formatJson(state.aggregate)}</pre> : <p className="module-empty-state">选择操作后查看聚合结果。</p>}
        </div>
      ) : null}
    </section>
  );
}

export default ArrayDetails;
