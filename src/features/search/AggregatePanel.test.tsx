import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AggregatePanel } from "./AggregatePanel";
import type { SearchAggregateResult } from "./aggregateApi";

const { aggregateSearch } = vi.hoisted(() => ({ aggregateSearch: vi.fn() }));
vi.mock("./aggregateApi", () => ({ aggregateSearch }));
const page = (name: string, offset = 0, next_offset: number | null = null): SearchAggregateResult => ({ offset, next_offset, columns: ["category", "amount"], rows: [{ fields: [{ name: "category", value: name }, { name: "amount", value: 12 }] }] });
beforeEach(() => { vi.resetAllMocks(); aggregateSearch.mockResolvedValue(page("新结果")); });
afterEach(cleanup);
const run = () => fireEvent.click(screen.getByRole("button", { name: "运行聚合" }));

describe("受限聚合面板", () => {
  it("构建 typed 分组和数值聚合并显示结果，分页替换旧页", async () => {
    aggregateSearch.mockResolvedValueOnce(page("第一页", 0, 50)).mockResolvedValueOnce(page("第二页", 50));
    render(<AggregatePanel connectionId="local" index="idx" />);
    fireEvent.change(screen.getByLabelText("加载字段"), { target: { value: "price, category" } });
    fireEvent.change(screen.getByLabelText("分组字段"), { target: { value: "category" } });
    fireEvent.change(screen.getByLabelText("聚合函数 1"), { target: { value: "sum" } });
    fireEvent.change(screen.getByLabelText("聚合字段 1"), { target: { value: "price" } });
    fireEvent.change(screen.getByLabelText("聚合别名 1"), { target: { value: "amount" } });
    fireEvent.change(screen.getByLabelText("排序字段"), { target: { value: "amount" } });
    run();
    expect(await screen.findByText("第一页")).toBeInTheDocument();
    expect(aggregateSearch).toHaveBeenLastCalledWith({ connection_id: "local", index: "idx", query: "*", load_fields: ["price", "category"], group_by: ["category"], reducers: [{ function: "sum", field: "price", alias: "amount" }], sort_by: [{ field: "amount", direction: "asc" }], offset: 0, limit: 50 });
    fireEvent.click(screen.getByRole("button", { name: "聚合下一页" }));
    expect(await screen.findByText("第二页")).toBeInTheDocument();
    expect(screen.queryByText("第一页")).not.toBeInTheDocument();
    expect(aggregateSearch).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 50 }));
    expect(screen.getByRole("button", { name: "聚合下一页" })).toBeDisabled();
  });
  it("查询编辑会取消旧响应与旧分页", async () => {
    let resolve!: (value: SearchAggregateResult) => void;
    aggregateSearch.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    render(<AggregatePanel connectionId="local" index="idx" />);
    run();
    fireEvent.change(screen.getByLabelText("聚合查询语句"), { target: { value: "@name:new" } });
    run();
    await screen.findByText("新结果");
    await act(async () => resolve(page("旧结果", 0, 50)));
    expect(screen.queryByText("旧结果")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "聚合下一页" })).toBeDisabled();
  });
  it.each(["connection", "index"])("切换%s清除状态并忽略旧失败", async (kind) => {
    let reject!: (value: unknown) => void;
    aggregateSearch.mockImplementationOnce(() => new Promise((_done, fail) => { reject = fail; }));
    const { rerender } = render(<AggregatePanel connectionId="old" index="idx" />);
    fireEvent.change(screen.getByLabelText("聚合查询语句"), { target: { value: "old" } });
    run();
    rerender(<AggregatePanel connectionId={kind === "connection" ? "new" : "old"} index={kind === "index" ? "other" : "idx"} />);
    expect(screen.getByLabelText("聚合查询语句")).toHaveValue("*");
    run();
    await screen.findByText("新结果");
    await act(async () => reject({ message: "secret password" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("失败使用固定错误且可重试当前分页", async () => {
    aggregateSearch.mockResolvedValueOnce(page("第一页", 0, 50)).mockRejectedValueOnce({ message: "secret password" }).mockResolvedValueOnce(page("第二页", 50));
    render(<AggregatePanel connectionId="local" index="idx" />);
    run();
    await screen.findByText("第一页");
    fireEvent.click(screen.getByRole("button", { name: "聚合下一页" }));
    expect(await screen.findByRole("alert")).not.toHaveTextContent("secret password");
    expect(screen.getByText("第一页")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试聚合" }));
    await screen.findByText("第二页");
    expect(aggregateSearch).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 50 }));
  });
  it("拒绝通配符LOAD和超限字段，禁用状态不调用后端", () => {
    const { rerender } = render(<AggregatePanel connectionId="local" index="idx" />);
    fireEvent.change(screen.getByLabelText("加载字段"), { target: { value: "*" } });
    run();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(aggregateSearch).not.toHaveBeenCalled();
    rerender(<AggregatePanel connectionId="local" index="idx" enabled={false} />);
    expect(screen.getByRole("button", { name: "运行聚合" })).toBeDisabled();
  });
});
