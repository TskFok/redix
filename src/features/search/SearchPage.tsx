import { useCallback, useEffect, useRef, useState } from "react";

import {
  createSearchIndex,
  deleteSearchIndex,
  getModuleCapabilities,
  getSearchIndex,
  listSearchIndexes,
  searchKeys,
} from "../../lib/tauri";
import type {
  CreateSearchIndexInput,
  SearchFieldType,
  SearchIndexInfo,
  SearchQueryResult,
  SearchKeyType,
} from "../../lib/types";
import {
  nextSearchOffset,
  replaceSearchResults,
  resetSearchState,
  searchCapabilityState,
  searchErrorMessage,
  type SearchCapabilityState,
  type SearchProbeState,
  type SearchState,
} from "./searchState";

interface SearchPageProps {
  connectionId: string;
}

interface SearchFieldDraft {
  name: string;
  field_type: SearchFieldType;
}

interface SearchCreateDraft {
  index: string;
  key_type: SearchKeyType;
  prefixes: string;
  fields: SearchFieldDraft[];
}

const SEARCH_PAGE_SIZE = 100;
const fieldTypeOptions: Array<{ value: SearchFieldType; label: string }> = [
  { value: "text", label: "TEXT" },
  { value: "tag", label: "TAG" },
  { value: "numeric", label: "NUMERIC" },
  { value: "geo", label: "GEO" },
  { value: "geoshape", label: "GEOSHAPE" },
  { value: "vector", label: "VECTOR" },
];

function newCreateDraft(): SearchCreateDraft {
  return {
    index: "",
    key_type: "hash",
    prefixes: "",
    fields: [{ name: "", field_type: "text" }],
  };
}

function formatCount(value: number | null): string {
  return value === null ? "不可用" : value.toLocaleString("zh-CN");
}

