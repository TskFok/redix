import { useMemo, useState } from "react";
import type { JsonValue, SearchKeyResult } from "../../lib/types";

function DocumentValue({ value }: { value: JsonValue }) {
  const [expanded, setExpanded] = useState(false);
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  const truncated = text.length > 2000;
  return <div className="search-document-value">
    <pre>{truncated && !expanded ? `${text.slice(0, 2000)}…` : text}</pre>
    {truncated ? <button type="button" className="button button-quiet" onClick={() => setExpanded((current) => !current)}>{expanded ? "收起字段" : "展开完整字段"}</button> : null}
  </div>;
}

export function SearchDocumentTable({ documents }: { documents: SearchKeyResult[] }) {
  const columns = useMemo(() => {
    const names = new Set<string>();
    for (const document of documents) {
      for (const field of document.fields ?? []) {
        if (names.size < 24) names.add(field.name);
      }
    }
    return [...names];
  }, [documents]);
  const hasExtra = documents.some((document) => document.fields?.some((field) => !columns.includes(field.name)));
  return <div className="database-table-wrap search-document-table-wrap">
    <table className="database-table search-results-table" aria-label="搜索文档结果">
      <thead><tr>
        <th scope="col">键名</th><th scope="col">类型</th>
        {columns.map((name) => <th scope="col" key={name}>{name || "（空字段）"}</th>)}
        {hasExtra ? <th scope="col">其他字段</th> : null}
        <th scope="col">文档状态</th>
      </tr></thead>
      <tbody>{documents.map((document) => {
        const fields = new Map(document.fields?.map((field) => [field.name, field.value]));
        const extras = document.fields?.filter((field) => !columns.includes(field.name)) ?? [];
        return <tr key={document.key}>
          <th scope="row"><code>{document.key}</code></th><td>{document.key_type}</td>
          {columns.map((name) => <td key={name}>{fields.has(name)
            ? <DocumentValue key={`${document.key}:${name}`} value={fields.get(name)!} />
            : <span aria-label="字段不存在">—</span>}</td>)}
          {hasExtra ? <td>{extras.length ? <details><summary>{extras.length} 个其他字段</summary><dl>
            {extras.map((field) => <div key={field.name}><dt>{field.name || "（空字段）"}</dt><dd><DocumentValue value={field.value} /></dd></div>)}
          </dl></details> : "—"}</td> : null}
          <td>{document.fields == null ? "文档已过期或内容不可用" : document.fields.length ? "已加载" : "无字段"}</td>
        </tr>;
      })}</tbody>
    </table>
  </div>;
}
