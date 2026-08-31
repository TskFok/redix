import { expect, it } from "vitest";
import { appendCliTranscript, cliError } from "./cliState";

it("终端只保留最近 200 条，并按 UTF-8 字节限制保留量", () => {
  let entries = Array.from({ length: 200 }, (_, index) => ({ command: `GET ${index}`, output: "OK", error: false }));
  entries = appendCliTranscript(entries, { command: "PING", output: "PONG", error: false });
  expect(entries).toHaveLength(200);
  expect(entries[0].command).toBe("GET 1");
  expect(entries[199].command).toBe("PING");
  const large = { command: "GET large", output: "中".repeat(80_000), error: false };
  const byteBounded = Array.from({ length: 8 }, (_, index) => ({ ...large, command: `GET ${index}` }));
  expect(appendCliTranscript(byteBounded, { ...large, command: "GET newest" }).map((entry) => entry.command))
    .toEqual(["GET 1", "GET 2", "GET 3", "GET 4", "GET 5", "GET 6", "GET 7", "GET newest"]);
});

it("未知错误信息不回显服务端或 IPC 中的敏感内容", () => {
  expect(cliError({ code: "server-secret", message: "password" })).toBe("IPC_ERROR：CLI 调用失败，请重新连接。");
  expect(cliError(new Error("password"))).toBe("IPC_ERROR：CLI 调用失败，请重新连接。");
  expect(cliError({ code: "constructor" })).toBe("IPC_ERROR：CLI 调用失败，请重新连接。");
});

it("超大最新输出显示省略提示并保留之前的历史", () => {
  const previous = { command: "PING", output: "PONG", error: false };
  const entries = appendCliTranscript([previous], { command: "GET large", output: "中".repeat(1_000_000), error: false });
  expect(entries[0]).toEqual(previous);
  expect(entries).toHaveLength(2);
  expect(entries[1].output).toContain("输出超过 256 KiB，已省略");
  expect(new TextEncoder().encode(entries[1].output).byteLength).toBeLessThanOrEqual(256 * 1024);
});
