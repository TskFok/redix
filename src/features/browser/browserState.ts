import type {
  KeyInfo,
  KeySummary,
  KeyValue,
  JsonValue,
  ModuleCapabilities,
  RedisValue,
  ScanPage,
  ScanCursor,
  NodeFailure,
} from "../../lib/types";

export type { JsonValue };

export interface BrowserPageState {
  pattern: string;
  keyType: string;
  cursor: ScanCursor;
  nodeFailures: NodeFailure[];
  keys: KeySummary[];
  selectedKey: string | null;
  selectedKeys: string[];
  detail: KeyValue | null;
  metadata: KeyInfo | null;
  loading: boolean;
  error: string | null;
  hasMore: boolean;
}

export type ModuleProbeState =
  | { status: "loading"; capabilities: null }
  | { status: "ready"; capabilities: ModuleCapabilities }
  | { status: "failed"; capabilities: null };

export const initialBrowserPageState: BrowserPageState = {
  pattern: "*",
  keyType: "",
  cursor: 0,
  nodeFailures: [],
  keys: [],
  selectedKey: null,
  selectedKeys: [],
  detail: null,
  metadata: null,
  loading: false,
  error: null,
  hasMore: false,
};

function matchesKeyType(summary: KeySummary, keyType: string): boolean {
  const requested = keyType.trim().toLowerCase();
  if (requested === "") {
    return true;
  }

  const actual = summary.key_type.trim().toLowerCase();
  if (requested === "json") {
    return actual === "json" || actual === "rejson-rl" || actual === "rejson-rs";
  }
  if (requested === "zset" || requested === "sortedset" || requested === "sorted-set") {
    return actual === "zset" || actual === "sortedset" || actual === "sorted-set";
  }
  return actual === requested;
}

export function applyScanPage(
  current: BrowserPageState,
  page: ScanPage,
  replace: boolean,
  keyType = current.keyType,
): BrowserPageState {
  const filteredPageKeys = page.keys.filter((summary) => matchesKeyType(summary, keyType));
  const mergedKeys = replace ? filteredPageKeys : [...current.keys, ...filteredPageKeys];
  const keysByName = new Map<string, KeySummary>();
  for (const key of mergedKeys) {
    keysByName.set(key.key, key);
  }
  const selectedKeys = replace
    ? []
    : current.selectedKeys.filter((key) => keysByName.has(key));
  // A successful page need not visit every failed node. Keep warnings until a
  // fresh scan or a complete traversal confirms recovery.
  const nodeFailures = new Map<string, NodeFailure>();
  if (!replace && page.has_more) {
    for (const failure of current.nodeFailures) nodeFailures.set(failure.node_id, failure);
  }
  for (const failure of page.node_failures) nodeFailures.set(failure.node_id, failure);

  return {
    ...current,
    keyType,
    cursor: page.cursor,
    keys: [...keysByName.values()],
    selectedKey: replace ? null : current.selectedKey,
    selectedKeys,
    detail: replace ? null : current.detail,
    metadata: replace ? null : current.metadata,
    hasMore: page.has_more,
    nodeFailures: [...nodeFailures.values()],
    loading: false,
    error: null,
  };
}

export function cloneRedisValue(value: RedisValue): RedisValue {
  if ("String" in value) {
    return { String: { value: value.String.value } };
  }
  if ("Hash" in value) {
    return {
      Hash: {
        fields: value.Hash.fields.map((entry) => ({ ...entry })),
      },
    };
  }
  if ("List" in value) {
    return { List: { items: [...value.List.items] } };
  }
  if ("Set" in value) {
    return { Set: { members: [...value.Set.members] } };
  }
  if ("SortedSet" in value) {
    return {
      SortedSet: {
        members: value.SortedSet.members.map((entry) => ({ ...entry })),
      },
    };
  }
  if ("Json" in value) {
    return {
      Json: {
        value: JSON.parse(JSON.stringify(value.Json.value)),
      },
    };
  }
  if ("Array" in value) {
    return { Array: { ...value.Array } };
  }
  if ("VectorSet" in value) {
    return { VectorSet: { ...value.VectorSet } };
  }
  return {
    Stream: {
      entries: value.Stream.entries.map((entry) => ({
        id: entry.id,
        fields: entry.fields.map((field) => ({ ...field })),
      })),
    },
  };
}

