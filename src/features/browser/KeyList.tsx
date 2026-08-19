import type { KeySummary } from "../../lib/types";
import { formatSize, formatTtl, keyTypeLabel } from "./browserState";

interface KeyListProps {
  pattern: string;
  keyType: string;
  keys: KeySummary[];
  selectedKey: string | null;
  selectedKeys: string[];
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
  hasMore,
  loading,
  onPatternChange,
  onPatternKeyDown,
  onKeyTypeChange,
  onSelect,
  onToggleSelect,
  onLoadMore,
}: KeyListProps) {
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
        </select>
      </label>

      {loading ? (
        <p className="loading-state browser-loading" role="status" aria-live="polite">
          正在扫描键…
        </p>
      ) : null}

      {keys.length === 0 && !loading ? (
        <p className="browser-empty-list">没有匹配的键。</p>
      ) : (
        <ul className="key-list" aria-label="Redis 键列表">
          {keys.map((summary) => {
            const selected = summary.key === selectedKey;
            const checked = selectedKeys.includes(summary.key);
            return (
              <li key={summary.key}>
                <div className={`key-row${selected ? " key-row-selected" : ""}`}>
                  <input
                    className="key-row-checkbox"
                    type="checkbox"
                    aria-label={`选择键 ${summary.key}`}
                    checked={checked}
                    onChange={() => onToggleSelect(summary.key)}
                    disabled={loading}
                  />
                  <button
                    type="button"
                    className="key-row-open"
                    aria-label={summary.key}
                    aria-pressed={selected}
                    onClick={() => onSelect(summary.key)}
                    disabled={loading}
                  >
                    <span className="key-row-main">
                      <code title={summary.key}>{summary.key}</code>
                      <span className="key-row-type">{keyTypeLabel(summary.key_type)}</span>
                    </span>
                    <span className="key-row-meta">
                      <span>
                        <span className="sr-only">TTL </span>
                        {formatTtl(summary.ttl_ms)}
                      </span>
                      <span>
                        <span className="sr-only">大小 </span>
                        {formatSize(summary.size)}
                      </span>
                    </span>
                  </button>
                </div>
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
