import Select from "../../components/Select";
import { useEffect, useRef, useState } from "react";
import type { SearchIndexAttribute, SearchVectorQueryResult } from "../../lib/types";
import { searchVectorIndex } from "./searchVectorApi";

type Props = { connectionId: string; index: string | null; attributes: SearchIndexAttribute[]; enabled: boolean };
const nameOf = (field: SearchIndexAttribute) => field.query_name ?? field.identifier;
const usable = (field: SearchIndexAttribute) => field.field_type.toUpperCase() === "VECTOR" && !field.no_index
  && /^[A-Za-z0-9_]{1,256}$/.test(nameOf(field)) && !!field.vector
  && ["FLOAT32", "FLOAT64"].includes(field.vector.data_type)
  && ["COSINE", "L2", "IP"].includes(field.vector.distance_metric)
  && Number.isInteger(field.vector.dimension) && field.vector.dimension > 0 && field.vector.dimension <= 32768;

export function VectorSearchPanel(props: Props) {
  return <VectorSearchForm key={JSON.stringify([props.connectionId, props.index, props.attributes, props.enabled])} {...props} />;
}

function VectorSearchForm({ connectionId, index, attributes, enabled }: Props) {
  const fields = attributes.filter(usable);
  const [fieldName, setFieldName] = useState(fields[0] ? nameOf(fields[0]) : "");
  const [source, setSource] = useState("");
  const [count, setCount] = useState("5");
  const [filter, setFilter] = useState("*");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SearchVectorQueryResult | null>(null);
  const generation = useRef(0);
  useEffect(() => () => { generation.current += 1; }, []);
  const schema = fields.find((field) => nameOf(field) === fieldName)?.vector;
  let vector: number[] | null = null;
  let validation = "请输入与索引维度相同的 JSON 数字数组。";
  if (source && schema) {
    try {
      if (source.length > 1_048_576) throw new Error("向量文本最多 1 MiB。");
      const value: unknown = JSON.parse(source);
      if (!Array.isArray(value) || value.length !== schema.dimension) throw new Error(`需要 ${schema.dimension} 维向量。`);
      if (!value.every((part) => typeof part === "number" && Number.isFinite(part) && (schema.data_type !== "FLOAT32" || Number.isFinite(Math.fround(part))))) throw new Error(`每个分量必须是可表示的有限 ${schema.data_type} 数字。`);
      if (schema.distance_metric === "COSINE" && !value.some((part: number) => (schema.data_type === "FLOAT32" ? Math.fround(part) : part) !== 0)) throw new Error("COSINE 查询不能使用零向量。");
      vector = value as number[];
      validation = "";
    } catch (reason) { validation = reason instanceof Error ? reason.message : "向量格式无效。"; }
  }
  const validCount = /^\d+$/.test(count) && Number(count) >= 1 && Number(count) <= 200;
  const validFilter = filter.trim().length > 0 && new TextEncoder().encode(filter).length <= 4096 && !/[\u0000-\u001f\u007f]/.test(filter);
  const canRun = enabled && !!index && !!schema && !!vector && validCount && validFilter && !busy;
  const change = (update: () => void) => { generation.current += 1; update(); setBusy(false); setResult(null); setError(null); };
  const execute = async () => {
    if (!canRun || !index || !vector) return;
    const request = ++generation.current;
    setBusy(true); setError(null); setResult(null);
    try {
      const next = await searchVectorIndex({ connection_id: connectionId, index, field: fieldName, vector, count: Number(count), filter });
      if (request === generation.current) setResult(next);
    } catch { if (request === generation.current) setError("KNN 查询失败。请确认过滤语法、索引配置和连接能力；索引配置变化后请刷新索引。"); }
    finally { if (request === generation.current) setBusy(false); }
  };
  return <section className="database-panel search-vector-query" aria-label="向量 KNN 查询">
    <div className="database-panel-heading"><div><p className="eyebrow">FT.SEARCH · PARAMS · DIALECT 2</p><h3>向量 KNN 查询</h3></div></div>
    <p className="browser-helper">粘贴已有向量；不调用嵌入服务。执行时按服务器当前 schema 校验并编码为二进制。最多返回 200 个键及距离，不读取向量原文。</p>
    {!fields.length ? <p className="browser-helper">当前索引缺少可用的 FLOAT32 / FLOAT64 向量元数据或安全字段别名，请刷新索引或检查配置。</p> : null}
    {!enabled ? <p className="browser-helper">等待索引详情和 RedisSearch 2.4 或更高版本连接就绪。</p> : null}
    <div className="form-grid">
      <label className="field"><span>向量字段</span><Select aria-label="向量字段" value={fieldName} disabled={!enabled || !fields.length} onChange={(event) => change(() => setFieldName(event.target.value))}>{fields.map((field) => <option key={nameOf(field)} value={nameOf(field)}>{nameOf(field)} · {field.vector!.data_type} · {field.vector!.dimension} 维 · {field.vector!.distance_metric}</option>)}</Select></label>
      <label className="field"><span>Top K</span><input autoCapitalize="off" autoCorrect="off" aria-label="Top K" type="number" min="1" max="200" step="1" value={count} onChange={(event) => change(() => setCount(event.target.value))} /></label>
    </div>
    <label className="field"><span>查询向量（JSON 数组）</span><textarea autoCapitalize="off" autoCorrect="off" aria-label="查询向量（JSON 数组）" rows={4} maxLength={1_048_576} value={source} placeholder="[0.12, -0.34, ...]" onChange={(event) => change(() => setSource(event.target.value))} spellCheck={false} /></label>
    <label className="field"><span>向量过滤条件</span><input autoCapitalize="off" autoCorrect="off" aria-label="向量过滤条件" value={filter} maxLength={4096} onChange={(event) => change(() => setFilter(event.target.value))} spellCheck={false} /><small>使用原生 RediSearch 过滤语法，例如 @tag:&#123;book&#125;；括号需配对，不包含额外查询后缀。* 表示全部文档。</small></label>
    {validation && source ? <p role="status" className="browser-helper">{validation}</p> : null}
    {!validCount ? <p role="status">Top K 必须是 1–200 的整数。</p> : null}
    {!validFilter ? <p role="status">过滤条件不能为空，最多 4096 字节且不能包含控制字符。</p> : null}
    <code className="search-vector-preview">({filter})=&gt;[KNN {count} @{fieldName || "字段"} $向量 AS 距离]</code>
    <div><button type="button" className="button button-primary" disabled={!canRun} onClick={() => void execute()}>{busy ? "KNN 查询中…" : "执行 KNN 查询"}</button></div>
    {error ? <p role="alert">{error}</p> : null}
    {result ? <div aria-live="polite"><p>返回 {result.returned} / Top {result.count} · {result.distance_metric} 距离（越小越近）。HNSW 为近似近邻；超时或文档变化可能减少结果。</p>{result.matches.length ? <table className="data-table"><thead><tr><th>键</th><th>距离</th></tr></thead><tbody>{result.matches.map((match) => <tr key={match.key}><td>{match.key}</td><td>{match.distance}</td></tr>)}</tbody></table> : <p>没有匹配的向量文档。</p>}</div> : null}
  </section>;
}
