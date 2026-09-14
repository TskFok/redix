import { useMemo, useState } from "react";

import type { KeySummary } from "../../lib/types";
import { keyTypeLabel } from "./browserState";
import VirtualKeyRows from "./VirtualKeyRows";
import "./browserTrees.css";

interface KeyRowsProps {
  keys: KeySummary[];
  separator?: string;
  selectedKey: string | null;
  selectedKeys: string[];
  loading: boolean;
  onSelect(key: string): void;
  onToggleSelect(key: string): void;
}

export function KeyRow({ summary, selected, checked, loading, onSelect, onToggleSelect, label }: {
  summary: KeySummary;
  selected: boolean;
  checked: boolean;
  loading: boolean;
  onSelect(key: string): void;
  onToggleSelect(key: string): void;
  label?: string;
}) {
  return <div className={`key-row${selected ? " key-row-selected" : ""}`}>
    <input autoCapitalize="off" autoCorrect="off" className="key-row-checkbox" type="checkbox" aria-label={`选择键 ${summary.key}`}
      checked={checked} onChange={() => onToggleSelect(summary.key)} disabled={loading} />
    <button type="button" className="key-row-open" aria-label={summary.key || "（空键）"}
      aria-pressed={selected} onClick={() => onSelect(summary.key)} disabled={loading}>
      <span className="key-row-main">
        <code title={summary.key}>{label ?? (summary.key || "（空键）")}</code>
        {summary.key_type && <span className="key-row-type">{keyTypeLabel(summary.key_type)}</span>}
      </span>
    </button>
  </div>;
}

interface KeyFolder {
  segment: string;
  prefix: string;
  folders: Map<string, KeyFolder>;
  leaves: { summary: KeySummary; label: string }[];
  count: number;
}

function buildFolders(keys: KeySummary[], separator: string): KeyFolder {
  const root: KeyFolder = { segment: "", prefix: "", folders: new Map(), leaves: [], count: keys.length };
  const folders = [root];
  for (const summary of keys) {
    const segments = separator ? summary.key.split(separator) : [summary.key];
    let folder = root;
    // Keep deeply delimited names usable without creating unbounded nesting.
    const folderDepth = Math.min(segments.length - 1, 32);
    for (let index = 0; index < folderDepth; index += 1) {
      const segment = segments[index];
      let next = folder.folders.get(segment);
      if (!next) {
        next = { segment, prefix: `${folder.prefix}${segment}${separator}`, folders: new Map(), leaves: [], count: 0 };
        folder.folders.set(segment, next);
        folders.push(next);
      }
      next.count += 1;
      folder = next;
    }
    folder.leaves.push({ summary, label: segments.slice(folderDepth).join(separator) || "（空段）" });
  }
  // SCAN order is unstable. Sort each level once when the scanned keys change.
  for (const folder of folders) {
    folder.folders = new Map([...folder.folders].sort(([left], [right]) => left.localeCompare(right, "en")));
    folder.leaves.sort((left, right) => left.summary.key.localeCompare(right.summary.key, "en"));
  }
  return root;
}

export function KeyTree({ keys, separator = ":", selectedKey, selectedKeys, loading, onSelect, onToggleSelect }: KeyRowsProps) {
  const root = useMemo(() => buildFolders(keys, separator), [keys, separator]);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const checked = useMemo(() => new Set(selectedKeys), [selectedKeys]);
  type TreeRow = { id: string; depth: number } & (
    { folder: KeyFolder } | { summary: KeySummary; label: string }
  );
  const rows = useMemo(() => {
    const visible: TreeRow[] = [];
    const visit = (folder: KeyFolder, depth: number) => {
      for (const child of folder.folders.values()) {
        visible.push({ id: `folder:${child.prefix}`, folder: child, depth });
        if (expanded.has(child.prefix)) visit(child, depth + 1);
      }
      for (const leaf of folder.leaves) {
        visible.push({ id: `key:${leaf.summary.key}`, ...leaf, depth });
      }
    };
    visit(root, 0);
    return visible;
  }, [root, expanded]);

  return <div className="key-tree-panel">
    <p className="browser-helper">{separator ? `按 ${separator} 前缀自动分类，目录优先，按名称排序。` : "按完整键名排序。"}数量为匹配键总数。</p>
    <VirtualKeyRows items={rows} label="Redis 键树" rowKey={(row) => row.id} rowDepth={(row) => row.depth}
      renderRow={(row) => {
        if (!("folder" in row)) return <KeyRow summary={row.summary} label={row.depth ? row.label : undefined}
          selected={selectedKey === row.summary.key} checked={checked.has(row.summary.key)} loading={loading}
          onSelect={onSelect} onToggleSelect={onToggleSelect} />;
        const child = row.folder;
        return <button type="button" className="key-tree-folder" aria-expanded={expanded.has(child.prefix)}
          aria-label={`${expanded.has(child.prefix) ? "折叠" : "展开"}前缀 ${child.prefix}`}
          title={child.prefix} onClick={() => setExpanded((current) => {
            const next = new Set(current);
            if (next.has(child.prefix)) next.delete(child.prefix); else next.add(child.prefix);
            return next;
          })}>
          <span aria-hidden="true">{expanded.has(child.prefix) ? "▾" : "▸"}</span>
          <code>{child.segment || "（空段）"}{separator}</code>
          <span className="key-tree-count">{child.count}</span>
        </button>;
      }} />
  </div>;
}
