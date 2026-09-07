import { describe, expect, it } from "vitest";

import {
  applyScanPage,
  browserErrorMessage,
  cloneRedisValue,
  initialBrowserPageState,
  keyTypeLabel,
  redisValueKind,
} from "./browserState";
import type { RedisValue } from "../../lib/types";

describe("Browser 状态 helper", () => {
  it("保留不透明游标并以 has_more 为完成依据", () => {
    const next = applyScanPage(initialBrowserPageState, {
      cursor: "cluster:complete", keys: [], has_more: false, node_failures: [],
    }, false);
    expect(next.cursor).toBe("cluster:complete");
    expect(next.hasMore).toBe(false);
  });

  it("追加页保留未确认恢复的节点失败，重新扫描时清空旧失败", () => {
    const failed = applyScanPage(initialBrowserPageState, {
      cursor: "cluster:retry", keys: [], has_more: true,
      node_failures: [{ node_id: "node-a", code: "CLUSTER_NODE_UNAVAILABLE" }],
    }, false);
    expect(failed.nodeFailures).toEqual([{ node_id: "node-a", code: "CLUSTER_NODE_UNAVAILABLE" }]);
    expect(failed.hasMore).toBe(true);
    const next = applyScanPage(failed, {
      cursor: "cluster:next", keys: [], has_more: true, node_failures: [],
    }, false);
    expect(next.nodeFailures).toEqual(failed.nodeFailures);
    expect(applyScanPage(next, { cursor: "cluster:done", keys: [], has_more: false, node_failures: [] }, false).nodeFailures).toEqual([]);
    expect(applyScanPage(next, { cursor: 0, keys: [], has_more: false, node_failures: [] }, true).nodeFailures).toEqual([]);
  });

  it.each(["CLUSTER_TOPOLOGY_FAILED", "CLUSTER_NODE_UNAVAILABLE", "PARTIAL_FAILURE", "CROSS_SLOT"])("为 %s 显示固定提示且不泄露后端原文", (code) => {
    const message = browserErrorMessage({ code, message: "credential-secret" }, "默认错误");
    expect(message).not.toBe("默认错误");
    expect(message).not.toContain("credential-secret");
    expect(browserErrorMessage({ code: "UNKNOWN", message: "credential-secret" }, "默认错误")).toBe("默认错误");
  });
  it("按类型过滤扫描摘要并在刷新时清空选择", () => {
    const state = {
      ...initialBrowserPageState,
      selectedKeys: ["user:1"],
    };
    const next = applyScanPage(
      state,
      {
        cursor: 0,
        node_failures: [], has_more: false,
        keys: [
          { key: "user:1", key_type: "string", ttl_ms: -1, size: 1 },
          { key: "user:2", key_type: "hash", ttl_ms: -1, size: 2 },
        ],
      },
      true,
      "hash",
    );

    expect(next.keys.map((key) => key.key)).toEqual(["user:2"]);
    expect(next.selectedKeys).toEqual([]);
  });

  it("追加扫描时只保留当前列表中的选择", () => {
    const state = {
      ...initialBrowserPageState,
      keys: [{ key: "user:1", key_type: "string", ttl_ms: -1, size: 1 }],
      selectedKeys: ["user:1", "stale:1"],
    };

    const next = applyScanPage(
      state,
      {
        cursor: 0,
        node_failures: [], has_more: false,
        keys: [{ key: "user:2", key_type: "hash", ttl_ms: -1, size: 2 }],
      },
      false,
    );

    expect(next.selectedKeys).toEqual(["user:1"]);
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
