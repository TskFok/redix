import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearSlowLogs,
  getSlowLogConfig,
  getSlowLogs,
  publishPubSub,
  startPubSub,
  stopPubSub,
  updateSlowLogConfig,
} from "../../lib/tauri";
import ObservabilityPage from "./ObservabilityPage";

const {
  clearSlowLogsMock,
  getSlowLogConfigMock,
  getSlowLogsMock,
  listenMock,
  publishPubSubMock,
  startProfilerMock,
  startPubSubMock,
  stopProfilerMock,
  stopPubSubMock,
  updateSlowLogConfigMock,
} = vi.hoisted(() => ({
  clearSlowLogsMock: vi.fn(),
  getSlowLogConfigMock: vi.fn(),
  getSlowLogsMock: vi.fn(),
  listenMock: vi.fn(),
  publishPubSubMock: vi.fn(),
  startProfilerMock: vi.fn(),
  startPubSubMock: vi.fn(),
  stopProfilerMock: vi.fn(),
  stopPubSubMock: vi.fn(),
  updateSlowLogConfigMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  clearSlowLogs: clearSlowLogsMock,
  getSlowLogConfig: getSlowLogConfigMock,
  getSlowLogs: getSlowLogsMock,
  publishPubSub: publishPubSubMock,
  startProfiler: startProfilerMock,
  startPubSub: startPubSubMock,
  stopProfiler: stopProfilerMock,
  stopPubSub: stopPubSubMock,
  updateSlowLogConfig: updateSlowLogConfigMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const listeners = new Map<string, (event: { payload: unknown }) => void>();

beforeEach(() => {
  vi.clearAllMocks();
  listeners.clear();
  listenMock.mockImplementation(
    (event: string, handler: (event: { payload: unknown }) => void) => {
      listeners.set(event, handler);
      return Promise.resolve(() => listeners.delete(event));
    },
  );
  getSlowLogConfigMock.mockResolvedValue({
    slowlog_max_len: 128,
    slowlog_log_slower_than: 10_000,
  });
  getSlowLogsMock.mockResolvedValue([
    {
      id: 7,
      time: 1_710_000_000,
      duration_us: 2_500,
      args: ["SET", "demo", "hello"],
      source: "127.0.0.1:6379",
      client: "redix-test",
    },
  ]);
  clearSlowLogsMock.mockResolvedValue(undefined);
  updateSlowLogConfigMock.mockResolvedValue({
    slowlog_max_len: 256,
    slowlog_log_slower_than: 5_000,
  });
  startPubSubMock.mockResolvedValue({
    connection_id: "local",
    session_id: "session-1",
    topics: [{ name: "events", pattern: false }],
  });
  stopPubSubMock.mockResolvedValue(undefined);
  publishPubSubMock.mockResolvedValue(1);
  startProfilerMock.mockResolvedValue({
    connection_id: "local",
    session_id: "profiler-1",
  });
  stopProfilerMock.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
});

describe("ObservabilityPage", () => {
  it("加载 Slow Log 配置与记录，并支持清空记录", async () => {
    render(<ObservabilityPage connectionId="local" />);

    expect(await screen.findByRole("heading", { name: "慢命令记录" })).toBeInTheDocument();
    expect(screen.getByText("SET demo hello")).toBeInTheDocument();
    expect(screen.getByDisplayValue("128")).toBeInTheDocument();
    expect(screen.getByDisplayValue("10000")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "清空记录" }));

    await waitFor(() => expect(clearSlowLogsMock).toHaveBeenCalledWith("local"));
    expect(screen.getByText("当前没有慢命令记录。")).toBeInTheDocument();
  });

  it("启动 Pub/Sub 后接收消息，并可停止会话", async () => {
    render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Pub\/Sub/ }));
    await screen.findByRole("heading", { name: "消息订阅" });

    fireEvent.click(screen.getByRole("button", { name: "开始订阅" }));
    await waitFor(() => expect(startPubSubMock).toHaveBeenCalledTimes(1));
    expect(startPubSubMock).toHaveBeenCalledWith({
      connection_id: "local",
      session_id: expect.any(String),
      topics: [{ name: "events", pattern: false }],
    });

    await act(async () => {
      listeners.get("redix://pubsub/message")?.({
        payload: {
          connection_id: "local",
          session_id: "session-1",
          channel: "events",
          pattern: null,
          message: "hello from redis",
          received_at_ms: 1,
        },
      });
    });
    expect(screen.getByText("hello from redis")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停止订阅" }));
    await waitFor(() =>
      expect(stopPubSubMock).toHaveBeenCalledWith({
        connection_id: "local",
        session_id: "session-1",
      }),
    );
  });

  it("启动 Profiler 后接收命令事件，并可停止监控", async () => {
    render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Profiler/ }));
    await screen.findByRole("heading", { name: "实时命令监控" });

    expect(
      screen.getByText(/MONITOR 会接收当前实例的全部命令/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "开始监控" }));
    await waitFor(() => expect(startProfilerMock).toHaveBeenCalledTimes(1));
    expect(startProfilerMock).toHaveBeenCalledWith({
      connection_id: "local",
      session_id: expect.any(String),
    });

    await act(async () => {
      listeners.get("redix://profiler/event")?.({
        payload: {
          connection_id: "local",
          session_id: "profiler-1",
          time: "1710000000.123456",
          database: 2,
          source: "127.0.0.1:6379",
          args: ["SET", "demo key", "hello redis"],
          received_at_ms: 1,
        },
      });
    });
    expect(screen.getByText('"SET" "demo key" "hello redis"')).toBeInTheDocument();
    expect(screen.getByText("DB2")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停止监控" }));
    await waitFor(() =>
      expect(stopProfilerMock).toHaveBeenCalledWith({
        connection_id: "local",
        session_id: "profiler-1",
      }),
    );
  });
});
