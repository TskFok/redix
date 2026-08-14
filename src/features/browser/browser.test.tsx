import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import BrowserPage from "./BrowserPage";

const {
  scanKeysMock,
  getKeyMock,
  setKeyMock,
  deleteKeyMock,
  setKeyTtlMock,
} = vi.hoisted(() => ({
  scanKeysMock: vi.fn(),
  getKeyMock: vi.fn(),
  setKeyMock: vi.fn(),
  deleteKeyMock: vi.fn(),
  setKeyTtlMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  scanKeys: scanKeysMock,
  getKey: getKeyMock,
  setKey: setKeyMock,
  deleteKey: deleteKeyMock,
  setKeyTtl: setKeyTtlMock,
}));

const stringSummary = {
  key: "user:1",
  key_type: "string",
  ttl_ms: -1,
  size: 5,
};

const stringDetail = {
  key: "user:1",
  key_type: "string",
  ttl_ms: -1,
  value: { String: { value: "Alice" } },
};

describe("Redis Browser", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    getKeyMock.mockResolvedValue(stringDetail);
    setKeyMock.mockResolvedValue(stringDetail);
    deleteKeyMock.mockResolvedValue(undefined);
    setKeyTtlMock.mockResolvedValue(-1);
  });

  afterEach(() => {
    cleanup();
  });

  it("按模式加载键并在点击键后读取详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    getKeyMock.mockResolvedValue(stringDetail);

    render(<BrowserPage connectionId="local" />);

    expect(await screen.findByText("user:1")).toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledWith({
      connection_id: "local",
      cursor: 0,
      pattern: "*",
      count: 100,
    });

    fireEvent.click(screen.getByRole("button", { name: "user:1" }));

    expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
    expect(getKeyMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "user:1",
    });
  });

  it("保存 String 后刷新详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(stringDetail)
      .mockResolvedValueOnce({
        ...stringDetail,
        value: { String: { value: "Bob" } },
      });
    setKeyMock.mockResolvedValue({
      ...stringDetail,
      value: { String: { value: "Bob" } },
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));

    const input = await screen.findByDisplayValue("Alice");
    fireEvent.change(input, { target: { value: "Bob" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
        value: { String: { value: "Bob" } },
      });
    });
    expect(getKeyMock).toHaveBeenCalledTimes(2);
  });

  it("加载更多复用游标，过滤时重置游标并替换列表", async () => {
    scanKeysMock
      .mockResolvedValueOnce({
        cursor: 42,
        keys: [stringSummary],
        has_more: true,
      })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [{ ...stringSummary, key: "admin:1" }],
        has_more: false,
      })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [{ ...stringSummary, key: "user:2" }],
        has_more: false,
      });

    render(<BrowserPage connectionId="local" />);
    expect(await screen.findByText("user:1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenNthCalledWith(2, {
        connection_id: "local",
        cursor: 42,
        pattern: "*",
        count: 100,
      });
    });
    expect(await screen.findByText("admin:1")).toBeInTheDocument();
    expect(screen.getByText("user:1")).toBeInTheDocument();

    const pattern = screen.getByLabelText("键过滤");
    fireEvent.change(pattern, { target: { value: "user:*" } });
    fireEvent.keyDown(pattern, { key: "Enter", code: "Enter" });

    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        cursor: 0,
        pattern: "user:*",
        count: 100,
      });
    });
    expect(await screen.findByText("user:2")).toBeInTheDocument();
    expect(screen.queryByText("admin:1")).not.toBeInTheDocument();
  });

  it("删除成功后清理选择并从列表移除键", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");

    fireEvent.click(screen.getByRole("button", { name: "删除" }));

    await waitFor(() => {
      expect(deleteKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
      });
    });
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    expect(screen.getByText("请选择一个键查看详情")).toBeInTheDocument();
  });

  it("设置 TTL 后刷新详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(stringDetail)
      .mockResolvedValueOnce({ ...stringDetail, ttl_ms: 60_000 });
    setKeyTtlMock.mockResolvedValue(60_000);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");

    fireEvent.change(screen.getByLabelText("TTL（毫秒）"), {
      target: { value: "60000" },
    });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));

    await waitFor(() => {
      expect(setKeyTtlMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
        ttl_ms: 60_000,
      });
    });
    expect(getKeyMock).toHaveBeenCalledTimes(2);
  });

  it("保存 Hash、List、Set 和 Sorted Set 时生成对应 RedisValue", async () => {
    const summaries = [
      { key: "hash:1", key_type: "hash", ttl_ms: -1, size: 2 },
      { key: "list:1", key_type: "list", ttl_ms: -1, size: 2 },
      { key: "set:1", key_type: "set", ttl_ms: -1, size: 2 },
      { key: "zset:1", key_type: "zset", ttl_ms: -1, size: 2 },
    ];
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: summaries, has_more: false });
    getKeyMock.mockImplementation(async ({ key }: { key: string }) => {
      if (key === "hash:1") {
        return {
          key,
          key_type: "hash",
          ttl_ms: -1,
          value: { Hash: { fields: [{ field: "name", value: "Alice" }] } },
        };
      }
      if (key === "list:1") {
        return {
          key,
          key_type: "list",
          ttl_ms: -1,
          value: { List: { items: ["first"] } },
        };
      }
      if (key === "set:1") {
        return {
          key,
          key_type: "set",
          ttl_ms: -1,
          value: { Set: { members: ["member"] } },
        };
      }
      return {
        key,
        key_type: "zset",
        ttl_ms: -1,
        value: { SortedSet: { members: [{ member: "member", score: 1 }] } },
      };
    });

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("hash:1");

    fireEvent.click(screen.getByRole("button", { name: "hash:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "hash:1",
        value: { Hash: { fields: [{ field: "name", value: "Alice" }] } },
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "list:1" }));
    await screen.findByDisplayValue("first");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "list:1",
        value: { List: { items: ["first"] } },
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "set:1" }));
    await screen.findByDisplayValue("member");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "set:1",
        value: { Set: { members: ["member"] } },
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "zset:1" }));
    await screen.findByDisplayValue("1");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "zset:1",
        value: { SortedSet: { members: [{ member: "member", score: 1 }] } },
      }),
    );
  });

  it("Set 保存前去重，Sorted Set 拒绝非有限 score", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [
        { key: "set:1", key_type: "set", ttl_ms: -1, size: 1 },
        { key: "zset:1", key_type: "zset", ttl_ms: -1, size: 1 },
      ],
      has_more: false,
    });
    getKeyMock.mockImplementation(async ({ key }: { key: string }) => ({
      key,
      key_type: key.startsWith("z") ? "zset" : "set",
      ttl_ms: -1,
      value: key.startsWith("z")
        ? { SortedSet: { members: [{ member: "member", score: 1 }] } }
        : { Set: { members: ["member"] } },
    }));

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("set:1");
    fireEvent.click(screen.getByRole("button", { name: "set:1" }));
    const setInput = await screen.findByDisplayValue("member");
    fireEvent.change(setInput, { target: { value: "duplicate" } });
    fireEvent.click(screen.getByRole("button", { name: "添加成员" }));
    const setInputs = screen.getAllByRole("textbox");
    fireEvent.change(setInputs[setInputs.length - 1], { target: { value: "duplicate" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(setKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "set:1",
        value: { Set: { members: ["duplicate"] } },
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "zset:1" }));
    await screen.findByDisplayValue("1");
    fireEvent.change(screen.getByLabelText("分数 1"), {
      target: { value: "Infinity" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("分数必须是有限数字");
    expect(setKeyMock).toHaveBeenCalledTimes(1);
  });

  it("加载失败时显示 alert 并在请求期间禁用重复过滤", async () => {
    let rejectScan: ((reason: unknown) => void) | undefined;
    scanKeysMock.mockImplementation(
      () => new Promise((_, reject) => {
        rejectScan = reject;
      }),
    );

    render(<BrowserPage connectionId="local" />);
    const pattern = screen.getByLabelText("键过滤");
    expect(pattern).toBeDisabled();
    fireEvent.keyDown(pattern, { key: "Enter", code: "Enter" });
    expect(scanKeysMock).toHaveBeenCalledTimes(1);

    rejectScan?.({ code: "COMMAND_FAILED", message: "加载失败" });
    expect(await screen.findByRole("alert")).toHaveTextContent("加载键失败");
  });
});
