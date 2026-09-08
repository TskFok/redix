import { useCallback, useRef, useState } from "react";
import type { KeySummary } from "../../lib/types";
import { KeyRow, KeyTree } from "./KeyTree";
import ScanFilterDialog from "./ScanFilterDialog";

interface KeyListProps {
  pattern: string;
  keyType: string;
  keys: KeySummary[];
  selectedKey: string | null;
  selectedKeys: string[];
  arraySupported?: boolean;
  vectorSetSupported?: boolean;
  loading: boolean;
  scanFailed: boolean;
  busy?: boolean;
  onPatternChange: (pattern: string) => void;
  onPatternKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onKeyTypeChange: (keyType: string) => void;
  onSelect: (key: string) => void;
  onToggleSelect: (key: string) => void;
}

export function KeyList({
  pattern,
  keyType,
  keys,
  selectedKey,
  selectedKeys,
  arraySupported = false,
  vectorSetSupported = false,
  loading,
  scanFailed,
  busy = false,
  onPatternChange,
  onPatternKeyDown,
  onKeyTypeChange,
  onSelect,
  onToggleSelect,
}: KeyListProps) {
  const [view, setView] = useState<"flat" | "tree">("tree");
  const [separator, setSeparator] = useState(":");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const filterTrigger = useRef<HTMLButtonElement>(null);
  const closeFilters = useCallback(() => setFiltersOpen(false), []);
  const hasFilters = (pattern.trim() || "*") !== "*" || keyType !== "";
  // Detail requests block interactions without adding a scan status above the keys.
  const controlsDisabled = loading || busy;
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

      <div className="browser-list-toolbar">
        <div className="key-view-switch" role="group" aria-label="键显示方式">
          <button type="button" className="button button-secondary" aria-pressed={view === "flat"} onClick={() => setView("flat")}>平铺</button>
          <button type="button" className="button button-secondary" aria-pressed={view === "tree"} onClick={() => setView("tree")}>树形</button>
        </div>
        <div className="browser-list-actions">
          <button ref={filterTrigger} type="button" className="button button-secondary browser-filter-trigger"
            aria-label="筛选" aria-haspopup="dialog" aria-expanded={filtersOpen} data-active={hasFilters}
            title={hasFilters ? "筛选已启用" : "筛选"}
            onClick={() => {
              filterTrigger.current?.focus({ preventScroll: true });
              setFiltersOpen(true);
            }}>
            <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M2 3h12L9 8.5V13l-2-1V8.5Z" /></svg>
            筛选
          </button>
        </div>
      </div>

      {filtersOpen ? (
        <ScanFilterDialog pattern={pattern} keyType={keyType} separator={separator} showSeparator={view === "tree"}
          arraySupported={arraySupported} vectorSetSupported={vectorSetSupported} loading={controlsDisabled}
          onPatternChange={onPatternChange} onPatternKeyDown={onPatternKeyDown} onKeyTypeChange={onKeyTypeChange}
          onSeparatorChange={setSeparator} onClose={closeFilters} />
      ) : null}

      {loading ? (
        <p className="loading-state browser-loading" role="status" aria-live="polite">
          正在扫描键…
        </p>
      ) : null}

      {loading ? null : scanFailed ? (
        <p className="browser-empty-list">键列表加载失败，请刷新重试。</p>
      ) : keys.length === 0 ? (
        <p className="browser-empty-list">
          没有匹配的键。
        </p>
      ) : view === "tree" ? (
        <KeyTree key={JSON.stringify([pattern, keyType, separator])} separator={separator} keys={keys} selectedKey={selectedKey}
          selectedKeys={selectedKeys} loading={controlsDisabled} onSelect={onSelect} onToggleSelect={onToggleSelect} />
      ) : (
        <ul className="key-list" aria-label="Redis 键列表">
          {keys.map((summary) => {
            const selected = summary.key === selectedKey;
            const checked = selectedKeys.includes(summary.key);
            return (
              <li key={summary.key}>
                <KeyRow summary={summary} selected={selected} checked={checked} loading={controlsDisabled}
                  onSelect={onSelect} onToggleSelect={onToggleSelect} />
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

export default KeyList;
