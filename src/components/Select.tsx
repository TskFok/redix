import { useId, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type SelectHTMLAttributes } from "react";
import { createPortal } from "react-dom";

interface SelectOption {
  value: string;
  label: string;
  disabled: boolean;
  selected: boolean;
}

function readOptions(select: HTMLSelectElement): SelectOption[] {
  return Array.from(select.options, (option) => ({
    value: option.value,
    label: option.label,
    disabled: option.disabled,
    selected: option.selected,
  }));
}

function initialIndex(options: SelectOption[]) {
  const selected = options.findIndex((option) => option.selected && !option.disabled);
  return selected >= 0 ? selected : options.findIndex((option) => !option.disabled);
}

function nextIndex(options: SelectOption[], current: number, direction: number) {
  for (let index = current + direction; index >= 0 && index < options.length; index += direction) {
    if (!options[index].disabled) return index;
  }
  return current;
}

/** Keep native form/label/change semantics while drawing the popup in the app theme. */
export default function Select({ children, className, disabled, value, onChange, onKeyDown, onMouseDown, onPointerDown, onClick, onBlur, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  const generatedId = useId();
  const selectId = props.id ?? `select-${generatedId}`;
  const menuId = `${selectId}-menu`;
  const selectRef = useRef<HTMLSelectElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [options, setOptions] = useState<SelectOption[]>([]);
  const [active, setActive] = useState(-1);
  const [position, setPosition] = useState<CSSProperties>({ position: "fixed" });
  const search = useRef({ text: "", time: 0 });
  const activeValue = useRef<string | undefined>(undefined);

  const close = () => {
    setOpen(false);
    search.current = { text: "", time: 0 };
  };

  const activate = (items: SelectOption[], index: number) => {
    setActive(index);
    activeValue.current = items[index]?.value;
  };

  const show = () => {
    const select = selectRef.current;
    if (!select || select.matches(":disabled")) return [];
    const items = readOptions(select);
    setOptions(items);
    activate(items, initialIndex(items));
    setOpen(true);
    select.focus({ preventScroll: true });
    return items;
  };

  const commit = (index: number) => {
    const select = selectRef.current;
    if (!select || select.matches(":disabled")) {
      close();
      return;
    }
    // Read the current DOM again so removed/disabled options cannot be committed.
    const option = select.options[index];
    if (!option || option.disabled || option.value !== options[index]?.value) return;
    close();
    select.focus({ preventScroll: true });
    if (select.value === option.value) return;
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(select, option.value);
    select.dispatchEvent(new Event("change", { bubbles: true }));
  };

  useLayoutEffect(() => {
    if (!open) return;
    const select = selectRef.current;
    if (!select || select.matches(":disabled")) {
      setOpen(false);
      return;
    }
    const items = readOptions(select);
    const preserved = items.findIndex((item) => item.value === activeValue.current && !item.disabled);
    setOptions(items);
    activate(items, preserved >= 0 ? preserved : initialIndex(items));
  }, [children, value, disabled, open]);

  // A fieldset can disable the native control without changing this component's props.
  useLayoutEffect(() => {
    if (open && selectRef.current?.matches(":disabled")) setOpen(false);
  });

  useLayoutEffect(() => {
    if (!open) return;
    const select = selectRef.current;
    const menu = menuRef.current;
    if (!select || !menu) return;

    const reposition = () => {
      const rect = select.getBoundingClientRect();
      const margin = 8;
      const gap = 6;
      const width = Math.min(rect.width, Math.max(0, window.innerWidth - margin * 2));
      const below = Math.max(0, window.innerHeight - rect.bottom - gap - margin);
      const above = Math.max(0, rect.top - gap - margin);
      const desiredHeight = Math.min(280, menu.scrollHeight || options.length * 36 + 12);
      const upward = below < desiredHeight && above > below;
      const maxHeight = Math.min(280, upward ? above : below);
      const height = Math.min(desiredHeight, maxHeight);
      setPosition({
        position: "fixed",
        width,
        maxHeight,
        left: Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin)),
        top: upward ? Math.max(margin, rect.top - gap - height) : Math.max(margin, rect.bottom + gap),
      });
    };
    const outside = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && !menu.contains(target) && !select.contains(target)) close();
    };
    const scroll = (event: Event) => {
      if (!(event.target instanceof Node) || !menu.contains(event.target)) close();
    };
    reposition();
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", scroll, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", scroll, true);
    };
  }, [open, options]);

  useLayoutEffect(() => {
    if (!open || active < 0) return;
    // Scroll only the menu: scrollIntoView can also move the surrounding workspace.
    const menu = menuRef.current;
    const option = menu?.children[active] as HTMLElement | undefined;
    if (!menu || !option) return;
    if (option.offsetTop < menu.scrollTop) menu.scrollTop = option.offsetTop;
    else if (option.offsetTop + option.offsetHeight > menu.scrollTop + menu.clientHeight) {
      menu.scrollTop = option.offsetTop + option.offsetHeight - menu.clientHeight;
    }
  }, [open, active, position]);

  const handleKeyDown = (event: KeyboardEvent<HTMLSelectElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || disabled || event.nativeEvent.isComposing) return;
    if (event.key === "Tab") {
      close();
      return;
    }
    if (event.key === "Escape") {
      if (open) {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) commit(active);
      else show();
      return;
    }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const items = open ? options : show();
      const current = open ? active : initialIndex(items);
      if (event.key === "Home") activate(items, items.findIndex((option) => !option.disabled));
      else if (event.key === "End") activate(items, nextIndex(items, items.length, -1));
      else activate(items, nextIndex(items, current, event.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      const items = open ? options : show();
      const now = Date.now();
      const typed = event.key.toLocaleLowerCase();
      const previous = now - search.current.time < 700 ? search.current.text : "";
      const text = previous === typed ? typed : previous + typed;
      search.current = { text, time: now };
      const start = text.length === 1 ? (open ? active : initialIndex(items)) + 1 : 0;
      for (let offset = 0; offset < items.length; offset += 1) {
        const index = (start + offset) % items.length;
        const option = items[index];
        if (!option.disabled && option.label.trim().toLocaleLowerCase().startsWith(text)) {
          activate(items, index);
          break;
        }
      }
    }
  };

  return <span className="app-select-control" data-open={open} data-disabled={!!disabled}>
    <select
      {...props}
      id={selectId}
      ref={selectRef}
      className={["app-select", className].filter(Boolean).join(" ")}
      disabled={disabled}
      value={value}
      aria-haspopup="listbox"
      aria-expanded={open}
      aria-controls={open ? menuId : undefined}
      aria-activedescendant={open && active >= 0 ? `${menuId}-${active}` : undefined}
      onChange={(event) => {
        close();
        onChange?.(event);
      }}
      onKeyDown={handleKeyDown}
      onPointerDown={(event) => {
        onPointerDown?.(event);
        if (event.button === 0 && !event.defaultPrevented) event.preventDefault();
      }}
      onMouseDown={(event) => {
        onMouseDown?.(event);
        if (event.button !== 0 || event.defaultPrevented) return;
        event.preventDefault();
        if (!disabled) event.currentTarget.focus({ preventScroll: true });
      }}
      onClick={(event) => {
        onClick?.(event);
        if (event.defaultPrevented || disabled) return;
        event.preventDefault();
        if (open) close();
        else show();
      }}
      onBlur={(event) => {
        onBlur?.(event);
        close();
      }}
    >{children}</select>
    <svg className="app-select-chevron" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="m4 6 4 4 4-4" /></svg>
    {open && createPortal(<div
      ref={menuRef}
      id={menuId}
      className="app-select-menu"
      style={position}
      role="listbox"
      aria-label={props["aria-label"]}
      aria-labelledby={props["aria-labelledby"] ?? (props["aria-label"] ? undefined : selectId)}
      onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); }}
      onPointerDown={(event) => { event.preventDefault(); event.stopPropagation(); }}
      onClick={(event) => { event.preventDefault(); event.stopPropagation(); }}
    >{options.map((option, index) => <div
      key={`${index}-${option.value}`}
      id={`${menuId}-${index}`}
      role="option"
      className="app-select-option"
      aria-selected={option.selected}
      aria-disabled={option.disabled}
      data-active={index === active}
      onPointerMove={() => { if (!option.disabled) activate(options, index); }}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        if (!option.disabled) commit(index);
      }}
    ><span className="app-select-option-label">{option.label}</span>{option.selected ? <svg className="app-select-check" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="m3 8 3 3 7-7" /></svg> : null}</div>)}</div>, document.body)}
  </span>;
}
