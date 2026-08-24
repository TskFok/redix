export type Workspace =
  | "browser"
  | "workbench"
  | "database"
  | "observability"
  | "query-library"
  | "settings";

export interface ConnectionProfile {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  has_password: boolean;
}

export interface SaveConnectionInput {
  profile: ConnectionProfile;
  password: string | null;
}

export interface ConnectionInfo {
  server_version: string;
}

export interface ModuleSummary {
  name: string;
  version: string | null;
}

export interface InstanceOverview {
  server_version: string | null;
  redis_mode: string | null;
  uptime_seconds: number | null;
  connected_clients: number | null;
  used_memory_bytes: number | null;
  max_memory_bytes: number | null;
  total_commands_processed: number | null;
  keyspace_hits: number | null;
  keyspace_misses: number | null;
  role: string | null;
  modules: ModuleSummary[];
}

export interface DatabaseOverview {
  database: number;
  key_count: number | null;
  expires: number | null;
  avg_ttl_ms: number | null;
}

export interface SlowLogEntry {
  id: number;
  time: number;
  duration_us: number;
  args: string[];
  source: string;
  client: string | null;
}

export interface SlowLogConfig {
  slowlog_max_len: number;
  slowlog_log_slower_than: number;
}

export interface GetSlowLogsInput {
  connection_id: string;
  count: number;
}

export interface UpdateSlowLogConfigInput {
  connection_id: string;
  slowlog_max_len: number | null;
  slowlog_log_slower_than: number | null;
}

export interface PubSubTopic {
  name: string;
  pattern: boolean;
}

export interface StartPubSubInput {
  connection_id: string;
  session_id: string;
  topics: PubSubTopic[];
}

export interface PubSubSession {
  connection_id: string;
  session_id: string;
  topics: PubSubTopic[];
}

export interface StopPubSubInput {
  connection_id: string;
  session_id: string;
}

export interface PublishPubSubInput {
  connection_id: string;
  channel: string;
  message: string;
}

export interface PubSubMessageEvent {
  connection_id: string;
  session_id: string;
  channel: string;
  pattern: string | null;
  message: string;
  received_at_ms: number;
}

export interface PubSubStatusEvent {
  connection_id: string;
  session_id: string;
  state: string;
  error_code: string | null;
}

export interface StartProfilerInput {
  connection_id: string;
  session_id: string;
}

export interface StopProfilerInput {
  connection_id: string;
  session_id: string;
}

export interface ProfilerSession {
  connection_id: string;
  session_id: string;
}

export interface ProfilerEvent {
  connection_id: string;
  session_id: string;
  time: string;
  database: number;
  source: string;
  args: string[];
  received_at_ms: number;
}

export interface ProfilerStatusEvent {
  connection_id: string;
  session_id: string;
  state: string;
  error_code: string | null;
}

export interface SelectDatabaseInput {
  connection_id: string;
  database: number;
}

export interface QueryLibraryItem {
  id: string;
  name: string;
  command: string;
  tags: string[];
  updated_at: number;
}

export interface QueryLibraryItemInput {
  id: string | null;
  name: string;
  command: string;
  tags: string[];
}

export type ThemePreference = "system" | "light" | "dark";
export type CommandDisplayFormat = "raw" | "text" | "json";

export interface AppSettings {
  version: number;
  theme: ThemePreference;
  result_format: CommandDisplayFormat;
  scan_count: number;
  continue_on_error: boolean;
}

export interface ScanKeysInput {
  connection_id: string;
  cursor: number;
  pattern: string;
  count: number;
  key_type: string | null;
}

export interface ScanPage {
  cursor: number;
  keys: KeySummary[];
  has_more: boolean;
}

export interface KeySummary {
  key: string;
  key_type: string;
  ttl_ms: number;
  size: number | null;
  memory_bytes?: number | null;
  encoding?: string | null;
  idle_seconds?: number | null;
}

