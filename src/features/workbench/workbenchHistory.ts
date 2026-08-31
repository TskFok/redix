import type {
  CommandExecutionItem,
  CommandHistoryEntry,
} from "../../lib/types";

export function filterHistoryEntry(command: string): boolean {
  // Match the backend tokenizer for quoted/escaped command names. Be conservative
  // when quoting is malformed: such commands should never enter persistent history.
  const token = command.trim().match(/^(?:"[^"]*"|'[^']*'|\\.|[^\s"'\\])+/)?.[0] ?? "";
  const commandName = token.replace(/\\(.)/g, "$1").replace(/["']/g, "").toUpperCase();
  return commandName.length > 0 && !["AUTH", "HELLO", "ACL", "CONFIG"].includes(commandName);
}

export function historyEntriesFromExecution(
  connectionId: string,
  items: CommandExecutionItem[],
  createdAt = new Date().toISOString(),
): CommandHistoryEntry[] {
  return items
    .filter((item) => filterHistoryEntry(item.command))
    .map((item, index) => ({
      connection_id: connectionId,
      command: item.command,
      result: item.result,
      error_code: item.error_code,
      created_at: new Date(Date.parse(createdAt) + index).toISOString(),
    }));
}

export function prependHistoryEntries(
  history: CommandHistoryEntry[],
  entries: CommandHistoryEntry[],
): CommandHistoryEntry[] {
  return entries.reduce(
    (current, entry) => [entry, ...current],
    history,
  ).slice(0, 100);
}
