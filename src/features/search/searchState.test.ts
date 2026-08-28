import { describe, expect, it } from "vitest";

import type { ModuleCapabilities, SearchQueryResult } from "../../lib/types";
import {
  initialSearchState,
  nextSearchOffset,
  replaceSearchResults,
  resetSearchState,
  searchCapabilityState,
  searchErrorMessage,
  type SearchRequestToken,
} from "./searchState";

const noSearch: ModuleCapabilities = {
  modules: [],
  json_supported: false,
  json_version: null,
  search_supported: false,
  search_version: null,
  array_supported: false,
  vector_set_supported: false,
};

const oldSearch: ModuleCapabilities = {
  ...noSearch,
  search_supported: true,
  search_version: "1.6.0",
};

const readySearch: ModuleCapabilities = {
  ...noSearch,
  search_supported: true,
  search_version: "2.0.0",
};

describe("RedisSearch 状态 helper", () => {
  it("按模块探测状态和版本决定功能可用性", () => {
    expect(searchCapabilityState({ status: "loading", capabilities: null }).status).toBe(
      "loading",
    );
    expect(searchCapabilityState({ status: "ready", capabilities: noSearch }).status).toBe(
      "unsupported",
    );
    expect(searchCapabilityState({ status: "ready", capabilities: oldSearch }).status).toBe(
      "unsupported",
    );
    expect(searchCapabilityState({ status: "ready", capabilities: readySearch }).status).toBe(
      "ready",
    );
  });

  it("将 IPC 错误转换为用户可理解的提示", () => {
    expect(searchErrorMessage({ code: "UNSUPPORTED_FEATURE" }, "查询失败")).toBe(
      "当前连接不支持 RedisSearch。",
    );
    expect(searchErrorMessage({ code: "CONNECTION_FAILED" }, "查询失败")).toBe(
      "Redis 连接已断开，请重新打开连接。",
    );
    expect(searchErrorMessage({ code: "IPC_ERROR" }, "查询失败")).toBe(
      "桌面端调用失败，请稍后重试。",
    );
    expect(searchErrorMessage({ code: "COMMAND_FAILED" }, "查询失败")).toBe("查询失败");
    expect(searchErrorMessage({ code: "OTHER" }, "查询失败")).toBe("查询失败");
  });

  it("根据结果总数计算下一页偏移量", () => {
    expect(nextSearchOffset(5, 0, 2)).toBe(2);
    expect(nextSearchOffset(2, 0, 2)).toBeNull();
    expect(nextSearchOffset(5, 2, 0)).toBeNull();
  });

  it("拒绝过期的查询结果覆盖当前请求", () => {
    const currentToken: SearchRequestToken = {
      connectionId: "local",
      index: "idx:users",
      requestId: 2,
    };
    const staleToken: SearchRequestToken = {
      ...currentToken,
      requestId: 1,
    };
    const result: SearchQueryResult = {
      total: 1,
      offset: 0,
      next_offset: null,
      max_results: 100,
      keys: [{ key: "user:1", key_type: "hash" }],
    };
    const current = {
      ...initialSearchState,
      requestToken: currentToken,
    };

    expect(replaceSearchResults(current, result, staleToken, currentToken)).toBe(current);
    expect(
      replaceSearchResults(current, result, currentToken, currentToken),
    ).toMatchObject({
      result,
      requestToken: currentToken,
      loading: false,
      error: null,
    });
  });

  it("可以将搜索状态恢复为干净初始状态", () => {
    expect(resetSearchState()).toEqual(initialSearchState);
  });
});
