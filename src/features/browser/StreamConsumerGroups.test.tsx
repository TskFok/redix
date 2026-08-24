import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import StreamConsumerGroups from "./StreamConsumerGroups";

const {
  acknowledgeMock,
  createGroupMock,
  deleteConsumerMock,
  deleteGroupMock,
  getGroupsMock,
  getConsumersMock,
  getPendingMock,
} = vi.hoisted(() => ({
  acknowledgeMock: vi.fn(),
  createGroupMock: vi.fn(),
  deleteConsumerMock: vi.fn(),
  deleteGroupMock: vi.fn(),
  getGroupsMock: vi.fn(),
  getConsumersMock: vi.fn(),
  getPendingMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  acknowledgeStreamPendingEntries: acknowledgeMock,
  createStreamConsumerGroup: createGroupMock,
  deleteStreamConsumer: deleteConsumerMock,
  deleteStreamConsumerGroup: deleteGroupMock,
  getStreamConsumerGroups: getGroupsMock,
  getStreamConsumers: getConsumersMock,
  getStreamPendingEntries: getPendingMock,
}));

const group = {
  name: "workers",
  consumers: 1,
  pending: 1,
  last_delivered_id: "2-0",
};

const consumer = {
  name: "consumer-1",
  pending: 1,
  idle_ms: 12,
};

const pending = {
  id: "1-0",
  consumer: "consumer-1",
  idle_ms: 12,
  deliveries: 1,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((promiseResolve) => {
    resolve = promiseResolve;
  });
  return { promise, resolve };
}

describe("Stream Consumer Groups", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("confirm", vi.fn(() => true));
    getGroupsMock.mockResolvedValue([group]);
    getConsumersMock.mockResolvedValue([consumer]);
    getPendingMock.mockResolvedValue([pending]);
    createGroupMock.mockResolvedValue(undefined);
    deleteGroupMock.mockResolvedValue(1);
    deleteConsumerMock.mockResolvedValue(0);
    acknowledgeMock.mockResolvedValue(1);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("加载 Group、消费者和 Pending 详情", async () => {
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);

    expect(await screen.findByText("workers")).toBeInTheDocument();
    expect((await screen.findAllByText("consumer-1")).length).toBe(2);
    expect(await screen.findByText("1-0")).toBeInTheDocument();
    expect(getGroupsMock).toHaveBeenCalledWith({ connection_id: "local", key: "events" });
    expect(getConsumersMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "events",
      group: "workers",
    });
    expect(getPendingMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "events",
      group: "workers",
      count: 100,
      consumer: null,
    });
  });

  it("创建 Group 前校验名称并提交起始 ID", async () => {
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);

    await screen.findByText("workers");
    fireEvent.click(screen.getByRole("button", { name: "创建 Group" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("名称不能为空");
    expect(createGroupMock).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Consumer Group 名称"), {
      target: { value: "billing" },
    });
    fireEvent.change(screen.getByLabelText("起始 ID"), {
      target: { value: "0-0" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建 Group" }));

    await waitFor(() => {
      expect(createGroupMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        name: "billing",
        last_delivered_id: "0-0",
      });
    });
  });

  it("切换 Group 后重新加载对应详情", async () => {
    getGroupsMock.mockResolvedValue([
      group,
      { ...group, name: "alerts", pending: 0, consumers: 0 },
    ]);
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("workers");

    fireEvent.change(screen.getByLabelText("选择 Consumer Group"), {
      target: { value: "alerts" },
    });

    await waitFor(() => {
      expect(getConsumersMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        key: "events",
        group: "alerts",
      });
    });
    expect(getPendingMock).toHaveBeenLastCalledWith({
      connection_id: "local",
      key: "events",
      group: "alerts",
      count: 100,
      consumer: null,
    });
  });

  it("确认选中的 Pending 并支持删除消费者和 Group", async () => {
    const confirmMock = vi.mocked(window.confirm);
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");

    fireEvent.click(screen.getByLabelText("选择 Pending 1-0"));
    fireEvent.click(screen.getByRole("button", { name: "确认选中" }));
    await waitFor(() => {
      expect(acknowledgeMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        group: "workers",
        entries: ["1-0"],
      });
    });

    fireEvent.click(screen.getByRole("button", { name: "删除消费者 consumer-1" }));
    await waitFor(() => {
      expect(deleteConsumerMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        group: "workers",
        consumer: "consumer-1",
      });
    });

    fireEvent.click(screen.getByRole("button", { name: "删除 Group" }));
    await waitFor(() => {
      expect(deleteGroupMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        name: "workers",
      });
    });
    expect(confirmMock).toHaveBeenCalledTimes(2);
  });

  it("忽略切换键后返回的旧 Group 响应", async () => {
    const first = deferred<typeof group[]>();
    const second = deferred<typeof group[]>();
    getGroupsMock.mockReset();
    getGroupsMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    const { rerender } = render(
      <StreamConsumerGroups connectionId="local" streamKey="events" />,
    );
    rerender(<StreamConsumerGroups connectionId="other" streamKey="events" />);
    second.resolve([]);
    await waitFor(() => {
      expect(screen.getByText("当前 Group 没有消费者。")).toBeInTheDocument();
    });
    first.resolve([group]);

    await waitFor(() => {
      expect(screen.queryByRole("option", { name: "workers" })).not.toBeInTheDocument();
    });
  });
});
