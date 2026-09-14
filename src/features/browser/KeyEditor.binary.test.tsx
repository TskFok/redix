import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import KeyEditor from "./KeyEditor";
import type { RedisValue } from "../../lib/types";

const binary = { base64: "/wCA" };
const setup = (value: RedisValue) => {
  const onSave = vi.fn().mockResolvedValue(undefined);
  render(<KeyEditor value={value} ttlMs={-1} busy={false} error={null} onSave={onSave} onDelete={vi.fn()} onSetTtl={vi.fn()} />);
  return onSave;
};
afterEach(cleanup);

describe("整值二进制编辑", () => {
  it("读取二进制 String 默认 Hex，转换 Base64 保持原始字节", async () => {
    const save = setup({ String: { value: binary } });
    expect(screen.getByLabelText("字符串值")).toHaveValue("ff 00 80");
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "base64" } });
    expect(screen.getByLabelText("字符串值")).toHaveValue("/wCA");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ String: { value: binary } }));
  });

  it("Hash 字段和值可以分别选择编码，空值合法", async () => {
    const save = setup({ Hash: { fields: [{ field: binary, value: binary }] } });
    expect(screen.getByLabelText("字段 1")).toHaveValue("ff 00 80");
    fireEvent.change(screen.getByLabelText("值 1"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ Hash: { fields: [{ field: binary, value: "" }] } }));
  });

  it("List 移除前项后保留后续项各自的编码", async () => {
    const save = setup({ List: { items: ["text", binary, "last"] } });
    fireEvent.click(screen.getByRole("button", { name: "删除元素 1" }));
    expect(screen.getByLabelText("元素 1编码")).toHaveValue("hex");
    expect(screen.getByLabelText("元素 1")).toHaveValue("ff 00 80");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ List: { items: [binary, "last"] } }));
  });

  it("Set 按实际字节去重，兼容文本和 Base64", async () => {
    const save = setup({ Set: { members: ["same", { base64: "c2FtZQ==" }, binary, { ...binary }] } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ Set: { members: ["same", binary] } }));
  });

  it("Sorted Set 修改分数仍保留成员的二进制字节", async () => {
    const save = setup({ SortedSet: { members: [{ member: binary, score: 1 }] } });
    fireEvent.change(screen.getByLabelText("分数 1"), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ SortedSet: { members: [{ member: binary, score: 3 }] } }));
  });

  it("非法 Hex 不保存且拒绝切换编码，原草稿保持不变", () => {
    const save = setup({ String: { value: binary } });
    fireEvent.change(screen.getByLabelText("字符串值"), { target: { value: "GG" } });
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "base64" } });
    expect(screen.getByLabelText("字符串值编码")).toHaveValue("hex");
    expect(screen.getByLabelText("字符串值")).toHaveValue("GG");
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Hex/); expect(save).not.toHaveBeenCalled();
  });

  it("非法 UTF8 不可转文本，控制字符默认 Hex 避免输入框丢失换行", () => {
    setup({ Hash: { fields: [{ field: "\n", value: binary }] } });
    expect(screen.getByLabelText("字段 1")).toHaveValue("0a");
    fireEvent.change(screen.getByLabelText("值 1编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("值 1编码")).toHaveValue("hex");
    expect(screen.getByRole("alert")).toHaveTextContent(/UTF-8/);
  });
  it.each([
    ["Hash 字段", "字段 1", { Hash: { fields: [{ field: "a\nb", value: "v" }] } }, { Hash: { fields: [{ field: "a\nc", value: "v" }] } }],
    ["Hash 值", "值 1", { Hash: { fields: [{ field: "f", value: "a\nb" }] } }, { Hash: { fields: [{ field: "f", value: "a\nc" }] } }],
    ["List", "元素 1", { List: { items: ["a\nb"] } }, { List: { items: ["a\nc"] } }],
    ["Set", "成员 1", { Set: { members: ["a\nb"] } }, { Set: { members: ["a\nc"] } }],
    ["Sorted Set", "成员 1", { SortedSet: { members: [{ member: "a\nb", score: 1 }] } }, { SortedSet: { members: [{ member: "a\nc", score: 1 }] } }],
  ] as [string, string, RedisValue, RedisValue][])("%s 单行输入拒绝UTF8换行转换，后续编辑不丢换行", async (_kind, label, value, expected) => {
    const save = setup(value);
    const input = screen.getByLabelText(label);
    expect(input).toBeInstanceOf(HTMLInputElement);
    expect(input).toHaveValue("61 0a 62");
    fireEvent.change(screen.getByLabelText(`${label}编码`), { target: { value: "utf8" } });
    expect(screen.getByLabelText(`${label}编码`)).toHaveValue("hex");
    expect(input).toHaveValue("61 0a 62");
    expect(screen.getByRole("alert")).toHaveTextContent(/单行输入/);
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "61 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith(expected));
  });

  it("String textarea仍允许LF转换为UTF8并保留换行", async () => {
    const save = setup({ String: { value: "a\nb" } });
    const input = screen.getByLabelText("字符串值");
    expect(input).toBeInstanceOf(HTMLTextAreaElement);
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "utf8" } });
    expect(input).toHaveValue("a\nb");
    fireEvent.change(input, { target: { value: "a\nc" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ String: { value: "a\nc" } }));
  });

  it("String多行输入拒绝CRLF转UTF8并保留后续编辑的CR字节", async () => {
    const save = setup({ String: { value: "a\r\nb" } });
    const input = screen.getByLabelText("字符串值");
    expect(input).toBeInstanceOf(HTMLTextAreaElement);
    expect(input).toHaveValue("61 0d 0a 62");
    fireEvent.change(screen.getByLabelText("字符串值编码"), { target: { value: "utf8" } });
    expect(screen.getByLabelText("字符串值编码")).toHaveValue("hex");
    expect(input).toHaveValue("61 0d 0a 62");
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "61 0d 0a 63" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ String: { value: "a\r\nc" } }));
  });

});
