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

it("Cluster 明确限制观察会话而不启动单节点订阅", async () => {
  render(<ObservabilityPage connectionId="cluster" isCluster />);
  expect(screen.getByRole("tab", { name: /Pub\/Sub/ })).toBeDisabled();
  expect(screen.getByRole("tab", { name: /Profiler/ })).toBeDisabled();
  expect(screen.getByText(/Cluster 暂不支持 Pub\/Sub 和 Profiler/)).toBeInTheDocument();
  expect(startPubSubMock).not.toHaveBeenCalled();
});

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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ObservabilityPage", () => {
  it("只在显式导出时下载当前筛选的 Profiler 快照", async () => {
    const createObjectURL = vi.fn().mockReturnValue("blob:test");
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL: vi.fn() });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Profiler/ }));
    fireEvent.click(screen.getByRole("button", { name: "开始监控" }));
    await screen.findByRole("button", { name: "停止监控" });
    const emit = (args: string[]) => act(async () => listeners.get("redix://profiler/event")?.({ payload: { connection_id: "local", session_id: "profiler-1", time: "1710000000.1", database: 0, source: "local", args, received_at_ms: 1 } }));
    await emit(["SET", "before", "value"]);
    fireEvent.click(screen.getByRole("button", { name: "暂停显示" }));
    await emit(["GET", "after"]);
    expect(stopProfilerMock).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "筛选命令" }), { target: { value: "SET" } });
    expect(createObjectURL).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "导出 Profiler LOG" }));
    expect(click).toHaveBeenCalledTimes(1);
    const blob = createObjectURL.mock.calls[0][0] as Blob;
    const content = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.readAsText(blob);
    });
    expect(content).toBe('1710000000.1 [0 local] "SET" "before" "value"\n');
    fireEvent.click(screen.getByRole("button", { name: "清空视图" }));
    expect(screen.getByRole("button", { name: "导出 Profiler LOG" })).toBeDisabled();
  });

  it("暂停 Pub/Sub 显示仍保留后台消息，恢复后可筛选", async () => {
    render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Pub\/Sub/ }));
    fireEvent.click(screen.getByRole("button", { name: "开始订阅" }));
    await screen.findByRole("button", { name: "停止订阅" });
    const emit = (message: string) => act(async () => {
      listeners.get("redix://pubsub/message")?.({ payload: { connection_id: "local", session_id: "session-1", channel: "events", pattern: null, message, received_at_ms: Date.now() } });
    });
    await emit("first payload");
    fireEvent.click(screen.getByRole("button", { name: "暂停显示" }));
    await emit("second payload");
    expect(screen.getByText("first payload")).toBeInTheDocument();
    expect(screen.queryByText("second payload")).not.toBeInTheDocument();
    expect(stopPubSubMock).not.toHaveBeenCalled();
    expect(screen.getByText(/后台继续接收/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "恢复显示" }));
    expect(screen.getByText("second payload")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "筛选消息" }), { target: { value: "second" } });
    expect(screen.queryByText("first payload")).not.toBeInTheDocument();
    expect(screen.getByText("second payload")).toBeInTheDocument();
  });

  it("连接切换时清除旧消息，不允许导出前一连接数据", async () => {
    const view = render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Pub\/Sub/ }));
    fireEvent.click(screen.getByRole("button", { name: "开始订阅" }));
    await screen.findByRole("button", { name: "停止订阅" });
    await act(async () => listeners.get("redix://pubsub/message")?.({ payload: { connection_id: "local", session_id: "session-1", channel: "events", pattern: null, message: "private old payload", received_at_ms: 1 } }));
    view.rerender(<ObservabilityPage connectionId="other" />);
    fireEvent.click(screen.getByRole("tab", { name: /Pub\/Sub/ }));
    expect(screen.queryByText("private old payload")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出消息 JSON" })).toBeDisabled();
  });

  it("离开连接后才返回的 Profiler 启动会话必须立即停止", async () => {
    let finish: ((session: { connection_id: string; session_id: string }) => void) | undefined;
    startProfilerMock.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    const view = render(<ObservabilityPage connectionId="local" />);
    fireEvent.click(screen.getByRole("tab", { name: /Profiler/ }));
    fireEvent.click(screen.getByRole("button", { name: "开始监控" }));
    await waitFor(() => expect(startProfilerMock).toHaveBeenCalled());
    view.rerender(<ObservabilityPage connectionId="other" />);
    await act(async () => finish?.({ connection_id: "local", session_id: "late" }));
    expect(stopProfilerMock).toHaveBeenCalledWith({ connection_id: "local", session_id: "late" });
  });

  it("加载 Slow Log 配置与记录，并支持清空记录", async () => {
    render(<ObservabilityPage connectionId="local" />);

    expect(await screen.findByRole("heading", { name: "慢命令记录" })).toBeInTheDocument();
    expect(await screen.findByText("SET demo hello")).toBeInTheDocument();
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
