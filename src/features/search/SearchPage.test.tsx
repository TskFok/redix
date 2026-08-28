import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  vi.clearAllMocks();
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
});

describe("RedisSearch / Query 页面", () => {
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
