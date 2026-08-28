import { invoke } from "@tauri-apps/api/core";

import type {
  AggregateArrayInput,
  AppendArrayInput,
  ArrayKeyInput,
  ArrayMultiGetInput,
  ArrayMutationResult,
  ArrayRange,
  ArrayRangeInput,
  ArrayScan,
  ArrayScanInput,
  ArraySearchResult,
  ArraySummary,
  CreateArrayInput,
  AppendJsonArrayInput,
  CommandDefinition,
  CommandExecutionItem,
  CommandHistoryEntry,
  CommandResult,
  AcknowledgeStreamPendingEntriesInput,
  AnalyzeDatabaseInput,
  AppSettings,
  ConnectionInfo,
  ConnectionProfile,
  ConnectionExportDocument,
  CreateSearchIndexInput,
  CreateKeyInput,
  CreateStreamConsumerGroupInput,
  DatabaseOverview,
  DatabaseAnalysisReport,
  DeleteKeysInput,
  DeleteKeyInput,
  DeleteArrayElementsInput,
  DeleteArrayRangeInput,
  DeleteStreamConsumerGroupInput,
  DeleteStreamConsumerInput,
  ExecuteCommandsInput,
  ExecuteCommandInput,
  ExportedKey,
  ExportKeysInput,
  GetSlowLogsInput,
  GetKeyInput,
  GetKeySearchIndexesInput,
  JsonMutationResult,
  JsonPathInput,
  JsonPathValue,
  GetStreamConsumerGroupsInput,
  GetStreamConsumersInput,
  GetStreamPendingEntriesInput,
  IpcError,
  ImportKeysInput,
  ImportConnectionsInput,
  ImportConnectionsResult,
  InstanceOverview,
  InstanceDetails,
  KeyInfo,
  KeyInfoInput,
  KeyValue,
  ModuleCapabilities,
  ListSearchIndexesResult,
  KeySearchIndexSummary,
  SearchIndexInfo,
  SearchIndexInput,
  SearchQueryInput,
  SearchQueryResult,
  SearchArrayInput,
  RenameKeyInput,
  SaveConnectionInput,
  SaveCommandHistoryInput,
  QueryLibraryItem,
  QueryLibraryItemInput,
  PublishPubSubInput,
  PubSubSession,
  ProfilerSession,
  ScanKeysInput,
  ScanPage,
  SelectDatabaseInput,
  SetJsonPathInput,
  SetKeyInput,
  SetKeyTtlInput,
  SetArrayElementInput,
  SlowLogConfig,
  SlowLogEntry,
  StartPubSubInput,
  StartProfilerInput,
  StopProfilerInput,
  StopPubSubInput,
  StreamConsumer,
  StreamConsumerGroup,
  StreamPendingEntry,
  UpdateSlowLogConfigInput,
} from "./types";

