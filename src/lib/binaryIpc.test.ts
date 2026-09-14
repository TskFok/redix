import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { invokeBinary } from "./binaryIpc";
import { keyFromBytes, keyToBytes } from "./redisBytes";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
beforeEach(() => call.mockReset());
describe("二进制键 IPC 边界", () => {
  it("扫描结果生成稳定身份，后续查询传回精确原始字节", async () => {
    const raw = { base64: "/wA=" };
    call.mockResolvedValueOnce([{ key: raw }, { key: "Hex: ff 00" }, { key: "" }]);
    const result = await invokeBinary<{ key: string }[]>("scan_all_keys", { input: { connection_id: "local", pattern: "*" } });
    expect(new Set(result.map(item => item.key)).size).toBe(3);
    expect(keyToBytes(result[0].key)).toEqual(raw);
    call.mockResolvedValueOnce({ key: raw, value: { String: { value: raw } } });
    const detail = await invokeBinary<{ key: string }>("get_browser_key", { input: { connection_id: "local", key: result[0].key } });
    expect(call).toHaveBeenLastCalledWith("get_browser_key", { input: { connection_id: "local", key: raw } });
    expect(detail.key).toBe(result[0].key);
  });
  it("重命名、批量删除与集合成员各自保留原始字节", async () => {
    const key = keyFromBytes({ base64: "/w==" });
    const renamed = keyFromBytes({ base64: "/g==" });
    await invokeBinary("rename_browser_key", { input: { key, new_key: renamed } });
    expect(call).toHaveBeenLastCalledWith("rename_browser_key", { input: { key: { base64: "/w==" }, new_key: { base64: "/g==" } } });
    await invokeBinary("start_bulk_delete", { input: { keys: [key, ""] } });
    expect(call).toHaveBeenLastCalledWith("start_bulk_delete", { input: { keys: [{ base64: "/w==" }, ""] } });
    await invokeBinary("mutate_collection", { input: { key, mutation: { operation: "hash_set", field: { base64: "/g==" }, value: "\0" } } });
    expect(call).toHaveBeenLastCalledWith("mutate_collection", { input: { key: { base64: "/w==" }, mutation: { operation: "hash_set", field: { base64: "/g==" }, value: "\0" } } });
  });
  it("不遍历JSON文档中的key字段，也不解释导入文件的字符串键为内部标记", async () => {
    const key = keyFromBytes({ base64: "/w==" });
    const value = { Json: { value: { key, keys: [key] } } };
    await invokeBinary("create_key", { input: { key, value } });
    expect(call).toHaveBeenLastCalledWith("create_key", { input: { key: { base64: "/w==" }, value } });
    const entries = [{ key, ttl_ms: -1, value }];
    await invokeBinary("import_keys", { input: { connection_id: "local", entries } });
    expect(call).toHaveBeenLastCalledWith("import_keys", { input: { connection_id: "local", entries } });
    call.mockResolvedValueOnce(entries);
    expect(await invokeBinary("export_keys", { input: { keys: [key] } })).toEqual(entries);
  });
});