export interface ExportKeysInput {
  connection_id: string;
  keys: string[];
}

export interface ExportedKey {
  key: string;
  ttl_ms: number;
  value: RedisValue;
}

export interface ImportKeysInput {
  connection_id: string;
  entries: ExportedKey[];
}

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface StreamField {
  field: string;
  value: string;
}

export interface StreamEntry {
  id: string;
  fields: StreamField[];
}

export interface GetStreamConsumerGroupsInput {
  connection_id: string;
  key: string;
}

export interface StreamConsumerGroup {
  name: string;
  consumers: number;
  pending: number;
  last_delivered_id: string;
}

export interface CreateStreamConsumerGroupInput {
  connection_id: string;
  key: string;
  name: string;
  last_delivered_id: string;
}

export interface DeleteStreamConsumerGroupInput {
  connection_id: string;
  key: string;
  name: string;
}

export interface GetStreamConsumersInput {
  connection_id: string;
  key: string;
  group: string;
}

export interface StreamConsumer {
  name: string;
  pending: number;
  idle_ms: number;
}

export interface GetStreamPendingEntriesInput {
  connection_id: string;
  key: string;
  group: string;
  count: number;
  consumer: string | null;
}

export interface StreamPendingEntry {
  id: string;
  consumer: string;
  idle_ms: number;
  deliveries: number;
}

export interface AcknowledgeStreamPendingEntriesInput {
  connection_id: string;
  key: string;
  group: string;
  entries: string[];
}

export interface DeleteStreamConsumerInput {
  connection_id: string;
  key: string;
  group: string;
  consumer: string;
}

export type RedisValue =
  | { String: { value: string } }
  | { Hash: { fields: Array<{ field: string; value: string }> } }
  | { List: { items: string[] } }
  | { Set: { members: string[] } }
  | { SortedSet: { members: Array<{ member: string; score: number }> } }
  | { Json: { value: JsonValue } }
  | { Stream: { entries: StreamEntry[] } };

export interface KeyValue {
  key: string;
  key_type: string;
  ttl_ms: number;
  value: RedisValue;
}

export interface GetKeyInput {
  connection_id: string;
  key: string;
}

export interface SetKeyInput {
  connection_id: string;
  key: string;
  value: RedisValue;
}

export interface CreateKeyInput {
  connection_id: string;
  key: string;
  value: RedisValue;
  ttl_ms: number | null;
}

export interface RenameKeyInput {
  connection_id: string;
  key: string;
  new_key: string;
}

export interface DeleteKeysInput {
  connection_id: string;
  keys: string[];
}

export interface KeyInfoInput {
  connection_id: string;
  key: string;
}

export interface KeyInfo {
  key: string;
  key_type: string;
  ttl_ms: number;
  size: number | null;
  memory_bytes: number | null;
  encoding: string | null;
  idle_seconds: number | null;
}

export interface DeleteKeyInput {
  connection_id: string;
  key: string;
}

export interface SetKeyTtlInput {
  connection_id: string;
  key: string;
  ttl_ms: number;
}

export interface ExecuteCommandInput {
  connection_id: string;
  command: string;
}

export interface CommandArgument {
  name: string;
  required: boolean;
  hint: string;
}

export interface CommandDefinition {
  name: string;
  summary: string;
  arguments: CommandArgument[];
}

export interface ExecuteCommandsInput {
  connection_id: string;
  commands: string[];
  continue_on_error: boolean;
}

export interface CommandResult {
  kind: string;
  value: unknown;
}

export interface CommandExecutionItem {
  command: string;
  result: CommandResult | null;
  error_code: string | null;
}

export interface CommandHistoryEntry {
  connection_id: string;
  command: string;
  result: CommandResult | null;
  error_code: string | null;
  created_at: string;
}

export interface SaveCommandHistoryInput {
  connection_id: string;
  entries: CommandHistoryEntry[];
}

export interface IpcError {
  code: string;
  message: string;
}
