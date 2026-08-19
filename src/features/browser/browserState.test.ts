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
  it("按类型过滤扫描摘要并在刷新时清空选择", () => {
    const state = {
      ...initialBrowserPageState,
      selectedKeys: ["user:1"],
    };
    const next = applyScanPage(
      state,
      {
        cursor: 0,
        has_more: false,
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
        has_more: false,
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
    expect(keyTypeLabel("ReJSON-RL")).toBe("JSON");
    expect(keyTypeLabel("ReJSON-RS")).toBe("JSON");
    expect(browserErrorMessage({ code: "UNSUPPORTED_DATA_TYPE", message: "secret" }, "读取失败"))
      .toBe("当前 Redis 数据类型暂不支持。");
  });
});
