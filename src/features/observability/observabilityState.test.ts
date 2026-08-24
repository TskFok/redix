import { describe, expect, it } from "vitest";

import type { PubSubMessageEvent } from "../../lib/types";
import {
  appendPubSubMessage,
  parsePubSubTopics,
  toUserFacingObservabilityError,
} from "./observabilityState";

const message: PubSubMessageEvent = {
  connection_id: "local",
  session_id: "session-1",
  channel: "events",
  pattern: null,
  message: "hello",
  received_at_ms: 1,
};

describe("observability state", () => {
  it("把主题文本标准化并去重", () => {
    expect(parsePubSubTopics(" events\n\nevents\nuser:* ", false)).toEqual([
      { name: "events", pattern: false },
      { name: "user:*", pattern: false },
    ]);
  });

  it("只保留最新的 5000 条 Pub/Sub 消息", () => {
    const history = Array.from({ length: 5000 }, (_, index) => ({
      ...message,
      received_at_ms: index,
    }));

    const next = appendPubSubMessage(history, message);

    expect(next).toHaveLength(5000);
    expect(next[0].received_at_ms).toBe(1);
    expect(next.at(-1)).toEqual(message);
  });

  it("将后端错误映射为固定的用户提示", () => {
    expect(
      toUserFacingObservabilityError({
        code: "COMMAND_FAILED",
        message: "包含敏感命令详情",
      }),
    ).toBe("Redis 运维操作失败，请稍后重试。");
    expect(toUserFacingObservabilityError({ code: "INVALID_INPUT" })).toBe(
      "输入参数无效，请检查后重试。",
    );
    expect(toUserFacingObservabilityError({ code: "UNKNOWN" })).toBe(
      "运维观察操作失败，请稍后重试。",
    );
  });
});
