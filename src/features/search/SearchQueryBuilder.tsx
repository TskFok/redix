import { useState } from "react";
import type { SearchIndexAttribute } from "../../lib/types";
import { buildSearchQuery, queryBuilderFields, type SearchFilter } from "./queryBuilder";

interface Props {
  attributes: SearchIndexAttribute[];
  disabled: boolean;
  onApply: (query: string) => void;
}

export function SearchQueryBuilder({ attributes, disabled, onApply }: Props) {
  const fields = queryBuilderFields(attributes);
  const newFilter = (): SearchFilter => ({ field: fields[0]?.name ?? "", type: fields[0]?.type ?? "TEXT", value: "", unit: "km" });
  const [filters, setFilters] = useState<SearchFilter[]>(() => fields.length ? [newFilter()] : []);
  const result = buildSearchQuery(filters);
  const update = (index: number, changes: Partial<SearchFilter>) => setFilters((current) => current.map((filter, position) => position === index ? { ...filter, ...changes } : filter));

  return <details className="search-query-builder">
    <summary>可视查询构建器</summary>
    <p className="browser-helper">所有条件同时满足。文本按短语匹配，标签按完整值匹配；回填后点击“查询”执行。</p>
    {fields.length === 0 ? <p className="empty-state-compact">当前索引没有可用于构建的字段。支持 TEXT、TAG、NUMERIC、GEO 和简单字段别名。</p> : <>
      <fieldset className="editor-fieldset" disabled={disabled}>
        <legend>查询条件</legend>
        {filters.map((filter, index) => <div className="search-filter-row" key={index}>
          <label className="field"><span>条件 {index + 1} 字段</span><select aria-label={`条件 ${index + 1} 字段`} value={filter.field} onChange={(event) => {
            const field = fields.find((item) => item.name === event.target.value);
            if (field) setFilters((current) => current.map((item, position) => position === index ? { field: field.name, type: field.type, value: "", unit: "km" } : item));
          }}>{fields.map((field) => <option value={field.name} key={field.name}>{field.name} · {field.type}</option>)}</select></label>
          <label className="field"><span>{filter.type === "NUMERIC" ? "下限（可留空）" : filter.type === "GEO" ? "经度" : "值"}</span><input aria-label={`条件 ${index + 1} 值`} value={filter.value} onChange={(event) => update(index, { value: event.target.value })} /></label>
          {filter.type === "NUMERIC" ? <label className="field"><span>上限（可留空）</span><input aria-label={`条件 ${index + 1} 上限`} value={filter.upper ?? ""} onChange={(event) => update(index, { upper: event.target.value })} /></label> : null}
          {filter.type === "GEO" ? <>
            <label className="field"><span>纬度</span><input aria-label={`条件 ${index + 1} 纬度`} value={filter.latitude ?? ""} onChange={(event) => update(index, { latitude: event.target.value })} /></label>
            <label className="field"><span>半径</span><input aria-label={`条件 ${index + 1} 半径`} value={filter.radius ?? ""} onChange={(event) => update(index, { radius: event.target.value })} /></label>
            <label className="field"><span>单位</span><select aria-label={`条件 ${index + 1} 单位`} value={filter.unit} onChange={(event) => update(index, { unit: event.target.value })}>{["m", "km", "mi", "ft"].map((unit) => <option key={unit}>{unit}</option>)}</select></label>
          </> : null}
          <button type="button" className="button button-quiet" aria-label={`删除条件 ${index + 1}`} onClick={() => setFilters((current) => current.filter((_, position) => position !== index))}>删除条件</button>
        </div>)}
        <button type="button" className="button button-secondary" disabled={filters.length >= 8} onClick={() => setFilters((current) => [...current, newFilter()])}>添加查询条件</button>
      </fieldset>
      {result.error ? <p className="browser-helper">{result.error}</p> : <pre aria-label="查询预览" className="search-query-preview">{result.query}</pre>}
      <button type="button" className="button button-secondary" disabled={disabled || result.query === null} onClick={() => { if (result.query !== null) onApply(result.query); }}>回填查询语句</button>
    </>}
  </details>;
}
