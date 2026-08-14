import type { KeySummary } from "../../lib/types";
import { formatSize, formatTtl, keyTypeLabel } from "./browserState";

interface KeyListProps {
  pattern: string;
  keys: KeySummary[];
  selectedKey: string | null;
  hasMore: boolean;
  loading: boolean;
  onPatternChange: (pattern: string) => void;
  onPatternKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onSelect: (key: string) => void;
  onLoadMore: () => void;
}

export function KeyList({
  pattern,
  keys,
  selectedKey,
  hasMore,
  loading,
  onPatternChange,
  onPatternKeyDown,
  onSelect,
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
            return (
              <li key={summary.key}>
                <button
                  type="button"
                  className={`key-row${selected ? " key-row-selected" : ""}`}
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
