import Select from "../../components/Select";
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
  SearchVectorConfig,
} from "../../lib/types";
import {
  replaceSearchResults,
  resetSearchState,
  searchCapabilityState,
  searchErrorMessage,
  vectorSearchSupported,
  type SearchCapabilityState,
  type SearchProbeState,
  type SearchState,
} from "./searchState";
import { SearchDocumentTable } from "./SearchDocumentTable";
import { AggregatePanel } from "./AggregatePanel";
import { VectorSearchPanel } from "./VectorSearchPanel";
import { SearchQueryBuilder } from "./SearchQueryBuilder";
import { defaultSearchVectorConfig, validSearchVectorConfig, VectorIndexOptions } from "./VectorIndexOptions";
import "./searchDocuments.css";

interface SearchPageProps {
  connectionId: string;
}

interface SearchFieldDraft {
  name: string;
  alias?: string;
  field_type: SearchFieldType;
  vector?: SearchVectorConfig;
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

function SearchResults({ result, onNext, busy, includeContent }: {
  result: SearchQueryResult | null;
  onNext: () => void;
  busy: boolean;
  includeContent: boolean;
}) {
  if (!result) {
    return <p className="empty-state-compact">输入查询语句后查看匹配键。</p>;
  }

  const nextOffset = result.next_offset;
  const pageSummary = result.keys.length > 0
    ? `${result.offset + 1}–${result.offset + result.keys.length}`
    : "无结果";
  return (
    <>
      <div className="search-result-summary">
        <span>匹配 {formatCount(result.total)} 个文档</span>
        <span>当前页 {pageSummary}</span>
      </div>
      {result.keys.length > 0 && includeContent ? <SearchDocumentTable documents={result.keys} /> : result.keys.length > 0 ? (
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
  const [deleteBusy, setDeleteBusy] = useState(false);
  const mountedRef = useRef(false);
  const connectionRequestRef = useRef(0);
  const indexRequestRef = useRef(0);
  const infoRequestRef = useRef(0);
  const searchRequestRef = useRef(0);

  const capability = searchCapabilityState(probe);
  const vectorSupported = capability.status === "ready" && vectorSearchSupported(capability.capabilities.search_version);
  const mutationBusy = createBusy || deleteBusy;

  const changeQuery = (changes: Partial<Pick<SearchState, "query" | "includeContent" | "selectedIndex">>) => {
    if (mutationBusy) return;
    searchRequestRef.current += 1;
    setState((current) => ({ ...current, ...changes, result: null, offset: 0, loading: false, requestToken: null, error: null,
      info: "selectedIndex" in changes ? null : current.info }));
  };

  const loadIndexes = useCallback(async (requestId: number) => {
    const indexRequestId = ++indexRequestRef.current;
    const isCurrent = () => mountedRef.current
      && connectionRequestRef.current === requestId
      && indexRequestRef.current === indexRequestId;
    setIndexLoading(true);
    try {
      const result = await listSearchIndexes(connectionId);
      if (!isCurrent()) {
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
          loading: selectionChanged ? false : current.loading,
          requestToken: selectionChanged ? null : current.requestToken,
          error: null,
        };
      });
    } catch (caught) {
      if (isCurrent()) {
        setState((current) => ({
          ...current,
          error: searchErrorMessage(caught, "读取索引列表失败，请稍后重试。"),
        }));
      }
    } finally {
      if (isCurrent()) {
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
    setIndexLoading(false);
    setShowCreate(false);
    setCreateDraft(newCreateDraft());
    setCreateError(null);
    setCreateBusy(false);
    setDeleteBusy(false);

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
    if (mutationBusy || capability.status !== "ready" || !state.selectedIndex) {
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
        ...(state.includeContent ? { include_content: true } : {}),
      });
      if (
        mountedRef.current &&
        searchRequestRef.current === requestId
      ) {
        setState((current) => replaceSearchResults(current, result, token, token));
      }
    } catch (caught) {
      if (mountedRef.current && searchRequestRef.current === requestId) {
        setState((current) => current.requestToken?.requestId !== requestId || current.selectedIndex !== token.index ? current : ({
          ...current,
          loading: false,
          error: searchErrorMessage(caught, state.includeContent
            ? "执行搜索失败，请检查查询语句；文档内容过大或含非 UTF-8 字段时，请关闭文档内容后重试。"
            : "执行搜索失败，请检查查询语句。"),
        }));
      }
    }
  };

  const handleCreate = async () => {
    if (mutationBusy) return;
    const requestGeneration = connectionRequestRef.current;
    const index = createDraft.index.trim();
    const fields = createDraft.fields
      .map(({ alias, ...field }) => ({ ...field, name: field.name.trim(), ...(alias?.trim() ? { alias: alias.trim() } : {}) }))
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
    const aliases = fields.flatMap((field) => field.alias ? [field.alias] : []);
    if (aliases.some((alias) => !/^[A-Za-z0-9_]{1,256}$/.test(alias)) || new Set(aliases).size !== aliases.length
      || fields.some((field) => field.alias && fields.some((other) => other !== field && other.name === field.alias))) {
      setCreateError("查询别名只能使用字母、数字或下划线，不得重复或与其他字段名称冲突。");
      return;
    }
    if (fields.some((field) => field.field_type === "vector" && !/^[A-Za-z0-9_]{1,256}$/.test(field.alias || field.name))) {
      setCreateError("VECTOR 字段需要安全查询别名（如 embedding），才能在 KNN 查询面板执行。");
      return;
    }
    if (fields.some((field) => field.field_type === "vector" && (!vectorSupported || !validSearchVectorConfig(field.vector)))) {
      setCreateError(vectorSupported ? "向量配置无效，请检查维度和算法参数的范围。" : "VECTOR 索引需要 RedisSearch 2.4 或更高版本。");
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
      if (!mountedRef.current || connectionRequestRef.current !== requestGeneration) return;
      setShowCreate(false);
      setCreateDraft(newCreateDraft());
      await loadIndexes(requestGeneration);
    } catch (caught) {
      if (mountedRef.current && connectionRequestRef.current === requestGeneration) setCreateError(searchErrorMessage(caught, "创建索引失败，请检查索引定义。"));
    } finally {
      if (mountedRef.current && connectionRequestRef.current === requestGeneration) setCreateBusy(false);
    }
  };

  const handleDelete = async () => {
    if (mutationBusy || state.loading || !state.selectedIndex || !window.confirm(`确定删除索引“${state.selectedIndex}”吗？`)) {
      return;
    }
    const index = state.selectedIndex;
    const requestGeneration = connectionRequestRef.current;
    setDeleteBusy(true);
    setState((current) => ({ ...current, loading: true, error: null }));
    try {
      await deleteSearchIndex({ connection_id: connectionId, index });
      if (!mountedRef.current || connectionRequestRef.current !== requestGeneration) return;
      setState((current) => ({
        ...current,
        indexes: current.indexes.filter((item) => item.name !== index),
        selectedIndex: null,
        info: null,
        result: null,
        loading: false,
        requestToken: null,
      }));
      await loadIndexes(requestGeneration);
    } catch (caught) {
      if (!mountedRef.current || connectionRequestRef.current !== requestGeneration) return;
      setState((current) => ({
        ...current,
        loading: false,
        error: searchErrorMessage(caught, "删除索引失败，请稍后重试。"),
      }));
    } finally {
      if (mountedRef.current && connectionRequestRef.current === requestGeneration) setDeleteBusy(false);
    }
  };

  const pageError = state.error ?? createError;
  const nextOffset = state.result?.next_offset ?? null;

  return (
    <section className="search-page" aria-labelledby="search-query-page-title" aria-busy={indexLoading || infoLoading || state.loading || mutationBusy}>
      <div className="page-heading search-page-heading">
        <div>
          <p className="eyebrow">REDISEARCH / QUERY</p>
          <h2 id="search-query-page-title">RedisSearch / Query</h2>
          <p className="page-description">
            管理 RedisSearch 索引，使用 FT.SEARCH 查询 Hash 或 JSON 文档。默认仅返回键名，也可选择加载文档内容。
          </p>
        </div>
        {capability.status === "ready" ? (
          <div className="page-heading-actions">
            <button type="button" className="button button-secondary" onClick={() => void loadIndexes(connectionRequestRef.current)} disabled={indexLoading || mutationBusy}>
              {indexLoading ? "刷新中…" : "刷新索引"}
            </button>
            <button type="button" className="button button-primary" onClick={() => { setShowCreate(true); setCreateError(null); }} disabled={mutationBusy}>
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
                <button type="button" className="button button-quiet" onClick={() => { setShowCreate(false); setCreateError(null); }} disabled={mutationBusy}>
                  取消
                </button>
              </div>
              <div className="form-grid search-create-grid">
                <label className="field">
                  <span>索引名称</span>
                  <input aria-label="索引名称" value={createDraft.index} onChange={(event) => setCreateDraft((current) => ({ ...current, index: event.target.value }))} disabled={mutationBusy} spellCheck={false} />
                </label>
                <label className="field">
                  <span>键类型</span>
                  <Select aria-label="键类型" value={createDraft.key_type} onChange={(event) => setCreateDraft((current) => ({ ...current, key_type: event.target.value as SearchKeyType }))} disabled={mutationBusy}>
                    <option value="hash">HASH</option>
                    <option value="json">JSON</option>
                  </Select>
                </label>
                <label className="field field-wide">
                  <span>键前缀</span>
                  <input aria-label="键前缀" value={createDraft.prefixes} onChange={(event) => setCreateDraft((current) => ({ ...current, prefixes: event.target.value }))} disabled={mutationBusy} placeholder="多个前缀用逗号分隔，可留空" spellCheck={false} />
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
                        <input aria-label={`字段 ${fieldIndex + 1}`} value={field.name} onChange={(event) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { ...item, name: event.target.value } : item) }))} disabled={mutationBusy} spellCheck={false} />
                      </label>
                      <label className="field">
                        <span>字段类型</span>
                        <Select aria-label={`字段 ${fieldIndex + 1} 类型`} value={field.field_type} onChange={(event) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { name: item.name, ...(item.alias ? { alias: item.alias } : {}), field_type: event.target.value as SearchFieldType, ...(event.target.value === "vector" ? { vector: defaultSearchVectorConfig() } : {}) } : item) }))} disabled={mutationBusy}>
                          {fieldTypeOptions.map((option) => <option key={option.value} value={option.value} disabled={option.value === "vector" && !vectorSupported}>{option.label}</option>)}
                        </Select>
                      </label>
                      <label className="field">
                        <span>查询别名（可选）</span>
                        <input aria-label={`字段 ${fieldIndex + 1} 查询别名`} value={field.alias ?? ""} placeholder={field.field_type === "vector" ? "建议：embedding" : "例如 name"} onChange={(event) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { ...item, alias: event.target.value || undefined } : item) }))} disabled={mutationBusy} spellCheck={false} />
                        <small>{field.field_type === "vector" ? "JSONPath 或含标点字段请显式填写，如 embedding；原字段路径保持不变。" : "AS 别名仅用于查询；留空保留原字段名称。"}</small>
                      </label>
                      <button type="button" className="button button-quiet" onClick={() => setCreateDraft((current) => ({ ...current, fields: current.fields.length > 1 ? current.fields.filter((_, index) => index !== fieldIndex) : current.fields }))} disabled={mutationBusy || createDraft.fields.length === 1}>
                        删除字段
                      </button>
                      {field.field_type === "vector" && field.vector ? <VectorIndexOptions value={field.vector} index={fieldIndex} disabled={mutationBusy} onChange={(vector) => setCreateDraft((current) => ({ ...current, fields: current.fields.map((item, index) => index === fieldIndex ? { ...item, vector } : item) }))} /> : null}
                    </div>
                  ))}
                </div>
                <button type="button" className="button button-secondary" onClick={() => setCreateDraft((current) => ({ ...current, fields: [...current.fields, { name: "", field_type: "text" }] }))} disabled={mutationBusy || createDraft.fields.length >= 64}>
                  添加字段
                </button>
              </fieldset>
              {!vectorSupported ? <p className="browser-helper">VECTOR 索引需要 RedisSearch 2.4 或更高版本。</p> : null}
              {createError ? <p className="inline-error" role="alert">{createError}</p> : null}
              <div className="form-actions">
                <button type="button" className="button button-primary" onClick={() => void handleCreate()} disabled={mutationBusy}>
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
                  <Select aria-label="当前索引" value={state.selectedIndex ?? ""} onChange={(event) => changeQuery({ selectedIndex: event.target.value || null })} disabled={indexLoading || mutationBusy}>
                    {state.indexes.map((index) => <option key={index.name} value={index.name}>{index.name}</option>)}
                  </Select>
                </label>
              ) : (
                <p className="empty-state-compact">还没有索引，请先新建一个索引。</p>
              )}
              {state.selectedIndex ? (
                <button type="button" className="button button-danger search-delete-button" onClick={() => void handleDelete()} disabled={state.loading || indexLoading || mutationBusy}>
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
                <span className="panel-hint">{state.includeContent ? "文档内容" : "NOCONTENT"} · LIMIT {SEARCH_PAGE_SIZE}</span>
              </div>
              <SearchQueryBuilder key={`${connectionId}:${state.selectedIndex}:${JSON.stringify(state.info?.attributes ?? [])}`} attributes={state.info?.attributes ?? []} disabled={!state.selectedIndex || state.loading || mutationBusy || infoLoading} onApply={(query) => changeQuery({ query })} />
              <div className="search-query-form">
                <label className="field">
                  <span>查询语句</span>
                  <input aria-label="查询语句" value={state.query} onChange={(event) => changeQuery({ query: event.target.value })} onKeyDown={(event) => { if (event.key === "Enter") void runSearch(0); }} disabled={!state.selectedIndex || state.loading || mutationBusy} spellCheck={false} />
                  <small>示例：*、@name:Alice、@age:[18 30]</small>
                </label>
                <button type="button" className="button button-primary" onClick={() => void runSearch(0)} disabled={!state.selectedIndex || state.loading || mutationBusy}>
                  {state.loading ? "查询中…" : "查询"}
                </button>
              </div>
              <label className="search-content-toggle">
                <input type="checkbox" checked={state.includeContent} onChange={(event) => changeQuery({ includeContent: event.target.checked })} disabled={!state.selectedIndex || mutationBusy} />
                返回文档内容
              </label>
              {state.includeContent ? <p className="browser-helper">最多 64 个字段/文档、256 KiB/字段、4 MiB/响应；支持 UTF-8 文本及 JSON，过大或二进制文档请使用仅键名模式。</p> : null}
              <SearchResults result={state.result} busy={state.loading || mutationBusy} includeContent={state.includeContent} onNext={() => { if (nextOffset !== null) void runSearch(nextOffset); }} />
            </section>
            <VectorSearchPanel connectionId={connectionId} index={state.selectedIndex} attributes={state.info?.attributes ?? []} enabled={vectorSupported && !mutationBusy && !infoLoading} />
            <AggregatePanel connectionId={connectionId} index={state.selectedIndex} enabled={!mutationBusy} />
          </div>
        </>
      ) : null}
    </section>
  );
}

export default SearchPage;
