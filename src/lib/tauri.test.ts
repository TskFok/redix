import * as core from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearSlowLogs,
  acknowledgeStreamPendingEntries,
  aggregateArray,
  appendArrayElements,
  appendJsonArray,
  closeConnection,
  createSearchIndex,
  createArray,
  createKey,
  createStreamConsumerGroup,
  deleteKeys,
  deleteConnection,
  deleteArrayElements,
  deleteArrayRange,
  deleteKey,
  deleteJsonPath,
  deleteSearchIndex,
  deleteStreamConsumer,
  exportConnections,
  deleteStreamConsumerGroup,
  executeCommands,
  executeCommand,
  exportKeys,
  getJsonPath,
  getArrayRange,
  getModuleCapabilities,
  getSlowLogConfig,
  getSlowLogs,
  getDatabaseOverview,
  getInstanceDetails,
  getCommandCatalog,
  getKey,
  getBrowserKey,
  getArraySummary,
  getArrayElements,
  getKeyInfo,
  getStreamConsumerGroups,
  getStreamConsumers,
  getStreamPendingEntries,
  getInstanceOverview,
  getKeySearchIndexes,
  getSearchIndex,
  analyzeDatabase,
  getAppSettings,
  importKeys,
  importConnections,
  listQueryLibrary,
  listCommandHistory,
  listConnections,
  listSearchIndexes,
  openConnection,
  publishPubSub,
  startProfiler,
  renameKey,
  renameBrowserKey,
  saveConnection,
  scanKeys,
  saveCommandHistory,
  saveAppSettings,
  saveQueryLibraryItem,
  searchKeys,
  selectDatabase,
  setJsonPath,
  startPubSub,
  stopPubSub,
  stopProfiler,
  setKey,
  setArrayElement,
  setKeyTtl,
  scanArray,
  searchArray,
  searchVectorSet,
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
  ConnectionExportDocument,
  ExportedKey,
  JsonMutationResult,
  JsonPathValue,
  KeyValue,
  ModuleCapabilities,
  CreateSearchIndexInput,
  KeySearchIndexSummary,
  ListSearchIndexesResult,
  SearchIndexInfo,
  SearchQueryResult,
  RedisValue,
  SaveConnectionInput,
  ImportConnectionsResult,
  SaveCommandHistoryInput,
  ScanPage,
  SlowLogConfig,
  SlowLogEntry,
  StreamConsumer,
  StreamConsumerGroup,
  StreamPendingEntry,
} from "./types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(core.invoke);

const profile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
  tls: false,
  verify_server_cert: true,
  ca_certificate_name: null,
  client_certificate_name: null,
  has_ca_certificate: false,
  has_client_certificate: false,
};

const connectionInput: SaveConnectionInput = {
  profile,
  password: null,
  ca_certificate: null,
  client_certificate: null,
  client_key: null,
  clear_ca_certificate: false,
  clear_client_certificate: false,
};

const stringValue: RedisValue = { String: { value: "value" } };

beforeEach(() => {
  invokeMock.mockReset();
});

