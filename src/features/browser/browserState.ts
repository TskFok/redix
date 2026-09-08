import type {
  KeyInfo,
  KeySummary,
  KeyValue,
  JsonValue,
  ModuleCapabilities,
  RedisValue,
} from "../../lib/types";

export type { JsonValue };

export interface BrowserPageState {
  pattern: string;
  keyType: string;
  // Complete scan results, including keys hidden by the current type filter.
  keys: KeySummary[];
  selectedKey: string | null;
  selectedKeys: string[];
  detail: KeyValue | null;
  metadata: KeyInfo | null;
  loading: boolean;
  scanFailed: boolean;
  error: string | null;
}

export type ModuleProbeState =
  | { status: "loading"; capabilities: null }
  | { status: "ready"; capabilities: ModuleCapabilities }
  | { status: "failed"; capabilities: null };

export const initialBrowserPageState: BrowserPageState = {
  pattern: "*",
  keyType: "",
  keys: [],
  selectedKey: null,
  selectedKeys: [],
  detail: null,
  metadata: null,
  loading: false,
  scanFailed: false,
  error: null,
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

export function filterKeysByType(keys: KeySummary[], keyType: string): KeySummary[] {
  return keys.filter((summary) => matchesKeyType(summary, keyType));
}

export function applyKeyTypeFilter(current: BrowserPageState, keyType: string): BrowserPageState {
  const visibleNames = new Set(filterKeysByType(current.keys, keyType).map((key) => key.key));
  const selectedKey = current.selectedKey !== null && visibleNames.has(current.selectedKey)
    ? current.selectedKey : null;
  return {
    ...current,
    keyType,
    selectedKey,
    selectedKeys: current.selectedKeys.filter((key) => visibleNames.has(key)),
    detail: selectedKey === null ? null : current.detail,
    metadata: selectedKey === null ? null : current.metadata,
  };
}

function matchesRedisPattern(pattern: string, key: string): boolean {
  // Redis glob operators match bytes, so a UTF-8 character can occupy multiple
  // question marks. Avoid translating user input into a regular expression.
  const encoder = new TextEncoder();
  const patternBytes = encoder.encode(pattern);
  const keyBytes = encoder.encode(key);
  let patternIndex = 0;
  let keyIndex = 0;
  let starPattern = -1;
  let starKey = 0;

  while (keyIndex < keyBytes.length) {
    const token = patternBytes[patternIndex];
    if (token === 42) { // *
      starPattern = ++patternIndex;
      starKey = keyIndex;
      continue;
    }

    let nextPattern = patternIndex + 1;
    let matches = token === 63 || token === keyBytes[keyIndex]; // ? or literal
    if (token === 92 && nextPattern < patternBytes.length) { // escaped literal
      matches = patternBytes[nextPattern++] === keyBytes[keyIndex];
    } else if (token === 91) { // character class
      const negated = patternBytes[nextPattern] === 94; // ^
      if (negated) nextPattern++;
      matches = false;
      while (nextPattern < patternBytes.length && patternBytes[nextPattern] !== 93) {
        const start = patternBytes[nextPattern];
        if (start === 92 && nextPattern + 1 < patternBytes.length) {
          matches ||= patternBytes[nextPattern + 1] === keyBytes[keyIndex];
          nextPattern += 2;
        } else if (nextPattern + 2 < patternBytes.length && patternBytes[nextPattern + 1] === 45) {
          const end = patternBytes[nextPattern + 2];
          matches ||= keyBytes[keyIndex] >= Math.min(start, end)
            && keyBytes[keyIndex] <= Math.max(start, end);
          nextPattern += 3;
        } else {
          matches ||= start === keyBytes[keyIndex];
          nextPattern++;
        }
      }
      if (nextPattern < patternBytes.length) nextPattern++;
      if (negated) matches = !matches;
    }

    if (matches) {
      patternIndex = nextPattern;
      keyIndex++;
    } else if (starPattern !== -1) {
      // Retry only the latest star, without recursion or exponential backtracking.
      patternIndex = starPattern;
      keyIndex = ++starKey;
    } else {
      return false;
    }
  }

  while (patternBytes[patternIndex] === 42) patternIndex++;
  return patternIndex === patternBytes.length;
}

export function applyCreatedKey(current: BrowserPageState, detail: KeyValue): BrowserPageState {
  if (!matchesRedisPattern(current.pattern.trim() || "*", detail.key)) {
    return current;
  }

  const keysByName = new Map(current.keys.map((summary) => [summary.key, summary]));
  keysByName.set(detail.key, {
    key: detail.key,
    key_type: detail.key_type,
    ttl_ms: detail.ttl_ms,
    size: null,
  });
  const selected = current.selectedKey === detail.key;
  return applyKeyTypeFilter({
    ...current,
    keys: [...keysByName.values()],
    detail: selected ? detail : current.detail,
    metadata: selected ? null : current.metadata,
  }, current.keyType);
}

export function applyScanResult(
  current: BrowserPageState,
  keys: KeySummary[],
): BrowserPageState {
  const keysByName = new Map(keys.map((summary) => [summary.key, summary]));
  return {
    ...current,
    keys: [...keysByName.values()],
    selectedKey: null,
    selectedKeys: [],
    detail: null,
    metadata: null,
    loading: false,
    scanFailed: false,
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
