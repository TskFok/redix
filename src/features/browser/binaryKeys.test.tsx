import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import KeyList from "./KeyList";
import KeyDetails from "./KeyDetails";
import { bytesFromInput, keyFromBytes } from "../../lib/redisBytes";
import { applyCreatedKey, initialBrowserPageState } from "./browserState";
import { parseImportEntries } from "./BrowserImportExport";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
beforeEach(() => call.mockReset());
afterEach(cleanup);
const raw = { base64: "/wA=" };
const key = keyFromBytes(raw);

describe("二进制键浏览与操作", () => {
  it("树和平铺显示Hex标签，选中时保留字节身份", () => {
    const select = vi.fn();
    render(<KeyList pattern="*" keyType="" keys={[{ key }, { key: "" }]} selectedKey={null} selectedKeys={[]} loading={false} scanFailed={false}
      onPatternChange={vi.fn()} onPatternKeyDown={vi.fn()} onKeyTypeChange={vi.fn()} onSelect={select} onToggleSelect={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Hex: ff 00" }));
    expect(select).toHaveBeenLastCalledWith(key);
    expect(screen.queryByRole("button", { name: /展开前缀/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    expect(screen.getByRole("button", { name: "Hex: ff 00" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "（空键）" }));
    expect(select).toHaveBeenLastCalledWith("");
  });
  it("按原始字节匹配新建键，不用显示标签或内部标识匹配glob", () => {
    const detail = { key, key_type: "string", ttl_ms: -1, value: { String: { value: "" } } };
    expect(applyCreatedKey({ ...initialBrowserPageState, pattern: "??" }, detail).keys).toHaveLength(1);
    expect(applyCreatedKey({ ...initialBrowserPageState, pattern: "?" }, detail).keys).toHaveLength(0);
  });
  it.each(["61 0a 62", "61 0d 62"])("重命名单行输入拒绝隐藏换行字节 %s", async (hex) => {
    call.mockImplementation(async (command) => {
      if (command === "get_string_value") return { base64: "", total_bytes: 0, ttl_ms: -1, truncated: false };
      if (command === "decode_string_value") return { text: "", byte_length: 0 };
      return undefined;
    });
    render(<KeyDetails connectionId="local" detail={{ key: keyFromBytes(bytesFromInput(hex, "hex")), key_type: "string", ttl_ms: -1, value: { String: { value: "" } } }} loading={false} onDeleted={vi.fn()} onDetailChange={vi.fn()} />);
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    await waitFor(() => expect(screen.getByLabelText("新键名编码")).toBeEnabled());
    expect(screen.getByLabelText("新键名")).toHaveValue(hex);
    fireEvent.change(screen.getByLabelText("新键名编码"), { target: { value: "utf8" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("换行");
    expect(screen.getByLabelText("新键名编码")).toHaveValue("hex");
    expect(screen.getByLabelText("新键名")).toHaveValue(hex);
  });
  it("重命名编辑器保留原始字节、拒绝非法Hex并允许空键名", async () => {
    call.mockImplementation(async (command) => {
      if (command === "get_string_value") return { base64: "", total_bytes: 0, ttl_ms: -1, truncated: false };
      if (command === "decode_string_value") return { text: "", byte_length: 0 };
      if (command === "rename_browser_key") return { key: "", key_type: "string", ttl_ms: -1, value: { String: { value: "" } } };
      return undefined;
    });
    render(<KeyDetails connectionId="local" detail={{ key, key_type: "string", ttl_ms: -1, value: { String: { value: "" } } }} loading={false} onDeleted={vi.fn()} onDetailChange={vi.fn()} />);
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    await waitFor(() => expect(screen.getByLabelText("新键名")).toBeEnabled());
    expect(screen.getByLabelText("新键名编码")).toHaveValue("hex");
    expect(screen.getByLabelText("新键名")).toHaveValue("ff 00");
    fireEvent.change(screen.getByLabelText("新键名"), { target: { value: "f" } });
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Hex");
    expect(call.mock.calls.some(([name]) => name === "rename_browser_key")).toBe(false);
    fireEvent.change(screen.getByLabelText("新键名"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "重命名" }));
    await waitFor(() => expect(call).toHaveBeenCalledWith("rename_browser_key", { input: { connection_id: "local", key: raw, new_key: "" } }));
  });
});

describe("二进制导入文件", () => {
  it("接受二进制及空键名，按字节拒绝重复或非法base64", () => {
    const entry = (name: unknown) => ({ key: name, ttl_ms: -1, value: { Hash: { fields: [{ field: raw, value: "" }] } } });
    const entries = [entry(raw), entry(""), entry(" ")];
    expect(parseImportEntries(JSON.stringify(entries))).toEqual(entries);
    expect(() => parseImportEntries(JSON.stringify([entry("a"), entry({ base64: "YQ==" })]))).toThrow("重复");
    expect(() => parseImportEntries(JSON.stringify([entry({ base64: "/w" })]))).toThrow();
    expect(() => parseImportEntries(JSON.stringify([entry({ base64: "/w==", unexpected: true })]))).toThrow();
  });
});
