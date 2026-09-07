import { useId, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";

import Select from "../../components/Select";
import "./scanFilterDialog.css";

interface ScanFilterDialogProps {
  pattern: string;
  keyType: string;
  separator: string;
  showSeparator: boolean;
  arraySupported: boolean;
  vectorSetSupported: boolean;
  loading: boolean;
  onPatternChange: (pattern: string) => void;
  onPatternKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onKeyTypeChange: (keyType: string) => void;
  onSeparatorChange: (separator: string) => void;
  onClose: () => void;
}

export default function ScanFilterDialog({
  pattern, keyType, separator, showSeparator, arraySupported, vectorSetSupported,
  loading, onPatternChange, onPatternKeyDown, onKeyTypeChange, onSeparatorChange, onClose,
}: ScanFilterDialogProps) {
  const titleId = useId();
  const dialog = useRef<HTMLElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);

  useLayoutEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (input.current && !input.current.disabled) input.current.focus({ preventScroll: true });
    else closeButton.current?.focus({ preventScroll: true });

    // Listen after field handlers so Escape first dismisses an open Select menu.
    const keydown = (event: KeyboardEvent) => {
      event.stopPropagation();
      if (event.defaultPrevented || event.isComposing || event.keyCode === 229) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "Tab") {
        const focusable = Array.from(dialog.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), select:not(:disabled)',
        ) ?? []);
        const currentIndex = focusable.indexOf(document.activeElement as HTMLElement);
        const nextIndex = currentIndex < 0
          ? event.shiftKey ? focusable.length - 1 : 0
          : (currentIndex + (event.shiftKey ? -1 : 1) + focusable.length) % focusable.length;
        event.preventDefault();
        focusable[nextIndex]?.focus({ preventScroll: true });
      }
    };
    document.addEventListener("keydown", keydown);
    return () => {
      document.removeEventListener("keydown", keydown);
      if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
    };
  }, [onClose]);

  return createPortal(
    <div className="scan-filter-backdrop" onClick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <section ref={dialog} className="scan-filter-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <div className="scan-filter-heading">
          <h2 id={titleId}>SCAN 筛选</h2>
          <button ref={closeButton} type="button" className="button button-quiet" aria-label="关闭筛选" onClick={onClose}>
            关闭
          </button>
        </div>
        <label className="browser-filter field">
          <span>键过滤</span>
          <input ref={input} value={pattern} onChange={(event) => onPatternChange(event.target.value)}
            onKeyDown={onPatternKeyDown} aria-describedby="key-filter-hint" disabled={loading} spellCheck={false} />
        </label>
        <p id="key-filter-hint" className="browser-helper">
          支持 Redis glob 模式，输入后自动刷新，也可按 Enter 立即扫描。
        </p>
        <label className="browser-filter browser-filter-type field">
          <span>类型过滤</span>
          <Select aria-label="类型过滤" value={keyType} onChange={(event) => onKeyTypeChange(event.target.value)} disabled={loading}>
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
          </Select>
        </label>
        {showSeparator ? (
          <label className="field browser-filter">
            <span>键树分隔符</span>
            <input value={separator} maxLength={16} onChange={(event) => onSeparatorChange(event.target.value)} placeholder="留空显示完整键名" />
          </label>
        ) : null}
      </section>
    </div>, document.body,
  );
}
