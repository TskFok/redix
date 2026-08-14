import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  closeConnection,
  deleteConnection,
  deleteKey,
  executeCommand,
  getKey,
  listConnections,
  openConnection,
  saveConnection,
  scanKeys,
  setKey,
  setKeyTtl,
  testConnection,
} from "./tauri";
import type {
  CommandResult,
  ConnectionInfo,
  ConnectionProfile,
  KeyValue,
  RedisValue,
  SaveConnectionInput,
  ScanPage,
} from "./types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

const profile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
};

const connectionInput: SaveConnectionInput = {
  profile,
  password: null,
};

const stringValue: RedisValue = { String: { value: "value" } };

beforeEach(() => {
  invokeMock.mockReset();
});

describe("Tauri IPC bridge", () => {
  it("用稳定命令名调用 scan_keys", async () => {
    const result: ScanPage = { cursor: 0, keys: [], has_more: false };
    invokeMock.mockResolvedValue(result);

    await expect(
      scanKeys({ connection_id: "local", cursor: 0, pattern: "*", count: 100 }),
    ).resolves.toEqual(result);

    expect(invokeMock).toHaveBeenCalledWith("scan_keys", {
      input: { connection_id: "local", cursor: 0, pattern: "*", count: 100 },
    });
  });

  it("为连接命令使用 Rust 的 snake_case 名称和参数形状", async () => {
    const info: ConnectionInfo = { server_version: "7.4.0" };
    invokeMock.mockResolvedValue(info);

    await testConnection(connectionInput);
    expect(invokeMock).toHaveBeenLastCalledWith("test_connection", {
      input: connectionInput,
    });

    await openConnection("local");
    expect(invokeMock).toHaveBeenLastCalledWith("open_connection", {
      connection_id: "local",
    });

    await closeConnection("local");
    expect(invokeMock).toHaveBeenLastCalledWith("close_connection", {
      connection_id: "local",
    });

    await deleteConnection("local");
    expect(invokeMock).toHaveBeenLastCalledWith("delete_connection", {
      connection_id: "local",
    });
  });

  it("为全部数据命令传递 input 对象并保留返回值", async () => {
    const keyValue: KeyValue = {
      key: "demo",
      key_type: "string",
      ttl_ms: -1,
      value: stringValue,
    };
    const commandResult: CommandResult = { kind: "string", value: "PONG" };
    invokeMock
      .mockResolvedValueOnce(keyValue)
      .mockResolvedValueOnce(keyValue)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(42)
      .mockResolvedValueOnce(commandResult);

    const getInput = { connection_id: "local", key: "demo" };
    const setInput = { ...getInput, value: stringValue };
    const ttlInput = { ...getInput, ttl_ms: 60_000 };

    await expect(getKey(getInput)).resolves.toEqual(keyValue);
    expect(invokeMock).toHaveBeenLastCalledWith("get_key", { input: getInput });

    await expect(setKey(setInput)).resolves.toEqual(keyValue);
    expect(invokeMock).toHaveBeenLastCalledWith("set_key", { input: setInput });

    await expect(deleteKey(getInput)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("delete_key", { input: getInput });

    await expect(setKeyTtl(ttlInput)).resolves.toBe(42);
    expect(invokeMock).toHaveBeenLastCalledWith("set_key_ttl", { input: ttlInput });

    const commandInput = { connection_id: "local", command: "PING" };
    await expect(executeCommand(commandInput)).resolves.toEqual(commandResult);
    expect(invokeMock).toHaveBeenLastCalledWith("execute_command", {
      input: commandInput,
    });
  });

  it("list_connections 使用无参数调用并返回 profile 列表", async () => {
    invokeMock.mockResolvedValue([profile]);

    await expect(listConnections()).resolves.toEqual([profile]);
    expect(invokeMock).toHaveBeenCalledWith("list_connections");
  });

  it("save_connection 使用 input 包装并返回不含密码的 profile", async () => {
    invokeMock.mockResolvedValue(profile);

    await expect(saveConnection(connectionInput)).resolves.toEqual(profile);
    expect(invokeMock).toHaveBeenCalledWith("save_connection", {
      input: connectionInput,
    });
  });

  it("只把 IPC 错误归一化为 code 和 message", async () => {
    invokeMock.mockRejectedValue({
      code: "CONNECTION_FAILED",
      message: "无法连接到 Redis 服务器",
      password: "secret-value",
      uri: "redis://:secret-value@127.0.0.1:6379/0",
      cause: new Error("底层连接错误"),
    });

    await expect(listConnections()).rejects.toEqual({
      code: "CONNECTION_FAILED",
      message: "无法连接到 Redis 服务器",
    });
  });

  it("未知底层错误不会泄露原始错误内容", async () => {
    invokeMock.mockRejectedValue(
      new Error("redis://:secret-value@127.0.0.1:6379/0 connection refused"),
    );

    await expect(listConnections()).rejects.toEqual({
      code: "IPC_ERROR",
      message: "IPC 调用失败",
    });
  });
});
