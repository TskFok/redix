import { describe, expect, it } from "vitest";

import {
  browserErrorMessage,
  cloneRedisValue,
  keyTypeLabel,
  redisValueKind,
} from "./browserState";
import type { RedisValue } from "../../lib/types";

describe("Browser 状态 helper", () => {
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
