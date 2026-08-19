export type Workspace = "browser" | "workbench";

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

export type CommandDisplayFormat = "raw" | "text" | "json";

export interface IpcError {
  code: string;
  message: string;
}
