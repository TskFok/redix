import type { CommandDefinition } from "../../lib/types";

interface CommandSuggestionsProps {
  suggestions: CommandDefinition[];
  activeIndex: number;
  onSelect: (command: CommandDefinition) => void;
}

export function filterCommandCatalog(
  command: string,
  catalog: CommandDefinition[],
): CommandDefinition[] {
  const firstLine = command.split(/\r?\n/).pop()?.trim() ?? "";
  if (firstLine.length === 0 || /\s/.test(firstLine)) {
    return [];
  }
  const prefix = firstLine.toUpperCase();
  return catalog
    .filter((item) => item.name.toUpperCase().startsWith(prefix))
    .slice(0, 6);
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
