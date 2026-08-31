import { useState } from "react";

function scalar(value: unknown): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value) ?? "undefined";
}

function members(value: unknown): [string, unknown][] | null {
  if (Array.isArray(value)) return value.map((item, index) => [String(index), item]);
  if (value !== null && typeof value === "object") return Object.entries(value);
  return null;
}

function RespNode({ label, value, depth = 0 }: { label: string; value: unknown; depth?: number }) {
  const [expanded, setExpanded] = useState(depth < 1);
  const [limit, setLimit] = useState(100);
  const children = members(value);
  if (!children) return <li role="treeitem"><code>{label}</code>: <span>{scalar(value)}</span></li>;
  return <li role="treeitem" aria-expanded={expanded}>
    <button type="button" className="button button-quiet" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
      {expanded ? "▾" : "▸"} {label} · {Array.isArray(value) ? "数组" : "对象"} ({children.length})
    </button>
    {expanded ? <ul role="group">
      {children.slice(0, limit).map(([key, item]) => <RespNode key={key} label={key} value={item} depth={depth + 1} />)}
      {children.length > limit ? <li role="none"><button type="button" className="button button-quiet" onClick={() => setLimit(limit + 100)}>显示更多（剩余 {children.length - limit} 项）</button></li> : null}
    </ul> : null}
  </li>;
}

export function RespTree({ value }: { value: unknown }) {
  return <ul role="tree" aria-label="RESP 结果树" className="resp-tree"><RespNode label="$" value={value} /></ul>;
}

export function RespTable({ value }: { value: unknown }) {
  const [limit, setLimit] = useState(100);
  const rows = members(value) ?? [["$", value]];
  return <>
    <div style={{ overflowX: "auto" }}>
      <table aria-label="RESP 结果表格" className="data-table">
        <thead><tr><th scope="col">索引 / 字段</th><th scope="col">类型</th><th scope="col">返回值</th></tr></thead>
        <tbody>{rows.slice(0, limit).map(([key, item]) => <tr key={key}>
          <th scope="row"><code>{key}</code></th>
          <td>{item === null ? "null" : Array.isArray(item) ? "array" : typeof item}</td>
          <td>{members(item) ? <details><summary>{JSON.stringify(item).slice(0, 100)}{JSON.stringify(item).length > 100 ? "…" : ""}</summary><RespTree value={item} /></details> : <code>{scalar(item)}</code>}</td>
        </tr>)}</tbody>
      </table>
    </div>
    {rows.length > limit ? <button type="button" className="button button-quiet" onClick={() => setLimit(limit + 100)}>显示更多行（剩余 {rows.length - limit} 行）</button> : null}
  </>;
}
