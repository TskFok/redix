import { describe, expect, it } from "vitest";

import {
  applyCreatedKey,
  applyScanResult,
  browserErrorMessage,
  cloneRedisValue,
  filterKeysByType,
  initialBrowserPageState,
  keyTypeLabel,
  redisValueKind,
} from "./browserState";
import type { KeyValue, RedisValue } from "../../lib/types";

describe("Browser 状态 helper", () => {
  it("新增键直接追加摘要并保留选择、详情和筛选", () => {
    const selected = { key: "user:1", key_type: "string", ttl_ms: -1, size: 10 };
    const current = {
      ...initialBrowserPageState,
      pattern: "user:*",
      keyType: "string",
      keys: [selected],
      selectedKey: selected.key,
      selectedKeys: [selected.key],
      detail: { ...selected, value: { String: { value: "Alice" } } },
      metadata: { ...selected, memory_bytes: 64, encoding: "embstr", idle_seconds: 1 },
      loading: true,
      error: "已有提示",
    };
    const created: KeyValue = {
      key: "user:2", key_type: "string", ttl_ms: 5000, value: { String: { value: "Bob" } },
    };

    const next = applyCreatedKey(current, created);

    expect(next).toEqual({
      ...current,
      keys: [selected, { key: "user:2", key_type: "string", ttl_ms: 5000, size: null }],
    });
    expect(current.keys).toEqual([selected]);
  });

  it("同名新增结果更新摘要和已选详情，去重且清空过期元信息", () => {
    const original = { key: "user:1", key_type: "string", ttl_ms: -1, size: 10 };
    const another = { key: "user:2", key_type: "hash", ttl_ms: -1, size: 2 };
    const created: KeyValue = {
      key: "user:1", key_type: "string", ttl_ms: 5000, value: { String: { value: "Updated" } },
    };
    const current = {
      ...initialBrowserPageState,
      keys: [original, another, original],
      selectedKey: original.key,
      selectedKeys: [original.key, another.key],
      detail: { ...original, value: { String: { value: "Old" } } },
      metadata: { ...original, memory_bytes: 64, encoding: "embstr", idle_seconds: 1 },
    };

    const next = applyCreatedKey(current, created);

    expect(next.keys).toEqual([
      { key: "user:1", key_type: "string", ttl_ms: 5000, size: null }, another,
    ]);
    expect(next.selectedKey).toBe("user:1");
    expect(next.selectedKeys).toEqual(["user:1", "user:2"]);
    expect(next.detail).toEqual(created);
    expect(next.metadata).toBeNull();
  });

  it("匹配名称但不匹配类型的新键加入缓存，清空类型筛选后可见", () => {
    const original = { key: "user:1", key_type: "string", ttl_ms: -1, size: 10 };
    const next = applyCreatedKey({
      ...initialBrowserPageState,
      pattern: "user:*",
      keyType: "string",
      keys: [original],
      selectedKey: original.key,
      selectedKeys: [original.key],
    }, {
      key: "user:2", key_type: "hash", ttl_ms: -1, value: { Hash: { fields: [] } },
    });

    expect(next.keys).toEqual([
      original, { key: "user:2", key_type: "hash", ttl_ms: -1, size: null },
    ]);
    expect(filterKeysByType(next.keys, next.keyType)).toEqual([original]);
    expect(filterKeysByType(next.keys, "").map((key) => key.key)).toEqual(["user:1", "user:2"]);
    expect(next.selectedKey).toBe("user:1");
    expect(next.selectedKeys).toEqual(["user:1"]);
  });

  it("同名键类型改变后移除隐藏选择和详情并保留缓存", () => {
    const original = { key: "user:1", key_type: "string", ttl_ms: -1, size: 10 };
    const next = applyCreatedKey({
      ...initialBrowserPageState,
      keyType: "string",
      keys: [original],
      selectedKey: original.key,
      selectedKeys: [original.key],
      detail: { ...original, value: { String: { value: "Old" } } },
      metadata: { ...original, memory_bytes: 64, encoding: "embstr", idle_seconds: 1 },
    }, {
      key: "user:1", key_type: "hash", ttl_ms: -1, value: { Hash: { fields: [] } },
    });

    expect(next.keys).toEqual([{ key: "user:1", key_type: "hash", ttl_ms: -1, size: null }]);
    expect(next.selectedKey).toBeNull();
    expect(next.selectedKeys).toEqual([]);
    expect(next.detail).toBeNull();
    expect(next.metadata).toBeNull();
  });

  it.each([
    { pattern: "user:*", key: "user:1", matches: true },
    { pattern: "user:*", key: "other:1", matches: false },
    { pattern: "user:*", key: "user:", matches: true },
    { pattern: "user:*", key: "user:\n1", matches: true },
    { pattern: "user:*:active", key: "user:a:active:b:active", matches: true },
    { pattern: "a**?c*", key: "abbcd", matches: true },
    { pattern: "a*b*c", key: "abbxd", matches: false },
    { pattern: "user:?", key: "user:1", matches: true },
    { pattern: "user:?", key: "user:12", matches: false },
    { pattern: "user:[1-3]", key: "user:2", matches: true },
    { pattern: "user:[3-1]", key: "user:2", matches: true },
    { pattern: "user:[1-3]", key: "user:4", matches: false },
    { pattern: "user:[^1-3]", key: "user:4", matches: true },
    { pattern: "user:[^1-3]", key: "user:2", matches: false },
    { pattern: "user:[!ab]", key: "user:!", matches: true },
    { pattern: "user:[ab", key: "user:a", matches: true },
    { pattern: "user:[]", key: "user:a", matches: false },
    { pattern: String.raw`user:\*`, key: "user:*", matches: true },
    { pattern: String.raw`user:\*`, key: "user:1", matches: false },
    { pattern: String.raw`user:\?\[\]`, key: "user:?[]", matches: true },
    { pattern: String.raw`user:[\]]`, key: "user:]", matches: true },
    { pattern: String.raw`user:[\-]`, key: "user:-", matches: true },
    { pattern: "user:\\", key: "user:\\", matches: true },
    { pattern: "user.(a)+{1}^$|", key: "user.(a)+{1}^$|", matches: true },
    { pattern: "user.(a)+{1}^$|", key: "userXa", matches: false },
    { pattern: "用户:*", key: "用户:张三😀", matches: true },
    { pattern: "用户:张三😀", key: "用户:张三😀", matches: true },
    { pattern: "用户:???", key: "用户:张", matches: true },
    { pattern: "用户:?", key: "用户:张", matches: false },
    { pattern: "", key: "any:1", matches: true },
    { pattern: "   ", key: "any:1", matches: true },
    { pattern: " user:* ", key: "user:1", matches: true },
  ])("新增 $key 按 Redis 模式 $pattern 决定是否缓存：$matches", ({ pattern, key, matches }) => {
    const current = { ...initialBrowserPageState, pattern };
    const next = applyCreatedKey(current, {
      key, key_type: "string", ttl_ms: -1, value: { String: { value: "value" } },
    });

    expect(next.keys.map((summary) => summary.key)).toEqual(matches ? [key] : []);
    expect(next.pattern).toBe(pattern);
    if (!matches) expect(next).toBe(current);
  });

  it.each(["CLUSTER_TOPOLOGY_FAILED", "CLUSTER_NODE_UNAVAILABLE", "PARTIAL_FAILURE", "CROSS_SLOT"])("为 %s 显示固定提示且不泄露后端原文", (code) => {
    const message = browserErrorMessage({ code, message: "credential-secret" }, "默认错误");
    expect(message).not.toBe("默认错误");
    expect(message).not.toContain("credential-secret");
    expect(browserErrorMessage({ code: "UNKNOWN", message: "credential-secret" }, "默认错误")).toBe("默认错误");
  });
  it("全量结果保留全部类型和筛选，替换旧缓存并清空选择与详情", () => {
    const selected = { key: "old", key_type: "string", ttl_ms: -1, size: 1 };
    const next = applyScanResult({
      ...initialBrowserPageState,
      keyType: "hash",
      keys: [selected],
      selectedKey: "old",
      selectedKeys: ["old"],
      detail: { ...selected, value: { String: { value: "old" } } },
      metadata: { ...selected, memory_bytes: 64, encoding: "embstr", idle_seconds: 1 },
      loading: true,
      error: "旧错误",
    }, [
      { key: "user:1", key_type: "string", ttl_ms: -1, size: 1 },
      { key: "user:2", key_type: "hash", ttl_ms: -1, size: 2 },
    ]);

    expect(next.keys.map((key) => key.key)).toEqual(["user:1", "user:2"]);
    expect(next.keyType).toBe("hash");
    expect(filterKeysByType(next.keys, next.keyType).map((key) => key.key)).toEqual(["user:2"]);
    expect(next.selectedKeys).toEqual([]);
    expect(next.selectedKey).toBeNull();
    expect(next.detail).toBeNull();
    expect(next.metadata).toBeNull();
    expect(next.loading).toBe(false);
    expect(next.error).toBeNull();
  });

  it.each([
    { keyType: " STREAM ", expected: ["stream:1"] },
    { keyType: "json", expected: ["json:1", "json:2", "json:3"] },
    { keyType: "zset", expected: ["sorted:1", "sorted:2", "sorted:3"] },
    { keyType: "sortedset", expected: ["sorted:1", "sorted:2", "sorted:3"] },
    { keyType: "sorted-set", expected: ["sorted:1", "sorted:2", "sorted:3"] },
  ])("本地筛选 $keyType 识别类型别名，清空筛选后恢复全部缓存", ({ keyType, expected }) => {
    const keys = [
      { key: "string:1", key_type: "string", ttl_ms: -1, size: 1 },
      { key: "stream:1", key_type: " Stream ", ttl_ms: -1, size: 1 },
      { key: "json:1", key_type: "JSON", ttl_ms: -1, size: 1 },
      { key: "json:2", key_type: "ReJSON-RL", ttl_ms: -1, size: 1 },
      { key: "json:3", key_type: "ReJSON-RS", ttl_ms: -1, size: 1 },
      { key: "sorted:1", key_type: "zset", ttl_ms: -1, size: 1 },
      { key: "sorted:2", key_type: "SortedSet", ttl_ms: -1, size: 1 },
      { key: "sorted:3", key_type: "sorted-set", ttl_ms: -1, size: 1 },
    ];

    expect(filterKeysByType(keys, keyType).map((key) => key.key)).toEqual(expected);
    expect(filterKeysByType(keys, "")).toEqual(keys);
    expect(keys).toHaveLength(8);
  });

  it("空的全量结果清空上一轮缓存和选择", () => {
    const next = applyScanResult({
      ...initialBrowserPageState,
      keys: [{ key: "old", key_type: "string", ttl_ms: -1, size: 1 }],
      selectedKey: "old",
      selectedKeys: ["old"],
    }, []);
    expect(next.keys).toEqual([]);
    expect(next.selectedKey).toBeNull();
    expect(next.selectedKeys).toEqual([]);
  });

  it("识别 JSON 和 Stream 类型并深拷贝其嵌套数据", () => {
    const json: RedisValue = {
      Json: {
        value: {
          user: { name: "Alice" },
          tags: ["redis"],
        },
      },
    };
    const stream: RedisValue = {
      Stream: {
        entries: [
          {
            id: "1-0",
            fields: [{ field: "event", value: "created" }],
          },
        ],
      },
    };

    expect(redisValueKind(json)).toBe("json");
    expect(redisValueKind(stream)).toBe("stream");

    const clonedJson = cloneRedisValue(json);
    const clonedStream = cloneRedisValue(stream);
    if ("Json" in clonedJson && typeof clonedJson.Json.value === "object" && clonedJson.Json.value) {
      (clonedJson.Json.value as { user: { name: string } }).user.name = "Bob";
    }
    if ("Stream" in clonedStream) {
      clonedStream.Stream.entries[0].fields[0].value = "updated";
    }

    expect(json).toEqual({
      Json: { value: { user: { name: "Alice" }, tags: ["redis"] } },
    });
    expect(stream).toEqual({
      Stream: {
        entries: [{ id: "1-0", fields: [{ field: "event", value: "created" }] }],
      },
    });
  });

  it("为模块类型提供稳定标签和不泄露原始错误的提示", () => {
    expect(keyTypeLabel("stream")).toBe("Stream");
    expect(keyTypeLabel("array")).toBe("Array");
    expect(keyTypeLabel("vectorset")).toBe("Vector Set");
    expect(keyTypeLabel("ReJSON-RL")).toBe("JSON");
    expect(keyTypeLabel("ReJSON-RS")).toBe("JSON");
    expect(browserErrorMessage({ code: "UNSUPPORTED_DATA_TYPE", message: "secret" }, "读取失败"))
      .toBe("当前 Redis 数据类型暂不支持。");
  });

  it("保留 Array 和 Vector Set 摘要的独立类型", () => {
    const array: RedisValue = { Array: { length: "4", count: "3" } };
    const vectorSet: RedisValue = {
      VectorSet: { total: "2", dimension: 3, quantization: "f32" },
    };

    expect(redisValueKind(array)).toBe("array");
    expect(redisValueKind(vectorSet)).toBe("vectorset");
    expect(cloneRedisValue(array)).toEqual(array);
    expect(cloneRedisValue(vectorSet)).toEqual(vectorSet);
  });
});
