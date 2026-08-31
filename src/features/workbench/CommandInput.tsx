interface CommandInputProps {
  value: string;
  canExecute: boolean;
  loading: boolean;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onKeyDown?: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  onCursorChange?: (position: number) => void;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
}

export function CommandInput({
  value,
  canExecute,
  loading,
  onChange,
  onSubmit,
  onKeyDown,
  onCursorChange,
  inputRef,
}: CommandInputProps) {
  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSubmit();
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented) {
      return;
    }
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      onSubmit();
    }
  };

  return (
    <form className="command-form" onSubmit={handleSubmit}>
      <label className="field" htmlFor="redis-command-input">
        <span>Redis 命令</span>
        <textarea
          id="redis-command-input"
          ref={inputRef}
          value={value}
          onChange={(event) => {
            onChange(event.target.value);
            onCursorChange?.(event.target.selectionStart);
          }}
          onSelect={(event) => onCursorChange?.(event.currentTarget.selectionStart)}
          onClick={(event) => onCursorChange?.(event.currentTarget.selectionStart)}
          onKeyUp={(event) => onCursorChange?.(event.currentTarget.selectionStart)}
          onKeyDown={handleKeyDown}
          aria-describedby="redis-command-helper"
          placeholder="例如：PING 或 GET app:session"
          spellCheck={false}
          rows={7}
          disabled={loading}
        />
      </label>
      <div className="command-actions">
        <p id="redis-command-helper" className="command-shortcut">
          使用 Cmd/Ctrl + Enter 执行；每行一条命令，# 或 // 开头为注释
        </p>
        <button
          type="submit"
          className="button button-primary button-compact"
          disabled={!canExecute || loading}
        >
          {loading ? "执行中…" : "执行"}
        </button>
      </div>
    </form>
  );
}

export default CommandInput;
