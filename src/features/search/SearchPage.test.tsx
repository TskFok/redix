import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ModuleCapabilities } from "../../lib/types";
import SearchPage from "./SearchPage";

const {
  getKeySearchIndexesMock,
  getModuleCapabilitiesMock,
  getSearchIndexMock,
  listSearchIndexesMock,
  createSearchIndexMock,
  deleteSearchIndexMock,
  searchKeysMock,
} = vi.hoisted(() => ({
  getKeySearchIndexesMock: vi.fn(),
  getModuleCapabilitiesMock: vi.fn(),
  getSearchIndexMock: vi.fn(),
  listSearchIndexesMock: vi.fn(),
  createSearchIndexMock: vi.fn(),
  deleteSearchIndexMock: vi.fn(),
  searchKeysMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  getKeySearchIndexes: getKeySearchIndexesMock,
  getModuleCapabilities: getModuleCapabilitiesMock,
  getSearchIndex: getSearchIndexMock,
  listSearchIndexes: listSearchIndexesMock,
  createSearchIndex: createSearchIndexMock,
  deleteSearchIndex: deleteSearchIndexMock,
  searchKeys: searchKeysMock,
}));

const readyCapabilities: ModuleCapabilities = {
  modules: [{ name: "search", version: "2.8.10" }],
  json_supported: false,
  json_version: null,
  search_supported: true,
  search_version: "2.8.10",
  array_supported: false,
  vector_set_supported: false,
};

const indexInfo = {
  index_name: "idx:users",
  key_type: "HASH",
  prefixes: ["user:"],
  attributes: [
    { identifier: "name", field_type: "TEXT", sortable: false, no_index: false },
  ],
  num_docs: 2,
  num_terms: 1,
  num_records: 2,
  total_index_memory_bytes: 512,
};

