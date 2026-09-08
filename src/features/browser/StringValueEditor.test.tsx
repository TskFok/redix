import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import StringValueEditor from "./StringValueEditor";
import type { StringValue } from "./valueCodecApi";

const { getValue, decode, encode, save } = vi.hoisted(() => ({ getValue: vi.fn(), decode: vi.fn(), encode: vi.fn(), save: vi.fn() }));
vi.mock("./valueCodecApi", () => ({ getStringValue: getValue, decodeStringValue: decode, encodeStringValue: encode, setStringValue: save }));
const value = (base64 = "AP+A", truncated = false): StringValue => ({ base64, total_bytes: truncated ? 5_000_000 : 3, ttl_ms: 5000, truncated });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; }

describe("String 原始字节解码与保存", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    getValue.mockResolvedValue(value());
    decode.mockImplementation(async ({ format }) => {
      if (format === "utf8") throw { code: "INVALID_INPUT" };
      return { text: format === "hex" ? "00 FF 80" : "AP+A", byte_length: 3 };
    });
    encode.mockResolvedValue("AAEC"); save.mockResolvedValue({ byte_length: 3, ttl_ms: 4500 });
  });
  afterEach(() => { cleanup(); vi.useRealTimers(); });
  it("保存成功反馈自动消失，已保存的值继续显示", async () => {
    render(<StringValueEditor connectionId="local" keyName="binary" />);
    await screen.findByDisplayValue("00 FF 80");
    vi.useFakeTimers();
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "00 01 02" } });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "保存值" })); });
    expect(screen.getByRole("status")).toHaveTextContent("值已保存");
    act(() => { vi.advanceTimersByTime(3000); });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByLabelText("String 值")).toHaveValue("00 01 02");
  });
  it("PHP 序列化视图只读，原始字节保留且不发写请求", async () => {
    render(<StringValueEditor connectionId="local" keyName="php" />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "php" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveAttribute("readonly"));
    expect(screen.getByRole("button", { name: "保存值" })).toBeDisabled();
    expect(decode).toHaveBeenLastCalledWith({ base64: "AP+A", format: "php", compression: "none" });
    expect(save).not.toHaveBeenCalled();
  });
  it("直接读取二进制并在UTF8失败后回退Hex，切换格式复用相同原始字节", async () => {
    render(<StringValueEditor connectionId="local" keyName="binary" />);
    expect(await screen.findByLabelText("String 值")).toHaveValue("00 FF 80");
    expect(getValue).toHaveBeenCalledWith({ connection_id: "local", key: "binary" });
    expect(screen.getByLabelText("值格式")).toHaveValue("hex");
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "base64" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveValue("AP+A"));
    expect(decode).toHaveBeenLastCalledWith({ base64: "AP+A", format: "base64", compression: "none" });
  });
  it("保存格式化草稿转换得到的字节，只发送一次写请求并报告保留的TTL", async () => {
    const onSaved = vi.fn();
    render(<StringValueEditor connectionId="local" keyName="binary" onSaved={onSaved} />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "00 01 02" } });
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith({ byte_length: 3, ttl_ms: 4500 }));
    expect(encode).toHaveBeenCalledWith({ text: "00 01 02", format: "hex", compression: "none" });
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith({ connection_id: "local", key: "binary", base64: "AAEC" });
    expect(screen.getByRole("status")).toHaveTextContent("保留原 TTL");
  });
  it("草稿未保存时阻止切换格式覆盖内容；失败保留草稿并隐藏底层错误", async () => {
    save.mockRejectedValue({ code: "COMMAND_FAILED", message: "secret" });
    render(<StringValueEditor connectionId="local" keyName="binary" />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "00 01 02" } });
    expect(screen.getByLabelText("值格式")).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    expect(await screen.findByRole("alert")).not.toHaveTextContent("secret");
    expect(screen.getByLabelText("String 值")).toHaveValue("00 01 02");
  });
  it("截断值与只读协议视图不可覆盖Redis原值", async () => {
    getValue.mockResolvedValue(value("AP+A", true));
    const { rerender } = render(<StringValueEditor connectionId="local" keyName="large" />);
    await screen.findByDisplayValue("00 FF 80");
    expect(screen.getByRole("button", { name: "保存值" })).toBeDisabled();
    expect(screen.getByText(/仅预览前/)).toBeInTheDocument();
    getValue.mockResolvedValue(value());
    rerender(<StringValueEditor connectionId="local" keyName="proto" />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "protobuf" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveValue("AP+A"));
    expect(screen.getByLabelText("String 值")).toHaveAttribute("readonly");
    expect(screen.getByRole("button", { name: "保存值" })).toBeDisabled();
    expect(save).not.toHaveBeenCalled();
  });
  it("A→B→A切换丢弃旧读取和写入结果", async () => {
    const oldRead = deferred<StringValue>(); const oldSave = deferred<{ byte_length: number; ttl_ms: number }>();
    getValue.mockReturnValueOnce(oldRead.promise);
    const onSaved = vi.fn();
    const { rerender } = render(<StringValueEditor connectionId="a" keyName="k" onSaved={onSaved} />);
    rerender(<StringValueEditor connectionId="b" keyName="k" onSaved={onSaved} />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "01" } });
    save.mockReturnValueOnce(oldSave.promise);
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    rerender(<StringValueEditor connectionId="a" keyName="k" onSaved={onSaved} />);
    await screen.findByDisplayValue("00 FF 80");
    await act(async () => { oldRead.resolve(value("b2xk")); oldSave.resolve({ byte_length: 1, ttl_ms: 1 }); });
    expect(screen.getByLabelText("String 值")).toHaveValue("00 FF 80");
    expect(onSaved).not.toHaveBeenCalled();
  });
  it("本地编码期间离开页面，不继续发起Redis写入", async () => {
    const encoded = deferred<string>(); encode.mockReturnValueOnce(encoded.promise);
    const { unmount } = render(<StringValueEditor connectionId="a" keyName="k" />);
    await screen.findByDisplayValue("00 FF 80");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "01" } });
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    await waitFor(() => expect(encode).toHaveBeenCalledTimes(1));
    unmount();
    await act(async () => { encoded.resolve("AQ=="); });
    expect(save).not.toHaveBeenCalled();
  });
});
