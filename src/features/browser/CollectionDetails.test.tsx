import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import CollectionDetails from "./CollectionDetails";
import type { CollectionPage } from "./collectionApi";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const page = (value: string, next = "0"): CollectionPage => ({
  entries: [{ id: value, value, score: null }], next_cursor: next,
  has_more: next !== "0", total: "2500", ttl_ms: 59000,
});

describe("CollectionDetails", () => {
  beforeEach(() => { invokeMock.mockReset(); vi.spyOn(window, "confirm").mockReturnValue(false); });
  afterEach(() => { cleanup(); vi.restoreAllMocks(); });

  it("直接请求分页并保留超过JS安全整数的游标，下一页替换显示", async () => {
    invokeMock.mockResolvedValueOnce(page("first", "18446744073709551615")).mockResolvedValueOnce(page("second"));
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    expect((await screen.findAllByText("first")).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect((await screen.findAllByText("second")).length).toBeGreaterThan(0);
    expect(screen.queryByText("first")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: {
      connection_id: "local", key: "h", kind: "hash", cursor: "18446744073709551615", count: 100, pattern: "*",
    } });
  });

  it("切键后忽略旧分页请求，不覆盖新键内容", async () => {
    let finishOld!: (value: CollectionPage) => void;
    invokeMock.mockImplementation((_command: string, { input }: { input: { key: string } }) => input.key === "old" ? new Promise<CollectionPage>((resolve) => { finishOld = resolve; }) : Promise.resolve({ ...page("new value"), entries: [{ id: "0", value: "new value", score: null }] }));
    const view = render(<CollectionDetails connectionId="local" keyName="old" kind="list" />);
    view.rerender(<CollectionDetails connectionId="local" keyName="new" kind="list" />);
    expect(await screen.findByText("new value")).toBeInTheDocument();
    await act(async () => finishOld(page("stale value")));
    expect(screen.queryByText("stale value")).not.toBeInTheDocument();
  });

  it("取消删除不写入；确认后只删除所选字段并刷新", async () => {
    invokeMock.mockResolvedValue(page("field"));
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    const remove = await screen.findByRole("button", { name: "删除 field" });
    fireEvent.click(remove);
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.click(remove);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: {
      connection_id: "local", key: "h", mutation: { operation: "hash_delete", field: "field" },
    } }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(3));
  });

  it("空SCAN页仍可继续，过滤搜索从首游标重新开始", async () => {
    invokeMock.mockResolvedValueOnce({ ...page("unused", "123"), entries: [] }).mockResolvedValueOnce(page("match"));
    render(<CollectionDetails connectionId="local" keyName="h" kind="set" />);
    expect(await screen.findByRole("button", { name: "下一页" })).toBeEnabled();
    fireEvent.change(screen.getByLabelText("成员匹配模式"), { target: { value: "match*" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    expect(await screen.findByText("match")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: {
      connection_id: "local", key: "h", kind: "set", cursor: "0", count: 100, pattern: "match*",
    } });
  });

  it("旧写入完成后不刷新已切换的新键或触发变更回调", async () => {
    let finishWrite!: () => void;
    const changed = vi.fn();
    invokeMock.mockImplementation((command: string, { input }: { input: { key: string } }) => command === "mutate_collection" ? new Promise<void>((resolve) => { finishWrite = resolve; }) : Promise.resolve(page(input.key)));
    const view = render(<CollectionDetails connectionId="local" keyName="old" kind="hash" onChanged={changed} />);
    fireEvent.click(await screen.findByRole("button", { name: "删除 old" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(finishWrite).toBeDefined());
    view.rerender(<CollectionDetails connectionId="local" keyName="new" kind="hash" onChanged={changed} />);
    expect((await screen.findAllByText("new")).length).toBeGreaterThan(0);
    const count = invokeMock.mock.calls.length;
    await act(async () => finishWrite());
    expect(invokeMock).toHaveBeenCalledTimes(count);
    expect(changed).not.toHaveBeenCalled();
  });

  it.each([
    ["hash", "字段名", "name", { operation: "hash_set", field: "name", value: "value" }],
    ["set", "成员", "name", { operation: "set_add", member: "name" }],
    ["zset", "成员", "name", { operation: "zset_add", member: "name", score: 0 }],
    ["list", null, null, { operation: "list_append", value: "value", prepend: false }],
  ] as const)("%s 添加仅在确认后提交正确参数", async (kind, label, name, mutation) => {
    invokeMock.mockResolvedValue(page("existing"));
    render(<CollectionDetails connectionId="local" keyName="items" kind={kind} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "添加" })).toBeEnabled());
    if (label) fireEvent.change(screen.getByLabelText(label), { target: { value: name } });
    if (kind === "hash" || kind === "list") fireEvent.change(screen.getByLabelText("值"), { target: { value: "value" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认添加" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: {
      connection_id: "local", key: "items", mutation,
    } }));
  });

  it.each(["键", "连接", "禁用", "编辑值", "编辑目标"])("确认期间变更%s会取消旧操作", async (change) => {
    invokeMock.mockResolvedValue(page("field"));
    const view = render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 field" }));
    fireEvent.click(screen.getByRole("button", { name: "保存此项" }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    if (change === "键") view.rerender(<CollectionDetails connectionId="local" keyName="other" kind="hash" />);
    else if (change === "连接") view.rerender(<CollectionDetails connectionId="remote" keyName="h" kind="hash" />);
    else if (change === "禁用") view.rerender(<CollectionDetails connectionId="local" keyName="h" kind="hash" disabled />);
    else if (change === "编辑值") fireEvent.change(screen.getByLabelText("值"), { target: { value: "changed" } });
    else fireEvent.click(screen.getByRole("button", { name: "取消编辑" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.some(([command]) => command === "mutate_collection")).toBe(false);
  });

  it("重复删除和重复接受只提交一次", async () => {
    invokeMock.mockImplementation((command: string) => command === "mutate_collection" ? new Promise(() => {}) : Promise.resolve(page("field")));
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    const remove = await screen.findByRole("button", { name: "删除 field" });
    fireEvent.click(remove); fireEvent.click(remove);
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    fireEvent.click(accept); fireEvent.click(accept);
    await waitFor(() => expect(invokeMock.mock.calls.filter(([command]) => command === "mutate_collection")).toHaveLength(1));
  });
});
