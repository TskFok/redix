import { useState } from "react";
import type { KeySummary } from "../../lib/types";
import { KeyRow, KeyTree } from "./KeyTree";

interface KeyListProps {
  pattern: string;
  keyType: string;
  keys: KeySummary[];
  selectedKey: string | null;
  selectedKeys: string[];
  arraySupported?: boolean;
  vectorSetSupported?: boolean;
  hasMore: boolean;
  loading: boolean;
  onPatternChange: (pattern: string) => void;
  onPatternKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onKeyTypeChange: (keyType: string) => void;
  onSelect: (key: string) => void;
  onToggleSelect: (key: string) => void;
  onLoadMore: () => void;
}

export function KeyList({
  pattern,
  keyType,
  keys,
  selectedKey,
  selectedKeys,
  arraySupported = false,
  vectorSetSupported = false,
  hasMore,
  loading,
  onPatternChange,
  onPatternKeyDown,
  onKeyTypeChange,
  onSelect,
  onToggleSelect,
  onLoadMore,
}: KeyListProps) {
  const [view, setView] = useState<"flat" | "tree">("tree");
  const [separator, setSeparator] = useState(":");
  return (
    <section className="browser-list-panel" aria-labelledby="key-list-title">
      <div className="browser-panel-heading">
        <div>
          <p className="eyebrow">SCAN</p>
          <h2 id="key-list-title">键列表</h2>
        </div>
        <span className="browser-count" aria-label={`当前 ${keys.length} 个键`}>
          {keys.length} 个
        </span>
      </div>

      <label className="browser-filter field">
        <span>键过滤</span>
        <input
          value={pattern}
          onChange={(event) => onPatternChange(event.target.value)}
          onKeyDown={onPatternKeyDown}
          aria-describedby="key-filter-hint"
          disabled={loading}
          spellCheck={false}
        />
      </label>
      <p id="key-filter-hint" className="browser-helper">
        支持 Redis glob 模式，输入后自动刷新，也可按 Enter 立即扫描。
      </p>

      <label className="browser-filter browser-filter-type field">
        <span>类型过滤</span>
        <select
          aria-label="类型过滤"
          value={keyType}
          onChange={(event) => onKeyTypeChange(event.target.value)}
          disabled={loading}
        >
          <option value="">全部类型</option>
          <option value="string">String</option>
          <option value="hash">Hash</option>
          <option value="list">List</option>
          <option value="set">Set</option>
          <option value="zset">Sorted Set</option>
          <option value="stream">Stream</option>
          <option value="json">JSON</option>
          {arraySupported ? <option value="array">Array</option> : null}
          {vectorSetSupported ? <option value="vectorset">Vector Set</option> : null}
        </select>
      </label>

      <div className="key-view-switch" role="group" aria-label="键显示方式">
        <button type="button" className="button button-secondary" aria-pressed={view === "flat"} onClick={() => setView("flat")}>平铺</button>
        <button type="button" className="button button-secondary" aria-pressed={view === "tree"} onClick={() => setView("tree")}>树形</button>
      </div>

      {view === "tree" ? <label className="field browser-filter"><span>键树分隔符</span><input value={separator} maxLength={16} onChange={(event) => setSeparator(event.target.value)} placeholder="留空显示完整键名" /></label> : null}

      {loading ? (
        <p className="loading-state browser-loading" role="status" aria-live="polite">
          正在扫描键…
        </p>
      ) : null}

      {keys.length === 0 && !loading ? (
        <p className="browser-empty-list">没有匹配的键。</p>
      ) : view === "tree" ? (
        <KeyTree key={JSON.stringify([pattern, keyType, separator])} separator={separator} keys={keys} selectedKey={selectedKey}
          selectedKeys={selectedKeys} loading={loading} onSelect={onSelect} onToggleSelect={onToggleSelect} />
      ) : (
        <ul className="key-list" aria-label="Redis 键列表">
          {keys.map((summary) => {
            const selected = summary.key === selectedKey;
            const checked = selectedKeys.includes(summary.key);
            return (
              <li key={summary.key}>
                <KeyRow summary={summary} selected={selected} checked={checked} loading={loading}
                  onSelect={onSelect} onToggleSelect={onToggleSelect} />
              </li>
            );
          })}
        </ul>
      )}

      {hasMore ? (
        <button
          type="button"
          className="button button-secondary browser-load-more"
          onClick={onLoadMore}
          disabled={loading}
        >
          {loading ? "加载中…" : "加载更多"}
        </button>
      ) : null}
    </section>
  );
}

export default KeyList;
