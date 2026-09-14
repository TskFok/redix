import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import CollectionDetails from "./CollectionDetails";
import { displayBytes, keyFromBytes, type RedisBytes } from "../../lib/redisBytes";
import type { CollectionKind, CollectionPage } from "./collectionApi";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
const binary = { base64: "/wCA" };
const page = (id: RedisBytes = binary, value: RedisBytes = binary): CollectionPage => ({ entries: [{ id, value, score: 2, ttl_ms: 1000 }], next_cursor: "0", has_more: false, total: "1", ttl_ms: -1, hash_field_ttl_supported: true });
const accept = (name: string) => fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name }));
const writes = () => invokeMock.mock.calls.filter(([command]) => command === "mutate_collection");

describe("集合二进制编辑", () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(cleanup);

  it("显示二进制字段和值，Hex 转 Base64 后编辑仍保留精确字段和键", async () => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName={keyFromBytes(binary)} kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: `编辑 ${displayBytes(binary)}` }));
    expect(screen.getByLabelText("字段名")).toHaveValue("ff 00 80");
    expect(screen.getByLabelText("值")).toHaveValue("ff 00 80");
    fireEvent.change(screen.getByLabelText("值编码"), { target: { value: "base64" } });
    expect(screen.getByLabelText("值")).toHaveValue("/wCA");
    fireEvent.change(screen.getByLabelText("值"), { target: { value: "/g==" } });
    fireEvent.click(screen.getByRole("button", { name: "保存此项" })); accept("确认保存");
    await waitFor(() => expect(writes()).toEqual([["mutate_collection", { input: { connection_id: "local", key: binary, mutation: { operation: "hash_set", field: binary, value: { base64: "/g==" } } } }]]));
  });

  it.each(["hash", "set", "zset"] as CollectionKind[])("%s 删除使用原始二进制身份", async (kind) => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="items" kind={kind} />);
    fireEvent.click(await screen.findByRole("button", { name: `删除 ${displayBytes(binary)}` })); accept("确认删除");
    const mutation = kind === "hash" ? { operation: "hash_delete", field: binary } : { operation: kind === "set" ? "set_remove" : "zset_remove", member: binary };
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "items", mutation } }));
  });

  it("字段 TTL 保留原始二进制字段名", async () => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    fireEvent.click(await screen.findByRole("button", { name: `设置字段 TTL ${displayBytes(binary)}` }));
    fireEvent.click(screen.getByRole("button", { name: "设置 TTL" })); accept("确认设置");
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "h", mutation: { operation: "hash_expire", field: binary, ttl_ms: "1000" } } }));
  });

  it("List 二进制值可以保存为空，索引仍是字符串", async () => {
    invokeMock.mockResolvedValue(page("0"));
    render(<CollectionDetails connectionId="local" keyName="l" kind="list" />);
    fireEvent.click(await screen.findByRole("button", { name: "编辑 0" }));
    fireEvent.change(screen.getByLabelText("值"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "保存此项" })); accept("确认保存");
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "l", mutation: { operation: "list_set", index: "0", value: "" } } }));
  });

  it("无效编码阻止确认、写入与格式切换，修复为空字段和值后可提交", async () => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    await screen.findByRole("button", { name: `编辑 ${displayBytes(binary)}` });
    fireEvent.change(screen.getByLabelText("字段名编码"), { target: { value: "hex" } });
    fireEvent.change(screen.getByLabelText("字段名"), { target: { value: "F" } });
    fireEvent.change(screen.getByLabelText("字段名编码"), { target: { value: "base64" } });
    expect(screen.getByLabelText("字段名编码")).toHaveValue("hex");
    expect(screen.getByLabelText("字段名")).toHaveValue("F");
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Hex/);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(); expect(writes()).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("字段名"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" })); accept("确认添加");
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "h", mutation: { operation: "hash_set", field: "", value: "" } } }));
  });

  it("二进制匹配模式透传字节，非法 Base64 不搜索", async () => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    await screen.findByRole("button", { name: `编辑 ${displayBytes(binary)}` });
    fireEvent.change(screen.getByLabelText("匹配模式编码"), { target: { value: "base64" } });
    expect(screen.getByLabelText("字段匹配模式")).toHaveValue("Kg==");
    fireEvent.change(screen.getByLabelText("字段匹配模式"), { target: { value: "/yo=" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    await waitFor(() => expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "h", kind: "hash", cursor: "0", count: 100, pattern: { base64: "/yo=" } } }));
    await waitFor(() => expect(screen.getByRole("button", { name: "搜索" })).toBeEnabled());
    fireEvent.change(screen.getByLabelText("字段匹配模式"), { target: { value: "%%%" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Base64/); expect(invokeMock).toHaveBeenCalledTimes(2);
  });
  it.each(["hash", "set", "zset"] as CollectionKind[])("%s 单行字段或成员拒绝UTF8换行转换，编辑后发送原始换行", async (kind) => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="items" kind={kind} />);
    await screen.findByRole("button", { name: `删除 ${displayBytes(binary)}` });
    const label = kind === "hash" ? "字段名" : "成员";
    const input = screen.getByLabelText(label);
    expect(input).toBeInstanceOf(HTMLInputElement);
    fireEvent.change(screen.getByLabelText(`${label}编码`), { target: { value: "hex" } });
    fireEvent.change(input, { target: { value: "61 0a 62" } });
    fireEvent.change(screen.getByLabelText(`${label}编码`), { target: { value: "utf8" } });
    expect(screen.getByLabelText(`${label}编码`)).toHaveValue("hex");
    expect(input).toHaveValue("61 0a 62");
    expect(screen.getByRole("alert")).toHaveTextContent(/单行输入/);
    expect(writes()).toHaveLength(0);
    fireEvent.change(input, { target: { value: "61 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" })); accept("确认添加");
    const mutation = kind === "hash" ? { operation: "hash_set", field: "a\nc", value: "" } : kind === "set" ? { operation: "set_add", member: "a\nc" } : { operation: "zset_add", member: "a\nc", score: 0 };
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "items", mutation } }));
  });

  it("匹配模式单行输入拒绝CRLF转UTF8，编辑后仍搜索完整字节", async () => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="h" kind="hash" />);
    await screen.findByRole("button", { name: `删除 ${displayBytes(binary)}` });
    const input = screen.getByLabelText("字段匹配模式");
    expect(input).toBeInstanceOf(HTMLInputElement);
    fireEvent.change(screen.getByLabelText("匹配模式编码"), { target: { value: "hex" } });
    fireEvent.change(input, { target: { value: "61 0d 0a 62" } });
    fireEvent.change(screen.getByLabelText("匹配模式编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("匹配模式编码")).toHaveValue("hex");
    expect(input).toHaveValue("61 0d 0a 62");
    expect(screen.getByRole("alert")).toHaveTextContent(/单行输入/);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    fireEvent.change(input, { target: { value: "61 0d 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    await waitFor(() => expect(invokeMock).toHaveBeenLastCalledWith("get_collection_page", { input: { connection_id: "local", key: "h", kind: "hash", cursor: "0", count: 100, pattern: "a\r\nc" } }));
  });

  it.each(["hash", "list"] as CollectionKind[])("%s 多行值拒绝CRLF转UTF8，后续编辑保留CR字节", async (kind) => {
    invokeMock.mockResolvedValue(page());
    render(<CollectionDetails connectionId="local" keyName="items" kind={kind} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "添加" })).toBeEnabled());
    const input = screen.getByLabelText("值");
    expect(input).toBeInstanceOf(HTMLTextAreaElement);
    fireEvent.change(screen.getByLabelText("值编码"), { target: { value: "hex" } });
    fireEvent.change(input, { target: { value: "61 0d 0a 62" } });
    fireEvent.change(screen.getByLabelText("值编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("值编码")).toHaveValue("hex");
    expect(input).toHaveValue("61 0d 0a 62");
    expect(writes()).toHaveLength(0);
    fireEvent.change(input, { target: { value: "61 0d 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" })); accept("确认添加");
    const mutation = kind === "hash" ? { operation: "hash_set", field: "", value: "a\r\nc" } : { operation: "list_append", value: "a\r\nc", prepend: false };
    await waitFor(() => expect(writes()[0]?.[1]).toEqual({ input: { connection_id: "local", key: "items", mutation } }));
  });

});
