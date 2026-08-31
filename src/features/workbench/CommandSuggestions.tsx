import type { CommandDefinition } from "../../lib/types";

interface CommandSuggestionsProps {
  suggestions: CommandDefinition[];
  activeIndex: number;
  onSelect: (command: CommandDefinition) => void;
}

export function filterCommandCatalog(
  command: string,
  catalog: CommandDefinition[],
  cursor = command.length,
): CommandDefinition[] {
  const firstLine = currentCommandLine(command, cursor).text.trim();
  if (firstLine.length === 0 || /\s/.test(firstLine)) {
    return [];
  }
  const prefix = firstLine.toUpperCase();
  return catalog
    .filter((item) => item.name.toUpperCase().startsWith(prefix))
    .slice(0, 6);
}

export function currentCommandLine(command: string, cursor: number) {
  const position = Math.max(0, Math.min(cursor, command.length));
  const start = position === 0 ? 0 : command.lastIndexOf("\n", position - 1) + 1;
  const nextLine = command.indexOf("\n", position);
  const end = nextLine < 0 ? command.length : nextLine;
  return { start, end, text: command.slice(start, end) };
}

export function currentCommandDefinition(command: string, cursor: number, catalog: CommandDefinition[]) {
  const line = currentCommandLine(command, cursor).text.trim();
  if (/^(#|\/\/)/.test(line)) return undefined;
  const normalized = line.toUpperCase();
  return catalog
    .filter((item) => normalized === item.name || normalized.startsWith(`${item.name} `))
    .sort((left, right) => right.name.length - left.name.length)[0];
}

export function CommandParameterHelp({ command }: { command?: CommandDefinition }) {
  if (!command) return null;
  return (
    <section aria-label="当前命令参数" className="command-parameter-help">
      <strong>{command.name}</strong> · {command.summary}
      <p><code>{command.name} {command.arguments.map((argument) =>
        argument.required ? `<${argument.name}>` : `[${argument.name}]`,
      ).join(" ")}</code></p>
      {command.arguments.length > 0 ? <dl>
        {command.arguments.map((argument) => <div key={argument.name}>
          <dt><code>{argument.name}</code> · {argument.required ? "必填" : "可选"}</dt>
          <dd>{argument.hint}</dd>
        </div>)}
      </dl> : <p>无必填参数。</p>}
      <small>本地目录仅提供语法帮助，实际可用性取决于服务器版本和模块。</small>
    </section>
  );
}

export function CommandSuggestions({
  suggestions,
  activeIndex,
  onSelect,
}: CommandSuggestionsProps) {
  if (suggestions.length === 0) {
    return null;
  }

  return (
    <ul className="command-suggestions" role="listbox" aria-label="命令提示">
      {suggestions.map((suggestion, index) => (
        <li
          key={suggestion.name}
          role="option"
          aria-selected={index === activeIndex}
          className={index === activeIndex ? "command-suggestion-active" : undefined}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => onSelect(suggestion)}
        >
          <strong>{suggestion.name}</strong>
          <span>{suggestion.summary}</span>
        </li>
      ))}
    </ul>
  );
}

export default CommandSuggestions;
