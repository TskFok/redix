import type {
  KeySummary,
  KeyValue,
  RedisValue,
  ScanPage,
} from "../../lib/types";

export interface BrowserPageState {
  pattern: string;
  cursor: number;
  keys: KeySummary[];
  selectedKey: string | null;
  detail: KeyValue | null;
  loading: boolean;
  error: string | null;
  hasMore: boolean;
}

export const initialBrowserPageState: BrowserPageState = {
  pattern: "*",
  cursor: 0,
  keys: [],
  selectedKey: null,
  detail: null,
  loading: false,
  error: null,
  hasMore: false,
};

export function applyScanPage(
  current: BrowserPageState,
  page: ScanPage,
  replace: boolean,
): BrowserPageState {
  return {
    ...current,
    cursor: page.cursor,
    keys: replace ? page.keys : [...current.keys, ...page.keys],
    selectedKey: replace ? null : current.selectedKey,
    detail: replace ? null : current.detail,
    hasMore: page.has_more || page.cursor !== 0,
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
  return {
    SortedSet: {
      members: value.SortedSet.members.map((entry) => ({ ...entry })),
    },
  };
}

export type RedisValueKind =
  | "string"
  | "hash"
  | "list"
  | "set"
  | "sorted-set";

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
  return "sorted-set";
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

export function browserErrorMessage(
  error: unknown,
  fallback: string,
): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    switch (code) {
      case "AUTHENTICATION_FAILED":
        return "Redis 身份验证失败，请检查用户名和密码。";
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
