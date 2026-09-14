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
  it("初始 UTF8 含 CR 自动回退 Hex，保存仍提交原始回车字节", async () => {
    getValue.mockResolvedValue(value("YQ1i"));
    decode.mockImplementation(async ({ format }) => ({ text: format === "hex" ? "61 0D 62" : "a\rb", byte_length: 3 }));
    encode.mockResolvedValue("YQ1iYw==");
    render(<StringValueEditor connectionId="local" keyName="carriage-return" />);
    await waitFor(() => expect(screen.getByLabelText("值格式")).toHaveValue("hex"));
    expect(screen.getByLabelText("String 值")).toHaveValue("61 0D 62");
    fireEvent.change(screen.getByLabelText("String 值"), { target: { value: "61 0D 62 63" } });
    fireEvent.click(screen.getByRole("button", { name: "保存值" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ connection_id: "local", key: "carriage-return", base64: "YQ1iYw==" }));
    expect(encode).toHaveBeenCalledWith({ text: "61 0D 62 63", format: "hex", compression: "none" });
  });
  it.each(["utf8", "ascii"])("显式选择 %s 时含 CR 的文本禁止编辑，切回 Base64 保留原始字节", async (format) => {
    getValue.mockResolvedValue(value("YQ1i"));
    decode.mockImplementation(async ({ format: next }) => {
      if (next === "hex") return { text: "61 0D 62", byte_length: 3 };
      if (next === "base64") return { text: "YQ1i", byte_length: 3 };
      return { text: "a\rb", byte_length: 3 };
    });
    render(<StringValueEditor connectionId="local" keyName="carriage-return" />);
    // Switch through Hex so this remains an explicit choice even before the fix.
    await screen.findByLabelText("String 值");
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "hex" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveValue("61 0D 62"));
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: format } });
    expect(await screen.findByRole("alert")).toHaveTextContent(/原始字节已保留.*Hex.*Base64/);
    expect(screen.queryByLabelText("String 值")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存值" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "base64" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveValue("YQ1i"));
    expect(decode).toHaveBeenLastCalledWith({ base64: "YQ1i", format: "base64", compression: "none" });
    expect(encode).not.toHaveBeenCalled(); expect(save).not.toHaveBeenCalled();
  });
  it("压缩后 UTF8 文本含 CR 也拒绝编辑且保留压缩的原始字节", async () => {
    decode.mockImplementation(async ({ format, compression }) => ({
      text: format === "hex" ? "00 FF 80" : compression === "gzip" ? "a\rb" : "plain", byte_length: 3,
    }));
    render(<StringValueEditor connectionId="local" keyName="compressed" />);
    await screen.findByDisplayValue("plain");
    fireEvent.change(screen.getByLabelText("压缩格式"), { target: { value: "gzip" } });
    expect(await screen.findByRole("alert")).toHaveTextContent(/原始字节已保留.*Hex.*Base64/);
    expect(screen.queryByLabelText("String 值")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存值" })).toBeDisabled();
    expect(decode).toHaveBeenLastCalledWith({ base64: "AP+A", format: "utf8", compression: "gzip" });
    expect(encode).not.toHaveBeenCalled(); expect(save).not.toHaveBeenCalled();
  });
  it("LF 文本仍可编辑，JSON 和只读结构视图不受 CR 检查影响", async () => {
    decode.mockImplementation(async ({ format }) => ({
      text: format === "utf8" || format === "ascii" ? "a\nb" : '{\r"value":1\r}', byte_length: 3,
    }));
    render(<StringValueEditor connectionId="local" keyName="line-feed" />);
    expect(await screen.findByLabelText("String 值")).toHaveValue("a\nb");
    expect(screen.getByLabelText("值格式")).toHaveValue("utf8");
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "ascii" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveValue("a\nb"));
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "json" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).not.toHaveAttribute("readonly"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("值格式"), { target: { value: "php" } });
    await waitFor(() => expect(screen.getByLabelText("String 值")).toHaveAttribute("readonly"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
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
    expect(await screen.findByRole("status")).toHaveTextContent("保留原 TTL");
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