beforeEach(() => {
  vi.resetAllMocks();
  getModuleCapabilitiesMock.mockResolvedValue(readyCapabilities);
  listSearchIndexesMock.mockResolvedValue({ indexes: [{ name: "idx:users" }] });
  getSearchIndexMock.mockResolvedValue(indexInfo);
  searchKeysMock.mockResolvedValue({
    total: 1,
    offset: 0,
    next_offset: null,
    max_results: 100,
    keys: [{ key: "user:1", key_type: "hash" }],
  });
  createSearchIndexMock.mockResolvedValue(undefined);
  deleteSearchIndexMock.mockResolvedValue(undefined);
  getKeySearchIndexesMock.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("RedisSearch / Query 页面", () => {
  it("文档模式显示异构字段、JSON 与空值，并按服务端游标分页", async () => {
    searchKeysMock.mockResolvedValueOnce({
      total: 20, offset: 0, next_offset: 2, max_results: 100,
      keys: [
        { key: "doc:1", key_type: "hash", fields: [{ name: "name", value: "Alice" }, { name: "optional", value: null }] },
        { key: "doc:2", key_type: "json", fields: [{ name: "$", value: '{"enabled":true}' }] },
      ],
    }).mockResolvedValueOnce({ total: 20, offset: 2, next_offset: null, max_results: 100, keys: [{ key: "doc:3", key_type: "none", fields: null }] });
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    expect(await screen.findByText("Alice")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "name" })).toBeInTheDocument();
    expect(screen.getByText("null")).toBeInTheDocument();
    expect(screen.getByText('{"enabled":true}')).toBeInTheDocument();
    expect(searchKeysMock).toHaveBeenLastCalledWith({ connection_id: "local", index: "idx:users", query: "*", offset: 0, limit: 100, include_content: true });
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect(await screen.findByText("doc:3")).toBeInTheDocument();
    expect(screen.getByText("文档已过期或内容不可用")).toBeInTheDocument();
    expect(searchKeysMock).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 2, include_content: true }));
    expect(screen.getByRole("button", { name: "下一页" })).toBeDisabled();
  });

  it("切换内容模式取消旧查询，旧响应不能污染新模式结果", async () => {
    let resolve!: (value: unknown) => void;
    searchKeysMock.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    expect(await screen.findByText("user:1")).toBeInTheDocument();
    await act(async () => resolve({ total: 1, offset: 0, next_offset: null, max_results: null, keys: [{ key: "stale:doc", key_type: "hash", fields: [{ name: "secret", value: "stale" }] }] }));
    expect(screen.queryByText("stale:doc")).not.toBeInTheDocument();
    expect(screen.getByText("user:1")).toBeInTheDocument();
    expect(searchKeysMock).toHaveBeenLastCalledWith({ connection_id: "local", index: "idx:users", query: "*", offset: 0, limit: 100 });
  });

  it("更改查询清除旧分页结果", async () => {
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    await waitFor(() => expect(screen.getByRole("button", { name: "查询" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    await screen.findByText("user:1");
    fireEvent.change(screen.getByRole("textbox", { name: "查询语句" }), { target: { value: "@name:Bob" } });
    expect(screen.queryByText("user:1")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "下一页" })).not.toBeInTheDocument();
  });

  it("索引详情尚未返回时可以查询，详情完成不会丢弃查询结果", async () => {
    let resolveInfo!: (value: unknown) => void;
    getSearchIndexMock.mockImplementationOnce(() => new Promise((resolve) => { resolveInfo = resolve; }));
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    await waitFor(() => expect(screen.getByRole("button", { name: "查询" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    expect(await screen.findByText("user:1")).toBeInTheDocument();
    await act(async () => resolveInfo(indexInfo));
    expect(screen.getByText("user:1")).toBeInTheDocument();
  });

  it("切换连接清除内容模式和旧请求结果", async () => {
    let resolve!: (value: unknown) => void;
    searchKeysMock.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const { rerender } = render(<SearchPage connectionId="old" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    rerender(<SearchPage connectionId="new" />);
    await screen.findByRole("option", { name: "idx:users" });
    expect(screen.getByRole("checkbox", { name: "返回文档内容" })).not.toBeChecked();
    await act(async () => resolve({ total: 1, offset: 0, next_offset: null, max_results: null, keys: [{ key: "old:doc", key_type: "hash", fields: [] }] }));
    expect(screen.queryByText("old:doc")).not.toBeInTheDocument();
  });

  it("切换索引后旧内容响应不可覆盖新查询", async () => {
    listSearchIndexesMock.mockResolvedValue({ indexes: [{ name: "idx:users" }, { name: "idx:orders" }] });
    let reject!: (reason: unknown) => void;
    searchKeysMock.mockImplementationOnce(() => new Promise((_done, fail) => { reject = fail; }));
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    fireEvent.change(screen.getByRole("combobox", { name: "当前索引" }), { target: { value: "idx:orders" } });
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    await screen.findByText("user:1");
    await act(async () => reject({ code: "COMMAND_FAILED" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(searchKeysMock).toHaveBeenLastCalledWith(expect.objectContaining({ index: "idx:orders", offset: 0 }));
  });

  it("刷新索引列表自动切换索引时丢弃旧内容响应", async () => {
    listSearchIndexesMock.mockResolvedValueOnce({ indexes: [{ name: "idx:users" }] }).mockResolvedValueOnce({ indexes: [{ name: "idx:orders" }] });
    let resolve!: (value: unknown) => void;
    searchKeysMock.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("checkbox", { name: "返回文档内容" }));
    fireEvent.click(screen.getByRole("button", { name: "查询" }));
    fireEvent.click(screen.getByRole("button", { name: "刷新索引" }));
    await screen.findByRole("option", { name: "idx:orders" });
    await act(async () => resolve({ total: 1, offset: 0, next_offset: null, max_results: null, keys: [{ key: "stale:users", key_type: "hash", fields: [] }] }));
    expect(screen.queryByText("stale:users")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查询" })).toBeEnabled();
  });

  it("删除索引期间阻止切换索引、内容模式和再次查询或删除", async () => {
    listSearchIndexesMock.mockResolvedValue({ indexes: [{ name: "idx:users" }, { name: "idx:orders" }] });
    let resolve!: () => void;
    deleteSearchIndexMock.mockImplementationOnce(() => new Promise<void>((done) => { resolve = done; }));
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    const queryButton = screen.getByRole("button", { name: "查询" });
    fireEvent.click(screen.getByRole("button", { name: "删除当前索引" }));
    expect(deleteSearchIndexMock).not.toHaveBeenCalled();
    await act(async () => fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" })));
    expect(deleteSearchIndexMock).toHaveBeenCalledWith({ connection_id: "local", index: "idx:users" });
    expect(screen.getByRole("combobox", { name: "当前索引" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "返回文档内容" })).toBeDisabled();
    expect(queryButton).toBeDisabled();
    expect(screen.getByRole("button", { name: "删除当前索引" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "删除当前索引" }));
    expect(deleteSearchIndexMock).toHaveBeenCalledTimes(1);
    await act(async () => resolve());
  });

  it("取消删除索引保留索引且不发请求", async () => {
    render(<SearchPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "删除当前索引" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("idx:users");
    await act(async () => fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" })));
    expect(deleteSearchIndexMock).not.toHaveBeenCalled();
    expect(screen.getByRole("option", { name: "idx:users" })).toBeInTheDocument();
  });

  it.each(["连接", "索引", "查询", "刷新", "新建索引"])("更改%s取消索引删除确认", async (context) => {
    listSearchIndexesMock.mockResolvedValue({ indexes: [{ name: "idx:users" }, { name: "idx:orders" }] });
    const view = render(<SearchPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "删除当前索引" }));
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    if (context === "连接") view.rerender(<SearchPage connectionId="other" />);
    else if (context === "索引") fireEvent.change(screen.getByRole("combobox", { name: "当前索引" }), { target: { value: "idx:orders" } });
    else if (context === "查询") fireEvent.change(screen.getByLabelText("查询语句"), { target: { value: "@name:Bob" } });
    else fireEvent.click(screen.getByRole("button", { name: context === "刷新" ? "刷新索引" : "新建索引" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await act(async () => fireEvent.click(accept));
    expect(deleteSearchIndexMock).not.toHaveBeenCalled();
  });

  it("确认接受后同批切换索引仍阻止旧删除请求", async () => {
    listSearchIndexesMock.mockResolvedValue({ indexes: [{ name: "idx:users" }, { name: "idx:orders" }] });
    render(<SearchPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "删除当前索引" }));
    act(() => {
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
      fireEvent.change(screen.getByRole("combobox", { name: "当前索引" }), { target: { value: "idx:orders" } });
    });
    await act(async () => {});
    expect(deleteSearchIndexMock).not.toHaveBeenCalled();
  });

  it.each(["成功", "失败"])("创建后忽略较早索引列表请求的%s响应", async (outcome) => {
    let resolve!: (value: unknown) => void;
    let reject!: (reason: unknown) => void;
    listSearchIndexesMock.mockImplementationOnce(() => new Promise((done, fail) => {
      resolve = done;
      reject = fail;
    })).mockResolvedValueOnce({ indexes: [{ name: "idx:orders" }] });
    render(<SearchPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "新建索引" }));
    fireEvent.change(screen.getByRole("textbox", { name: "索引名称" }), { target: { value: "idx:orders" } });
    fireEvent.change(screen.getByRole("textbox", { name: "字段 1" }), { target: { value: "customer" } });
    fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
    await screen.findByRole("option", { name: "idx:orders" });
    await act(async () => {
      if (outcome === "成功") resolve({ indexes: [{ name: "idx:users" }] });
      else reject({ code: "COMMAND_FAILED" });
    });
    expect(screen.getByRole("option", { name: "idx:orders" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "idx:users" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("加载索引、查询结果并显示索引详情", async () => {
    render(<SearchPage connectionId="local" />);

    expect(await screen.findByRole("heading", { name: "RedisSearch / Query" })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: "idx:users" })).toBeInTheDocument();
    expect(getSearchIndexMock).toHaveBeenCalledWith({
      connection_id: "local",
      index: "idx:users",
    });

    fireEvent.change(screen.getByRole("textbox", { name: "查询语句" }), {
      target: { value: "@name:Alice" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查询" }));

    expect(await screen.findByText("user:1")).toBeInTheDocument();
    expect(searchKeysMock).toHaveBeenCalledWith({
      connection_id: "local",
      index: "idx:users",
      query: "@name:Alice",
      offset: 0,
      limit: 100,
    });
    expect(screen.getByText("字段 name")).toBeInTheDocument();
  });

  it("不支持 Search 时显示原因并阻止索引操作", async () => {
    getModuleCapabilitiesMock.mockResolvedValue({
      ...readyCapabilities,
      search_supported: false,
      search_version: null,
    });

    render(<SearchPage connectionId="local" />);

    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("当前连接不支持 RedisSearch。");
    });
    expect(listSearchIndexesMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("checkbox", { name: "返回文档内容" })).not.toBeInTheDocument();
  });

  it("通过表单创建索引并刷新列表", async () => {
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });

    fireEvent.click(screen.getByRole("button", { name: "新建索引" }));
    fireEvent.change(screen.getByRole("textbox", { name: "索引名称" }), {
      target: { value: "idx:orders" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "字段 1" }), {
      target: { value: "customer" },
    });
    fireEvent.click(screen.getByRole("button", { name: "创建索引" }));

    await waitFor(() => {
      expect(createSearchIndexMock).toHaveBeenCalledWith({
        connection_id: "local",
        index: "idx:orders",
        key_type: "hash",
        prefixes: [],
        fields: [{ name: "customer", field_type: "text" }],
      });
    });
  });
});


it("VECTOR 表单提交必需配置，非法维度阻止创建", async () => {
  render(<SearchPage connectionId="local" />);
  fireEvent.click(await screen.findByRole("button", { name: "新建索引" }));
  fireEvent.change(screen.getByLabelText("索引名称"), { target: { value: "idx:vectors" } });
  fireEvent.change(screen.getByLabelText("字段 1"), { target: { value: "embedding" } });
  fireEvent.change(screen.getByLabelText("字段 1 类型"), { target: { value: "vector" } });
  fireEvent.change(screen.getByLabelText("字段 1 向量维度"), { target: { value: "0" } });
  fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
  expect(createSearchIndexMock).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("字段 1 向量维度"), { target: { value: "128" } });
  fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
  await waitFor(() => expect(createSearchIndexMock).toHaveBeenCalledWith(expect.objectContaining({
    fields: [{ name: "embedding", field_type: "vector", vector: {
      algorithm: "FLAT", data_type: "FLOAT32", dimension: 128, distance_metric: "COSINE",
    } }],
  })));
});

it("Search 2.2 降级禁止选择 VECTOR，普通索引仍可创建", async () => {
  getModuleCapabilitiesMock.mockResolvedValue({ ...readyCapabilities, search_version: "2.2.0" });
  render(<SearchPage connectionId="local" />);
  fireEvent.click(await screen.findByRole("button", { name: "新建索引" }));
  expect(screen.getByRole("option", { name: "VECTOR" })).toBeDisabled();
  expect(screen.getByRole("option", { name: "TEXT" })).toBeEnabled();
});

it("构建器显式回填查询后才由查询按钮执行", async () => {
  render(<SearchPage connectionId="local" />);
  await screen.findByText("字段 name");
  fireEvent.click(screen.getByText("可视查询构建器"));
  fireEvent.change(await screen.findByLabelText("条件 1 值"), { target: { value: "Alice" } });
  expect(screen.getByLabelText("查询语句")).toHaveValue("*");
  fireEvent.click(screen.getByRole("button", { name: "回填查询语句" }));
  expect(screen.getByLabelText("查询语句")).toHaveValue('@name:("Alice")');
  expect(searchKeysMock).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "查询" }));
  await screen.findByText("user:1");
  expect(searchKeysMock).toHaveBeenLastCalledWith(expect.objectContaining({ query: '@name:("Alice")' }));
});

it("JSON VECTOR 创建显式填写查询别名并保留源路径", async () => {
    render(<SearchPage connectionId="local" />);
    await screen.findByRole("option", { name: "idx:users" });
    fireEvent.click(screen.getByRole("button", { name: "新建索引" }));
    fireEvent.change(screen.getByLabelText("索引名称"), { target: { value: "idx:json-vector" } });
    fireEvent.change(screen.getByLabelText("键类型"), { target: { value: "json" } });
    fireEvent.change(screen.getByLabelText("字段 1"), { target: { value: "$.embedding" } });
    fireEvent.change(screen.getByLabelText("字段 1 类型"), { target: { value: "vector" } });
    expect(screen.getByLabelText("字段 1 查询别名")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
    expect((await screen.findAllByText(/VECTOR 字段需要安全查询别名/)).length).toBeGreaterThan(0);
    expect(createSearchIndexMock).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("字段 1 查询别名"), { target: { value: "embedding" } });
    fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
    await waitFor(() => expect(createSearchIndexMock).toHaveBeenCalledWith(expect.objectContaining({ fields: [expect.objectContaining({ name: "$.embedding", alias: "embedding", field_type: "vector" })] })));
  });


it("创建索引拒绝别名重复和与其他源字段冲突", async () => {
  render(<SearchPage connectionId="local" />);
  fireEvent.click(await screen.findByRole("button", { name: "新建索引" }));
  fireEvent.change(screen.getByLabelText("索引名称"), { target: { value: "idx:aliases" } });
  fireEvent.change(screen.getByLabelText("字段 1"), { target: { value: "$.a" } });
  fireEvent.change(screen.getByLabelText("字段 1 查询别名"), { target: { value: "b" } });
  fireEvent.click(screen.getByRole("button", { name: "添加字段" }));
  fireEvent.change(screen.getByLabelText("字段 2"), { target: { value: "b" } });
  fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
  expect((await screen.findAllByText(/查询别名只能使用/)).length).toBeGreaterThan(0);
  expect(createSearchIndexMock).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("字段 2"), { target: { value: "$.b" } });
  fireEvent.change(screen.getByLabelText("字段 2 查询别名"), { target: { value: "b" } });
  fireEvent.click(screen.getByRole("button", { name: "创建索引" }));
  expect(createSearchIndexMock).not.toHaveBeenCalled();
});
