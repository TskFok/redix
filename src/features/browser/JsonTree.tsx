import { useEffect, useMemo, useState } from "react";

import type { JsonValue } from "../../lib/types";
import "./browserTrees.css";

const PAGE_SIZE = 50;
const MAX_VISIBLE_NODES = 500;
const MAX_DEPTH = 32;

export interface JsonTreeNode {
  path: string;
  label: string;
  value: JsonValue;
  depth: number;
  blockedReason: string | null;
}

interface JsonTreeProps {
  value: JsonValue;
  selectedPath: string;
  busy: boolean;
  onSelect(node: JsonTreeNode): void;
}

function childNode(parent: JsonTreeNode, segment: string | number, value: JsonValue): JsonTreeNode {
  const suffix = typeof segment === "number"
    ? `[${segment}]`
    : /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(segment) ? `.${segment}` : `[${JSON.stringify(segment)}]`;
  const path = parent.path + suffix;
  let blockedReason = parent.blockedReason;
  // Match the backend whitelist: only quote/backslash escapes are supported.
  // JSON.stringify escapes control characters and unpaired UTF-16 surrogates.
  if (typeof segment === "string" && /[\u0000-\u001f\u007f-\u009f]|[\ud800-\udfff]/u.test(segment)) {
    blockedReason = "属性名称含后端路径不支持的字符，仅可查看；请通过可编辑的父节点修改。";
  }
  if (new TextEncoder().encode(path).length > 512) {
    blockedReason = "路径超过后端 512 字节上限，仅可查看；请通过可编辑的父节点修改。";
  }
  return { path, label: typeof segment === "number" ? `[${segment}]` : JSON.stringify(segment), value, depth: parent.depth + 1, blockedReason };
}

function containerSize(value: JsonValue): number | null {
  if (Array.isArray(value)) return value.length;
  if (value !== null && typeof value === "object") return Object.keys(value).length;
  return null;
}

function preview(value: JsonValue, size: number | null): string {
  if (Array.isArray(value)) return `[${size} 项]`;
  if (value !== null && typeof value === "object") return `{${size} 个属性}`;
  const text = JSON.stringify(value);
  return text.length > 160 ? `${text.slice(0, 160)}…` : text;
}

type TreeRow = { kind: "node"; node: JsonTreeNode; size: number | null }
  | { kind: "more"; node: JsonTreeNode; remaining: number };

export function JsonTree({ value, selectedPath, busy, onSelect }: JsonTreeProps) {
  const [expanded, setExpanded] = useState(() => new Set(["$"]));
  const [limits, setLimits] = useState<Map<string, number>>(() => new Map());

  useEffect(() => {
    setExpanded(new Set(["$"]));
    setLimits(new Map());
  }, [value]);

  const { rows, truncated } = useMemo(() => {
    const rows: TreeRow[] = [];
    let count = 0;
    let truncated = false;
    const visit = (node: JsonTreeNode) => {
      if (count >= MAX_VISIBLE_NODES) { truncated = true; return; }
      count += 1;
      const size = containerSize(node.value);
      rows.push({ kind: "node", node, size });
      if (!size || !expanded.has(node.path) || node.depth >= MAX_DEPTH) return;
      const shown = Math.min(size, limits.get(node.path) ?? PAGE_SIZE);
      const objectKeys = Array.isArray(node.value) ? null : Object.keys(node.value as object);
      for (let index = 0; index < shown; index += 1) {
        if (count >= MAX_VISIBLE_NODES) { truncated = true; break; }
        const segment = objectKeys ? objectKeys[index] : index;
        const childValue = Array.isArray(node.value) ? node.value[index] : (node.value as Record<string, JsonValue>)[segment];
        visit(childNode(node, segment, childValue));
      }
      if (shown < size) {
        if (count >= MAX_VISIBLE_NODES) truncated = true;
        else rows.push({ kind: "more", node, remaining: size - shown });
      }
    };
    visit({ path: "$", label: "$", value, depth: 0, blockedReason: null });
    return { rows, truncated };
  }, [value, expanded, limits]);

  const toggle = (path: string) => setExpanded((current) => {
    const next = new Set(current);
    if (next.has(path)) next.delete(path); else next.add(path);
    return next;
  });

  return <div className="json-tree-panel">
    <p className="browser-helper">点击节点填入路径和值；展开对象或数组查看子节点。</p>
    <ul className="json-node-tree" role="tree" aria-label="JSON 节点树">
      {rows.map((row) => row.kind === "more" ? (
        <li key={`more:${row.node.path}`} role="none" style={{ paddingInlineStart: (row.node.depth + 1) * 16 }}>
          <button type="button" className="tree-more" disabled={busy || truncated}
            aria-label={`显示更多 ${row.node.path} 的子节点`}
            onClick={() => setLimits((current) => new Map(current).set(row.node.path, (current.get(row.node.path) ?? PAGE_SIZE) + PAGE_SIZE))}>
            显示更多（剩余 {row.remaining} 项）
          </button>
        </li>
      ) : (
        <li key={row.node.path} role="treeitem" aria-level={row.node.depth + 1}
          aria-selected={selectedPath === row.node.path}
          aria-expanded={row.size && row.node.depth < MAX_DEPTH ? expanded.has(row.node.path) : undefined}
          className="json-tree-row" style={{ paddingInlineStart: row.node.depth * 16 }}>
          {row.size && row.node.depth < MAX_DEPTH ? (
            <button type="button" className="tree-disclosure" disabled={busy}
              aria-label={`${expanded.has(row.node.path) ? "折叠" : "展开"} ${row.node.path}`}
              onClick={() => toggle(row.node.path)}>{expanded.has(row.node.path) ? "▾" : "▸"}</button>
          ) : <span className="tree-disclosure-spacer" />}
          <button type="button" className="json-tree-select" disabled={busy}
            aria-label={`定位 ${row.node.path}`} aria-pressed={selectedPath === row.node.path}
            title={row.node.path} onClick={() => onSelect(row.node)}>
            <code className="json-tree-label">{row.node.label}</code>
            <code className="json-tree-preview">{preview(row.node.value, row.size)}</code>
            {row.node.blockedReason ? <span className="tree-readonly">仅可查看</span> : null}
          </button>
          {row.size && row.node.depth >= MAX_DEPTH ? <span className="tree-readonly">已达展开深度上限，可定位父节点查看</span> : null}
        </li>
      ))}
    </ul>
    {truncated ? <p className="browser-helper" role="status">已达到 {MAX_VISIBLE_NODES} 个可见节点上限，请折叠部分分支后继续浏览，或使用路径编辑器。</p> : null}
  </div>;
}