const IPC_ERROR: IpcError = {
  code: "IPC_ERROR",
  message: "IPC 调用失败",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function normalizeError(error: unknown): IpcError {
  if (
    isRecord(error) &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return {
      code: error.code,
      message: error.message,
    };
  }

  return { ...IPC_ERROR };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return args === undefined ? await invoke<T>(command) : await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export function listConnections(): Promise<ConnectionProfile[]> {
  return call<ConnectionProfile[]>("list_connections");
}

export function saveConnection(
  input: SaveConnectionInput,
): Promise<ConnectionProfile> {
  return call<ConnectionProfile>("save_connection", { input });
}

export function deleteConnection(connectionId: string): Promise<void> {
  return call<void>("delete_connection", { connection_id: connectionId });
}

export function exportConnections(): Promise<ConnectionExportDocument> {
  return call<ConnectionExportDocument>("export_connections");
}

export function importConnections(
  input: ImportConnectionsInput,
): Promise<ImportConnectionsResult> {
  return call<ImportConnectionsResult>("import_connections", { input });
}

export function testConnection(input: SaveConnectionInput): Promise<ConnectionInfo> {
  return call<ConnectionInfo>("test_connection", { input });
}

export function openConnection(connectionId: string): Promise<ConnectionInfo> {
  return call<ConnectionInfo>("open_connection", { connection_id: connectionId });
}

export function closeConnection(connectionId: string): Promise<void> {
  return call<void>("close_connection", { connection_id: connectionId });
}

export function scanKeys(input: ScanKeysInput): Promise<ScanPage> {
  return call<ScanPage>("scan_keys", { input });
}

export function exportKeys(input: ExportKeysInput): Promise<ExportedKey[]> {
  return call<ExportedKey[]>("export_keys", { input });
}

export function importKeys(input: ImportKeysInput): Promise<number> {
  return call<number>("import_keys", { input });
}

export function getInstanceOverview(connectionId: string): Promise<InstanceOverview> {
  return call<InstanceOverview>("get_instance_overview", {
    connection_id: connectionId,
  });
}

export function getInstanceDetails(connectionId: string): Promise<InstanceDetails> {
  return call<InstanceDetails>("get_instance_details", {
    connection_id: connectionId,
  });
}

export function getModuleCapabilities(
  connectionId: string,
): Promise<ModuleCapabilities> {
  return call<ModuleCapabilities>("get_module_capabilities", {
    connection_id: connectionId,
  });
}

export function listSearchIndexes(
  connectionId: string,
): Promise<ListSearchIndexesResult> {
  return call<ListSearchIndexesResult>("list_search_indexes", {
    connection_id: connectionId,
  });
}

export function createSearchIndex(input: CreateSearchIndexInput): Promise<void> {
  return call<void>("create_search_index", { input });
}

export function getSearchIndex(input: SearchIndexInput): Promise<SearchIndexInfo> {
  return call<SearchIndexInfo>("get_search_index", { input });
}

export function deleteSearchIndex(input: SearchIndexInput): Promise<void> {
  return call<void>("delete_search_index", { input });
}

export function searchKeys(input: SearchQueryInput): Promise<SearchQueryResult> {
  return call<SearchQueryResult>("search_keys", { input });
}

export function getKeySearchIndexes(
  input: GetKeySearchIndexesInput,
): Promise<KeySearchIndexSummary[]> {
  return call<KeySearchIndexSummary[]>("get_key_search_indexes", { input });
}

export function analyzeDatabase(
  input: AnalyzeDatabaseInput,
): Promise<DatabaseAnalysisReport> {
  return call<DatabaseAnalysisReport>("analyze_database", { input });
}

export function getDatabaseOverview(connectionId: string): Promise<DatabaseOverview[]> {
  return call<DatabaseOverview[]>("get_database_overview", {
    connection_id: connectionId,
  });
}

export function selectDatabase(input: SelectDatabaseInput): Promise<ConnectionProfile> {
  return call<ConnectionProfile>("select_database", { input });
}

export function getSlowLogs(input: GetSlowLogsInput): Promise<SlowLogEntry[]> {
  return call<SlowLogEntry[]>("get_slow_logs", { input });
}

export function clearSlowLogs(connectionId: string): Promise<void> {
  return call<void>("clear_slow_logs", { connection_id: connectionId });
}

export function getSlowLogConfig(connectionId: string): Promise<SlowLogConfig> {
  return call<SlowLogConfig>("get_slow_log_config", {
    connection_id: connectionId,
  });
}

export function updateSlowLogConfig(
  input: UpdateSlowLogConfigInput,
): Promise<SlowLogConfig> {
  return call<SlowLogConfig>("update_slow_log_config", { input });
}

export function startPubSub(input: StartPubSubInput): Promise<PubSubSession> {
  return call<PubSubSession>("start_pub_sub", { input });
}

export function stopPubSub(input: StopPubSubInput): Promise<void> {
  return call<void>("stop_pub_sub", { input });
}

export function publishPubSub(input: PublishPubSubInput): Promise<number> {
  return call<number>("publish_pub_sub", { input });
}

export function startProfiler(input: StartProfilerInput): Promise<ProfilerSession> {
  return call<ProfilerSession>("start_profiler", { input });
}

export function stopProfiler(input: StopProfilerInput): Promise<void> {
  return call<void>("stop_profiler", { input });
}

export function listQueryLibrary(): Promise<QueryLibraryItem[]> {
  return call<QueryLibraryItem[]>("list_query_library");
}

export function saveQueryLibraryItem(
  input: QueryLibraryItemInput,
): Promise<QueryLibraryItem> {
  return call<QueryLibraryItem>("save_query_library_item", { input });
}

export function deleteQueryLibraryItem(id: string): Promise<void> {
  return call<void>("delete_query_library_item", { id });
}

export function getAppSettings(): Promise<AppSettings> {
  return call<AppSettings>("get_app_settings");
}

export function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return call<AppSettings>("save_app_settings", { settings });
}

export function getKey(input: GetKeyInput): Promise<KeyValue> {
  return call<KeyValue>("get_key", { input });
}

export function createArray(input: CreateArrayInput): Promise<KeyValue> {
  return call<KeyValue>("create_array", { input });
}

export function getArraySummary(input: ArrayKeyInput): Promise<ArraySummary> {
  return call<ArraySummary>("get_array_summary", { input });
}

export function getArrayRange(input: ArrayRangeInput): Promise<ArrayRange> {
  return call<ArrayRange>("get_array_range", { input });
}

export function scanArray(input: ArrayScanInput): Promise<ArrayScan> {
  return call<ArrayScan>("scan_array", { input });
}

export function getArrayElements(input: ArrayMultiGetInput): Promise<Array<(string | null)>> {
  return call<Array<(string | null)>>("get_array_elements", { input });
}

export function setArrayElement(input: SetArrayElementInput): Promise<ArrayMutationResult> {
  return call<ArrayMutationResult>("set_array_element", { input });
}

export function appendArrayElements(input: AppendArrayInput): Promise<ArrayMutationResult> {
  return call<ArrayMutationResult>("append_array_elements", { input });
}

export function deleteArrayElements(
  input: DeleteArrayElementsInput,
): Promise<ArrayMutationResult> {
  return call<ArrayMutationResult>("delete_array_elements", { input });
}

export function deleteArrayRange(input: DeleteArrayRangeInput): Promise<ArrayMutationResult> {
  return call<ArrayMutationResult>("delete_array_range", { input });
}

export function searchArray(input: SearchArrayInput): Promise<ArraySearchResult> {
  return call<ArraySearchResult>("search_array", { input });
}

export function aggregateArray(input: AggregateArrayInput): Promise<import("./types").ArrayAggregateResult> {
  return call<import("./types").ArrayAggregateResult>("aggregate_array", { input });
}

export function getJsonPath(input: JsonPathInput): Promise<JsonPathValue> {
  return call<JsonPathValue>("get_json_path", { input });
}

export function setKey(input: SetKeyInput): Promise<KeyValue> {
  return call<KeyValue>("set_key", { input });
}

export function setJsonPath(
  input: SetJsonPathInput,
): Promise<JsonMutationResult> {
  return call<JsonMutationResult>("set_json_path", { input });
}

export function createKey(input: CreateKeyInput): Promise<KeyValue> {
  return call<KeyValue>("create_key", { input });
}

export function renameKey(input: RenameKeyInput): Promise<KeyValue> {
  return call<KeyValue>("rename_key", { input });
}

export function deleteKey(input: DeleteKeyInput): Promise<void> {
  return call<void>("delete_key", { input });
}

export function deleteKeys(input: DeleteKeysInput): Promise<number> {
  return call<number>("delete_keys", { input });
}

export function setKeyTtl(input: SetKeyTtlInput): Promise<number> {
  return call<number>("set_key_ttl", { input });
}

export function appendJsonArray(
  input: AppendJsonArrayInput,
): Promise<JsonMutationResult> {
  return call<JsonMutationResult>("append_json_array", { input });
}

export function deleteJsonPath(
  input: JsonPathInput,
): Promise<JsonMutationResult> {
  return call<JsonMutationResult>("delete_json_path", { input });
}

export function getKeyInfo(input: KeyInfoInput): Promise<KeyInfo> {
  return call<KeyInfo>("get_key_info", { input });
}

export function getStreamConsumerGroups(
  input: GetStreamConsumerGroupsInput,
): Promise<StreamConsumerGroup[]> {
  return call<StreamConsumerGroup[]>("get_stream_consumer_groups", { input });
}

export function createStreamConsumerGroup(
  input: CreateStreamConsumerGroupInput,
): Promise<void> {
  return call<void>("create_stream_consumer_group", { input });
}

export function deleteStreamConsumerGroup(
  input: DeleteStreamConsumerGroupInput,
): Promise<number> {
  return call<number>("delete_stream_consumer_group", { input });
}

export function getStreamConsumers(input: GetStreamConsumersInput): Promise<StreamConsumer[]> {
  return call<StreamConsumer[]>("get_stream_consumers", { input });
}

export function getStreamPendingEntries(
  input: GetStreamPendingEntriesInput,
): Promise<StreamPendingEntry[]> {
  return call<StreamPendingEntry[]>("get_stream_pending_entries", { input });
}

export function acknowledgeStreamPendingEntries(
  input: AcknowledgeStreamPendingEntriesInput,
): Promise<number> {
  return call<number>("acknowledge_stream_pending_entries", { input });
}

export function deleteStreamConsumer(input: DeleteStreamConsumerInput): Promise<number> {
  return call<number>("delete_stream_consumer", { input });
}

export function executeCommand(input: ExecuteCommandInput): Promise<CommandResult> {
  return call<CommandResult>("execute_command", { input });
}

export function executeCommands(
  input: ExecuteCommandsInput,
): Promise<CommandExecutionItem[]> {
  return call<CommandExecutionItem[]>("execute_commands", { input });
}

export function getCommandCatalog(): Promise<CommandDefinition[]> {
  return call<CommandDefinition[]>("get_command_catalog");
}

export function listCommandHistory(connectionId: string): Promise<CommandHistoryEntry[]> {
  return call<CommandHistoryEntry[]>("list_command_history", {
    connection_id: connectionId,
  });
}

export function saveCommandHistory(input: SaveCommandHistoryInput): Promise<void> {
  return call<void>("save_command_history", { input });
}
