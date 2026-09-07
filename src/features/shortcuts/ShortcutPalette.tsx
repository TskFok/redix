export interface ShortcutAction {
  id: string;
  label: string;
  description: string;
  shortcut?: { key: string; shift?: boolean; label: string };
  unavailable?: () => string | undefined;
  run: () => void;
}

function composing(event: { isComposing?: boolean; keyCode: number }, active: boolean) {
  return active || event.isComposing || event.keyCode === 229;
}

function editing(target: EventTarget | null) {
  return target instanceof Element && Boolean(target.closest(
    'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="combobox"]',
  ));
}

export default function ShortcutPalette({ actions }: { actions: ShortcutAction[] }) {
  const [opened, setOpened] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const pendingAction = useRef<string | null>(null);
  const wasOpened = useRef(false);
  const isComposing = useRef(false);
  const currentActions = useRef(actions);
  const baseId = useId();
  const listId = `${baseId}-actions`;

  useLayoutEffect(() => { currentActions.current = actions; }, [actions]);
  const restoreFocus = () => {
    const previous = previousFocus.current;
    if (previous?.isConnected && !previous.matches(":disabled")) previous.focus();
    else trigger.current?.focus();
  };
  const open = () => {
    previousFocus.current = document.activeElement instanceof HTMLElement && document.activeElement !== document.body
      ? document.activeElement : trigger.current;
    pendingAction.current = null;
    setQuery("");
    setSelectedId(null);
    setOpened(true);
  };
  const close = (action?: ShortcutAction) => {
    if (action?.unavailable?.()) return;
    // Closing with the mouse can remove the composing input before it emits
    // compositionend, so do not retain that state after the dialog disappears.
    isComposing.current = false;
    pendingAction.current = action?.id ?? null;
    setOpened(false);
  };

  useLayoutEffect(() => {
    if (opened) input.current?.focus();
    else if (wasOpened.current) {
      const action = currentActions.current.find((item) => item.id === pendingAction.current);
      pendingAction.current = null;
      restoreFocus();
      if (action && !action.unavailable?.()) action.run();
    }
    wasOpened.current = opened;
  }, [opened]);

  useEffect(() => {
    const startComposition = () => { isComposing.current = true; };
    const endComposition = () => { isComposing.current = false; };
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || event.altKey || composing(event, isComposing.current)) return;
      if (event.ctrlKey === event.metaKey) return;
      const key = event.key.toLowerCase();
      if (key === "k" && !event.shiftKey) {
        const otherDialog = event.target instanceof Element && event.target.closest('[role="dialog"], dialog');
        if (otherDialog && otherDialog !== dialog.current) return;
        event.preventDefault();
        if (wasOpened.current) close(); else open();
        return;
      }
      if (wasOpened.current || editing(event.target)) return;
      const action = currentActions.current.find((item) => item.shortcut
        && item.shortcut.key === key && Boolean(item.shortcut.shift) === event.shiftKey);
      if (!action || action.unavailable?.()) return;
      event.preventDefault();
      action.run();
    };
    window.addEventListener("keydown", keydown);
    window.addEventListener("compositionstart", startComposition);
    window.addEventListener("compositionend", endComposition);
    return () => {
      window.removeEventListener("keydown", keydown);
      window.removeEventListener("compositionstart", startComposition);
      window.removeEventListener("compositionend", endComposition);
      if (wasOpened.current) restoreFocus();
    };
  }, []);

  const words = query.trim().toLocaleLowerCase().split(/\s+/u);
  const visible = actions.filter((action) => words.every((word) =>
    `${action.label} ${action.description}`.toLocaleLowerCase().includes(word)));
  const enabled = visible.filter((action) => !action.unavailable?.());
  const selected = enabled.find((action) => action.id === selectedId) ?? enabled[0];
  const optionId = (id: string) => `${baseId}-${id}`;
  useEffect(() => {
    if (opened && selected) document.getElementById(optionId(selected.id))?.scrollIntoView?.({ block: "nearest" });
  }, [opened, selected?.id]);

  return <>
    <button ref={trigger} type="button" className="button button-quiet shortcuts-trigger"
      aria-label="快捷键与操作" aria-haspopup="dialog" aria-expanded={opened}
      aria-keyshortcuts="Control+k Meta+k" onClick={open}>
      快捷键与操作 <kbd>Ctrl/Cmd+K</kbd>
    </button>
    {opened && <div className="shortcuts-backdrop" onMouseDown={(event) => {
      if (event.target === event.currentTarget) { event.preventDefault(); close(); }
    }}>
      <div ref={dialog} className="shortcuts-dialog" role="dialog" aria-modal="true"
        aria-labelledby={`${baseId}-title`} aria-describedby={`${baseId}-hint`}
        onKeyDown={(event) => {
          if (composing(event.nativeEvent, isComposing.current)) return;
          if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
          else if (event.key === "Tab") {
            const focusable = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)') ?? []);
            const index = focusable.indexOf(document.activeElement as HTMLElement);
            event.preventDefault();
            focusable[(index + (event.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
          }
        }}>
        <div className="shortcuts-heading">
          <h2 id={`${baseId}-title`}>快捷键与操作</h2>
          <button type="button" className="button button-quiet" aria-label="关闭操作面板" onClick={() => close()}>关闭 <kbd>Esc</kbd></button>
        </div>
        <input ref={input} className="shortcuts-search" role="combobox" aria-label="搜索操作"
          aria-controls={listId} aria-expanded="true" aria-autocomplete="list"
          aria-activedescendant={selected ? optionId(selected.id) : undefined}
          value={query} placeholder="搜索工作区或操作…" autoComplete="off" maxLength={256}
          onChange={(event) => { setQuery(event.target.value); setSelectedId(null); }}
          onKeyDown={(event) => {
            if (composing(event.nativeEvent, isComposing.current) || event.repeat) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              const index = selected ? enabled.indexOf(selected) : 0;
              const next = enabled[(index + (event.key === "ArrowDown" ? 1 : -1) + enabled.length) % enabled.length];
              setSelectedId(next?.id ?? null);
            } else if (event.key === "Enter") {
              event.preventDefault();
              if (selected) close(selected);
            }
          }} />
        <ul id={listId} className="shortcuts-list" role="listbox" aria-label="可用操作">
          {visible.map((action) => {
            const unavailable = action.unavailable?.();
            return <li key={action.id} id={optionId(action.id)} role="option"
              aria-selected={selected?.id === action.id} aria-disabled={Boolean(unavailable)}
              onMouseMove={() => { if (!unavailable) setSelectedId(action.id); }} onClick={() => close(action)}>
              <span><strong>{action.label}</strong><small>{unavailable ?? action.description}</small></span>
              {action.shortcut && <kbd>{action.shortcut.label}</kbd>}
            </li>;
          })}
        </ul>
        {!visible.length && <p role="status" className="shortcuts-empty">没有匹配的操作。</p>}
        <p id={`${baseId}-hint`} className="shortcuts-help">↑↓ 选择 · Enter 打开 · Esc 关闭。导航与聚焦快捷键在非输入状态使用；Ctrl/Cmd+K 可在输入框内打开面板。输入法组字期间不触发。</p>
        <div className="shortcuts-existing" aria-label="已有工作区快捷键">
          <span>Workbench：<kbd>Ctrl/Cmd+Enter</kbd> 执行已输入命令</span>
          <span>CLI：<kbd>Enter</kbd> 执行已输入命令</span>
          <span>键过滤 / Search 查询：<kbd>Enter</kbd> 查询</span>
        </div>
      </div>
    </div>}
  </>;
}
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import "./shortcuts.css";
