import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import StreamConsumerGroups from "./StreamConsumerGroups";

const {
  claimMock,
  acknowledgeMock,
  createGroupMock,
  deleteConsumerMock,
  deleteGroupMock,
  getGroupsMock,
  getConsumersMock,
  getPendingMock,
} = vi.hoisted(() => ({
  claimMock: vi.fn(),
  acknowledgeMock: vi.fn(),
  createGroupMock: vi.fn(),
  deleteConsumerMock: vi.fn(),
  deleteGroupMock: vi.fn(),
  getGroupsMock: vi.fn(),
  getConsumersMock: vi.fn(),
  getPendingMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  claimStreamPendingEntries: claimMock,
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
    claimMock.mockResolvedValue(["1-0"]);
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


describe("Pending Claim", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getGroupsMock.mockResolvedValue([group]); getConsumersMock.mockResolvedValue([consumer]);
    getPendingMock.mockResolvedValue([pending]); claimMock.mockResolvedValue(["1-0"]);
    vi.stubGlobal("confirm", vi.fn(() => true));
  });
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

  it("将选中消息转移到目标消费者并显示实际成功数量", async () => {
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");
    fireEvent.click(screen.getByLabelText("选择 Pending 1-0"));
    fireEvent.change(screen.getByLabelText("目标消费者"), { target: { value: "replacement" } });
    fireEvent.change(screen.getByLabelText("最小空闲时间（毫秒）"), { target: { value: "1000" } });
    fireEvent.click(screen.getByRole("button", { name: "转移选中 Pending" }));
    expect(await screen.findByRole("status")).toHaveTextContent("已转移 1 / 1 条");
    expect(claimMock).toHaveBeenCalledWith({ connection_id: "local", key: "events", group: "workers",
      consumer: "replacement", min_idle_ms: 1000, entries: ["1-0"] });
  });

  it("Claim 失败保留选中消息和目标，不显示底层错误", async () => {
    claimMock.mockRejectedValue({ code: "COMMAND_FAILED", message: "secret" });
    render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");
    fireEvent.click(screen.getByLabelText("选择 Pending 1-0"));
    fireEvent.change(screen.getByLabelText("目标消费者"), { target: { value: "replacement" } });
    fireEvent.click(screen.getByRole("button", { name: "转移选中 Pending" }));
    expect(await screen.findByRole("alert")).not.toHaveTextContent("secret");
    expect(screen.getByLabelText("选择 Pending 1-0")).toBeChecked();
    expect(screen.getByLabelText("目标消费者")).toHaveValue("replacement");
  });

  it("切换键后忽略进行中的 Claim 响应", async () => {
    const response = deferred<string[]>(); claimMock.mockReturnValue(response.promise);
    const { rerender } = render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");
    fireEvent.click(screen.getByLabelText("选择 Pending 1-0"));
    fireEvent.change(screen.getByLabelText("目标消费者"), { target: { value: "replacement" } });
    fireEvent.click(screen.getByRole("button", { name: "转移选中 Pending" }));
    rerender(<StreamConsumerGroups connectionId="local" streamKey="other" />);
    await waitFor(() => expect(screen.getByLabelText("目标消费者")).toHaveValue(""));
    response.resolve(["1-0"]);
    await waitFor(() => expect(screen.getByRole("button", { name: "刷新 Groups" })).not.toBeDisabled());
    expect(screen.queryByText(/已转移/)).not.toBeInTheDocument();
  });
  it("离开并返回同一键后仍丢弃旧 Claim", async () => {
    const response = deferred<string[]>(); claimMock.mockReturnValue(response.promise);
    const { rerender } = render(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");
    fireEvent.click(screen.getByLabelText("选择 Pending 1-0"));
    fireEvent.change(screen.getByLabelText("目标消费者"), { target: { value: "replacement" } });
    fireEvent.click(screen.getByRole("button", { name: "转移选中 Pending" }));
    rerender(<StreamConsumerGroups connectionId="local" streamKey="other" />);
    await screen.findByText("1-0");
    rerender(<StreamConsumerGroups connectionId="local" streamKey="events" />);
    await screen.findByText("1-0");
    const groupRequests = getGroupsMock.mock.calls.length;
    await act(async () => { response.resolve(["1-0"]); await response.promise; });
    expect(getGroupsMock).toHaveBeenCalledTimes(groupRequests);
    expect(screen.queryByText(/已转移/)).not.toBeInTheDocument();
  });

});
