import { describe, expect, it } from "vitest";
import { buildProfilerExport, buildPubSubExport, buildSlowLogExport } from "./observabilityExport";
import type { ProfilerEvent, PubSubMessageEvent, SlowLogEntry } from "../../lib/types";

const profiler: ProfilerEvent = { connection_id: "local", session_id: "one", time: "1710000000.123456", database: 2, source: "127.0.0.1:6379", args: ["SET", "a key", "line\nnext"], received_at_ms: 1 };
const message: PubSubMessageEvent = { connection_id: "local", session_id: "one", channel: "events", pattern: null, message: 'hello "world"\nnext', received_at_ms: 1 };
const slow: SlowLogEntry = { id: 1, time: 1710000000, duration_us: 2500, args: ["GET", 'a,"key"'], source: "127.0.0.1:6379", client: "=HYPERLINK(\"bad\")\nnext" };

describe("运维日志导出", () => {
  it("Profiler 按查询过滤，保留微秒时间与命令转义，一条事件一行", () => {
    const output = buildProfilerExport([profiler, { ...profiler, args: ["PING"] }], "sEt");
    expect(output.filename).toBe("profiler.log");
    expect(output.content).toBe('1710000000.123456 [2 127.0.0.1:6379] "SET" "a key" "line\\nnext"\n');
  });
  it("Pub/Sub JSON 导出保留原始消息和当前过滤范围", () => {
    const output = buildPubSubExport([message, { ...message, channel: "other", message: "ignore" }], "events");
    expect(JSON.parse(output.content)).toEqual([message]);
    expect(output.filename).toBe("pubsub.json");
  });
  it("Slow Log CSV 正确转义逗号、引号、换行并中和公式前缀", () => {
    const output = buildSlowLogExport([slow], "csv", "get");
    expect(output.content).toContain('"\'=HYPERLINK(""bad"")\nnext"');
    expect(output.content).toContain('"2500"');
    expect(output.content).toContain('"id","time","duration_us","command","source","client"\r\n');
    expect(JSON.parse(buildSlowLogExport([slow], "json").content)).toEqual([slow]);
    expect(JSON.parse(buildSlowLogExport([slow], "json", "missing").content)).toEqual([]);
  });
});
