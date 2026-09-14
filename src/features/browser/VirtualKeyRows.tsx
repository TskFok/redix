import { useLayoutEffect, useRef, useState, type Key, type ReactNode } from "react";

// Must match the fixed row height in browserTrees.css.
const ROW_HEIGHT = 48;
const OVERSCAN = 6;

interface VirtualKeyRowsProps<T> {
  items: readonly T[];
  label: string;
  rowKey(item: T): Key;
  renderRow(item: T): ReactNode;
  rowDepth?(item: T): number;
}

export default function VirtualKeyRows<T>({ items, label, rowKey, renderRow, rowDepth }: VirtualKeyRowsProps<T>) {
  const viewport = useRef<HTMLUListElement>(null);
  const [height, setHeight] = useState(480);
  const [scrollTop, setScrollTop] = useState(0);
  const [focusRequest, setFocusRequest] = useState<{ index: number } | null>(null);
  const maxScroll = Math.max(0, items.length * ROW_HEIGHT - height);
  const offset = Math.max(0, Math.min(scrollTop, maxScroll));
  const start = Math.max(0, Math.floor(offset / ROW_HEIGHT) - OVERSCAN);
  const end = Math.min(items.length, Math.ceil((offset + height) / ROW_HEIGHT) + OVERSCAN);

  useLayoutEffect(() => {
    const element = viewport.current!;
    const measure = () => {
      if (element.clientHeight > 0) setHeight(element.clientHeight);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);

  useLayoutEffect(() => {
    // A filter, refresh or collapsed folder can shorten the list while scrolled down.
    if (scrollTop !== offset) {
      viewport.current!.scrollTop = offset;
      setScrollTop(offset);
    }
  }, [scrollTop, offset]);

  useLayoutEffect(() => {
    if (focusRequest === null) return;
    viewport.current?.querySelector<HTMLElement>(
      `[data-virtual-index="${focusRequest.index}"] .key-row-open, [data-virtual-index="${focusRequest.index}"] .key-tree-folder`,
    )?.focus({ preventScroll: true });
  }, [focusRequest]);

  return <ul ref={viewport} className="key-list key-virtual-list" aria-label={label} tabIndex={0}
    onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    onKeyDown={(event) => {
      if (!items.length || !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const row = (event.target as HTMLElement).closest<HTMLElement>("[data-virtual-index]");
      const current = row ? Number(row.dataset.virtualIndex) : -1;
      const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
        : Math.max(0, Math.min(items.length - 1, current + (event.key === "ArrowDown" ? 1 : -1)));
      const nextOffset = Math.min(maxScroll, Math.max(0,
        next * ROW_HEIGHT < offset ? next * ROW_HEIGHT
          : (next + 1) * ROW_HEIGHT > offset + height ? (next + 1) * ROW_HEIGHT - height : offset));
      viewport.current!.scrollTop = nextOffset;
      setScrollTop(nextOffset);
      setFocusRequest({ index: next });
    }}>
    {start > 0 && <li role="presentation" aria-hidden="true" style={{ height: start * ROW_HEIGHT }} />}
    {items.slice(start, end).map((item, index) => {
      const depth = rowDepth?.(item) ?? 0;
      return <li key={rowKey(item)} className="key-virtual-row" data-virtual-index={start + index}
        aria-posinset={start + index + 1} aria-setsize={items.length} aria-level={depth + 1}
        style={{ height: ROW_HEIGHT, paddingInlineStart: depth * 14 }}>
        {renderRow(item)}
      </li>;
    })}
    {end < items.length && <li role="presentation" aria-hidden="true" style={{ height: (items.length - end) * ROW_HEIGHT }} />}
  </ul>;
}