describe("Tauri IPC bridge", () => {
  it("把 Array 查询作为 typed input 传给 Tauri", async () => {
    const input = {
      connection_id: "local",
      key: "events",
      start: "0",
      end: "99",
    };
    const result = {
      cells: [],
      start: "0",
      end: "99",
      has_more: false,
    };
    invokeMock.mockResolvedValue(result);

    await expect(getArrayRange(input)).resolves.toEqual(result);
    expect(invokeMock).toHaveBeenLastCalledWith("get_array_range", { input });
  });

  it("为 VSIM 保留三种输入之一和属性开关", async () => {
    const input = {
      connection_id: "local",
      key: "embeddings",
      by_element: "seed",
      by_vector: null,
      by_vector_base64: null,
      count: 10,
      with_attributes: true,
    };
    const result = { matches: [], has_more: false };
    invokeMock.mockResolvedValue(result);

    await expect(searchVectorSet(input)).resolves.toEqual(result);
    expect(invokeMock).toHaveBeenLastCalledWith("search_vector_set", { input });
  });

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

  it("为连接配置导入导出使用 typed IPC 合同", async () => {
    const document: ConnectionExportDocument = { version: 1, connections: [] };
    const result: ImportConnectionsResult = {
      imported: [],
      failed: [],
      ignored_secret_fields: 0,
    };
    invokeMock.mockResolvedValueOnce(document).mockResolvedValueOnce(result);

    await expect(exportConnections()).resolves.toEqual(document);
    expect(invokeMock).toHaveBeenLastCalledWith("export_connections");

    await expect(importConnections({ content: "{}" })).resolves.toEqual(result);
    expect(invokeMock).toHaveBeenLastCalledWith("import_connections", {
      input: { content: "{}" },
    });
  });

  it("为 RedisJSON 模块与路径操作传递稳定 IPC 合同", async () => {
    const capabilities: ModuleCapabilities = {
      modules: [{ name: "ReJSON", version: "2.8.4" }],
      json_supported: true,
      json_version: "2.8.4",
      search_supported: false,
      search_version: null,
      array_supported: false,
      vector_set_supported: false,
    };
    const pathValue: JsonPathValue = {
      key: "doc",
      path: "$.user",
      found: true,
      value: { name: "redix" },
      ttl_ms: -1,
    };
    const mutation: JsonMutationResult = {
      key: "doc",
      path: "$.items",
      affected: 1,
      new_length: 3,
      ttl_ms: -1,
    };
    invokeMock
      .mockResolvedValueOnce(capabilities)
      .mockResolvedValueOnce(pathValue)
      .mockResolvedValueOnce(mutation)
      .mockResolvedValueOnce(mutation)
      .mockResolvedValueOnce(mutation);

    await expect(getModuleCapabilities("local")).resolves.toEqual(capabilities);
    expect(invokeMock).toHaveBeenLastCalledWith("get_module_capabilities", {
      connection_id: "local",
    });

    const getInput = { connection_id: "local", key: "doc", path: "$.user" };
    await expect(getJsonPath(getInput)).resolves.toEqual(pathValue);
    expect(invokeMock).toHaveBeenLastCalledWith("get_json_path", {
      input: getInput,
    });

    const setInput = {
      connection_id: "local",
      key: "doc",
      path: "$.user",
      value: { name: "redix" },
    };
    await expect(setJsonPath(setInput)).resolves.toEqual(mutation);
    expect(invokeMock).toHaveBeenLastCalledWith("set_json_path", {
      input: setInput,
    });

    const appendInput = {
      connection_id: "local",
      key: "doc",
      path: "$.items",
      values: ["a", "b"],
    };
    await expect(appendJsonArray(appendInput)).resolves.toEqual(mutation);
    expect(invokeMock).toHaveBeenLastCalledWith("append_json_array", {
      input: appendInput,
    });

    const deleteInput = { connection_id: "local", key: "doc", path: "$.items[0]" };
    await expect(deleteJsonPath(deleteInput)).resolves.toEqual(mutation);
    expect(invokeMock).toHaveBeenLastCalledWith("delete_json_path", {
      input: deleteInput,
    });
  });

  it("为 RedisSearch 使用稳定 IPC 合同", async () => {
    const indexes: ListSearchIndexesResult = {
      indexes: [{ name: "idx:users" }],
    };
    const created: CreateSearchIndexInput = {
      connection_id: "local",
      index: "idx:users",
      key_type: "hash",
      prefixes: ["user:"],
      fields: [{ name: "name", field_type: "text" }],
    };
    const indexInput = { connection_id: "local", index: "idx:users" };
    const info: SearchIndexInfo = {
      index_name: "idx:users",
      key_type: "HASH",
      prefixes: ["user:"],
      attributes: [],
      num_docs: 1,
      num_terms: null,
      num_records: null,
      total_index_memory_bytes: null,
    };
    const query: SearchQueryResult = {
      total: 1,
      offset: 0,
      next_offset: null,
      max_results: 100,
      keys: [{ key: "user:1", key_type: "hash" }],
    };
    const keyIndexes: KeySearchIndexSummary[] = [
      { name: "idx:users", key_type: "HASH", prefixes: ["user:"] },
    ];

    invokeMock
      .mockResolvedValueOnce(indexes)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(info)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(query)
      .mockResolvedValueOnce(keyIndexes);

    await expect(listSearchIndexes("local")).resolves.toEqual(indexes);
    expect(invokeMock).toHaveBeenLastCalledWith("list_search_indexes", {
      connection_id: "local",
    });

    await expect(createSearchIndex(created)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("create_search_index", {
      input: created,
    });

    await expect(getSearchIndex(indexInput)).resolves.toEqual(info);
    expect(invokeMock).toHaveBeenLastCalledWith("get_search_index", {
      input: indexInput,
    });

    await expect(deleteSearchIndex(indexInput)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("delete_search_index", {
      input: indexInput,
    });

    const queryInput = {
      connection_id: "local",
      index: "idx:users",
      query: "@name:Alice",
      offset: 0,
      limit: 100,
    };
    await expect(searchKeys(queryInput)).resolves.toEqual(query);
    expect(invokeMock).toHaveBeenLastCalledWith("search_keys", {
      input: queryInput,
    });

    const keyInput = { connection_id: "local", key: "user:1" };
    await expect(getKeySearchIndexes(keyInput)).resolves.toEqual(keyIndexes);
    expect(invokeMock).toHaveBeenLastCalledWith("get_key_search_indexes", {
      input: keyInput,
    });
  });

  it("为 Browser 有界预览和重命名调用独立 typed command", async () => {
    const value: KeyValue = {
      key: "large", key_type: "hash", ttl_ms: -1, value: { Hash: { fields: [] } },
    };
    const input = { connection_id: "local", key: "large" };
    const renameInput = { ...input, new_key: "renamed" };
    invokeMock.mockResolvedValueOnce(value).mockResolvedValueOnce({ ...value, key: "renamed" });
    await expect(getBrowserKey(input)).resolves.toEqual(value);
    expect(invokeMock).toHaveBeenLastCalledWith("get_browser_key", { input });
    await expect(renameBrowserKey(renameInput)).resolves.toEqual({ ...value, key: "renamed" });
    expect(invokeMock).toHaveBeenLastCalledWith("rename_browser_key", { input: renameInput });
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

  it("使用 snake_case 参数调用实例详情和数据库分析 command", async () => {
    const invoke = vi.mocked(core.invoke);
    invoke.mockResolvedValueOnce({ overview: null }).mockResolvedValueOnce({ progress: {} });

    await getInstanceDetails("local");
    await analyzeDatabase({
      connection_id: "local",
      pattern: "user:*",
      delimiter: ":",
      max_keys: 1000,
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "get_instance_details", {
      connection_id: "local",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "analyze_database", {
      input: {
        connection_id: "local",
        pattern: "user:*",
        delimiter: ":",
        max_keys: 1000,
      },
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

  it("为 Stream Consumer Group 命令保持 snake_case 和 input 包装", async () => {
    const group: StreamConsumerGroup = {
      name: "workers",
      consumers: 1,
      pending: 2,
      last_delivered_id: "2-0",
    };
    const consumer: StreamConsumer = {
      name: "consumer-1",
      pending: 2,
      idle_ms: 10,
    };
    const pending: StreamPendingEntry = {
      id: "1-0",
      consumer: "consumer-1",
      idle_ms: 10,
      deliveries: 1,
    };
    invokeMock
      .mockResolvedValueOnce([group])
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(1)
      .mockResolvedValueOnce([consumer])
      .mockResolvedValueOnce([pending])
      .mockResolvedValueOnce(1)
      .mockResolvedValueOnce(0);

    const groupQuery = { connection_id: "local", key: "events" };
    await expect(getStreamConsumerGroups(groupQuery)).resolves.toEqual([group]);
    expect(invokeMock).toHaveBeenLastCalledWith("get_stream_consumer_groups", {
      input: groupQuery,
    });

    const createInput = {
      connection_id: "local",
      key: "events",
      name: "workers",
      last_delivered_id: "0-0",
    };
    await expect(createStreamConsumerGroup(createInput)).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("create_stream_consumer_group", {
      input: createInput,
    });

    const deleteGroupInput = { connection_id: "local", key: "events", name: "workers" };
    await expect(deleteStreamConsumerGroup(deleteGroupInput)).resolves.toBe(1);
    expect(invokeMock).toHaveBeenLastCalledWith("delete_stream_consumer_group", {
      input: deleteGroupInput,
    });

    const consumerQuery = { ...groupQuery, group: "workers" };
    await expect(getStreamConsumers(consumerQuery)).resolves.toEqual([consumer]);
    expect(invokeMock).toHaveBeenLastCalledWith("get_stream_consumers", {
      input: consumerQuery,
    });

    const pendingQuery = { ...consumerQuery, count: 100, consumer: null };
    await expect(getStreamPendingEntries(pendingQuery)).resolves.toEqual([pending]);
    expect(invokeMock).toHaveBeenLastCalledWith("get_stream_pending_entries", {
      input: pendingQuery,
    });

    const acknowledgeInput = {
      ...consumerQuery,
      entries: [pending.id],
    };
    await expect(acknowledgeStreamPendingEntries(acknowledgeInput)).resolves.toBe(1);
    expect(invokeMock).toHaveBeenLastCalledWith("acknowledge_stream_pending_entries", {
      input: acknowledgeInput,
    });

    const deleteConsumerInput = { ...consumerQuery, consumer: consumer.name };
    await expect(deleteStreamConsumer(deleteConsumerInput)).resolves.toBe(0);
    expect(invokeMock).toHaveBeenLastCalledWith("delete_stream_consumer", {
      input: deleteConsumerInput,
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
