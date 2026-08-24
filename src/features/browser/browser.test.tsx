import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import BrowserPage from "./BrowserPage";
import {
  applyScanPage,
  initialBrowserPageState,
} from "./browserState";
import KeyDetails from "./KeyDetails";
import KeyEditor from "./KeyEditor";
import type { KeyValue, RedisValue } from "../../lib/types";

const {
  scanKeysMock,
  getKeyMock,
  setKeyMock,
  deleteKeyMock,
  createKeyMock,
  deleteKeysMock,
  exportKeysMock,
  renameKeyMock,
  getKeyInfoMock,
  importKeysMock,
  setKeyTtlMock,
} = vi.hoisted(() => ({
  scanKeysMock: vi.fn(),
  getKeyMock: vi.fn(),
  setKeyMock: vi.fn(),
  deleteKeyMock: vi.fn(),
  createKeyMock: vi.fn(),
  deleteKeysMock: vi.fn(),
  exportKeysMock: vi.fn(),
  renameKeyMock: vi.fn(),
  getKeyInfoMock: vi.fn(),
  importKeysMock: vi.fn(),
  setKeyTtlMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  scanKeys: scanKeysMock,
  getKey: getKeyMock,
  setKey: setKeyMock,
  deleteKey: deleteKeyMock,
  createKey: createKeyMock,
  deleteKeys: deleteKeysMock,
  exportKeys: exportKeysMock,
  renameKey: renameKeyMock,
  getKeyInfo: getKeyInfoMock,
  importKeys: importKeysMock,
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

describe("Redis Browser", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("confirm", vi.fn(() => true));
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    getKeyMock.mockResolvedValue(stringDetail);
    setKeyMock.mockResolvedValue(stringDetail);
    deleteKeyMock.mockResolvedValue(undefined);
    createKeyMock.mockResolvedValue(stringDetail);
    deleteKeysMock.mockResolvedValue(0);
    renameKeyMock.mockResolvedValue({ ...stringDetail, key: "user:renamed" });
    exportKeysMock.mockResolvedValue([
      {
        key: "user:1",
        ttl_ms: -1,
        value: { String: { value: "Alice" } },
      },
    ]);
    importKeysMock.mockResolvedValue(1);
    getKeyInfoMock.mockResolvedValue({
      key: "user:1",
      key_type: "string",
      ttl_ms: -1,
      size: 5,
      memory_bytes: 64,
      encoding: "embstr",
      idle_seconds: 2,
    });
    setKeyTtlMock.mockResolvedValue(-1);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
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
      key_type: null,
    });

    fireEvent.click(screen.getByRole("button", { name: "user:1" }));

    expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
    expect(getKeyMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "user:1",
    });
  });

  it("使用设置传入的 SCAN 数量", async () => {
    render(<BrowserPage connectionId="local" scanCount={250} />);

    await screen.findByText("没有匹配的键。");
    expect(scanKeysMock).toHaveBeenCalledWith({
      connection_id: "local",
      cursor: 0,
      pattern: "*",
      count: 250,
      key_type: null,
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
        key_type: null,
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
        key_type: null,
      });
    });
    expect(await screen.findByText("user:2")).toBeInTheDocument();
    expect(screen.queryByText("admin:1")).not.toBeInTheDocument();
  });

  it("类型选择传入后端并在刷新时重置游标", async () => {
    scanKeysMock
      .mockResolvedValueOnce({
        cursor: 42,
        keys: [stringSummary, { ...stringSummary, key: "hash:1", key_type: "hash" }],
        has_more: true,
      })
      .mockResolvedValue({ cursor: 0, keys: [], has_more: false });

    render(<BrowserPage connectionId="local" />);
    expect(await screen.findByText("user:1")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("类型过滤"), {
      target: { value: "hash" },
    });
    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: "hash",
      });
    });

    fireEvent.click(screen.getByRole("button", { name: "刷新键列表" }));
    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: "hash",
      });
    });
  });

  it("勾选键后只导出一次，并通过 Blob 下载且不包含连接密码", async () => {
    const createObjectURLMock = vi.fn(() => "blob:browser-export");
    const revokeObjectURLMock = vi.fn();
    vi.stubGlobal("URL", {
      createObjectURL: createObjectURLMock,
      revokeObjectURL: revokeObjectURLMock,
    });
    exportKeysMock.mockResolvedValue([
      {
        key: "user:1",
        ttl_ms: -1,
        value: { String: { value: "Alice" } },
      },
    ]);
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("user:1");
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user:1" }));
    fireEvent.click(screen.getByRole("button", { name: "导出选中键" }));

    await waitFor(() => {
      expect(exportKeysMock).toHaveBeenCalledTimes(1);
      expect(exportKeysMock).toHaveBeenCalledWith({
        connection_id: "local",
        keys: ["user:1"],
      });
    });
    const blob = (createObjectURLMock.mock.calls[0] as unknown[])[0] as Blob;
    expect(await blob.text()).not.toContain("password");
    expect(revokeObjectURLMock).toHaveBeenCalledWith("blob:browser-export");
  });

  it("导入 JSON 文件后调用一次 typed importKeys 并刷新列表", async () => {
    const entry = {
      key: "imported:1",
      ttl_ms: 10_000,
      value: { String: { value: "imported" } },
    };
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    const file = new File([JSON.stringify([entry])], "keys.json", {
      type: "application/json",
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.change(screen.getByLabelText("导入 JSON 文件"), {
      target: { files: [file] },
    });

    await waitFor(() => {
      expect(importKeysMock).toHaveBeenCalledTimes(1);
      expect(importKeysMock).toHaveBeenCalledWith({
        connection_id: "local",
        entries: [entry],
      });
    });
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
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

  it("取消删除确认时不调用 deleteKey，也不进入 busy", async () => {
    vi.stubGlobal("confirm", vi.fn(() => false));
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("button", { name: "删除" }));

    expect(confirm).toHaveBeenCalledWith("确定删除键“user:1”吗？");
    expect(deleteKeyMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "删除" })).not.toBeDisabled();
  });

  it("确认删除后调用 deleteKey 并清理键", async () => {
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
      expect(confirm).toHaveBeenCalledWith("确定删除键“user:1”吗？");
      expect(deleteKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
      });
    });
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
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

  it("TTL 为 0 时按立即删除处理，不刷新已删除详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    setKeyTtlMock.mockResolvedValue(-2);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.change(screen.getByLabelText("TTL（毫秒）"), {
      target: { value: "0" },
    });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));

    await waitFor(() => {
      expect(setKeyTtlMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
        ttl_ms: 0,
      });
    });
    expect(getKeyMock).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    expect(screen.getByText("请选择一个键查看详情")).toBeInTheDocument();
  });

  it("连接切换后忽略旧连接保存完成，不刷新旧详情", async () => {
    const save = deferred<typeof stringDetail>();
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    setKeyMock.mockImplementation(() => save.promise);

    const { rerender } = render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    const getCallsBeforeSwitch = getKeyMock.mock.calls.length;

    rerender(<BrowserPage connectionId="remote" />);
    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenCalledWith({
        connection_id: "remote",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: null,
      });
    });
    save.resolve(stringDetail);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(getKeyMock).toHaveBeenCalledTimes(getCallsBeforeSwitch);
    expect(screen.getByText("请选择一个键查看详情")).toBeInTheDocument();
  });

  it("详情组件卸载后忽略未完成 TTL，不刷新或更新详情", async () => {
    const ttl = deferred<number>();
    const onDetailChange = vi.fn();
    setKeyTtlMock.mockImplementation(() => ttl.promise);

    const { unmount } = render(
      <KeyDetails
        connectionId="local"
        detail={stringDetail}
        loading={false}
        onDetailChange={onDetailChange}
        onDeleted={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText("TTL（毫秒）"), {
      target: { value: "1000" },
    });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    unmount();
    ttl.resolve(1000);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(getKeyMock).not.toHaveBeenCalled();
    expect(onDetailChange).not.toHaveBeenCalled();
  });

  it("详情组件卸载后忽略未完成删除，不调用 onDeleted", async () => {
    const deletion = deferred<void>();
    const onDeleted = vi.fn();
    deleteKeyMock.mockImplementation(() => deletion.promise);

    const { unmount } = render(
      <KeyDetails
        connectionId="local"
        detail={stringDetail}
        loading={false}
        onDetailChange={vi.fn()}
        onDeleted={onDeleted}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    unmount();
    deletion.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onDeleted).not.toHaveBeenCalled();
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

  it("同一详情身份的父级重渲染不会覆盖 TTL 和 Sorted Set score 输入", () => {
    const detail: KeyValue = {
      key: "zset:1",
      key_type: "zset",
      ttl_ms: -1,
      value: { SortedSet: { members: [{ member: "member", score: 1 }] } },
    };
    const onDetailChange = vi.fn();
    const onDeleted = vi.fn();

    const { rerender } = render(
      <KeyDetails
        connectionId="local"
        detail={detail}
        loading={false}
        onDetailChange={onDetailChange}
        onDeleted={onDeleted}
      />,
    );
    fireEvent.change(screen.getByLabelText("TTL（毫秒）"), {
      target: { value: "60000" },
    });
    fireEvent.change(screen.getByLabelText("分数 1"), {
      target: { value: "Infinity" },
    });

    rerender(
      <KeyDetails
        connectionId="local"
        detail={{
          ...detail,
          value: { SortedSet: { members: [{ member: "member", score: 1 }] } },
        }}
        loading={false}
        onDetailChange={onDetailChange}
        onDeleted={onDeleted}
      />,
    );

    expect(screen.getByLabelText("TTL（毫秒）")).toHaveValue(60000);
    expect(screen.getByLabelText("分数 1")).toHaveValue("Infinity");
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

  it("SCAN 替换和追加按 key 去重，并保留最新摘要", () => {
    const current = {
      ...initialBrowserPageState,
      keys: [{ ...stringSummary, ttl_ms: 1000 }],
      cursor: 7,
      hasMore: true,
    };
    const page = {
      cursor: 0,
      keys: [
        { ...stringSummary, ttl_ms: 2000 },
        { ...stringSummary, ttl_ms: 3000 },
        { ...stringSummary, key: "user:2", size: 3 },
      ],
      has_more: false,
    };

    expect(applyScanPage(current, page, true).keys).toEqual([
      { ...stringSummary, ttl_ms: 3000 },
      { ...stringSummary, key: "user:2", size: 3 },
    ]);
    expect(applyScanPage(current, page, false).keys).toEqual([
      { ...stringSummary, ttl_ms: 3000 },
      { ...stringSummary, key: "user:2", size: 3 },
    ]);
  });

  it("四类集合为空时阻止保存并显示 inline alert", () => {
    const cases: Array<{ value: RedisValue; remove: string }> = [
      {
        value: { Hash: { fields: [{ field: "field", value: "value" }] } },
        remove: "删除字段 1",
      },
      {
        value: { List: { items: ["item"] } },
        remove: "删除元素 1",
      },
      {
        value: { Set: { members: ["member"] } },
        remove: "删除成员 1",
      },
      {
        value: { SortedSet: { members: [{ member: "member", score: 1 }] } },
        remove: "删除成员 1",
      },
    ];

    for (const testCase of cases) {
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(
        <KeyEditor
          value={testCase.value}
          ttlMs={-1}
          busy={false}
          error={null}
          onSave={onSave}
          onDelete={vi.fn().mockResolvedValue(undefined)}
          onSetTtl={vi.fn().mockResolvedValue(undefined)}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: testCase.remove }));
      fireEvent.click(screen.getByRole("button", { name: "保存" }));

      expect(screen.getByRole("alert")).toHaveTextContent("至少保留一项");
      expect(onSave).not.toHaveBeenCalled();
      cleanup();
    }
  });

  it("新增 String 键并在创建完成后刷新列表", async () => {
    const created = {
      ...stringDetail,
      key: "new:user",
    };
    scanKeysMock
      .mockResolvedValueOnce({ cursor: 0, keys: [], has_more: false })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [{ ...stringSummary, key: "new:user" }],
        has_more: false,
      });
    createKeyMock.mockResolvedValue(created);

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "新增键" }));
    fireEvent.change(screen.getByLabelText("键名"), {
      target: { value: "new:user" },
    });
    fireEvent.change(screen.getByLabelText("字符串值"), {
      target: { value: "Alice" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => {
      expect(createKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "new:user",
        value: { String: { value: "Alice" } },
        ttl_ms: null,
      });
      expect(scanKeysMock).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText("new:user")).toBeInTheDocument();
  });

  it("新增 Stream 键时生成 Stream DTO", async () => {
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    createKeyMock.mockResolvedValue({
      key: "events",
      key_type: "stream",
      ttl_ms: -1,
      value: {
        Stream: {
          entries: [{ id: "1-0", fields: [{ field: "event", value: "created" }] }],
        },
      },
    });

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "新增键" }));
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "events" } });
    fireEvent.change(screen.getByLabelText("数据类型"), { target: { value: "stream" } });
    fireEvent.change(screen.getByLabelText("Stream 条目 JSON"), {
      target: {
        value: JSON.stringify([
          { id: "1-0", fields: [{ field: "event", value: "created" }] },
        ]),
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => {
      expect(createKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        value: {
          Stream: {
            entries: [{ id: "1-0", fields: [{ field: "event", value: "created" }] }],
          },
        },
        ttl_ms: null,
      });
    });
  });

  it("拒绝空键名和后端重复键错误", async () => {
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "新增键" }));
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("键名不能为空");
    expect(createKeyMock).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "existing" } });
    createKeyMock.mockRejectedValueOnce({ code: "COMMAND_FAILED", message: "duplicate" });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("创建键失败");
  });

  it("选择多个键后只发起一次批量删除，并在刷新后清空选择", async () => {
    const summaries = [
      stringSummary,
      { ...stringSummary, key: "user:2" },
    ];
    scanKeysMock
      .mockResolvedValueOnce({ cursor: 0, keys: summaries, has_more: false })
      .mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    deleteKeysMock.mockResolvedValue(2);

    render(<BrowserPage connectionId="local" />);
    await screen.findByText("user:2");
    fireEvent.click(screen.getByLabelText("选择键 user:1"));
    fireEvent.click(screen.getByLabelText("选择键 user:2"));
    expect(screen.getByRole("button", { name: "批量删除（2）" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "批量删除（2）" }));

    await waitFor(() => {
      expect(deleteKeysMock).toHaveBeenCalledTimes(1);
      expect(deleteKeysMock).toHaveBeenCalledWith({
        connection_id: "local",
        keys: ["user:1", "user:2"],
      });
      expect(scanKeysMock).toHaveBeenCalledTimes(2);
    });
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
  });

  it("显式刷新从游标 0 重新扫描并清空已有选择", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    render(<BrowserPage connectionId="local" />);
    await screen.findByText("user:1");
    fireEvent.click(screen.getByLabelText("选择键 user:1"));
    fireEvent.click(screen.getByRole("button", { name: "刷新键列表" }));

    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: null,
      });
    });
    expect(screen.getByLabelText("选择键 user:1")).not.toBeChecked();
  });

  it("连接切换后忽略未完成新增键响应", async () => {
    const creation = deferred<typeof stringDetail>();
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    createKeyMock.mockImplementation(() => creation.promise);

    const { rerender } = render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "新增键" }));
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "stale" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    rerender(<BrowserPage connectionId="remote" />);
    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenCalledWith({
        connection_id: "remote",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: null,
      });
    });
    creation.resolve(stringDetail);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(scanKeysMock).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("stale")).not.toBeInTheDocument();
  });

  it("JSON 编辑器格式化显示、解析保存并拒绝非法 JSON", async () => {
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", active: true } } },
    };
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <KeyEditor
        value={jsonDetail.value}
        ttlMs={-1}
        busy={false}
        error={null}
        onSave={onSave}
        onDelete={vi.fn().mockResolvedValue(undefined)}
        onSetTtl={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    const editor = screen.getByLabelText("JSON 文档");
    expect(editor).toHaveValue(JSON.stringify({ name: "Alice", active: true }, null, 2));
    fireEvent.change(editor, { target: { value: '{"name":' } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("JSON 格式无效");
    expect(onSave).not.toHaveBeenCalled();

    fireEvent.change(editor, { target: { value: '{"name":"Bob","active":false}' } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith({
        Json: { value: { name: "Bob", active: false } },
      });
    });
  });

  it("Stream 编辑器允许编辑字段并拒绝空字段名", async () => {
    const streamValue: RedisValue = {
      Stream: {
        entries: [{
          id: "1-0",
          fields: [{ field: "event", value: "created" }],
        }],
      },
    };
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <KeyEditor
        value={streamValue}
        ttlMs={-1}
        busy={false}
        error={null}
        onSave={onSave}
        onDelete={vi.fn().mockResolvedValue(undefined)}
        onSetTtl={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    fireEvent.change(screen.getByLabelText("Stream 字段 1 名称"), {
      target: { value: "kind" },
    });
    fireEvent.change(screen.getByLabelText("Stream 字段 1 值"), {
      target: { value: "created" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => {
      expect(onSave).toHaveBeenCalledWith({
        Stream: {
          entries: [{
            id: "1-0",
            fields: [{ field: "kind", value: "created" }],
          }],
        },
      });
    });

    onSave.mockClear();
    fireEvent.change(screen.getByLabelText("Stream 字段 1 名称"), {
      target: { value: " " },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Stream 字段名不能为空");
    expect(onSave).not.toHaveBeenCalled();
  });

  it("详情重命名更新身份，并可刷新详细元数据", async () => {
    const renamedDetail = { ...stringDetail, key: "user:renamed" };
    const onRenamed = vi.fn();
    renameKeyMock.mockResolvedValue(renamedDetail);
    getKeyInfoMock.mockResolvedValue({
      key: "user:renamed",
      key_type: "string",
      ttl_ms: 5000,
      size: 5,
      memory_bytes: 64,
      encoding: "embstr",
      idle_seconds: 2,
    });
    const { rerender } = render(
      <KeyDetails
        connectionId="local"
        detail={stringDetail}
        loading={false}
        onDetailChange={vi.fn()}
        onRenamed={onRenamed}
        onDeleted={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByLabelText("新键名"), {
      target: { value: "user:renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));
    await waitFor(() => {
      expect(renameKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
        new_key: "user:renamed",
      });
      expect(onRenamed).toHaveBeenCalledWith("user:1", renamedDetail);
    });

    rerender(
      <KeyDetails
        connectionId="local"
        detail={renamedDetail}
        loading={false}
        onDetailChange={vi.fn()}
        onRenamed={onRenamed}
        onDeleted={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "刷新元数据" }));
    await waitFor(() => {
      expect(getKeyInfoMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:renamed",
      });
    });
    expect(await screen.findByText("64 B")).toBeInTheDocument();
  });

  it("BrowserPage 重命名后同步列表和当前选中键身份", async () => {
    const renamedDetail = { ...stringDetail, key: "user:renamed" };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      has_more: false,
    });
    getKeyMock.mockResolvedValue(stringDetail);
    renameKeyMock.mockResolvedValue(renamedDetail);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.change(screen.getByLabelText("新键名"), {
      target: { value: "user:renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "user:renamed" })).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
  });
});
