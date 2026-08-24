import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import QueryLibraryPage from "./QueryLibraryPage";
import type { QueryLibraryItem } from "../../lib/types";

const {
  listQueryLibraryMock,
  saveQueryLibraryItemMock,
  deleteQueryLibraryItemMock,
} = vi.hoisted(() => ({
  listQueryLibraryMock: vi.fn(),
  saveQueryLibraryItemMock: vi.fn(),
  deleteQueryLibraryItemMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  listQueryLibrary: listQueryLibraryMock,
  saveQueryLibraryItem: saveQueryLibraryItemMock,
  deleteQueryLibraryItem: deleteQueryLibraryItemMock,
}));

const item: QueryLibraryItem = {
  id: "query-1",
  name: "读取用户",
  command: "GET user:1",
  tags: ["用户", "读取"],
  updated_at: 1,
};

describe("Query Library", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("confirm", vi.fn(() => true));
    listQueryLibraryMock.mockResolvedValue([item]);
    saveQueryLibraryItemMock.mockResolvedValue(item);
    deleteQueryLibraryItemMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("加载查询、按标签搜索并显示空结果状态", async () => {
    render(<QueryLibraryPage onFill={vi.fn()} />);

    expect(await screen.findByText("读取用户")).toBeInTheDocument();
    expect(screen.getByText("用户")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("搜索已保存查询"), {
      target: { value: "不存在" },
    });
    expect(screen.getByText("没有匹配的已保存查询")).toBeInTheDocument();
    expect(screen.queryByText("GET user:1")).not.toBeInTheDocument();
  });

  it("保存新查询、回填 Workbench 且删除前需要确认", async () => {
    const onFill = vi.fn();
    const saved = { ...item, id: "query-2", name: "读取订单", command: "GET order:1" };
    saveQueryLibraryItemMock.mockResolvedValue(saved);
    render(<QueryLibraryPage onFill={onFill} />);

    await screen.findByText("读取用户");
    fireEvent.click(screen.getByRole("button", { name: "新建查询" }));
    fireEvent.change(screen.getByLabelText("查询名称"), {
      target: { value: "读取订单" },
    });
    fireEvent.change(screen.getByLabelText("Redis 命令"), {
      target: { value: "GET order:1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存查询" }));

    await waitFor(() => {
      expect(saveQueryLibraryItemMock).toHaveBeenCalledWith({
        id: null,
        name: "读取订单",
        command: "GET order:1",
        tags: [],
      });
    });
    expect(await screen.findByText("读取订单")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "回填 Workbench 读取订单" }));
    expect(onFill).toHaveBeenCalledWith("GET order:1");

    fireEvent.click(screen.getByRole("button", { name: "删除查询 读取订单" }));
    await waitFor(() => expect(deleteQueryLibraryItemMock).toHaveBeenCalledWith("query-2"));
    expect(confirm).toHaveBeenCalledTimes(1);
  });

  it("后端拒绝敏感命令时只显示固定错误", async () => {
    saveQueryLibraryItemMock.mockRejectedValue({
      code: "INVALID_CONNECTION",
      message: "CONFIG SET requirepass secret",
    });
    render(<QueryLibraryPage onFill={vi.fn()} />);

    await screen.findByText("读取用户");
    fireEvent.click(screen.getByRole("button", { name: "新建查询" }));
    fireEvent.change(screen.getByLabelText("查询名称"), {
      target: { value: "敏感命令" },
    });
    fireEvent.change(screen.getByLabelText("Redis 命令"), {
      target: { value: "CONFIG SET requirepass secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存查询" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "查询无法保存，请检查名称、命令和标签。",
    );
    expect(screen.getByRole("alert")).not.toHaveTextContent("requirepass");
  });
});
