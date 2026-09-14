import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CreateArrayInput, CreateKeyInput, CreateVectorSetInput, KeyValue, RedisValue } from "../../lib/types";
import { bytesToInput, keyToBytes } from "../../lib/redisBytes";
import AddKey from "./AddKey";

const { createKeyMock, createArrayMock, createVectorSetMock } = vi.hoisted(() => ({
  createKeyMock: vi.fn(),
  createArrayMock: vi.fn(),
  createVectorSetMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  createKey: createKeyMock,
  createArray: createArrayMock,
  createVectorSet: createVectorSetMock,
}));

function renderAddKey(busy = false) {
  const onCreated = vi.fn();
  render(
    <AddKey
      connectionId="local"
      busy={busy}
      arraySupported
      vectorSetSupported
      onCreated={onCreated}
      onCancel={vi.fn()}
    />,
  );
  return { onCreated };
}

function chooseKind(kind: string) {
  fireEvent.change(screen.getByLabelText("数据类型"), { target: { value: kind } });
}

function fillExample() {
  fireEvent.click(screen.getByRole("button", { name: "添加示例数据" }));
  expect(createKeyMock).not.toHaveBeenCalled();
  expect(createArrayMock).not.toHaveBeenCalled();
  expect(createVectorSetMock).not.toHaveBeenCalled();
}

function expectValidOrdinaryValue(value: RedisValue) {
  if ("String" in value) {
    expect(bytesToInput(value.String.value, "utf8").trim()).not.toBe("");
  } else if ("Hash" in value) {
    expect(value.Hash.fields.length).toBeGreaterThan(0);
    expect(value.Hash.fields.every(({ field, value: fieldValue }) => bytesToInput(field, "utf8").trim() !== "" && fieldValue !== "")).toBe(true);
    expect(new Set(value.Hash.fields.map(({ field }) => field)).size).toBe(value.Hash.fields.length);
  } else if ("List" in value) {
    expect(value.List.items.length).toBeGreaterThan(0);
    expect(value.List.items.every((item) => bytesToInput(item, "utf8").trim() !== "")).toBe(true);
  } else if ("Set" in value) {
    expect(value.Set.members.length).toBeGreaterThan(0);
    expect(value.Set.members.every((member) => bytesToInput(member, "utf8").trim() !== "")).toBe(true);
    expect(new Set(value.Set.members).size).toBe(value.Set.members.length);
  } else if ("SortedSet" in value) {
    expect(value.SortedSet.members.length).toBeGreaterThan(0);
    expect(value.SortedSet.members.every(({ member, score }) => bytesToInput(member, "utf8").trim() !== "" && Number.isFinite(score))).toBe(true);
  } else if ("Json" in value) {
    expect(value.Json.value).not.toBeNull();
    expect(JSON.parse(JSON.stringify(value.Json.value))).toEqual(value.Json.value);
  } else if ("Stream" in value) {
    expect(value.Stream.entries.length).toBeGreaterThan(0);
    for (const entry of value.Stream.entries) {
      expect(entry.id).toMatch(/^(\*|\d+-\d+)$/);
      expect(entry.fields.length).toBeGreaterThan(0);
      expect(entry.fields.every(({ field, value: fieldValue }) => bytesToInput(field, "utf8").trim() !== "" && fieldValue !== "")).toBe(true);
      expect(new Set(entry.fields.map(({ field }) => field)).size).toBe(entry.fields.length);
    }
  } else {
    throw new Error("普通类型示例不能通过模块 DTO 创建。");
  }
}

