import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import BrowserPage from "./BrowserPage";
import {
  applyScanPage,
  initialBrowserPageState,
} from "./browserState";
import KeyDetails from "./KeyDetails";
import KeyEditor from "./KeyEditor";
import AddKey from "./AddKey";
import type { KeyValue, RedisValue, ScanPage } from "../../lib/types";

const { getStringValueMock, decodeStringValueMock, encodeStringValueMock, setStringValueMock } = vi.hoisted(() => ({
  getStringValueMock: vi.fn(), decodeStringValueMock: vi.fn(), encodeStringValueMock: vi.fn(), setStringValueMock: vi.fn(),
}));
vi.mock("./valueCodecApi", () => ({ getStringValue: getStringValueMock, decodeStringValue: decodeStringValueMock, encodeStringValue: encodeStringValueMock, setStringValue: setStringValueMock }));

const {
  acknowledgeStreamPendingEntriesMock,
  scanKeysMock,
  getKeyMock,
  getModuleCapabilitiesMock,
  getKeySearchIndexesMock,
  getJsonPathMock,
  setJsonPathMock,
  appendJsonArrayMock,
  deleteJsonPathMock,
  setKeyMock,
  deleteKeyMock,
  createKeyMock,
  createArrayMock,
  createVectorSetMock,
  getArraySummaryMock,
  getArrayRangeMock,
  setArrayElementMock,
  appendArrayElementsMock,
  deleteArrayElementsMock,
  deleteArrayRangeMock,
  searchArrayMock,
  aggregateArrayMock,
  getVectorSetSummaryMock,
  listVectorSetElementsMock,
  getVectorSetElementMock,
  addVectorSetElementsMock,
  setVectorSetAttributesMock,
  deleteVectorSetAttributesMock,
  deleteVectorSetElementsMock,
  searchVectorSetMock,
  downloadVectorEmbeddingMock,
  deleteKeysMock,
  exportKeysMock,
  renameKeyMock,
  getKeyInfoMock,
  importKeysMock,
  setKeyTtlMock,
  createStreamConsumerGroupMock,
  deleteStreamConsumerMock,
  deleteStreamConsumerGroupMock,
  getStreamConsumerGroupsMock,
  getStreamConsumersMock,
  getStreamPendingEntriesMock,
} = vi.hoisted(() => ({
  acknowledgeStreamPendingEntriesMock: vi.fn(),
  scanKeysMock: vi.fn(),
  getKeyMock: vi.fn(),
  getModuleCapabilitiesMock: vi.fn(),
  getKeySearchIndexesMock: vi.fn(),
  getJsonPathMock: vi.fn(),
  setJsonPathMock: vi.fn(),
  appendJsonArrayMock: vi.fn(),
  deleteJsonPathMock: vi.fn(),
  setKeyMock: vi.fn(),
  deleteKeyMock: vi.fn(),
  createKeyMock: vi.fn(),
  createArrayMock: vi.fn(),
  createVectorSetMock: vi.fn(),
  getArraySummaryMock: vi.fn(),
  getArrayRangeMock: vi.fn(),
  setArrayElementMock: vi.fn(),
  appendArrayElementsMock: vi.fn(),
  deleteArrayElementsMock: vi.fn(),
  deleteArrayRangeMock: vi.fn(),
  searchArrayMock: vi.fn(),
  aggregateArrayMock: vi.fn(),
  getVectorSetSummaryMock: vi.fn(),
  listVectorSetElementsMock: vi.fn(),
  getVectorSetElementMock: vi.fn(),
  addVectorSetElementsMock: vi.fn(),
  setVectorSetAttributesMock: vi.fn(),
  deleteVectorSetAttributesMock: vi.fn(),
  deleteVectorSetElementsMock: vi.fn(),
  searchVectorSetMock: vi.fn(),
  downloadVectorEmbeddingMock: vi.fn(),
  deleteKeysMock: vi.fn(),
  exportKeysMock: vi.fn(),
  renameKeyMock: vi.fn(),
  getKeyInfoMock: vi.fn(),
  importKeysMock: vi.fn(),
  setKeyTtlMock: vi.fn(),
  createStreamConsumerGroupMock: vi.fn(),
  deleteStreamConsumerMock: vi.fn(),
  deleteStreamConsumerGroupMock: vi.fn(),
  getStreamConsumerGroupsMock: vi.fn(),
  getStreamConsumersMock: vi.fn(),
  getStreamPendingEntriesMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  scanKeys: scanKeysMock,
  getKey: getKeyMock,
  getBrowserKey: getKeyMock,
  getModuleCapabilities: getModuleCapabilitiesMock,
  getKeySearchIndexes: getKeySearchIndexesMock,
  getJsonPath: getJsonPathMock,
  setJsonPath: setJsonPathMock,
  appendJsonArray: appendJsonArrayMock,
  deleteJsonPath: deleteJsonPathMock,
  setKey: setKeyMock,
  deleteKey: deleteKeyMock,
  createKey: createKeyMock,
  createArray: createArrayMock,
  createVectorSet: createVectorSetMock,
  getArraySummary: getArraySummaryMock,
  getArrayRange: getArrayRangeMock,
  setArrayElement: setArrayElementMock,
  appendArrayElements: appendArrayElementsMock,
  deleteArrayElements: deleteArrayElementsMock,
  deleteArrayRange: deleteArrayRangeMock,
  searchArray: searchArrayMock,
  aggregateArray: aggregateArrayMock,
  getVectorSetSummary: getVectorSetSummaryMock,
  listVectorSetElements: listVectorSetElementsMock,
  getVectorSetElement: getVectorSetElementMock,
  addVectorSetElements: addVectorSetElementsMock,
  setVectorSetAttributes: setVectorSetAttributesMock,
  deleteVectorSetAttributes: deleteVectorSetAttributesMock,
  deleteVectorSetElements: deleteVectorSetElementsMock,
  searchVectorSet: searchVectorSetMock,
  downloadVectorEmbedding: downloadVectorEmbeddingMock,
  deleteKeys: deleteKeysMock,
  exportKeys: exportKeysMock,
  renameKey: renameKeyMock,
  renameBrowserKey: renameKeyMock,
  getKeyInfo: getKeyInfoMock,
  importKeys: importKeysMock,
  setKeyTtl: setKeyTtlMock,
  acknowledgeStreamPendingEntries: acknowledgeStreamPendingEntriesMock,
  createStreamConsumerGroup: createStreamConsumerGroupMock,
  deleteStreamConsumer: deleteStreamConsumerMock,
  deleteStreamConsumerGroup: deleteStreamConsumerGroupMock,
  getStreamConsumerGroups: getStreamConsumerGroupsMock,
  getStreamConsumers: getStreamConsumersMock,
  getStreamPendingEntries: getStreamPendingEntriesMock,
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
  it("部分节点失败可继续重试，完成 opaque cursor 不显示更多", async () => {
    scanKeysMock.mockResolvedValueOnce({ cursor: "cluster:retry", keys: [], node_failures: [{ node_id: "node-b", code: "CONNECTION_FAILED" }], has_more: true }).mockResolvedValueOnce({ cursor: "cluster:complete", keys: [], node_failures: [], has_more: false });
    render(<BrowserPage connectionId="cluster" />);
    expect(await screen.findByRole("alert")).toHaveTextContent("1 个节点扫描失败");
    expect(screen.queryByText("没有匹配的键。")).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "继续扫描并重试" }));
    await waitFor(() => expect(scanKeysMock).toHaveBeenCalledTimes(2));
    expect(scanKeysMock.mock.calls[1][0].cursor).toBe("cluster:retry");
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
  });
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("confirm", vi.fn(() => true));
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
    getKeyMock.mockResolvedValue(stringDetail);
    getStringValueMock.mockResolvedValue({ base64: btoa("Alice"), total_bytes: 5, ttl_ms: -1, truncated: false });
    decodeStringValueMock.mockImplementation(async ({ base64 }) => ({ text: atob(base64), byte_length: atob(base64).length }));
    encodeStringValueMock.mockImplementation(async ({ text }) => btoa(text));
    setStringValueMock.mockResolvedValue({ byte_length: 3, ttl_ms: -1 });
    getModuleCapabilitiesMock.mockResolvedValue({
      modules: [{ name: "ReJSON", version: "20611" }],
      json_supported: true,
      json_version: "20611",
      search_supported: false,
      search_version: null,
      array_supported: false,
      vector_set_supported: false,
    });
    getKeySearchIndexesMock.mockResolvedValue([]);
    getJsonPathMock.mockResolvedValue({
      key: "profile:1",
      path: "$",
      found: true,
      value: { name: "Alice" },
      ttl_ms: -1,
    });
    setJsonPathMock.mockResolvedValue({
      key: "profile:1",
      path: "$.name",
      affected: 1,
      new_length: null,
      ttl_ms: -1,
    });
    appendJsonArrayMock.mockResolvedValue({
      key: "profile:1",
      path: "$.tags",
      affected: 1,
      new_length: 2,
      ttl_ms: -1,
    });
    deleteJsonPathMock.mockResolvedValue({
      key: "profile:1",
      path: "$.obsolete",
      affected: 1,
      new_length: null,
      ttl_ms: -1,
    });
    setKeyMock.mockResolvedValue(stringDetail);
    deleteKeyMock.mockResolvedValue(undefined);
    createKeyMock.mockResolvedValue(stringDetail);
    createArrayMock.mockResolvedValue(stringDetail);
    createVectorSetMock.mockResolvedValue(stringDetail);
    getArraySummaryMock.mockResolvedValue({
      key: "events",
      length: "0",
      count: "0",
      next_index: "0",
    });
    getArrayRangeMock.mockResolvedValue({
      start: "0",
      end: "499",
      cells: [],
      has_more: false,
    });
    setArrayElementMock.mockResolvedValue({ affected: 1, key_exists: true, next_index: null });
    appendArrayElementsMock.mockResolvedValue({ affected: 1, key_exists: true, next_index: "0" });
    deleteArrayElementsMock.mockResolvedValue({ affected: 1, key_exists: true, next_index: null });
    deleteArrayRangeMock.mockResolvedValue({ affected: 1, key_exists: true, next_index: null });
    searchArrayMock.mockResolvedValue({ elements: [], total: "0" });
    aggregateArrayMock.mockResolvedValue({ operation: "SUM", value: "0" });
    getVectorSetSummaryMock.mockResolvedValue({
      key: "embeddings",
      total: "0",
      dimension: 3,
      quantization: "f32",
    });
    listVectorSetElementsMock.mockResolvedValue({ elements: [], cursor: null, has_more: false });
    getVectorSetElementMock.mockResolvedValue({ name: "one", score: null, vector_base64: null, attributes: null });
    addVectorSetElementsMock.mockResolvedValue(undefined);
    setVectorSetAttributesMock.mockResolvedValue({ name: "one", score: null, vector_base64: null, attributes: null });
    deleteVectorSetAttributesMock.mockResolvedValue(undefined);
    deleteVectorSetElementsMock.mockResolvedValue(0);
    searchVectorSetMock.mockResolvedValue({ matches: [], has_more: false });
    downloadVectorEmbeddingMock.mockResolvedValue("");
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
    getStreamConsumerGroupsMock.mockResolvedValue([]);
    getStreamConsumersMock.mockResolvedValue([]);
    getStreamPendingEntriesMock.mockResolvedValue([]);
    createStreamConsumerGroupMock.mockResolvedValue(undefined);
    deleteStreamConsumerMock.mockResolvedValue(0);
    deleteStreamConsumerGroupMock.mockResolvedValue(1);
    acknowledgeStreamPendingEntriesMock.mockResolvedValue(0);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("筛选关闭后继续防抖扫描，重开保留模式、类型和分隔符", async () => {
    render(<BrowserPage connectionId="local" />);
    await waitFor(() => expect(screen.queryByText("正在扫描键…")).not.toBeInTheDocument());
    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "user:*" } });
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "user::*" } });
    fireEvent.change(screen.getByLabelText("键树分隔符"), { target: { value: "::" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));

    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
    expect(scanKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", cursor: 0, pattern: "user::*", count: 100, key_type: null,
    });

    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("键过滤")).toHaveValue("user::*");
    expect(screen.getByLabelText("键树分隔符")).toHaveValue("::");
    fireEvent.click(screen.getByRole("combobox", { name: "类型过滤" }));
    fireEvent.click(within(screen.getByRole("listbox", { name: "类型过滤" })).getByRole("option", { name: "Hash" }));
    await act(async () => { await Promise.resolve(); });
    expect(scanKeysMock).toHaveBeenCalledTimes(2);

    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("键过滤")).toHaveValue("user::*");
    expect(screen.getByLabelText("类型过滤")).toHaveValue("hash");
    expect(screen.getByLabelText("键树分隔符")).toHaveValue("::");
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
  });

  it("按模式加载键并在点击键后读取详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValue(stringDetail);

    render(<BrowserPage connectionId="local" />);

    expect(await screen.findByRole("button", { name: "展开前缀 user:" })).toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledWith({
      connection_id: "local",
      cursor: 0,
      pattern: "*",
      count: 100,
      key_type: null,
    });

    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));

    expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
    expect(getKeyMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "user:1",
    });
  });

  it.each(["树形", "平铺"])("%s视图读取详情时不插入扫描提示，且保持操作禁用", async (view) => {
    scanKeysMock.mockResolvedValue({
      cursor: 10, keys: [stringSummary], node_failures: [], has_more: true,
    });
    const pendingDetail = deferred<KeyValue>();
    getKeyMock.mockReturnValueOnce(pendingDetail.promise);
    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    if (view === "平铺") {
      fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    } else {
      fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    }

    fireEvent.click(screen.getByRole("button", { name: "user:1" }));

    const listPanel = screen.getByRole("region", { name: "键列表" });
    expect(within(screen.getByRole("region", { name: "键详情" })).getByRole("status"))
      .toHaveTextContent("正在读取键详情…");
    expect(within(listPanel).queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "加载更多" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "user:1" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新键列表" })).toBeDisabled();

    await act(async () => pendingDetail.resolve(stringDetail));

    expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
    expect(within(listPanel).queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "user:1" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "加载更多" })).toBeEnabled();
  });

  it("扫描分页期间保留扫描提示和操作禁用，完成后追加键", async () => {
    const pendingScan = deferred<{
      cursor: number; keys: typeof stringSummary[]; node_failures: []; has_more: boolean;
    }>();
    scanKeysMock.mockResolvedValueOnce({
      cursor: 10, keys: [stringSummary], node_failures: [], has_more: true,
    }).mockReturnValueOnce(pendingScan.promise);
    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));

    const listPanel = screen.getByRole("region", { name: "键列表" });
    expect(within(listPanel).getByRole("status")).toHaveTextContent("正在扫描键…");
    expect(screen.getByRole("button", { name: "加载中…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "user:1" })).toBeDisabled();

    await act(async () => pendingScan.resolve({
      cursor: 0, keys: [{ ...stringSummary, key: "user:2" }], node_failures: [], has_more: false,
    }));

    expect(screen.getByRole("button", { name: "user:2" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "user:1" })).toBeEnabled();
    expect(within(listPanel).queryByRole("status")).not.toBeInTheDocument();
  });

  it("Array 和 Vector Set 键路由到专用详情视图", async () => {
    const onDetailChange = vi.fn();
    const onDeleted = vi.fn();
    const moduleProbe = {
      status: "ready" as const,
      capabilities: {
        modules: [
          { name: "RedisArray", version: "1.0.0" },
          { name: "RedisVSet", version: "1.0.0" },
        ],
        json_supported: false,
        json_version: null,
        search_supported: false,
        search_version: null,
        array_supported: true,
        vector_set_supported: true,
      },
    };

    const arrayDetail: KeyValue = {
      key: "events",
      key_type: "array",
      ttl_ms: -1,
      value: { Array: { length: "2", count: "2" } },
    };
    const vectorDetail: KeyValue = {
      key: "embeddings",
      key_type: "vectorset",
      ttl_ms: -1,
      value: { VectorSet: { total: "1", dimension: 3, quantization: "f32" } },
    };

    const { rerender } = render(
      <KeyDetails
        connectionId="local"
        detail={arrayDetail}
        loading={false}
        moduleProbe={moduleProbe}
        onDetailChange={onDetailChange}
        onDeleted={onDeleted}
      />,
    );
    expect(await screen.findByRole("heading", { name: "Array" })).toBeInTheDocument();
    expect(getArraySummaryMock).toHaveBeenCalledWith({ connection_id: "local", key: "events" });

    rerender(
      <KeyDetails
        connectionId="local"
        detail={vectorDetail}
        loading={false}
        moduleProbe={moduleProbe}
        onDetailChange={onDetailChange}
        onDeleted={onDeleted}
      />,
    );
    expect(await screen.findByRole("heading", { name: "Vector Set" })).toBeInTheDocument();
    await waitFor(() => {
      expect(getVectorSetSummaryMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "embeddings",
      });
    });
  });

  it("加载页面时探测模块能力且不阻塞初始扫描", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);

    expect(await screen.findByRole("button", { name: "展开前缀 user:" })).toBeInTheDocument();
    await waitFor(() => {
      expect(getModuleCapabilitiesMock).toHaveBeenCalledWith("local");
    });
    expect(scanKeysMock).toHaveBeenCalledWith({
      connection_id: "local",
      cursor: 0,
      pattern: "*",
      count: 100,
      key_type: null,
    });
  });

  it("模块探测失败时仍然扫描并保留普通 Browser", async () => {
    getModuleCapabilitiesMock.mockRejectedValue({
      code: "CONNECTION_FAILED",
      message: "module probe failed",
    });
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);

    expect(await screen.findByRole("button", { name: "展开前缀 user:" })).toBeInTheDocument();
    expect(screen.queryByText("RedisJSON 路径编辑器暂不可用。")).not.toBeInTheDocument();
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

  it("保存 String 原始字节后更新详情，不触发旧UTF8整值读取", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(stringDetail)
      .mockResolvedValueOnce({
        ...stringDetail,
        value: { String: { value: "Bob" } },
      });
    setStringValueMock.mockResolvedValue({ byte_length: 3, ttl_ms: -1 });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));

    const input = await screen.findByDisplayValue("Alice");
    fireEvent.change(input, { target: { value: "Bob" } });
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));

    await waitFor(() => {
      expect(setStringValueMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
        base64: btoa("Bob"),
      });
    });
    expect(getKeyMock).toHaveBeenCalledTimes(1);
    expect(setKeyMock).not.toHaveBeenCalled();
  });

  it("默认按前缀分类，展开不扫描，分页归并目录并保留完整键名，过滤重置列表", async () => {
    scanKeysMock
      .mockResolvedValueOnce({
        cursor: 42,
        keys: [stringSummary],
        node_failures: [], has_more: true,
      })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [
          { ...stringSummary, key: "admin:1" },
          { ...stringSummary, key: "user:2" },
          stringSummary,
        ],
        node_failures: [], has_more: false,
      })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [{ ...stringSummary, key: "user:3" }],
        node_failures: [], has_more: false,
      });

    getKeyMock.mockResolvedValue({ ...stringDetail, key: "user:2" });

    render(<BrowserPage connectionId="local" />);
    const userFolder = await screen.findByRole("button", { name: "展开前缀 user:" });
    expect(screen.getByRole("button", { name: "树形" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("list", { name: "Redis 键树" })).toBeInTheDocument();
    expect(userFolder).toHaveAttribute("aria-expanded", "false");
    expect(within(userFolder).getByText("1")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(1);

    fireEvent.click(userFolder);
    expect(screen.getByRole("button", { name: "user:1" })).toHaveTextContent("1");
    expect(screen.queryByText("user:1")).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    expect(getKeyMock).not.toHaveBeenCalled();

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
    const adminFolder = await screen.findByRole("button", { name: "展开前缀 admin:" });
    expect(screen.getAllByRole("button", { name: "折叠前缀 user:" })).toHaveLength(1);
    expect(within(screen.getByRole("button", { name: "折叠前缀 user:" })).getByText("2")).toBeInTheDocument();
    const userKeys = within(screen.getByRole("list", { name: "前缀 user: 的键" }));
    expect(userKeys.getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual(["user:1", "user:2"]);
    expect(screen.getByLabelText("当前 3 个键")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();

    fireEvent.click(adminFolder);
    expect(screen.getByRole("button", { name: "admin:1" })).toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
    fireEvent.click(userKeys.getByRole("button", { name: "user:2" }));
    expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
    expect(getKeyMock).toHaveBeenCalledExactlyOnceWith({ connection_id: "local", key: "user:2" });

    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    const pattern = screen.getByLabelText("键过滤");
    fireEvent.change(pattern, { target: { value: "user:*" } });
    fireEvent.keyDown(pattern, { key: "Enter", code: "Enter" });
    expect(scanKeysMock).toHaveBeenCalledTimes(3);

    await waitFor(() => {
      expect(scanKeysMock).toHaveBeenLastCalledWith({
        connection_id: "local",
        cursor: 0,
        pattern: "user:*",
        count: 100,
        key_type: null,
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    expect(screen.getByRole("button", { name: "user:3" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "展开前缀 admin:" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "user:2" })).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(3);
  });

  it("键名防抖扫描保留期间切换的类型，并重置旧缓存", async () => {
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: 42, keys: [stringSummary], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: 0, keys: [
        { ...stringSummary, key: "new-stream", key_type: "stream" },
        { ...stringSummary, key: "new-string" },
      ], node_failures: [], has_more: false });
    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "new-*" } });
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "stream" } });
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(500); });
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
    expect(scanKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", cursor: 0, pattern: "new-*", count: 100, key_type: null,
    });
    expect(screen.getByLabelText("类型过滤")).toHaveValue("stream");
    expect(screen.getByRole("button", { name: "new-stream" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "new-string" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "" } });
    expect(screen.getByRole("button", { name: "new-string" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "展开前缀 user:" })).not.toBeInTheDocument();
  });

  it("类型筛选保留可见选择和详情，清除隐藏选择但保留键缓存", async () => {
    scanKeysMock.mockReset().mockResolvedValueOnce({
      cursor: 42, keys: [stringSummary, { ...stringSummary, key: "events", key_type: "stream" }], node_failures: [], has_more: true,
    });
    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "events" });
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user:1" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 events" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "string" } });
    expect(screen.getByDisplayValue("Alice")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).toBeChecked();
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "stream" } });
    expect(screen.queryByDisplayValue("Alice")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "选择键 events" })).not.toBeChecked();
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "" } });
    expect(screen.getByRole("button", { name: "user:1" })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).not.toBeChecked();
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    expect(getKeyMock).toHaveBeenCalledTimes(1);
  });

  it.each(["设置 TTL", "重命名"])("%s 回读改变类型时清除隐藏键的勾选和详情，并保留新类型缓存", async (action) => {
    scanKeysMock.mockReset().mockResolvedValueOnce({ cursor: 0, keys: [stringSummary], node_failures: [], has_more: false });
    const updated = { ...stringDetail, key: action === "重命名" ? "user:renamed" : "user:1", key_type: "json", ttl_ms: 60000, value: { Json: { value: { name: "Alice" } } } };
    getKeyMock.mockReset().mockResolvedValueOnce(stringDetail);
    if (action === "设置 TTL") getKeyMock.mockResolvedValueOnce(updated);
    else renameKeyMock.mockResolvedValueOnce(updated);
    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user:1" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "string" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.change(screen.getByLabelText(action === "设置 TTL" ? "TTL（毫秒）" : "新键名"), {
      target: { value: action === "设置 TTL" ? "60000" : "user:renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: action }));

    expect(await screen.findByText("请选择一个键查看详情")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导出选中键" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "json" } });
    expect(screen.getByRole("button", { name: updated.key })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: `选择键 ${updated.key}` })).not.toBeChecked();
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
  });

  it("主动刷新重置缓存和游标，但保留当前类型筛选", async () => {
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: 42, keys: [stringSummary, { ...stringSummary, key: "profile", key_type: "hash" }], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: 0, keys: [{ ...stringSummary, key: "fresh" }, { ...stringSummary, key: "new-profile", key_type: "hash" }], node_failures: [], has_more: false });

    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "profile" });
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "hash" } });
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(screen.getByRole("button", { name: "刷新键列表" }));
    expect(await screen.findByRole("button", { name: "new-profile" })).toBeEnabled();
    expect(scanKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", cursor: 0, pattern: "*", count: 100, key_type: null,
    });
    expect(screen.queryByRole("button", { name: "fresh" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("类型过滤")).toHaveValue("hash");
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "" } });
    expect(screen.getByRole("button", { name: "fresh" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "profile" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "展开前缀 user:" })).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
  });

  it.each([
    { mode: "单机", cursors: [42, 17, 9, 3, 0] },
    { mode: "集群", cursors: ["cluster:a", "cluster:b", "cluster:c", "cluster:d", "cluster:done"] },
  ])("$mode 多次加载后切换 STREAM 立即复用缓存，后续沿原游标缓存全部类型", async ({ cursors }) => {
    const stream = { ...stringSummary, key: "events", key_type: "stream" };
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: cursors[0], keys: [stringSummary], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: cursors[1], keys: [{ ...stringSummary, key: "profile", key_type: "hash" }], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: cursors[2], keys: [stream], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: cursors[3], keys: [{ ...stringSummary, key: "settings" }], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: cursors[4], keys: [{ ...stream, key: "notifications" }], node_failures: [], has_more: false });

    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    await screen.findByRole("button", { name: "profile" });
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    await screen.findByRole("button", { name: "events" });
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.click(screen.getByLabelText("类型过滤"));
    fireEvent.click(within(screen.getByRole("listbox")).getByRole("option", { name: "Stream" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));

    expect(scanKeysMock).toHaveBeenCalledTimes(3);
    expect(screen.getByRole("button", { name: "events" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "profile" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("当前 1 个键")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    expect(await screen.findByRole("button", { name: "notifications" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "events" })).toBeEnabled();
    expect(screen.getByLabelText("当前 2 个键")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    expect(scanKeysMock.mock.calls.map(([input]) => input)).toEqual(
      [0, ...cursors.slice(0, 4)].map((cursor) => ({
        connection_id: "local", cursor, pattern: "*", count: 100, key_type: null,
      })),
    );

    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "hash" } });
    expect(screen.getByRole("button", { name: "profile" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "events" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "" } });
    for (const key of ["user:1", "profile", "events", "settings", "notifications"]) {
      expect(screen.getByRole("button", { name: key })).toBeEnabled();
    }
    expect(screen.getByLabelText("当前 5 个键")).toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(5);
  });

  it("缓存中没有 STREAM 时不自动重新扫描，用户继续加载后才推进游标", async () => {
    const pendingPage = deferred<ScanPage>();
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: 42, keys: [stringSummary], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: 17, keys: [], node_failures: [], has_more: true })
      .mockReturnValueOnce(pendingPage.promise);

    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "stream" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    expect(scanKeysMock).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("没有匹配的键。")).not.toBeInTheDocument();
    expect(screen.queryByText("正在扫描键…")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    await waitFor(() => expect(scanKeysMock).toHaveBeenCalledTimes(3));
    expect(screen.getByText("正在扫描键…")).toBeInTheDocument();

    await act(async () => pendingPage.resolve({ cursor: 0, keys: [], node_failures: [], has_more: false }));
    expect(screen.getByText("没有匹配的键。")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "" } });
    expect(screen.getByRole("button", { name: "展开前缀 user:" })).toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(3);
  });

  it("空批次后切换连接会终止旧 STREAM 筛选并清空旧缓存", async () => {
    const pendingPage = deferred<ScanPage>();
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: 42, keys: [stringSummary], node_failures: [], has_more: true })
      .mockResolvedValueOnce({ cursor: 17, keys: [], node_failures: [], has_more: true })
      .mockReturnValueOnce(pendingPage.promise)
      .mockResolvedValueOnce({ cursor: 0, keys: [{ ...stringSummary, key: "other" }], node_failures: [], has_more: false });

    const { rerender } = render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 user:" });
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "stream" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    await waitFor(() => expect(scanKeysMock).toHaveBeenCalledTimes(3));

    rerender(<BrowserPage connectionId="other" />);
    expect(await screen.findByRole("button", { name: "other" })).toBeEnabled();
    await act(async () => pendingPage.resolve({ cursor: 9, keys: [], node_failures: [], has_more: true }));
    expect(scanKeysMock).toHaveBeenCalledTimes(4);
    expect(screen.getByRole("button", { name: "other" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "展开前缀 user:" })).not.toBeInTheDocument();
    expect(screen.queryByText("正在扫描键…")).not.toBeInTheDocument();
  });

  it.each(["游标停滞", "请求失败"])("续扫遇到%s时停止并保留游标供重试", async (reason) => {
    scanKeysMock.mockReset()
      .mockResolvedValueOnce({ cursor: 42, keys: [], node_failures: [], has_more: true });
    if (reason === "游标停滞") {
      scanKeysMock.mockResolvedValueOnce({ cursor: 42, keys: [], node_failures: [], has_more: true });
    } else {
      scanKeysMock.mockRejectedValueOnce({ code: "CONNECTION_FAILED" });
    }
    scanKeysMock.mockResolvedValueOnce({
      cursor: 0, keys: [{ ...stringSummary, key: "events", key_type: "stream" }], node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    expect(await screen.findByRole("button", { name: "加载更多" })).toBeEnabled();
    expect(screen.queryByText("没有匹配的键。")).not.toBeInTheDocument();
    expect(screen.queryByText("正在扫描键…")).not.toBeInTheDocument();
    expect(scanKeysMock).toHaveBeenCalledTimes(2);
    if (reason === "请求失败") expect(screen.getByRole("alert")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    expect(await screen.findByRole("button", { name: "events" })).toBeEnabled();
    expect(scanKeysMock.mock.calls[2][0].cursor).toBe(42);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
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
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
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
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");

    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));

    expect(deleteKeyMock).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));

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
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));

    const dialog = screen.getByRole("alertdialog", { name: "确认删除" });
    expect(dialog).toHaveTextContent("user:1");
    await act(async () => { fireEvent.click(within(dialog).getByRole("button", { name: "取消" })); });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(deleteKeyMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "删除" })).not.toBeDisabled();
  });

  it("确认删除后调用 deleteKey 并清理键", async () => {
    vi.stubGlobal("confirm", vi.fn(() => false));
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    expect(deleteKeyMock).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));

    await waitFor(() => {
      expect(deleteKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "user:1",
      });
    });
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
  });

  it.each(["连接", "键"])("等待删除确认时切换%s会取消旧删除", async (scope) => {
    const onDeleted = vi.fn();
    const props = { connectionId: "local", detail: stringDetail, loading: false, onDetailChange: vi.fn(), onDeleted };
    const { rerender } = render(<KeyDetails {...props} />);
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    const approve = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    rerender(<KeyDetails {...props} connectionId={scope === "连接" ? "other" : "local"} detail={scope === "键" ? { ...stringDetail, key: "other" } : stringDetail} />);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await act(async () => { fireEvent.click(approve); });
    expect(deleteKeyMock).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("设置 TTL 后刷新详情", async () => {
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(stringDetail)
      .mockResolvedValueOnce({ ...stringDetail, ttl_ms: 60_000 });
    setKeyTtlMock.mockResolvedValue(60_000);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");

    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
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
      node_failures: [], has_more: false,
    });
    setKeyTtlMock.mockResolvedValue(-2);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
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
    const save = deferred<{ byte_length: number; ttl_ms: number }>();
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [stringSummary],
      node_failures: [], has_more: false,
    });
    setStringValueMock.mockImplementation(() => save.promise);

    const { rerender } = render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "Bob" } });
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    await waitFor(() => expect(setStringValueMock).toHaveBeenCalledTimes(1));
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
    save.resolve({ byte_length: 3, ttl_ms: -1 });
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
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
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
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(deleteKeyMock).toHaveBeenCalledOnce());
    unmount();
    deletion.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onDeleted).not.toHaveBeenCalled();
  });

  it("Hash、List、Set 和 Sorted Set 使用分页详情且不再执行整键覆盖保存", async () => {
    const summaries = [
      { key: "hash:1", key_type: "hash", ttl_ms: -1, size: 2 },
      { key: "list:1", key_type: "list", ttl_ms: -1, size: 2 },
      { key: "set:1", key_type: "set", ttl_ms: -1, size: 2 },
      { key: "zset:1", key_type: "zset", ttl_ms: -1, size: 2 },
    ];
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: summaries, node_failures: [], has_more: false });
    getKeyMock.mockImplementation(async ({ key }: { key: string }) => {
      const summary = summaries.find((item) => item.key === key)!;
      const value = summary.key_type === "hash" ? { Hash: { fields: [] } }
        : summary.key_type === "list" ? { List: { items: [] } }
        : summary.key_type === "set" ? { Set: { members: [] } }
        : { SortedSet: { members: [] } };
      return { key, key_type: summary.key_type, ttl_ms: -1, value };
    });

    render(<BrowserPage connectionId="local" />);
    await screen.findByRole("button", { name: "展开前缀 hash:" });
    for (const [key, label] of [["hash:1", "Hash 分页详情"], ["list:1", "List 分页详情"], ["set:1", "Set 分页详情"], ["zset:1", "Sorted Set 分页详情"]]) {
      fireEvent.click(screen.getByRole("button", { name: `展开前缀 ${key.split(":")[0]}:` }));
      await waitFor(() => expect(screen.getByRole("button", { name: key })).toBeEnabled());
      fireEvent.click(screen.getByRole("button", { name: key }));
      expect(await screen.findByRole("region", { name: label })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument();
    }
    expect(setKeyMock).not.toHaveBeenCalled();
  });

  it("同一分页详情身份的父级重渲染不会覆盖 TTL 输入", () => {
    const detail: KeyValue = {
      key: "zset:1", key_type: "zset", ttl_ms: -1,
      value: { SortedSet: { members: [] } },
    };
    const { rerender } = render(<KeyDetails connectionId="local" detail={detail} loading={false} onDetailChange={vi.fn()} onDeleted={vi.fn()} />);
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    fireEvent.change(screen.getByLabelText("TTL（毫秒）"), { target: { value: "60000" } });
    rerender(<KeyDetails connectionId="local" detail={{ ...detail, value: { SortedSet: { members: [] } } }} loading={false} onDetailChange={vi.fn()} onDeleted={vi.fn()} />);
    expect(screen.getByLabelText("TTL（毫秒）")).toHaveValue(60000);
  });

  it("加载失败时显示 alert 并在请求期间禁用重复过滤", async () => {
    let rejectScan: ((reason: unknown) => void) | undefined;
    scanKeysMock.mockImplementation(
      () => new Promise((_, reject) => {
        rejectScan = reject;
      }),
    );

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
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
      node_failures: [], has_more: false,
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

  it("新增键以弹窗打开，支持焦点循环、关闭和重新打开", async () => {
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    const trigger = screen.getByRole("button", { name: "新增键" });
    trigger.focus();
    fireEvent.click(trigger);

    const dialog = screen.getByRole("dialog", { name: "新增键" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByLabelText("键名")).toHaveFocus();
    expect(screen.getByText("没有匹配的键。")).toBeInTheDocument();
    const submit = within(dialog).getByRole("button", { name: "创建键" });
    submit.focus();
    fireEvent.keyDown(submit, { key: "Tab" });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    expect(submit).not.toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
    expect(submit).toHaveFocus();

    const kind = screen.getByLabelText("数据类型");
    fireEvent.click(kind);
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    fireEvent.keyDown(kind, { key: "Escape" });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(dialog).toBeInTheDocument();
    fireEvent.keyDown(kind, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    fireEvent.click(trigger);
    expect(screen.getByLabelText("键名")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(createKeyMock).not.toHaveBeenCalled();
  });

  it("创建请求期间禁用示例填充并阻止关闭弹窗，失败后保留表单", async () => {
    const creation = deferred<KeyValue>();
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
    createKeyMock.mockReturnValueOnce(creation.promise);
    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "新增键" }));
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "draft:key" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("button", { name: "添加示例数据" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
    fireEvent.keyDown(document, { key: "Escape" });
    const dialog = screen.getByRole("dialog", { name: "新增键" });
    fireEvent.click(dialog.parentElement!);
    expect(dialog).toBeInTheDocument();
    await act(async () => creation.reject({ code: "COMMAND_FAILED", message: "duplicate" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("创建键失败");
    expect(screen.getByLabelText("键名")).toHaveValue("draft:key");
    fireEvent.click(dialog.parentElement!);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("新增 String 键并在创建完成后刷新列表", async () => {
    const created = {
      ...stringDetail,
      key: "new:user",
    };
    scanKeysMock
      .mockResolvedValueOnce({ cursor: 0, keys: [], node_failures: [], has_more: false })
      .mockResolvedValueOnce({
        cursor: 0,
        keys: [{ ...stringSummary, key: "new:user" }],
        node_failures: [], has_more: false,
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
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 new:" }));
    expect(screen.getByRole("button", { name: "new:user" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "新增键" })).not.toBeInTheDocument();
  });

  it.each([false, true])("新增键成功刷新后恢复入口焦点，尊重用户主动移焦（%s）", async (focusMoved) => {
    const refresh = deferred<ScanPage>();
    scanKeysMock
      .mockResolvedValueOnce({ cursor: 0, keys: [], node_failures: [], has_more: false })
      .mockReturnValueOnce(refresh.promise);
    createKeyMock.mockResolvedValue({ ...stringDetail, key: "new:user" });
    render(<BrowserPage connectionId="local" />);
    await screen.findByText("没有匹配的键。");
    const trigger = screen.getByRole("button", { name: "新增键" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "new:user" } });
    const submit = screen.getByRole("button", { name: "创建键" });
    submit.focus();
    fireEvent.click(submit);

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "新增键" })).not.toBeInTheDocument());
    expect(trigger).toBeDisabled();
    // jsdom neither blurs newly disabled buttons nor allows blur() while disabled.
    (trigger as HTMLButtonElement).disabled = false;
    trigger.blur();
    (trigger as HTMLButtonElement).disabled = true;
    if (focusMoved) {
      const otherControl = screen.getByLabelText("键列表自动刷新");
      otherControl.focus();
      otherControl.blur();
    }
    expect(document.body).toHaveFocus();
    await act(async () => refresh.resolve({ cursor: 0, keys: [], node_failures: [], has_more: false }));
    expect(trigger).toBeEnabled();
    expect(focusMoved ? document.body : trigger).toHaveFocus();
  });

  it("新增 Stream 键时生成 Stream DTO", async () => {
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
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

  it("新增 Array 键使用模块 typed command", async () => {
    const onCreated = vi.fn();
    createArrayMock.mockResolvedValue({
      key: "events",
      key_type: "array",
      ttl_ms: -1,
      value: { Array: { length: "2", count: "2" } },
    });

    render(
      <AddKey
        connectionId="local"
        busy={false}
        arraySupported
        vectorSetSupported={false}
        onCreated={onCreated}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "events" } });
    fireEvent.change(screen.getByLabelText("数据类型"), { target: { value: "array" } });
    fireEvent.change(screen.getByLabelText("Array 起始索引"), { target: { value: "4" } });
    fireEvent.change(screen.getByLabelText("Array 值"), { target: { value: "a\nb" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => {
      expect(createArrayMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        mode: "contiguous",
        start_index: "4",
        values: ["a", "b"],
        elements: [],
        ttl_ms: null,
      });
    });
    expect(onCreated).toHaveBeenCalled();
  });

  it("新增 Vector Set 键使用维度和向量 typed command", async () => {
    const onCreated = vi.fn();
    createVectorSetMock.mockResolvedValue({
      key: "embeddings",
      key_type: "vectorset",
      ttl_ms: -1,
      value: { VectorSet: { total: "1", dimension: 3, quantization: "f32" } },
    });

    render(
      <AddKey
        connectionId="local"
        busy={false}
        arraySupported={false}
        vectorSetSupported
        onCreated={onCreated}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "embeddings" } });
    fireEvent.change(screen.getByLabelText("数据类型"), { target: { value: "vectorset" } });
    fireEvent.change(screen.getByLabelText("Vector Set 维度"), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText("Vector Set 元素"), { target: { value: "one|[0.1,0.2,0.3]" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => {
      expect(createVectorSetMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "embeddings",
        dimension: 3,
        quantization: null,
        elements: [
          {
            name: "one",
            vector_values: [0.1, 0.2, 0.3],
            vector_fp32_base64: null,
            attributes: null,
          },
        ],
        ttl_ms: null,
      });
    });
    expect(onCreated).toHaveBeenCalled();
  });

  it("拒绝空键名和后端重复键错误", async () => {
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
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
      .mockResolvedValueOnce({ cursor: 0, keys: summaries, node_failures: [], has_more: false })
      .mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
    deleteKeysMock.mockResolvedValue(2);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByLabelText("选择键 user:1"));
    fireEvent.click(screen.getByLabelText("选择键 user:2"));
    expect(screen.getByRole("button", { name: "批量删除（2）" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "批量删除（2）" }));
    expect(deleteKeysMock).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));

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
      node_failures: [], has_more: false,
    });
    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
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
    expect(await screen.findByLabelText("选择键 user:1")).not.toBeChecked();
  });

  it("连接切换后忽略未完成新增键响应", async () => {
    const creation = deferred<typeof stringDetail>();
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], node_failures: [], has_more: false });
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

  it("模块不可用时 JSON key 保留根编辑器并显示稳定降级提示", async () => {
    getModuleCapabilitiesMock.mockResolvedValue({
      modules: [],
      json_supported: false,
      json_version: null,
      search_supported: false,
      search_version: null,
    });
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValue(jsonDetail);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));

    expect(await screen.findByLabelText("JSON 文档")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "JSON Path" }));
    expect(screen.getByText("RedisJSON 路径编辑器暂不可用。")).toBeVisible();
    expect(screen.queryByRole("button", { name: "读取路径" })).not.toBeInTheDocument();
  });

  it("JSON key 渲染路径编辑器，并在 mutation 成功后刷新详情", async () => {
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    const refreshedDetail: KeyValue = {
      ...jsonDetail,
      value: { Json: { value: { name: "Bob", tags: ["redis"] } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(jsonDetail)
      .mockResolvedValueOnce(refreshedDetail);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));

    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    expect(await screen.findByRole("button", { name: "读取路径" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$.name" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: '"Bob"' },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));

    await waitFor(() => {
      expect(setJsonPathMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "profile:1",
        path: "$.name",
        value: "Bob",
      });
    });
    await waitFor(() => {
      expect(getKeyMock).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(screen.getByLabelText("JSON 文档")).toHaveValue(
        JSON.stringify({ name: "Bob", tags: ["redis"] }, null, 2),
      );
    });
    expect(screen.getByRole("textbox", { name: "JSON Path" })).toHaveValue("$");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue(
      JSON.stringify({ name: "Bob", tags: ["redis"] }, null, 2),
    );
  });

  it("根路径删除需要单独确认，取消时不调用 deleteJsonPath", async () => {
    vi.stubGlobal("confirm", vi.fn(() => false));
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValue(jsonDetail);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));
    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    await screen.findByRole("button", { name: "读取路径" });

    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$" },
    });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));

    expect(screen.getByRole("alertdialog")).toHaveTextContent("确定删除整个 JSON 键“profile:1”吗？");
    expect(deleteJsonPathMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(confirm).not.toHaveBeenCalled();
    expect(deleteJsonPathMock).not.toHaveBeenCalled();
  });

  it("根路径删除成功且键已不存在时清理详情而不是保留旧 key", async () => {
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValueOnce(jsonDetail);
    deleteJsonPathMock.mockResolvedValue({
      key: "profile:1",
      path: "$",
      affected: 0,
      new_length: null,
      ttl_ms: -2,
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));
    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    await screen.findByRole("button", { name: "读取路径" });
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$" },
    });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));

    expect(deleteJsonPathMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => {
      expect(deleteJsonPathMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "profile:1",
        path: "$",
      });
    });
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "profile:1" })).not.toBeInTheDocument(),
    );
    expect(screen.getByText("请选择一个键查看详情")).toBeInTheDocument();
  });

  it("根路径删除 affected 为 0 且 key 仍存在时刷新详情而不调用 onDeleted", async () => {
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    const refreshedDetail: KeyValue = {
      ...jsonDetail,
      ttl_ms: 5000,
      value: { Json: { value: { name: "Alice", tags: ["redis", "json"] } } },
    };
    const onDeleted = vi.fn();
    const onDetailChange = vi.fn();
    getKeyMock.mockResolvedValue(refreshedDetail);
    deleteJsonPathMock.mockResolvedValue({
      key: "profile:1",
      path: "$",
      affected: 0,
      new_length: null,
      ttl_ms: 5000,
    });

    render(
      <KeyDetails
        connectionId="local"
        detail={jsonDetail}
        loading={false}
        moduleProbe={{
          status: "ready",
          capabilities: {
            modules: [{ name: "RedisJSON", version: "2.0.0" }],
            json_supported: true,
            json_version: "2.0.0",
            search_supported: false,
            search_version: null,
            array_supported: false,
            vector_set_supported: false,
          },
        }}
        onDetailChange={onDetailChange}
        onDeleted={onDeleted}
      />,
    );

    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    await screen.findByRole("button", { name: "读取路径" });
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$" },
    });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));

    expect(deleteJsonPathMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => {
      expect(deleteJsonPathMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "profile:1",
        path: "$",
      });
    });
    await waitFor(() => {
      expect(getKeyMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "profile:1",
      });
    });
    expect(onDeleted).not.toHaveBeenCalled();
    expect(onDetailChange).toHaveBeenCalledWith(refreshedDetail);
  });

  it("JSON mutation 已提交后 detail refresh 失败不显示失败态", async () => {
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice", tags: ["redis"] } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock
      .mockResolvedValueOnce(jsonDetail)
      .mockRejectedValueOnce({ code: "COMMAND_FAILED", message: "refresh failed" });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));
    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    await screen.findByRole("button", { name: "读取路径" });
    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$.name" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: "\"Bob\"" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));

    await waitFor(() => {
      expect(setJsonPathMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "profile:1",
        path: "$.name",
        value: "Bob",
      });
    });
    await waitFor(() => {
      expect(getKeyMock).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存路径" })).toBeEnabled();
    expect(screen.getByRole("textbox", { name: "JSON Path" })).toHaveValue("$.name");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue('"Bob"');
  });

  it("JSON Path 读取失败时展示稳定映射文案", async () => {
    const jsonSummary = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      size: 26,
    };
    const jsonDetail: KeyValue = {
      key: "profile:1",
      key_type: "ReJSON-RL",
      ttl_ms: -1,
      value: { Json: { value: { name: "Alice" } } },
    };
    scanKeysMock.mockResolvedValue({
      cursor: 0,
      keys: [jsonSummary],
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValue(jsonDetail);
    getJsonPathMock.mockRejectedValue({
      code: "JSON_PATH_NOT_FOUND",
      message: "path not found",
    });

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "profile:1" }));
    fireEvent.click(await screen.findByRole("tab", { name: "JSON Path" }));
    await screen.findByRole("button", { name: "读取路径" });

    fireEvent.change(screen.getByRole("textbox", { name: "JSON Path" }), {
      target: { value: "$.missing" },
    });
    fireEvent.click(screen.getByRole("button", { name: "读取路径" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "未找到匹配的 JSON Path。",
    );
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

  it("Hash 详情显示所属 RedisSearch 索引", async () => {
    const hashDetail: KeyValue = {
      key: "user:1",
      key_type: "hash",
      ttl_ms: -1,
      value: { Hash: { fields: [{ field: "name", value: "Alice" }] } },
    };
    getKeySearchIndexesMock.mockResolvedValue([
      { name: "idx:users", key_type: "HASH", prefixes: ["user:"] },
    ]);

    render(
      <KeyDetails
        connectionId="local"
        detail={hashDetail}
        loading={false}
        moduleProbe={{
          status: "ready",
          capabilities: {
            modules: [{ name: "search", version: "2.8.10" }],
            json_supported: false,
            json_version: null,
            search_supported: true,
            search_version: "2.8.10",
            array_supported: false,
            vector_set_supported: false,
          },
        }}
        onDetailChange={vi.fn()}
        onDeleted={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("tab", { name: "索引" }));
    expect(await screen.findByRole("heading", { name: "所属 RedisSearch 索引" })).toBeInTheDocument();
    expect(getKeySearchIndexesMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "user:1",
    });
    expect(screen.getByText("idx:users")).toBeInTheDocument();
    expect(screen.getByText("HASH · user:")).toBeInTheDocument();
  });

  it("RedisSearch 版本不满足时不读取键索引关联", async () => {
    const hashDetail: KeyValue = {
      key: "user:1",
      key_type: "hash",
      ttl_ms: -1,
      value: { Hash: { fields: [{ field: "name", value: "Alice" }] } },
    };

    render(
      <KeyDetails
        connectionId="local"
        detail={hashDetail}
        loading={false}
        moduleProbe={{
          status: "ready",
          capabilities: {
            modules: [{ name: "search", version: "1.6.0" }],
            json_supported: false,
            json_version: null,
            search_supported: true,
            search_version: "1.6.0",
            array_supported: false,
            vector_set_supported: false,
          },
        }}
        onDetailChange={vi.fn()}
        onDeleted={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(getKeySearchIndexesMock).not.toHaveBeenCalled();
    });
    expect(screen.queryByRole("heading", { name: "所属 RedisSearch 索引" })).not.toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
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

    fireEvent.click(screen.getByRole("tab", { name: "元数据" }));
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
      node_failures: [], has_more: false,
    });
    getKeyMock.mockResolvedValue(stringDetail);
    renameKeyMock.mockResolvedValue(renamedDetail);

    render(<BrowserPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:1" }));
    await screen.findByDisplayValue("Alice");
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
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
