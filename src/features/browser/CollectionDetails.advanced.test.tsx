import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import CollectionDetails from "./CollectionDetails";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const hashPage = (ttl = -1, supported = true) => ({
  entries: [{ id: "field", value: "value", score: null, ttl_ms: supported ? ttl : null }],
  next_cursor: "0", has_more: false, total: "1", ttl_ms: -1, hash_field_ttl_supported: supported,
});
const listPage = { ...hashPage(), entries: [{ id: "0", value: "first", score: null, ttl_ms: null }], total: "250", hash_field_ttl_supported: null };
const accept = (name: string) => fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name }));

describe("集合高级操作", () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(cleanup);

  it("显示字段 TTL，确认设置后刷新，并可移除过期时间", async () => {
    invokeMock.mockResolvedValueOnce(hashPage()).mockResolvedValueOnce(undefined).mockResolvedValueOnce(hashPage(60000));
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    expect(await screen.findByText("不过期")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "设置字段 TTL field" }));
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "60000" } });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    accept("确认设置");
    expect(await screen.findByText("60000 ms")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: { connection_id: "local", key: "h", mutation: { operation: "hash_expire", field: "field", ttl_ms: "60000" } } });
    invokeMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(hashPage());
    fireEvent.click(screen.getByRole("button", { name: "设置字段 TTL field" }));
    fireEvent.click(screen.getByRole("button", { name: "移除 TTL" }));
    accept("确认移除");
    expect(await screen.findByText("不过期")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: { connection_id: "local", key: "h", mutation: { operation: "hash_persist", field: "field" } } });
  });

  it("字段 TTL 不可用时保持 Hash 浏览编辑可用", async () => {
    invokeMock.mockResolvedValue(hashPage(-1, false));
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    expect(await screen.findByText(/需要 Redis 7.4/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置字段 TTL field" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "编辑 field" })).toBeEnabled();
    expect(screen.getByText("value")).toBeInTheDocument();
  });

  it("TTL 非正整数不提交；修改 TTL 输入会取消待确认操作", async () => {
    invokeMock.mockResolvedValue(hashPage());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: "设置字段 TTL field" }));
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    expect(screen.getByText(/TTL 必须是/)).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "50" } });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "60" } });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("TTL 写入权限不足时解释命令要求，不自动重试或丢弃字段", async () => {
    invokeMock.mockResolvedValueOnce(hashPage()).mockRejectedValueOnce({ code: "UNSUPPORTED_FEATURE" });
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: "设置字段 TTL field" }));
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    accept("确认设置");
    expect(await screen.findByText(/请确认 Redis 版本至少为 7.4/)).toBeInTheDocument();
    expect(screen.getByText("value")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("TTL 限制在服务端可接受的百年范围内，超过上限时不提交", async () => {
    invokeMock.mockResolvedValue(hashPage());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: "设置字段 TTL field" }));
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "3153600000001" } });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    expect(screen.getByText(/TTL 必须是 1 到 3153600000000/)).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.change(screen.getByLabelText("字段 TTL（毫秒）"), { target: { value: "3153600000000" } });
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" }));
    accept("确认设置");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: { connection_id: "local", key: "h", mutation: { operation: "hash_expire", field: "field", ttl_ms: "3153600000000" } } }));
  });

  it("按负索引查询，使用返回的绝对索引编辑，并在越界时清除旧结果", async () => {
    invokeMock.mockResolvedValueOnce(listPage).mockResolvedValueOnce({ id: "249", value: "tail", score: null, ttl_ms: null });
    render(<CollectionDetails connectionId="local" keyName="l" kind="list" />);
    await screen.findByText("first");
    fireEvent.change(screen.getByLabelText("查询索引"), { target: { value: "-1" } });
    fireEvent.click(screen.getByRole("button", { name: "查询索引" }));
    expect(await screen.findByText("tail")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenLastCalledWith("get_list_entry", { input: { connection_id: "local", key: "l", index: "-1" } });
    fireEvent.click(screen.getByRole("button", { name: "编辑查询结果 249" }));
    expect(screen.getByLabelText("值")).toHaveValue("tail");
    fireEvent.change(screen.getByLabelText("值"), { target: { value: "updated" } });
    invokeMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(listPage);
    fireEvent.click(screen.getByRole("button", { name: "保存此项" }));
    accept("确认保存");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: { connection_id: "local", key: "l", mutation: { operation: "list_set", index: "249", value: "updated" } } }));
    await waitFor(() => expect(screen.getByRole("button", { name: "查询索引" })).toBeEnabled());
    invokeMock.mockResolvedValueOnce(null);
    fireEvent.change(screen.getByLabelText("查询索引"), { target: { value: "9223372036854775807" } });
    fireEvent.click(screen.getByRole("button", { name: "查询索引" }));
    expect(await screen.findByText(/该索引不存在/)).toBeInTheDocument();
    expect(screen.queryByText("tail")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenLastCalledWith("get_list_entry", { input: { connection_id: "local", key: "l", index: "9223372036854775807" } });
  });

  it("切换键后忽略旧索引查询结果", async () => {
    let resolve!: (value: unknown) => void;
    invokeMock.mockImplementation((command: string) => command === "get_list_entry" ? new Promise((done) => { resolve = done; }) : Promise.resolve(listPage));
    const view = render(<CollectionDetails connectionId="local" keyName="old" kind="list" />);
    await screen.findByText("first");
    fireEvent.click(screen.getByRole("button", { name: "查询索引" }));
    view.rerender(<CollectionDetails connectionId="local" keyName="new" kind="list" />);
    await screen.findByText("first");
    await act(async () => resolve({ id: "0", value: "stale", score: null, ttl_ms: null }));
    expect(screen.queryByText("stale")).not.toBeInTheDocument();
  });

  it.each([["从头部删除", true], ["从尾部删除", false]] as const)("%s 要求确认，完成后清空旧索引并回到首页", async (label, fromHead) => {
    invokeMock.mockResolvedValueOnce({ ...listPage, next_cursor: "100", has_more: true }).mockResolvedValueOnce({ ...listPage, entries: [{ id: "100", value: "middle", score: null }] });
    const changed = vi.fn();
    render(<CollectionDetails connectionId="local" keyName="l" kind="list" onChanged={changed} />);
    const nextPage = screen.getByRole("button", { name: "下一页" });
    await waitFor(() => expect(nextPage).toBeEnabled());
    fireEvent.click(nextPage);
    await screen.findByText("middle");
    fireEvent.change(screen.getByLabelText("删除数量"), { target: { value: "150" } });
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(within(screen.getByRole("alertdialog")).getByText(/150/)).toBeInTheDocument();
    accept("取消");
    expect(invokeMock).toHaveBeenCalledTimes(2);
    invokeMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(listPage);
    fireEvent.click(screen.getByRole("button", { name: label }));
    accept("确认删除");
    expect(await screen.findByText("first")).toBeInTheDocument();
    expect(screen.queryByText("middle")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled();
    expect(changed).toHaveBeenCalledOnce();
    expect(invokeMock).toHaveBeenCalledWith("mutate_collection", { input: { connection_id: "local", key: "l", mutation: { operation: "list_trim", count: "150", from_head: fromHead } } });
  });

  it("拒绝无效索引和批量删除数量，清空整个 List 后显示空状态", async () => {
    invokeMock.mockResolvedValueOnce(listPage);
    render(<CollectionDetails connectionId="local" keyName="l" kind="list" />);
    await screen.findByText("first");
    fireEvent.change(screen.getByLabelText("查询索引"), { target: { value: "9223372036854775808" } });
    fireEvent.click(screen.getByRole("button", { name: "查询索引" }));
    expect(screen.getByText(/索引必须是/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("删除数量"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "从头部删除" }));
    expect(screen.getByText(/删除数量必须是/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.change(screen.getByLabelText("删除数量"), { target: { value: "1000" } });
    invokeMock.mockResolvedValueOnce(undefined).mockRejectedValueOnce({ code: "KEY_NOT_FOUND" });
    fireEvent.click(screen.getByRole("button", { name: "从头部删除" }));
    accept("确认删除");
    expect(await screen.findByText(/键已不存在/)).toBeInTheDocument();
    expect(screen.queryByText("first")).not.toBeInTheDocument();
  });

  it("Sorted Set 默认分数升序，跨页沿用排序，切换降序重置分页并保留扫描搜索", async () => {
    const sorted = (member: string, score: number, next = "0") => ({ ...listPage, entries: [{ id: member, value: member, score, ttl_ms: null }], next_cursor: next, has_more: next !== "0" });
    invokeMock.mockResolvedValueOnce(sorted("lowest", -10, "100")).mockResolvedValueOnce(sorted("middle", 5)).mockResolvedValueOnce(sorted("highest", 100, "100")).mockResolvedValueOnce(sorted("scan-result", 7)).mockResolvedValueOnce(sorted("match-result", 3));
    render(<CollectionDetails connectionId="local" keyName="z" kind="zset" />);
    await screen.findByText("lowest");
    expect(screen.getByLabelText("浏览顺序")).toHaveValue("score_asc");
    expect(screen.queryByLabelText("成员匹配模式")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    await screen.findByText("middle");
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "z", kind: "zset", cursor: "100", count: 100, pattern: "*", order: "score_asc" } });
    fireEvent.change(screen.getByLabelText("浏览顺序"), { target: { value: "score_desc" } });
    await screen.findByText("highest");
    expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled();
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "z", kind: "zset", cursor: "0", count: 100, pattern: "*", order: "score_desc" } });
    fireEvent.change(screen.getByLabelText("浏览顺序"), { target: { value: "scan" } });
    await screen.findByText("scan-result");
    fireEvent.change(screen.getByLabelText("成员匹配模式"), { target: { value: "match*" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    await screen.findByText("match-result");
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "z", kind: "zset", cursor: "0", count: 100, pattern: "match*", order: "scan" } });
  });

  it("切换排序请求失败时保留原排序与页码，写入后沿用原排序刷新", async () => {
    const sorted = { ...listPage, entries: [{ id: "member", value: "member", score: 2, ttl_ms: null }] };
    invokeMock.mockResolvedValueOnce({ ...sorted, next_cursor: "100", has_more: true }).mockResolvedValueOnce(sorted).mockRejectedValueOnce({ code: "COMMAND_FAILED" });
    render(<CollectionDetails connectionId="local" keyName="z" kind="zset" />);
    fireEvent.click(await screen.findByRole("button", { name: "下一页" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "上一页" })).toBeEnabled());
    fireEvent.change(screen.getByLabelText("浏览顺序"), { target: { value: "score_desc" } });
    await screen.findByText(/集合读取失败/);
    expect(screen.getByLabelText("浏览顺序")).toHaveValue("score_asc");
    expect(screen.getByRole("button", { name: "上一页" })).toBeEnabled();
    invokeMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(sorted);
    fireEvent.click(screen.getByRole("button", { name: "编辑 member" }));
    fireEvent.change(screen.getByLabelText("分数"), { target: { value: "-5" } });
    fireEvent.click(screen.getByRole("button", { name: "保存此项" }));
    accept("确认保存");
    await waitFor(() => expect(screen.getByRole("button", { name: "上一页" })).toBeDisabled());
    expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "z", kind: "zset", cursor: "0", count: 100, pattern: "*", order: "score_asc" } });
  });
});