function formatBytes(value: number | null): string {
  if (value === null) {
    return "不可用";
  }
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function CapabilityNotice({ state }: { state: SearchCapabilityState }) {
  if (state.status === "loading") {
    return <p role="status">正在检测 RedisSearch 模块…</p>;
  }

  if (state.status === "ready") {
    return null;
  }

  return (
    <div className="empty-state search-unavailable" role={state.status === "failed" ? "alert" : "status"}>
      <div className="empty-state-mark" aria-hidden="true">FT</div>
      <h2>{state.message}</h2>
      <p>请安装 RedisSearch 2.0 或更高版本后重试。</p>
    </div>
  );
}

function IndexInfo({ info }: { info: SearchIndexInfo | null }) {
  if (!info) {
    return <p className="empty-state-compact">请选择索引查看详情。</p>;
  }

  return (
    <div className="search-index-info">
      <dl className="detail-info-grid" aria-label="索引统计">
        <div>
          <dt>键类型</dt>
          <dd>{info.key_type}</dd>
        </div>
        <div>
          <dt>文档数</dt>
          <dd>{formatCount(info.num_docs)}</dd>
        </div>
        <div>
          <dt>记录数</dt>
          <dd>{formatCount(info.num_records)}</dd>
        </div>
        <div>
          <dt>索引内存</dt>
          <dd>{formatBytes(info.total_index_memory_bytes)}</dd>
        </div>
      </dl>
      <div className="search-index-summary">
        <span>键前缀</span>
        <code>{info.prefixes.length > 0 ? info.prefixes.join(", ") : "全部键"}</code>
      </div>
      <div className="search-attributes" aria-label="索引字段">
        <p className="search-section-label">索引字段</p>
        {info.attributes.length > 0 ? (
          <ul>
            {info.attributes.map((attribute) => (
              <li key={`${attribute.identifier}:${attribute.field_type}`}>
                <span>字段 {attribute.identifier}</span>
                <span>{attribute.field_type}</span>
                {attribute.sortable ? <small>可排序</small> : null}
                {attribute.no_index ? <small>仅存储</small> : null}
              </li>
            ))}
          </ul>
        ) : (
          <p className="empty-state-compact">未读取到字段定义。</p>
        )}
      </div>
    </div>
  );
}

function SearchResults({ result, onNext, busy }: {
  result: SearchQueryResult | null;
  onNext: () => void;
  busy: boolean;
}) {
  if (!result) {
    return <p className="empty-state-compact">输入查询语句后查看匹配键。</p>;
  }

  const nextOffset = nextSearchOffset(result.total, result.offset, result.keys.length);
  const pageSummary = result.keys.length > 0
    ? `${result.offset + 1}–${result.offset + result.keys.length}`
    : "无结果";
  return (
    <>
      <div className="search-result-summary">
        <span>匹配 {formatCount(result.total)} 个文档</span>
        <span>当前页 {pageSummary}</span>
      </div>
      {result.keys.length > 0 ? (
        <div className="database-table-wrap">
          <table className="database-table search-results-table">
            <thead>
              <tr>
                <th scope="col">键名</th>
                <th scope="col">类型</th>
              </tr>
            </thead>
            <tbody>
              {result.keys.map((item) => (
                <tr key={item.key}>
                  <th scope="row"><code>{item.key}</code></th>
                  <td>{item.key_type}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="empty-state-compact">没有匹配的键。</p>
      )}
      <div className="search-pagination">
        <button
          type="button"
          className="button button-secondary"
          onClick={onNext}
          disabled={busy || nextOffset === null}
        >
          下一页
        </button>
      </div>
    </>
  );
}

export function SearchPage({ connectionId }: SearchPageProps) {
  const [probe, setProbe] = useState<SearchProbeState>({
    status: "loading",
    capabilities: null,
  });
  const [state, setState] = useState<SearchState>(() => resetSearchState());
  const [indexLoading, setIndexLoading] = useState(false);
  const [infoLoading, setInfoLoading] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [createDraft, setCreateDraft] = useState<SearchCreateDraft>(() => newCreateDraft());
  const [createError, setCreateError] = useState<string | null>(null);
  const [createBusy, setCreateBusy] = useState(false);
  const mountedRef = useRef(false);
  const connectionRequestRef = useRef(0);
  const infoRequestRef = useRef(0);
  const searchRequestRef = useRef(0);

  const capability = searchCapabilityState(probe);

  const loadIndexes = useCallback(async (requestId: number) => {
    setIndexLoading(true);
    try {
      const result = await listSearchIndexes(connectionId);
      if (!mountedRef.current || connectionRequestRef.current !== requestId) {
        return;
      }
      setState((current) => {
        const selectedIndex = current.selectedIndex && result.indexes.some(
          (index) => index.name === current.selectedIndex,
        )
          ? current.selectedIndex
          : result.indexes[0]?.name ?? null;
        const selectionChanged = selectedIndex !== current.selectedIndex;
        return {
          ...current,
          indexes: result.indexes,
          selectedIndex,
          info: selectionChanged ? null : current.info,
          result: selectionChanged ? null : current.result,
          offset: selectionChanged ? 0 : current.offset,
          error: null,
        };
      });
    } catch (caught) {
      if (mountedRef.current && connectionRequestRef.current === requestId) {
        setState((current) => ({
          ...current,
          error: searchErrorMessage(caught, "读取索引列表失败，请稍后重试。"),
        }));
      }
    } finally {
      if (connectionRequestRef.current === requestId) {
        setIndexLoading(false);
      }
    }
  }, [connectionId]);

  useEffect(() => {
    mountedRef.current = true;
    const requestId = connectionRequestRef.current + 1;
    connectionRequestRef.current = requestId;
    infoRequestRef.current += 1;
    searchRequestRef.current += 1;
    setProbe({ status: "loading", capabilities: null });
    setState(resetSearchState());
    setShowCreate(false);
    setCreateDraft(newCreateDraft());
    setCreateError(null);

    void getModuleCapabilities(connectionId)
      .then((capabilities) => {
        if (!mountedRef.current || connectionRequestRef.current !== requestId) {
          return;
        }
        setProbe({ status: "ready", capabilities });
        const nextCapability = searchCapabilityState({ status: "ready", capabilities });
        if (nextCapability.status === "ready") {
          void loadIndexes(requestId);
        }
      })
      .catch((caught) => {
        if (mountedRef.current && connectionRequestRef.current === requestId) {
          setProbe({ status: "failed", capabilities: null });
          setState((current) => ({
            ...current,
            error: searchErrorMessage(caught, "检测 RedisSearch 失败，请稍后重试。"),
          }));
        }
      });

    return () => {
      mountedRef.current = false;
      connectionRequestRef.current += 1;
      infoRequestRef.current += 1;
      searchRequestRef.current += 1;
    };
  }, [connectionId, loadIndexes]);

  useEffect(() => {
    const requestId = infoRequestRef.current + 1;
    infoRequestRef.current = requestId;
    if (capability.status !== "ready" || !state.selectedIndex) {
      setInfoLoading(false);
      setState((current) => (current.info === null ? current : { ...current, info: null }));
      return;
    }

    const index = state.selectedIndex;
    setInfoLoading(true);
    void getSearchIndex({ connection_id: connectionId, index })
      .then((info) => {
        if (
          mountedRef.current &&
          infoRequestRef.current === requestId
        ) {
          setState((current) => ({ ...current, info, error: null }));
        }
      })
      .catch((caught) => {
        if (mountedRef.current && infoRequestRef.current === requestId) {
          setState((current) => ({
            ...current,
            info: null,
            error: searchErrorMessage(caught, "读取索引详情失败，请稍后重试。"),
          }));
        }
      })
      .finally(() => {
        if (infoRequestRef.current === requestId) {
          setInfoLoading(false);
        }
      });
  }, [capability.status, connectionId, state.selectedIndex]);

  const runSearch = async (requestedOffset: number) => {
    if (capability.status !== "ready" || !state.selectedIndex) {
      return;
    }
    const query = state.query.trim();
    if (!query) {
      setState((current) => ({ ...current, error: "查询语句不能为空。" }));
      return;
    }

    const requestId = searchRequestRef.current + 1;
    searchRequestRef.current = requestId;
    const token = {
      connectionId,
      index: state.selectedIndex,
      requestId,
    };
    setState((current) => ({
      ...current,
      offset: requestedOffset,
      loading: true,
      error: null,
      requestToken: token,
    }));

    try {
      const result = await searchKeys({
        connection_id: connectionId,
        index: state.selectedIndex,
        query,
        offset: requestedOffset,
        limit: SEARCH_PAGE_SIZE,
      });
      if (
        mountedRef.current &&
        searchRequestRef.current === requestId
      ) {
        setState((current) => replaceSearchResults(current, result, token, token));
      }
    } catch (caught) {
      if (mountedRef.current && searchRequestRef.current === requestId) {
        setState((current) => ({
          ...current,
          loading: false,
          error: searchErrorMessage(caught, "执行搜索失败，请检查查询语句。"),
        }));
      }
    }
  };

  const handleCreate = async () => {
    const index = createDraft.index.trim();
    const fields = createDraft.fields
      .map((field) => ({ ...field, name: field.name.trim() }))
      .filter((field) => field.name.length > 0);
    if (!index) {
      setCreateError("索引名称不能为空。");
      return;
    }
    if (fields.length === 0) {
      setCreateError("至少添加一个索引字段。");
      return;
    }
    if (new Set(fields.map((field) => field.name)).size !== fields.length) {
      setCreateError("索引字段名称不能重复。");
      return;
    }

    const input: CreateSearchIndexInput = {
      connection_id: connectionId,
      index,
      key_type: createDraft.key_type,
      prefixes: createDraft.prefixes
        .split(",")
        .map((prefix) => prefix.trim())
        .filter(Boolean),
      fields,
    };
    setCreateBusy(true);
    setCreateError(null);
    try {
      await createSearchIndex(input);
      setShowCreate(false);
      setCreateDraft(newCreateDraft());
      await loadIndexes(connectionRequestRef.current);
    } catch (caught) {
      setCreateError(searchErrorMessage(caught, "创建索引失败，请检查索引定义。"));
    } finally {
      setCreateBusy(false);
    }
  };

  const handleDelete = async () => {
    if (!state.selectedIndex || !window.confirm(`确定删除索引“${state.selectedIndex}”吗？`)) {
      return;
    }
    const index = state.selectedIndex;
    setState((current) => ({ ...current, loading: true, error: null }));
    try {
      await deleteSearchIndex({ connection_id: connectionId, index });
      setState((current) => ({
        ...current,
        indexes: current.indexes.filter((item) => item.name !== index),
        selectedIndex: null,
        info: null,
        result: null,
        loading: false,
        requestToken: null,
      }));
      await loadIndexes(connectionRequestRef.current);
    } catch (caught) {
      setState((current) => ({
        ...current,
        loading: false,
        error: searchErrorMessage(caught, "删除索引失败，请稍后重试。"),
      }));
    }
  };

  const pageError = state.error ?? createError;
  const nextOffset = state.result
    ? nextSearchOffset(state.result.total, state.result.offset, state.result.keys.length)
    : null;

  return (
    <section className="search-page" aria-labelledby="search-query-page-title" aria-busy={indexLoading || infoLoading || state.loading || createBusy}>
      <div className="page-heading search-page-heading">
        <div>
          <p className="eyebrow">REDISEARCH / QUERY</p>
          <h2 id="search-query-page-title">RedisSearch / Query</h2>
          <p className="page-description">
            管理 RedisSearch 索引，使用 FT.SEARCH 查询 Hash 或 JSON 文档。查询仅返回键名，避免把大文档加载到界面。
          </p>
        </div>
        {capability.status === "ready" ? (
          <div className="page-heading-actions">
            <button type="button" className="button button-secondary" onClick={() => void loadIndexes(connectionRequestRef.current)} disabled={indexLoading || createBusy}>
              {indexLoading ? "刷新中…" : "刷新索引"}
            </button>
            <button type="button" className="button button-primary" onClick={() => { setShowCreate(true); setCreateError(null); }} disabled={createBusy}>
              新建索引
            </button>
          </div>
        ) : null}
      </div>

      <CapabilityNotice state={capability} />

      {capability.status === "ready" ? (
        <>
          {pageError ? <p className="inline-error" role="alert">{pageError}</p> : null}

          {showCreate ? (
            <section className="database-panel search-create-panel" aria-labelledby="search-create-title">
              <div className="database-panel-heading">
                <div>
                  <p className="eyebrow">FT.CREATE</p>
                  <h3 id="search-create-title">创建索引</h3>
                </div>
                <button type="button" className="button button-quiet" onClick={() => { setShowCreate(false); setCreateError(null); }} disabled={createBusy}>
                  取消
                </button>
              </div>
              <div className="form-grid search-create-grid">
                <label className="field">
                  <span>索引名称</span>
                  <input aria-label="索引名称" value={createDraft.index} onChange={(event) => setCreateDraft((current) => ({ ...current, index: event.target.value }))} disabled={createBusy} spellCheck={false} />
                </label>
                <label className="field">
                  <span>键类型</span>
                  <select aria-label="键类型" value={createDraft.key_type} onChange={(event) => setCreateDraft((current) => ({ ...current, key_type: event.target.value as SearchKeyType }))} disabled={createBusy}>
                    <option value="hash">HASH</option>
                    <option value="json">JSON</option>
                  </select>
                </label>
                <label className="field field-wide">
                  <span>键前缀</span>
                  <input aria-label="键前缀" value={createDraft.prefixes} onChange={(event) => setCreateDraft((current) => ({ ...current, prefixes: event.target.value }))} disabled={createBusy} placeholder="多个前缀用逗号分隔，可留空" spellCheck={false} />
                  <small>留空表示匹配该类型的全部键。</small>
                </label>
              </div>
              <fieldset className="editor-fieldset search-fields-fieldset">
                <legend>索引字段</legend>
                <div className="search-field-list">
                  {createDraft.fields.map((field, fieldIndex) => (
                    <div className="search-field-row" key={`field-${fieldIndex}`}>
                      <label className="field">
                        <span>字段 {fieldIndex + 1}</span>
                        <input aria-label={`字段 ${fieldIndex + 1}`} value={field.name} onChange={(event) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { ...item, name: event.target.value } : item) }))} disabled={createBusy} spellCheck={false} />
                      </label>
                      <label className="field">
                        <span>字段类型</span>
                        <select aria-label={`字段 ${fieldIndex + 1} 类型`} value={field.field_type} onChange={(event) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { ...item, field_type: event.target.value as SearchFieldType } : item) }))} disabled={createBusy}>
                          {fieldTypeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                        </select>
                      </label>
                      <button type="button" className="button button-quiet" onClick={() => setCreateDraft((current) => ({ ...current, fields: current.fields.length > 1 ? current.fields.filter((_, index) => index !== fieldIndex) : current.fields }))} disabled={createBusy || createDraft.fields.length === 1}>
                        删除字段
                      </button>
                    </div>
                  ))}
                </div>
                <button type="button" className="button button-secondary" onClick={() => setCreateDraft((current) => ({ ...current, fields: [...current.fields, { name: "", field_type: "text" }] }))} disabled={createBusy || createDraft.fields.length >= 64}>
                  添加字段
                </button>
              </fieldset>
              {createError ? <p className="inline-error" role="alert">{createError}</p> : null}
              <div className="form-actions">
                <button type="button" className="button button-primary" onClick={() => void handleCreate()} disabled={createBusy}>
                  {createBusy ? "创建中…" : "创建索引"}
                </button>
              </div>
            </section>
          ) : null}

          <div className="search-layout">
            <section className="database-panel search-index-panel" aria-labelledby="search-indexes-title">
              <div className="database-panel-heading">
                <div>
                  <p className="eyebrow">INDEXES</p>
                  <h3 id="search-indexes-title">索引列表</h3>
                </div>
                <span className="panel-hint">{state.indexes.length} 个索引</span>
              </div>
              {state.indexes.length > 0 ? (
                <label className="field">
                  <span>当前索引</span>
                  <select aria-label="当前索引" value={state.selectedIndex ?? ""} onChange={(event) => setState((current) => ({ ...current, selectedIndex: event.target.value || null, info: null, result: null, offset: 0, error: null }))} disabled={indexLoading || state.loading}>
                    {state.indexes.map((index) => <option key={index.name} value={index.name}>{index.name}</option>)}
                  </select>
                </label>
              ) : (
                <p className="empty-state-compact">还没有索引，请先新建一个索引。</p>
              )}
              {state.selectedIndex ? (
                <button type="button" className="button button-danger search-delete-button" onClick={() => void handleDelete()} disabled={state.loading || indexLoading}>
                  删除当前索引
                </button>
              ) : null}
              <IndexInfo info={state.info} />
            </section>

            <section className="database-panel search-query-panel" aria-labelledby="search-query-title">
              <div className="database-panel-heading">
                <div>
                  <p className="eyebrow">FT.SEARCH</p>
                  <h3 id="search-query-title">查询键</h3>
                </div>
                <span className="panel-hint">NOCONTENT · LIMIT {SEARCH_PAGE_SIZE}</span>
              </div>
              <div className="search-query-form">
                <label className="field">
                  <span>查询语句</span>
                  <input aria-label="查询语句" value={state.query} onChange={(event) => setState((current) => ({ ...current, query: event.target.value, error: null }))} onKeyDown={(event) => { if (event.key === "Enter") void runSearch(0); }} disabled={!state.selectedIndex || state.loading} spellCheck={false} />
                  <small>示例：*、@name:Alice、@age:[18 30]</small>
                </label>
                <button type="button" className="button button-primary" onClick={() => void runSearch(0)} disabled={!state.selectedIndex || state.loading}>
                  {state.loading ? "查询中…" : "查询"}
                </button>
              </div>
              <SearchResults result={state.result} busy={state.loading} onNext={() => { if (nextOffset !== null) void runSearch(nextOffset); }} />
            </section>
          </div>
        </>
      ) : null}
    </section>
  );
}

export default SearchPage;
