import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";

import {
  addVectorSetElements,
  deleteVectorSetAttributes,
  deleteVectorSetElements,
  downloadVectorEmbedding,
  getVectorSetElement,
  getVectorSetSummary,
  listVectorSetElements,
  searchVectorSet,
  setVectorSetAttributes,
} from "../../lib/tauri";
import type {
  JsonValue,
  VectorSetElement,
  VectorSetElementPayload,
  VectorSetSummary,
} from "../../lib/types";
import { browserErrorMessage } from "./browserState";
import {
  applyVectorSetPage,
  applyVectorSimilarityResult,
  createVectorSetDetailState,
  isCurrentVectorSetRequest,
  type VectorSetDetailState,
  type VectorSetRequestToken,
} from "./vectorSetState";

interface VectorSetDetailsProps {
  connectionId: string;
  keyName: string;
  initialSummary?: VectorSetSummary;
  disabled?: boolean;
  onBusyChange?: (busy: boolean) => void;
}

type VectorTab = "elements" | "similarity";
type QueryMode = "element" | "vector";

const VECTOR_PAGE_SIZE = 50;

function formatJson(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

function compactJson(value: JsonValue | null): string {
  return value === null ? "—" : JSON.stringify(value) ?? "null";
}

function parseVector(text: string): number[] | null {
  try {
    const parsed: unknown = JSON.parse(text);
    if (
      !Array.isArray(parsed) ||
      parsed.length === 0 ||
      parsed.some((value) => typeof value !== "number" || !Number.isFinite(value))
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function parseAttributes(text: string): JsonValue | null {
  if (text.trim() === "") {
    return null;
  }
  try {
    return JSON.parse(text) as JsonValue;
  } catch {
    return null;
  }
}

function downloadBase64(value: string, fileName: string) {
  const binary = window.atob(value);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  const url = window.URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  window.URL.revokeObjectURL(url);
}

function VectorSummaryCards({ summary }: { summary: VectorSetSummary | null }) {
  const cards = [
    ["元素总数", summary?.total ?? "—"],
    ["向量维度", summary?.dimension === null || summary?.dimension === undefined ? "—" : String(summary.dimension)],
    ["量化类型", summary?.quantization ?? "—"],
  ];
  return (
    <div className="module-summary-grid" aria-label="Vector Set 摘要">
      {cards.map(([label, value]) => (
        <div className="module-summary-card" key={label}>
          <span>{label}</span>
          <strong>{value}</strong>
        </div>
      ))}
    </div>
  );
}

export function VectorSetDetails({
  connectionId,
  keyName,
  initialSummary,
  disabled = false,
  onBusyChange,
}: VectorSetDetailsProps) {
  const [state, setState] = useState<VectorSetDetailState>(() =>
    createVectorSetDetailState(initialSummary ?? null),
  );
  const [selectedDetails, setSelectedDetails] = useState<VectorSetElement | null>(null);
  const [attributesDraft, setAttributesDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [addName, setAddName] = useState("");
  const [addVectorText, setAddVectorText] = useState("[]");
  const [addAttributesText, setAddAttributesText] = useState("");
  const [queryMode, setQueryMode] = useState<QueryMode>("element");
  const [queryElement, setQueryElement] = useState("");
  const [queryVectorText, setQueryVectorText] = useState("[]");
  const [queryCount, setQueryCount] = useState("10");
  const [queryWithAttributes, setQueryWithAttributes] = useState(true);
  const requestRef = useRef<VectorSetRequestToken | null>(null);
  const requestIdRef = useRef(0);

  const beginRequest = (tab: VectorTab = state.activeTab): VectorSetRequestToken => {
    const token: VectorSetRequestToken = {
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

  const isCurrent = (token: VectorSetRequestToken) =>
    isCurrentVectorSetRequest(requestRef.current, token);

  const finishRequest = (token: VectorSetRequestToken) => {
    if (isCurrent(token)) {
      setBusy(false);
      setState((current) => ({ ...current, loading: false }));
    }
  };

  const loadPageData = async (
    token: VectorSetRequestToken,
    start: string | null,
    replace: boolean,
  ) => {
    const [summary, page] = await Promise.all([
      getVectorSetSummary({ connection_id: token.connectionId, key: token.key }),
      listVectorSetElements({
        connection_id: token.connectionId,
        key: token.key,
        start,
        end: null,
        limit: VECTOR_PAGE_SIZE,
      }),
    ]);
    if (!isCurrent(token)) {
      return;
    }
    setState((current) => applyVectorSetPage({ ...current, summary }, page, replace));
  };

  useEffect(() => {
    setState(createVectorSetDetailState(initialSummary ?? null));
    setSelectedDetails(null);
    setAttributesDraft("");
    const token: VectorSetRequestToken = {
      connectionId,
      key: keyName,
      requestId: requestIdRef.current + 1,
    };
    requestIdRef.current = token.requestId;
    requestRef.current = token;
    setBusy(true);
    setState((current) => ({ ...current, loading: true }));
    void loadPageData(token, null, true)
      .catch((caught) => {
        if (isCurrent(token)) {
          setState((current) => ({
            ...current,
            loading: false,
            error: browserErrorMessage(caught, "读取 Vector Set 失败，请稍后重试。"),
          }));
        }
      })
      .finally(() => finishRequest(token));
    return () => {
      requestIdRef.current += 1;
      requestRef.current = null;
    };
    // The initial summary is a seed only; key changes own this request lifecycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId, keyName]);

  useEffect(() => {
    onBusyChange?.(busy);
  }, [busy, onBusyChange]);

  const reloadFirstPage = async (token: VectorSetRequestToken) => {
    await loadPageData(token, null, true);
    if (isCurrent(token)) {
      setSelectedDetails(null);
      setAttributesDraft("");
    }
  };

  const handleLoadMore = async () => {
    const cursor = state.page?.cursor;
    if (!state.page?.has_more || !cursor || busy || disabled) {
      return;
    }
    const token = beginRequest("elements");
    try {
      await loadPageData(token, cursor, false);
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: browserErrorMessage(caught, "加载更多 Vector Set 元素失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleSelectElement = async (name: string) => {
    const token = beginRequest(state.activeTab);
    try {
      const element = await getVectorSetElement({
        connection_id: connectionId,
        key: keyName,
        element: name,
      });
      if (isCurrent(token)) {
        setSelectedDetails(element);
        setAttributesDraft(formatJson(element.attributes));
        setState((current) => ({ ...current, selectedElement: name, loading: false }));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: browserErrorMessage(caught, "读取 Vector Set 元素失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleAddElement = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = addName.trim();
    const vector = parseVector(addVectorText);
    const attributes = parseAttributes(addAttributesText);
    if (name === "" || vector === null || (addAttributesText.trim() !== "" && attributes === null)) {
      setState((current) => ({
        ...current,
        error: "元素名称不能为空，向量必须是有限数字 JSON 数组，属性 JSON 必须有效。",
      }));
      return;
    }
    const payload: VectorSetElementPayload = {
      name,
      vector_values: vector,
      vector_fp32_base64: null,
      attributes,
    };
    const token = beginRequest("elements");
    try {
      await addVectorSetElements({ connection_id: connectionId, key: keyName, elements: [payload] });
      await reloadFirstPage(token);
      if (isCurrent(token)) {
        setAddName("");
        setAddVectorText("[]");
        setAddAttributesText("");
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: browserErrorMessage(caught, "添加 Vector Set 元素失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleDeleteSelected = async () => {
    if (state.selectedElements.length === 0 || !window.confirm(`确定删除选中的 ${state.selectedElements.length} 个 Vector Set 元素吗？`)) {
      return;
    }
    const token = beginRequest("elements");
    try {
      await deleteVectorSetElements({
        connection_id: connectionId,
        key: keyName,
        elements: state.selectedElements,
      });
      await reloadFirstPage(token);
      if (isCurrent(token)) {
        setState((current) => ({ ...current, selectedElements: [] }));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({
          ...current,
          loading: false,
          error: browserErrorMessage(caught, "删除 Vector Set 元素失败，请稍后重试。"),
        }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleSaveAttributes = async () => {
    if (!selectedDetails) {
      return;
    }
    const attributes = parseAttributes(attributesDraft);
    if (attributes === null) {
      setState((current) => ({ ...current, error: "属性必须是有效 JSON；清空属性请使用删除属性。" }));
      return;
    }
    const token = beginRequest(state.activeTab);
    try {
      const element = await setVectorSetAttributes({
        connection_id: connectionId,
        key: keyName,
        element: selectedDetails.name,
        attributes,
      });
      if (isCurrent(token)) {
        setSelectedDetails(element);
        setAttributesDraft(formatJson(element.attributes));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({ ...current, loading: false, error: browserErrorMessage(caught, "保存属性失败，请稍后重试。") }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleDeleteAttributes = async () => {
    if (!selectedDetails || !window.confirm(`确定清除元素“${selectedDetails.name}”的属性吗？`)) {
      return;
    }
    const token = beginRequest(state.activeTab);
    try {
      await deleteVectorSetAttributes({
        connection_id: connectionId,
        key: keyName,
        element: selectedDetails.name,
      });
      if (isCurrent(token)) {
        setSelectedDetails({ ...selectedDetails, attributes: null });
        setAttributesDraft("null");
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({ ...current, loading: false, error: browserErrorMessage(caught, "清除属性失败，请稍后重试。") }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const handleSearch = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const count = Number(queryCount);
    const vector = queryMode === "vector" ? parseVector(queryVectorText) : null;
    if (!Number.isInteger(count) || count < 1 || count > 200 || (queryMode === "element" ? queryElement.trim() === "" : vector === null)) {
      setState((current) => ({ ...current, error: "查询元素不能为空，向量必须是有效数组，数量必须是 1 到 200。" }));
      return;
    }
    const token = beginRequest("similarity");
    try {
      const result = await searchVectorSet({
        connection_id: connectionId,
        key: keyName,
        by_element: queryMode === "element" ? queryElement.trim() : null,
        by_vector: vector,
        by_vector_base64: null,
        count,
        with_attributes: queryWithAttributes,
      });
      if (isCurrent(token)) {
        setState((current) => applyVectorSimilarityResult(current, result));
      }
    } catch (caught) {
      if (isCurrent(token)) {
        setState((current) => ({ ...current, loading: false, error: browserErrorMessage(caught, "Vector 相似度搜索失败，请稍后重试。") }));
      }
    } finally {
      finishRequest(token);
    }
  };

  const selectedSet = new Set(state.selectedElements);

  return (
    <section className="module-details vector-set-details" aria-labelledby="vector-set-details-title" aria-busy={busy || disabled}>
      <div className="module-details-heading">
        <div>
          <p className="eyebrow">REDIS MODULE</p>
          <h3 id="vector-set-details-title">Vector Set</h3>
        </div>
        <span className="module-detail-badge">V*</span>
      </div>
      <VectorSummaryCards summary={state.summary} />
      <div className="module-tabs" role="tablist" aria-label="Vector Set 视图">
        {([
          ["elements", "元素"],
          ["similarity", "相似度搜索"],
        ] as const).map(([tab, label]) => (
          <button key={tab} type="button" role="tab" aria-selected={state.activeTab === tab} className={`module-tab${state.activeTab === tab ? " module-tab-active" : ""}`} onClick={() => setState((current) => ({ ...current, activeTab: tab }))} disabled={busy || disabled}>{label}</button>
        ))}
      </div>
      {state.error ? <p className="feedback feedback-error" role="alert">{state.error}</p> : null}
      {state.activeTab === "elements" ? (
        <div className="module-tab-content">
          <form className="module-action-card vector-add-card" onSubmit={(event) => void handleAddElement(event)}>
            <div className="module-card-heading"><h4>添加元素</h4><span>VADD</span></div>
            <div className="module-form-grid">
              <label className="field"><span>名称</span><input value={addName} onChange={(event) => setAddName(event.target.value)} disabled={busy || disabled} /></label>
              <label className="field"><span>向量 JSON</span><input value={addVectorText} onChange={(event) => setAddVectorText(event.target.value)} disabled={busy || disabled} /></label>
            </div>
            <label className="field"><span>属性 JSON（可选）</span><input value={addAttributesText} onChange={(event) => setAddAttributesText(event.target.value)} disabled={busy || disabled} placeholder='{"label":"demo"}' /></label>
            <button type="submit" className="button button-secondary" disabled={busy || disabled}>添加元素</button>
          </form>
          <div className="module-selection-toolbar">
            <span>{state.selectedElements.length > 0 ? `已选择 ${state.selectedElements.length} 个元素` : "可多选元素后批量删除"}</span>
            <button type="button" className="button button-danger" onClick={() => void handleDeleteSelected()} disabled={busy || disabled || state.selectedElements.length === 0}>删除选中</button>
          </div>
          {state.page && state.page.elements.length > 0 ? (
            <div className="module-table-wrap">
              <table className="module-table"><caption>Vector Set 元素</caption><thead><tr><th scope="col">选择</th><th scope="col">名称</th><th scope="col">属性</th><th scope="col">操作</th></tr></thead><tbody>{state.page.elements.map((element) => <tr key={element.name}><td><input type="checkbox" aria-label={`选择元素 ${element.name}`} checked={selectedSet.has(element.name)} onChange={() => setState((current) => ({ ...current, selectedElements: selectedSet.has(element.name) ? current.selectedElements.filter((name) => name !== element.name) : [...current.selectedElements, element.name] }))} disabled={busy || disabled} /></td><td><code>{element.name}</code></td><td><code className="module-value">{compactJson(element.attributes)}</code></td><td><button type="button" className="button button-quiet button-compact" onClick={() => void handleSelectElement(element.name)} disabled={busy || disabled}>查看</button></td></tr>)}</tbody></table>
            </div>
          ) : !state.loading ? <p className="module-empty-state">当前 Vector Set 没有元素。</p> : <p className="loading-state" role="status">正在读取元素…</p>}
          {state.page?.has_more ? <button type="button" className="button button-secondary module-load-more" onClick={() => void handleLoadMore()} disabled={busy || disabled}>{busy ? "加载中…" : "加载更多"}</button> : null}
          {selectedDetails ? (
            <section className="module-element-panel" aria-labelledby="vector-element-title">
              <div className="module-card-heading"><h4 id="vector-element-title">元素详情：{selectedDetails.name}</h4><button type="button" className="button button-quiet" onClick={() => setSelectedDetails(null)} disabled={busy || disabled}>关闭</button></div>
              <div className="module-element-meta"><span>向量</span><code>{selectedDetails.vector_base64 ? `${selectedDetails.vector_base64.slice(0, 48)}…` : "未读取"}</code></div>
              <label className="field"><span>属性 JSON</span><textarea value={attributesDraft} onChange={(event) => setAttributesDraft(event.target.value)} disabled={busy || disabled} rows={5} spellCheck={false} /></label>
              <div className="module-row-actions"><button type="button" className="button button-primary" onClick={() => void handleSaveAttributes()} disabled={busy || disabled}>保存属性</button><button type="button" className="button button-danger" onClick={() => void handleDeleteAttributes()} disabled={busy || disabled}>清除属性</button>{selectedDetails.vector_base64 ? <button type="button" className="button button-secondary" onClick={() => { try { downloadBase64(selectedDetails.vector_base64 ?? "", `${selectedDetails.name}.fp32`); } catch { setState((current) => ({ ...current, error: "向量下载失败，请稍后重试。" })); } }} disabled={busy || disabled}>下载 FP32</button> : null}</div>
            </section>
          ) : null}
        </div>
      ) : (
        <div className="module-tab-content">
          <form className="module-query-form" onSubmit={(event) => void handleSearch(event)}>
            <div className="module-form-grid module-form-grid-wide">
              <label className="field"><span>查询方式</span><Select value={queryMode} onChange={(event) => setQueryMode(event.target.value as QueryMode)} disabled={busy || disabled}><option value="element">按元素</option><option value="vector">按向量</option></Select></label>
              {queryMode === "element" ? <label className="field"><span>元素名称</span><input value={queryElement} onChange={(event) => setQueryElement(event.target.value)} disabled={busy || disabled} /></label> : <label className="field"><span>向量 JSON</span><input value={queryVectorText} onChange={(event) => setQueryVectorText(event.target.value)} disabled={busy || disabled} /></label>}
              <label className="field"><span>返回数量</span><input type="number" min="1" max="200" value={queryCount} onChange={(event) => setQueryCount(event.target.value)} disabled={busy || disabled} /></label>
            </div>
            <div className="module-checkbox-row"><label><input type="checkbox" checked={queryWithAttributes} onChange={(event) => setQueryWithAttributes(event.target.checked)} disabled={busy || disabled} />返回属性</label><button type="submit" className="button button-primary" disabled={busy || disabled}>执行搜索</button></div>
          </form>
          {state.similarity ? (
            <div className="module-table-wrap"><table className="module-table"><caption>相似度结果</caption><thead><tr><th scope="col">元素</th><th scope="col">距离</th><th scope="col">属性</th><th scope="col">操作</th></tr></thead><tbody>{state.similarity.matches.map((match) => <tr key={match.name}><td><code>{match.name}</code></td><td><code>{Number.isFinite(match.score) ? match.score.toFixed(6) : "—"}</code></td><td><code className="module-value">{compactJson(match.attributes)}</code></td><td><button type="button" className="button button-quiet button-compact" onClick={() => void handleSelectElement(match.name)} disabled={busy || disabled}>查看</button></td></tr>)}</tbody></table></div>
          ) : <p className="module-empty-state">输入元素或向量后执行相似度搜索。</p>}
        </div>
      )}
    </section>
  );
}

export default VectorSetDetails;