export type RedisValueKind =
  | "string"
  | "hash"
  | "list"
  | "set"
  | "sorted-set"
  | "json"
  | "stream"
  | "array"
  | "vectorset";

export function redisValueKind(value: RedisValue): RedisValueKind {
  if ("String" in value) {
    return "string";
  }
  if ("Hash" in value) {
    return "hash";
  }
  if ("List" in value) {
    return "list";
  }
  if ("Set" in value) {
    return "set";
  }
  if ("SortedSet" in value) {
    return "sorted-set";
  }
  if ("Json" in value) {
    return "json";
  }
  if ("Array" in value) {
    return "array";
  }
  if ("VectorSet" in value) {
    return "vectorset";
  }
  return "stream";
}

export function keyTypeLabel(keyType: string): string {
  switch (keyType.toLowerCase()) {
    case "string":
      return "String";
    case "hash":
      return "Hash";
    case "list":
      return "List";
    case "set":
      return "Set";
    case "zset":
    case "sortedset":
    case "sorted-set":
      return "Sorted Set";
    case "stream":
      return "Stream";
    case "rejson-rl":
    case "rejson-rs":
    case "json":
      return "JSON";
    case "array":
      return "Array";
    case "vectorset":
    case "vector-set":
      return "Vector Set";
    default:
      return keyType || "未知";
  }
}

export function formatTtl(ttlMs: number): string {
  if (ttlMs === -1) {
    return "永久";
  }
  if (ttlMs === -2) {
    return "已过期";
  }
  if (ttlMs < 0) {
    return "未知";
  }
  return `${ttlMs} ms`;
}

export function formatSize(size: number | null): string {
  return size === null ? "—" : String(size);
}

export function formatJsonValue(value: JsonValue | null): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

export function jsonPathUnavailableMessage(state: ModuleProbeState): string | null {
  if (state.status === "failed") {
    return "RedisJSON 路径编辑器暂不可用。";
  }
  if (state.status === "ready" && !state.capabilities.json_supported) {
    return "RedisJSON 路径编辑器暂不可用。";
  }
  return null;
}

export function jsonPathErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    switch (code) {
      case "JSON_PATH_NOT_FOUND":
        return "未找到匹配的 JSON Path。";
      case "JSON_PATH_INVALID":
        return "JSON Path 格式无效。";
      case "JSON_VALUE_INVALID":
        return "JSON 值格式无效。";
      default:
        return browserErrorMessage(error, fallback);
    }
  }

  return fallback;
}

export function browserErrorMessage(
  error: unknown,
  fallback: string,
): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    switch (code) {
      case "AUTHENTICATION_FAILED":
        return "Redis 身份验证失败，请检查用户名和密码。";
      case "CLUSTER_TOPOLOGY_FAILED":
        return "无法读取 Cluster 拓扑，请检查连接后刷新。";
      case "CLUSTER_NODE_UNAVAILABLE":
        return "部分 Cluster 节点不可用，请检查节点连接后重试。";
      case "PARTIAL_FAILURE":
        return "部分节点操作失败，当前结果不完整，请重试。";
      case "CROSS_SLOT":
        return "这些键位于不同槽位，无法在同一操作中处理。";
      case "UNSUPPORTED_FEATURE":
        return "当前连接拓扑暂不支持此功能。";
      case "CONNECTION_FAILED":
        return "Redis 连接已断开，请重新打开连接。";
      case "UNSUPPORTED_DATA_TYPE":
        return "当前 Redis 数据类型暂不支持。";
      case "COMMAND_FAILED":
        return fallback;
      case "IPC_ERROR":
        return "桌面端调用失败，请稍后重试。";
      default:
        return fallback;
    }
  }

  return fallback;
}
