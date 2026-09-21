import type {
  CommandExecutionItem,
  CommandHistoryEntry,
} from "../../lib/types";

const SAFE_HISTORY_COMMANDS = new Set([
  "PING", "GET", "MGET", "GETRANGE", "GETBIT", "STRLEN", "EXISTS", "TYPE", "TTL", "PTTL",
  "DBSIZE", "INFO", "SCAN", "HSCAN", "SSCAN", "ZSCAN", "RANDOMKEY", "OBJECT", "MEMORY",
  "TIME", "COMMAND", "ROLE", "LASTSAVE", "HEXISTS", "HGET", "HMGET", "HGETALL", "HKEYS",
  "HLEN", "HVALS", "HSTRLEN", "LINDEX", "LLEN", "LPOS", "LRANGE", "SCARD", "SISMEMBER",
  "SMISMEMBER", "SMEMBERS", "SRANDMEMBER", "ZCARD", "ZCOUNT", "ZRANGE", "ZRANGEBYLEX",
  "ZRANGEBYSCORE", "ZRANK", "ZREVRANGE", "ZREVRANGEBYLEX", "ZREVRANGEBYSCORE", "ZREVRANK",
  "ZMSCORE", "ZSCORE", "ZLEXCOUNT", "XINFO", "XLEN", "XRANGE", "XREVRANGE", "XREAD",
  "XREADGROUP", "XPENDING", "JSON.GET", "JSON.TYPE", "JSON.MGET", "JSON.ARRLEN", "JSON.OBJKEYS",
  "FT.SEARCH", "FT.AGGREGATE", "FT.INFO", "FT.EXPLAIN", "FT.PROFILE", "FT._LIST", "TS.GET",
  "TS.RANGE", "TS.REVRANGE", "TS.MRANGE", "TS.INFO", "TS.QUERYINDEX", "BF.EXISTS", "BF.MEXISTS",
  "BF.INFO", "CF.EXISTS", "CF.COUNT", "CF.INFO", "CMS.QUERY", "CMS.INFO", "TOPK.QUERY",
  "TOPK.LIST", "TOPK.INFO", "TDIGEST.QUANTILE", "TDIGEST.CDF", "TDIGEST.MIN", "TDIGEST.MAX",
  "TDIGEST.INFO", "VSIM", "VRANGE", "VEMB", "VGETATTR", "VCARD", "VDIM", "VINFO", "ARGET",
  "ARMGET", "ARGETRANGE", "ARLEN",
]);

export function filterHistoryEntry(command: string): boolean {
  // Match the backend tokenizer for quoted/escaped command names. Be conservative
  // when quoting is malformed: such commands should never enter persistent history.
  const token = command.trim().match(/^(?:"[^"]*"|'[^']*'|\\.|[^\s"'\\])+/)?.[0] ?? "";
  const commandName = token.replace(/\\(.)/g, "$1").replace(/["']/g, "").toUpperCase();
  return SAFE_HISTORY_COMMANDS.has(commandName);
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
      result: null,
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
