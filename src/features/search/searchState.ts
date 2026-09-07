import type {
  ModuleCapabilities,
  SearchIndexInfo,
  SearchIndexSummary,
  SearchQueryResult,
} from "../../lib/types";

export const REDISEARCH_MIN_VERSION = "2.0.0";

export type SearchProbeState =
  | { status: "loading"; capabilities: null }
  | { status: "ready"; capabilities: ModuleCapabilities }
  | { status: "failed"; capabilities: null };

export type SearchCapabilityState =
  | { status: "loading"; capabilities: null; message?: undefined }
  | { status: "ready"; capabilities: ModuleCapabilities; message?: undefined }
  | { status: "unsupported"; capabilities: ModuleCapabilities; message: string }
  | { status: "failed"; capabilities: null; message: string };

export interface SearchRequestToken {
  connectionId: string;
  index: string;
  requestId: number;
}

export interface SearchState {
  indexes: SearchIndexSummary[];
  selectedIndex: string | null;
  info: SearchIndexInfo | null;
  query: string;
  includeContent: boolean;
  offset: number;
  result: SearchQueryResult | null;
  loading: boolean;
  error: string | null;
  requestToken: SearchRequestToken | null;
}

export const initialSearchState: SearchState = {
  indexes: [],
  selectedIndex: null,
  info: null,
  query: "*",
  includeContent: false,
  offset: 0,
  result: null,
  loading: false,
  error: null,
  requestToken: null,
};

function parseVersion(version: string | null): [number, number, number] | null {
  if (!version || version.trim() === "") {
    return null;
  }

  const parts = version.trim().split(".");
  if (parts.length > 3 || parts.some((part) => !/^\d+$/.test(part))) {
    return null;
  }

  const parsed = parts.map(Number);
  return [parsed[0] ?? 0, parsed[1] ?? 0, parsed[2] ?? 0];
}

function compareVersions(left: [number, number, number], right: [number, number, number]) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] > right[index] ? 1 : -1;
    }
  }
  return 0;
}

export function vectorSearchSupported(version: string | null): boolean {
  const parsed = parseVersion(version);
  return parsed !== null && compareVersions(parsed, [2, 4, 0]) >= 0;
}

export function searchCapabilityState(state: SearchProbeState): SearchCapabilityState {
  if (state.status === "loading") {
    return state;
  }
  if (state.status === "failed") {
    return {
      status: "failed",
      capabilities: null,
      message: "RedisSearch 模块检测失败，请稍后重试。",
    };
  }

  const current = parseVersion(state.capabilities.search_version);
  const minimum = parseVersion(REDISEARCH_MIN_VERSION);
  if (!state.capabilities.search_supported || !current || !minimum || compareVersions(current, minimum) < 0) {
    return {
      status: "unsupported",
      capabilities: state.capabilities,
      message: "当前连接不支持 RedisSearch。",
    };
  }

  return state;
}

export function searchErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    switch (code) {
      case "UNSUPPORTED_FEATURE":
        return "当前连接不支持 RedisSearch。";
      case "CONNECTION_FAILED":
        return "Redis 连接已断开，请重新打开连接。";
      case "IPC_ERROR":
        return "桌面端调用失败，请稍后重试。";
      case "COMMAND_FAILED":
      default:
        return fallback;
    }
  }

  return fallback;
}

export function nextSearchOffset(
  total: number,
  offset: number,
  returned: number,
): number | null {
  if (returned <= 0) {
    return null;
  }
  const next = offset + returned;
  return next < total ? next : null;
}

function sameRequestToken(left: SearchRequestToken | null, right: SearchRequestToken | null) {
  return (
    left !== null &&
    right !== null &&
    left.connectionId === right.connectionId &&
    left.index === right.index &&
    left.requestId === right.requestId
  );
}

export function replaceSearchResults(
  current: SearchState,
  result: SearchQueryResult,
  token: SearchRequestToken,
  expectedToken: SearchRequestToken,
): SearchState {
  if (!sameRequestToken(current.requestToken, expectedToken) || !sameRequestToken(token, expectedToken)) {
    return current;
  }

  return {
    ...current,
    offset: result.offset,
    result,
    loading: false,
    error: null,
    requestToken: token,
  };
}

export function resetSearchState(): SearchState {
  return {
    ...initialSearchState,
    indexes: [],
  };
}