describe("AddKey 示例数据", () => {
  beforeEach(() => {
    createKeyMock.mockImplementation(async (input: CreateKeyInput): Promise<KeyValue> => ({
      key: input.key,
      key_type: "string",
      ttl_ms: input.ttl_ms ?? -1,
      value: input.value,
    }));
    createArrayMock.mockResolvedValue({
      key: "example:array",
      key_type: "array",
      ttl_ms: -1,
      value: { Array: { length: "3", count: "3" } },
    } satisfies KeyValue);
    createVectorSetMock.mockResolvedValue({
      key: "example:vectorset",
      key_type: "vectorset",
      ttl_ms: -1,
      value: { VectorSet: { total: "2", dimension: 3, quantization: "f32" } },
    } satisfies KeyValue);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.resetAllMocks();
  });

  it.each([
    ["string", "String"],
    ["hash", "Hash"],
    ["list", "List"],
    ["set", "Set"],
    ["zset", "SortedSet"],
    ["json", "Json"],
    ["stream", "Stream"],
  ])("填充 %s 示例后显式提交合法的 %s 数据", async (kind, dtoKind) => {
    const { onCreated } = renderAddKey();
    chooseKind(kind);
    fillExample();

    expect(screen.getByLabelText("数据类型")).toHaveValue(kind);
    expect((screen.getByLabelText("键名") as HTMLInputElement).value).toMatch(new RegExp(`^example:${kind}:.+`));
    expect(screen.getByLabelText("TTL（毫秒，可选）")).toHaveValue(null);
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => expect(createKeyMock).toHaveBeenCalledTimes(1));
    const input = createKeyMock.mock.calls[0][0] as CreateKeyInput;
    expect(input).toMatchObject({ connection_id: "local", ttl_ms: null });
    expect(Object.keys(input.value)).toEqual([dtoKind]);
    expectValidOrdinaryValue(input.value);
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  });

  it.each(["contiguous", "sparse"])("Array 示例遵循 %s 模式并可创建", async (mode) => {
    renderAddKey();
    chooseKind("array");
    fireEvent.change(screen.getByLabelText("Array 创建模式"), { target: { value: mode } });
    if (mode === "contiguous") {
      fireEvent.change(screen.getByLabelText("Array 起始索引"), { target: { value: "99" } });
    }
    fillExample();

    expect(screen.getByLabelText("数据类型")).toHaveValue("array");
    expect(screen.getByLabelText("Array 创建模式")).toHaveValue(mode);
    if (mode === "contiguous") expect(screen.getByLabelText("Array 起始索引")).toHaveValue("0");
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => expect(createArrayMock).toHaveBeenCalledTimes(1));
    const input = createArrayMock.mock.calls[0][0] as CreateArrayInput;
    expect(input).toMatchObject({ connection_id: "local", mode, ttl_ms: null });
    expect(input.key).toMatch(/^example:array:.+/);
    if (mode === "contiguous") {
      expect(input.start_index).toBe("0");
      expect(input.elements).toEqual([]);
      expect(input.values.length).toBeGreaterThan(0);
      expect(input.values.every((value) => value.trim() !== "")).toBe(true);
    } else {
      expect(input.start_index).toBeNull();
      expect(input.values).toEqual([]);
      expect(input.elements.length).toBeGreaterThan(0);
      expect(input.elements[0].index).toBe("0");
      expect(input.elements.every(({ index, value }) => /^\d+$/.test(index) && value.trim() !== "")).toBe(true);
      expect(new Set(input.elements.map(({ index }) => index)).size).toBe(input.elements.length);
    }
    expect(createKeyMock).not.toHaveBeenCalled();
    expect(createVectorSetMock).not.toHaveBeenCalled();
  });

  it("Vector Set 示例填充匹配的三维向量及 f32 量化", async () => {
    renderAddKey();
    chooseKind("vectorset");
    fireEvent.change(screen.getByLabelText("Vector Set 维度"), { target: { value: "7" } });
    fireEvent.change(screen.getByLabelText("量化类型（可选）"), { target: { value: "old" } });
    fillExample();

    expect(screen.getByLabelText("数据类型")).toHaveValue("vectorset");
    expect(screen.getByLabelText("Vector Set 维度")).toHaveValue(3);
    expect(screen.getByLabelText("量化类型（可选）")).toHaveValue("f32");
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => expect(createVectorSetMock).toHaveBeenCalledTimes(1));
    const input = createVectorSetMock.mock.calls[0][0] as CreateVectorSetInput;
    expect(input).toMatchObject({ connection_id: "local", dimension: 3, quantization: "f32", ttl_ms: null });
    expect(input.key).toMatch(/^example:vectorset:.+/);
    expect(input.elements.length).toBeGreaterThan(0);
    expect(new Set(input.elements.map(({ name }) => name)).size).toBe(input.elements.length);
    for (const element of input.elements) {
      expect(element.name.trim()).not.toBe("");
      expect(element.vector_values).toHaveLength(input.dimension);
      expect(element.vector_values?.every(Number.isFinite)).toBe(true);
      expect(element.vector_fp32_base64).toBeNull();
    }
    expect(createKeyMock).not.toHaveBeenCalled();
    expect(createArrayMock).not.toHaveBeenCalled();
  });

  it("保留已填键名和 TTL，允许修改示例后再提交", async () => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "custom:user" } });
    fireEvent.change(screen.getByLabelText("TTL（毫秒，可选）"), { target: { value: "60000" } });
    fillExample();

    expect(screen.getByLabelText("键名")).toHaveValue("custom:user");
    expect(screen.getByLabelText("TTL（毫秒，可选）")).toHaveValue(60000);
    fireEvent.change(screen.getByLabelText("字符串值"), { target: { value: "用户修改后的值" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));

    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith({
      connection_id: "local",
      key: "custom:user",
      ttl_ms: 60000,
      value: { String: { value: "用户修改后的值" } },
    }));
  });

  it("空白键名生成示例名称，重新清空后填充不会复用同一个名称", () => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "   " } });
    fillExample();
    const firstName = (screen.getByLabelText("键名") as HTMLInputElement).value;
    expect(firstName).toMatch(/^example:string:.+/);

    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "" } });
    fillExample();
    expect((screen.getByLabelText("键名") as HTMLInputElement).value).not.toBe(firstName);
  });

  it("填充示例后清除之前的编码错误", () => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名编码"), { target: { value: "hex" } });
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "GG" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Hex/i);
    fillExample();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("重复触发相同错误时重新开始 3 秒倒计时", () => {
    vi.useFakeTimers();
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名编码"), { target: { value: "hex" } });
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "GG" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Hex/i);

    act(() => { vi.advanceTimersByTime(2000); });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByRole("alert")).toHaveTextContent(/Hex/i);

    act(() => { vi.advanceTimersByTime(2000); });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("支持空键以及空字符串值", async () => {
    renderAddKey();
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith({ connection_id: "local", key: "", ttl_ms: null, value: { String: { value: "" } } }));
  });

  it("二进制键名和字符串值按所选编码创建", async () => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名编码"), { target: { value: "hex" } });
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: "FF 00" } });
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "base64" } });
    fireEvent.change(screen.getByLabelText("字符串值"), { target: { value: "/4A=" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledTimes(1));
    const input = createKeyMock.mock.calls[0][0] as CreateKeyInput;
    expect(keyToBytes(input.key)).toEqual({ base64: "/wA=" });
    expect(input.value).toEqual({ String: { value: { base64: "/4A=" } } });
  });

  it("二进制集合 JSON 支持空字段名、空值并拒绝非法编码", async () => {
    renderAddKey(); chooseKind("hash");
    fireEvent.change(screen.getByLabelText("集合值编码"), { target: { value: "base64" } });
    fireEvent.change(screen.getByLabelText("集合值（JSON 数组）"), { target: { value: '[{"field":"/w==","value":"%%%"}]' } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Base64/);
    expect(createKeyMock).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("集合值（JSON 数组）"), { target: { value: '[{"field":"/w==","value":""},{"field":"","value":"gA=="}]' } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith(expect.objectContaining({ value: { Hash: { fields: [{ field: { base64: "/w==" }, value: "" }, { field: "", value: { base64: "gA==" } }] } } })));
  });

  it("文本键名保留首尾空格", async () => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名"), { target: { value: " key " } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith(expect.objectContaining({ key: " key " })));
  });

  it.each([["61 0a 62", "61 0a 63", "a\nc"], ["61 0d 0a 62", "61 0d 0a 63", "a\r\nc"]])("键名单行输入拒绝将 %s 转为 UTF8，后续编辑保留换行字节", async (before, after, expected) => {
    renderAddKey();
    fireEvent.change(screen.getByLabelText("键名编码"), { target: { value: "hex" } });
    const input = screen.getByLabelText("键名");
    expect(input).toBeInstanceOf(HTMLInputElement);
    fireEvent.change(input, { target: { value: before } });
    fireEvent.change(screen.getByLabelText("键名编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("键名编码")).toHaveValue("hex");
    expect(input).toHaveValue(before);
    expect(screen.getByRole("alert")).toHaveTextContent(/单行输入/);
    expect(createKeyMock).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: after } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledTimes(1));
    expect(keyToBytes(createKeyMock.mock.calls[0][0].key)).toBe(expected);
  });

  it("String多行输入拒绝CRLF转UTF8并保留后续编辑的CR字节", async () => {
    renderAddKey();
    const input = screen.getByLabelText("字符串值");
    expect(input).toBeInstanceOf(HTMLTextAreaElement);
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "hex" } });
    fireEvent.change(input, { target: { value: "61 0d 0a 62" } });
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("字符串值编码")).toHaveValue("hex");
    expect(input).toHaveValue("61 0d 0a 62");
    expect(createKeyMock).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "61 0d 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith(expect.objectContaining({ value: { String: { value: "a\r\nc" } } })));
  });

  it("集合JSON转换按行UTF8时拒绝中间CR，继续编辑保留CR字节", async () => {
    renderAddKey(); chooseKind("list");
    fireEvent.change(screen.getByLabelText("集合值编码"), { target: { value: "hex" } });
    const input = screen.getByLabelText("集合值（JSON 数组）");
    expect(input).toBeInstanceOf(HTMLTextAreaElement);
    fireEvent.change(input, { target: { value: '["61 0d 62"]' } });
    fireEvent.change(screen.getByLabelText("集合值编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("集合值编码")).toHaveValue("hex");
    expect(input).toHaveValue('["61 0d 62"]');
    expect(createKeyMock).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: '["61 0d 63"]' } });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    await waitFor(() => expect(createKeyMock).toHaveBeenCalledWith(expect.objectContaining({ value: { List: { items: ["a\rc"] } } })));
  });

  it("忙碌时禁用添加示例数据按钮", () => {
    renderAddKey(true);
    expect(screen.getByRole("button", { name: "添加示例数据" })).toBeDisabled();
  });
});
