import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearSlowLogs,
  closeConnection,
  createKey,
  deleteKeys,
  deleteConnection,
  deleteKey,
  executeCommands,
  executeCommand,
  exportKeys,
  getSlowLogConfig,
  getSlowLogs,
  getDatabaseOverview,
  getCommandCatalog,
  getKey,
  getKeyInfo,
  getInstanceOverview,
  getAppSettings,
  importKeys,
  listQueryLibrary,
  listCommandHistory,
  listConnections,
  openConnection,
  publishPubSub,
  startProfiler,
  renameKey,
  saveConnection,
  scanKeys,
  saveCommandHistory,
  saveAppSettings,
  saveQueryLibraryItem,
  selectDatabase,
  startPubSub,
  stopPubSub,
  stopProfiler,
  setKey,
  setKeyTtl,
  deleteQueryLibraryItem,
  testConnection,
  updateSlowLogConfig,
} from "./tauri";
import type {
  CommandDefinition,
  CommandExecutionItem,
  CommandHistoryEntry,
  CommandResult,
  AppSettings,
  DatabaseOverview,
  InstanceOverview,
  PubSubSession,
  ProfilerSession,
  QueryLibraryItem,
  QueryLibraryItemInput,
  ConnectionInfo,
  ConnectionProfile,
  ExportedKey,
  KeyValue,
  RedisValue,
  SaveConnectionInput,
  SaveCommandHistoryInput,
  ScanPage,
  SlowLogConfig,
  SlowLogEntry,
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
      scanKeys({
        connection_id: "local",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: null,
      }),
    ).resolves.toEqual(result);

    expect(invokeMock).toHaveBeenCalledWith("scan_keys", {
      input: {
        connection_id: "local",
        cursor: 0,
        pattern: "*",
        count: 100,
        key_type: null,
      },
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

  it("为 Workbench 批量执行、目录和历史使用稳定 IPC 合同", async () => {
    const batchInput = {
      connection_id: "local",
      commands: ["PING", "DBSIZE"],
      continue_on_error: false,
    };
    const batchResults: CommandExecutionItem[] = [
      { command: "PING", result: { kind: "string", value: "PONG" }, error_code: null },
    ];
    const catalog: CommandDefinition[] = [
      { name: "PING", summary: "检查 Redis 连接", arguments: [] },
    ];
    const history: CommandHistoryEntry[] = [];
    const historyInput: SaveCommandHistoryInput = {
      connection_id: "local",
      entries: history,
    };
    invokeMock
      .mockResolvedValueOnce(batchResults)
      .mockResolvedValueOnce(catalog)
      .mockResolvedValueOnce(history)
      .mockResolvedValueOnce(undefined);

    await expect(executeCommands(batchInput)).resolves.toEqual(batchResults);
    expect(invokeMock).toHaveBeenLastCalledWith("execute_commands", { input: batchInput });

    await expect(getCommandCatalog()).resolves.toEqual(catalog);
    expect(invokeMock).toHaveBeenLastCalledWith("get_command_catalog");

    await expect(listCommandHistory("local")).resolves.toEqual(history);
    expect(invokeMock).toHaveBeenLastCalledWith("list_command_history", {
      connection_id: "local",
    });

    await expect(saveCommandHistory(historyInput)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("save_command_history", {
      input: historyInput,
    });
  });

  it("为 Database 概览和数据库切换使用稳定 IPC 合同", async () => {
    const instance: InstanceOverview = {
      server_version: "7.2.5",
      redis_mode: "standalone",
      uptime_seconds: 42,
      connected_clients: 3,
      used_memory_bytes: 1024,
      max_memory_bytes: null,
      total_commands_processed: 9,
      keyspace_hits: 4,
      keyspace_misses: 1,
      role: "master",
      modules: [],
    };
    const databases: DatabaseOverview[] = [
      { database: 0, key_count: 8, expires: 2, avg_ttl_ms: 1200 },
    ];
    invokeMock
      .mockResolvedValueOnce(instance)
      .mockResolvedValueOnce(databases)
      .mockResolvedValueOnce(profile);

    await expect(getInstanceOverview("local")).resolves.toEqual(instance);
    expect(invokeMock).toHaveBeenLastCalledWith("get_instance_overview", {
      connection_id: "local",
    });
    await expect(getDatabaseOverview("local")).resolves.toEqual(databases);
    expect(invokeMock).toHaveBeenLastCalledWith("get_database_overview", {
      connection_id: "local",
    });
    await expect(
      selectDatabase({ connection_id: "local", database: 0 }),
    ).resolves.toEqual(profile);
    expect(invokeMock).toHaveBeenLastCalledWith("select_database", {
      input: { connection_id: "local", database: 0 },
    });
  });

  it("为 Query Library 和 Settings 使用稳定的本地资源 IPC 合同", async () => {
    const item: QueryLibraryItem = {
      id: "query-1",
      name: "读取用户",
      command: "GET user:1",
      tags: ["用户"],
      updated_at: 1,
    };
    const itemInput: QueryLibraryItemInput = {
      id: null,
      name: item.name,
      command: item.command,
      tags: item.tags,
    };
    const settings: AppSettings = {
      version: 1,
      theme: "dark",
      result_format: "json",
      scan_count: 200,
      continue_on_error: true,
    };
    invokeMock
      .mockResolvedValueOnce([item])
      .mockResolvedValueOnce(item)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(settings)
      .mockResolvedValueOnce(settings);

    await expect(listQueryLibrary()).resolves.toEqual([item]);
    expect(invokeMock).toHaveBeenLastCalledWith("list_query_library");
    await expect(saveQueryLibraryItem(itemInput)).resolves.toEqual(item);
    expect(invokeMock).toHaveBeenLastCalledWith("save_query_library_item", {
      input: itemInput,
    });
    await expect(deleteQueryLibraryItem(item.id)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("delete_query_library_item", {
      id: item.id,
    });
    await expect(getAppSettings()).resolves.toEqual(settings);
    expect(invokeMock).toHaveBeenLastCalledWith("get_app_settings");
    await expect(saveAppSettings(settings)).resolves.toEqual(settings);
    expect(invokeMock).toHaveBeenLastCalledWith("save_app_settings", { settings });
  });

  it("为 Slow Log 和 Pub/Sub 使用稳定的运维 IPC 合同", async () => {
    const config: SlowLogConfig = {
      slowlog_max_len: 128,
      slowlog_log_slower_than: 10_000,
    };
    const entry: SlowLogEntry = {
      id: 7,
      time: 1_710_000_000,
      duration_us: 2_500,
      args: ["SET", "demo", "hello"],
      source: "127.0.0.1:6379",
      client: null,
    };
    const session: PubSubSession = {
      connection_id: "local",
      session_id: "session-1",
      topics: [{ name: "events", pattern: false }],
    };
    invokeMock
      .mockResolvedValueOnce([entry])
      .mockResolvedValueOnce(config)
      .mockResolvedValueOnce(config)
      .mockResolvedValueOnce(session)
      .mockResolvedValueOnce({
        connection_id: "local",
        session_id: "profiler-1",
      } satisfies ProfilerSession)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(1)
      .mockResolvedValueOnce(undefined);

    const slowLogInput = { connection_id: "local", count: 50 };
    await expect(getSlowLogs(slowLogInput)).resolves.toEqual([entry]);
    expect(invokeMock).toHaveBeenLastCalledWith("get_slow_logs", {
      input: slowLogInput,
    });
    await expect(getSlowLogConfig("local")).resolves.toEqual(config);
    expect(invokeMock).toHaveBeenLastCalledWith("get_slow_log_config", {
      connection_id: "local",
    });
    const configInput = {
      connection_id: "local",
      slowlog_max_len: 128,
      slowlog_log_slower_than: 10_000,
    };
    await expect(updateSlowLogConfig(configInput)).resolves.toEqual(config);
    expect(invokeMock).toHaveBeenLastCalledWith("update_slow_log_config", {
      input: configInput,
    });
    await expect(startPubSub({
      connection_id: "local",
      session_id: "session-1",
      topics: [{ name: "events", pattern: false }],
    })).resolves.toEqual(session);
    expect(invokeMock).toHaveBeenLastCalledWith("start_pub_sub", {
      input: {
        connection_id: "local",
        session_id: "session-1",
        topics: [{ name: "events", pattern: false }],
      },
    });
    const profilerInput = {
      connection_id: "local",
      session_id: "profiler-1",
    };
    await expect(startProfiler(profilerInput)).resolves.toEqual({
      connection_id: "local",
      session_id: "profiler-1",
    });
    expect(invokeMock).toHaveBeenLastCalledWith("start_profiler", {
      input: profilerInput,
    });
    await expect(stopProfiler(profilerInput)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("stop_profiler", {
      input: profilerInput,
    });
    await expect(stopPubSub({
      connection_id: "local",
      session_id: "session-1",
    })).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("stop_pub_sub", {
      input: { connection_id: "local", session_id: "session-1" },
    });
    await expect(publishPubSub({
      connection_id: "local",
      channel: "events",
      message: "hello",
    })).resolves.toBe(1);
    expect(invokeMock).toHaveBeenLastCalledWith("publish_pub_sub", {
      input: { connection_id: "local", channel: "events", message: "hello" },
    });
    await expect(clearSlowLogs("local")).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("clear_slow_logs", {
      connection_id: "local",
    });
  });

  it("为 Browser 扩展命令使用稳定命令名和 input 包装", async () => {
    const keyValue: KeyValue = {
      key: "demo",
      key_type: "string",
      ttl_ms: -1,
      value: stringValue,
    };
    const keyInfo = {
      key: "demo",
      key_type: "string",
      ttl_ms: -1,
      size: 5,
      memory_bytes: 64,
      encoding: "embstr",
      idle_seconds: 2,
    };
    invokeMock
      .mockResolvedValueOnce(keyValue)
      .mockResolvedValueOnce(keyValue)
      .mockResolvedValueOnce(2)
      .mockResolvedValueOnce(keyInfo)
      .mockResolvedValueOnce([
        { key: "demo:renamed", ttl_ms: -1, value: stringValue },
      ])
      .mockResolvedValueOnce(2);

    const createInput = {
      connection_id: "local",
      key: "demo",
      value: stringValue,
      ttl_ms: null,
    };
    await expect(createKey(createInput)).resolves.toEqual(keyValue);
    expect(invokeMock).toHaveBeenLastCalledWith("create_key", { input: createInput });

    const renameInput = {
      connection_id: "local",
      key: "demo",
      new_key: "demo:renamed",
    };
    await expect(renameKey(renameInput)).resolves.toEqual(keyValue);
    expect(invokeMock).toHaveBeenLastCalledWith("rename_key", { input: renameInput });

    const deleteInput = {
      connection_id: "local",
      keys: ["demo:renamed", "other"],
    };
    await expect(deleteKeys(deleteInput)).resolves.toBe(2);
    expect(invokeMock).toHaveBeenLastCalledWith("delete_keys", { input: deleteInput });

    const infoInput = { connection_id: "local", key: "demo:renamed" };
    await expect(getKeyInfo(infoInput)).resolves.toEqual(keyInfo);
    expect(invokeMock).toHaveBeenLastCalledWith("get_key_info", { input: infoInput });

    const exportInput = { connection_id: "local", keys: ["demo:renamed"] };
    const exported: ExportedKey[] = [
      { key: "demo:renamed", ttl_ms: -1, value: stringValue },
    ];
    await expect(exportKeys(exportInput)).resolves.toEqual(exported);
    expect(invokeMock).toHaveBeenLastCalledWith("export_keys", { input: exportInput });

    const importInput = { connection_id: "local", entries: exported };
    await expect(importKeys(importInput)).resolves.toBe(2);
    expect(invokeMock).toHaveBeenLastCalledWith("import_keys", { input: importInput });
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
