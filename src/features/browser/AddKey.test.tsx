import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CreateArrayInput, CreateKeyInput, CreateVectorSetInput, KeyValue, RedisValue } from "../../lib/types";
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
    expect(value.String.value.trim()).not.toBe("");
  } else if ("Hash" in value) {
    expect(value.Hash.fields.length).toBeGreaterThan(0);
    expect(value.Hash.fields.every(({ field, value: fieldValue }) => field.trim() !== "" && fieldValue !== "")).toBe(true);
    expect(new Set(value.Hash.fields.map(({ field }) => field)).size).toBe(value.Hash.fields.length);
  } else if ("List" in value) {
    expect(value.List.items.length).toBeGreaterThan(0);
    expect(value.List.items.every((item) => item.trim() !== "")).toBe(true);
  } else if ("Set" in value) {
    expect(value.Set.members.length).toBeGreaterThan(0);
    expect(value.Set.members.every((member) => member.trim() !== "")).toBe(true);
    expect(new Set(value.Set.members).size).toBe(value.Set.members.length);
  } else if ("SortedSet" in value) {
    expect(value.SortedSet.members.length).toBeGreaterThan(0);
    expect(value.SortedSet.members.every(({ member, score }) => member.trim() !== "" && Number.isFinite(score))).toBe(true);
  } else if ("Json" in value) {
    expect(value.Json.value).not.toBeNull();
    expect(JSON.parse(JSON.stringify(value.Json.value))).toEqual(value.Json.value);
  } else if ("Stream" in value) {
    expect(value.Stream.entries.length).toBeGreaterThan(0);
    for (const entry of value.Stream.entries) {
      expect(entry.id).toMatch(/^(\*|\d+-\d+)$/);
      expect(entry.fields.length).toBeGreaterThan(0);
      expect(entry.fields.every(({ field, value: fieldValue }) => field.trim() !== "" && fieldValue !== "")).toBe(true);
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

  it("填充示例后清除之前的空键名错误", () => {
    renderAddKey();
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("alert")).toHaveTextContent("键名不能为空");
    fillExample();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("重复触发相同错误时重新开始 3 秒倒计时", () => {
    vi.useFakeTimers();
    renderAddKey();
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    expect(screen.getByRole("alert")).toHaveTextContent("键名不能为空");

    act(() => { vi.advanceTimersByTime(2000); });
    fireEvent.click(screen.getByRole("button", { name: "创建键" }));
    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByRole("alert")).toHaveTextContent("键名不能为空");

    act(() => { vi.advanceTimersByTime(2000); });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("忙碌时禁用添加示例数据按钮", () => {
    renderAddKey(true);
    expect(screen.getByRole("button", { name: "添加示例数据" })).toBeDisabled();
  });
});
