export type Workspace =
  | "browser"
  | "search-query"
  | "workbench"
  | "cli"
  | "database"
  | "database-analysis"
  | "observability"
  | "topology"
  | "query-library"
  | "settings";

export interface ConnectionProfile {
  ssh?: SshConfig | null;
  sentinel?: SentinelConfig | null;
  cluster?: ClusterConfig | null;
  id: string;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  has_password: boolean;
  tls: boolean;
  verify_server_cert: boolean;
  ca_certificate_name: string | null;
  client_certificate_name: string | null;
  has_ca_certificate: boolean;
  has_client_certificate: boolean;
}

export interface SaveConnectionInput {
  sentinel_password?: string | null;
  ssh_password?: string | null;
  ssh_private_key?: string | null;
  ssh_passphrase?: string | null;
  ssh_identity_file?: string | null;
  ssh_known_hosts_file?: string | null;
  clear_ssh_secrets: boolean;
  profile: ConnectionProfile;
  password: string | null;
  ca_certificate: string | null;
  client_certificate: string | null;
  client_key: string | null;
  clear_ca_certificate: boolean;
  clear_client_certificate: boolean;
}

export interface ConnectionExportProfile {
  ssh?: SshExportConfig | null;
  sentinel?: Omit<SentinelConfig, "has_password"> | null;
  cluster?: ClusterConfig | null;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  tls: boolean;
  verify_server_cert: boolean;
  ca_certificate_name: string | null;
  client_certificate_name: string | null;
}

export interface ConnectionExportDocument {
  version: number;
  connections: ConnectionExportProfile[];
}

export interface ImportConnectionsInput {
  content: string;
}

export interface ConnectionImportFailure {
  index: number;
  name: string | null;
  code: string;
  message: string;
}

export interface ImportConnectionsResult {
  imported: ConnectionProfile[];
  failed: ConnectionImportFailure[];
  ignored_secret_fields: number;
}

export interface ConnectionInfo {
  server_version: string;
  resolved_endpoint?: ConnectionEndpoint | null;
}

export interface ConnectionEndpoint {
  host: string;
  port: number;
}

export interface SentinelConfig {
  master_name: string;
  nodes: ConnectionEndpoint[];
  username: string | null;
  has_password: boolean;
  tls: boolean;
}

export type SshAuthMethod = "agent" | "password" | "private_key";

export interface SshExportConfig {
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
  has_password: boolean;
  has_private_key: boolean;
  has_passphrase: boolean;
  has_identity_file: boolean;
  has_known_hosts_file: boolean;
}

export type SshConfig = SshExportConfig;
export type TestConnectionInput = SaveConnectionInput;
export type ConnectionTargetKind = "standalone" | "sentinel" | "cluster";
export type NodeScope = "routed" | "primary_nodes" | "all_nodes" | { node: string };
export type ClusterNodeRole = "primary" | "replica";
export type ClusterNodeHealth = "online" | "offline" | "loading";

export interface ClusterConfig {
  nodes: ConnectionEndpoint[];
  read_from_replicas: boolean;
}

export interface NodeFailure { node_id: string; code: string }
export interface SlotRange { start: number; end: number }
export interface ClusterSummary {
  state: string;
  slots_assigned: number;
  slots_ok: number;
  slots_pfail: number;
  slots_fail: number;
  current_epoch: number;
  size: number;
  known_nodes: number;
}

export interface ClusterNodeMetrics {
  server_version: string | null;
  redis_mode: string | null;
  total_keys: number | null;
  maxmemory_bytes: number | null;
  used_memory_bytes: number | null;
  ops_per_second: number | null;
  connections_received: number | null;
  connected_clients: number | null;
  commands_processed: number | null;
  network_in_kbps: number | null;
  network_out_kbps: number | null;
  cache_hit_ratio: number | null;
  replication_offset: number | null;
  replication_lag: number | null;
  uptime_seconds: number | null;
}

export interface ClusterNode {
  id: string;
  endpoint: ConnectionEndpoint;
  connection_endpoint: ConnectionEndpoint | null;
  role: ClusterNodeRole;
  health: ClusterNodeHealth;
  primary_id: string | null;
  slots: SlotRange[];
  metrics: ClusterNodeMetrics;
}

export interface ClusterTopology {
  summary: ClusterSummary;
  nodes: ClusterNode[];
  failures: NodeFailure[];
}

export interface ModuleSummary {
  name: string;
  version: string | null;
}

export interface ModuleCapabilities {
  modules: ModuleSummary[];
  json_supported: boolean;
  json_version: string | null;
  search_supported: boolean;
  search_version: string | null;
  array_supported: boolean;
  vector_set_supported: boolean;
}

export type SearchKeyType = "hash" | "json";
export type SearchFieldType =
  | "text"
  | "tag"
  | "numeric"
  | "geo"
  | "geoshape"
  | "vector";

export interface SearchIndexFieldInput {
  name: string;
  alias?: string | null;
  field_type: SearchFieldType;
  vector?: SearchVectorConfig | null;
}

export interface SearchVectorConfig {
  algorithm: "FLAT" | "HNSW";
  data_type: "FLOAT32" | "FLOAT64";
  dimension: number;
  distance_metric: "COSINE" | "L2" | "IP";
  initial_capacity?: number;
  block_size?: number;
  m?: number;
  ef_construction?: number;
  ef_runtime?: number;
}

export interface SearchIndexSummary {
  name: string;
}

export interface ListSearchIndexesResult {
  indexes: SearchIndexSummary[];
}

export interface CreateSearchIndexInput {
  connection_id: string;
  index: string;
  key_type: SearchKeyType;
  prefixes: string[];
  fields: SearchIndexFieldInput[];
}

export interface SearchIndexInput {
  connection_id: string;
  index: string;
}

export interface GetKeySearchIndexesInput {
  connection_id: string;
  key: string;
}

export interface KeySearchIndexSummary {
  name: string;
  key_type: string;
  prefixes: string[];
}

export interface SearchVectorFieldInfo {
  data_type: string;
  dimension: number;
  distance_metric: string;
}
export interface SearchVectorQueryInput {
  connection_id: string;
  index: string;
  field: string;
  vector: number[];
  count: number;
  filter: string;
}
export interface SearchVectorQueryResult {
  matches: { key: string; distance: number }[];
  returned: number;
  count: number;
  distance_metric: string;
}

export interface SearchIndexAttribute {
  identifier: string;
  query_name?: string | null;
  field_type: string;
  sortable: boolean;
  no_index: boolean;
  vector?: SearchVectorFieldInfo | null;
}

export interface SearchIndexInfo {
  index_name: string;
  key_type: string;
  prefixes: string[];
  attributes: SearchIndexAttribute[];
  num_docs: number | null;
  num_terms: number | null;
  num_records: number | null;
  total_index_memory_bytes: number | null;
}

export interface SearchQueryInput {
  connection_id: string;
  index: string;
  query: string;
  offset: number;
  limit: number;
  include_content?: boolean;
}

export interface SearchKeyResult {
  key: string;
  key_type: string;
  fields?: SearchDocumentField[] | null;
}

export interface SearchDocumentField {
  name: string;
  value: JsonValue;
}

export interface SearchQueryResult {
  total: number;
  offset: number;
  next_offset: number | null;
  max_results: number | null;
  keys: SearchKeyResult[];
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

export interface ClientDetails {
  connected_clients: number | null;
  blocked_clients: number | null;
  tracking_clients: number | null;
  max_clients: number | null;
}

export interface MemoryDetails {
  used_memory_bytes: number | null;
  used_memory_peak_bytes: number | null;
  used_memory_rss_bytes: number | null;
  mem_fragmentation_ratio: number | null;
  allocator_active_bytes: number | null;
  allocator_resident_bytes: number | null;
}

export interface StatsDetails {
  instantaneous_ops_per_sec: number | null;
  expired_keys: number | null;
  evicted_keys: number | null;
  hit_rate: number | null;
}

export interface PersistenceDetails {
  loading: boolean | null;
  rdb_last_save_time: number | null;
  rdb_changes_since_last_save: number | null;
  aof_enabled: boolean | null;
  aof_rewrite_in_progress: boolean | null;
}

export interface ReplicationDetails {
  role: string | null;
  connected_replicas: number | null;
  master_link_status: string | null;
  master_repl_offset: number | null;
}

export interface CommandStat {
  command: string;
  calls: number | null;
  usec: number | null;
  usec_per_call: number | null;
  rejected_calls: number | null;
  failed_calls: number | null;
}

export interface InstanceDetails {
  overview: InstanceOverview;
  clients: ClientDetails;
  memory: MemoryDetails;
  stats: StatsDetails;
  persistence: PersistenceDetails;
  replication: ReplicationDetails;
  command_stats: CommandStat[];
}

export interface AnalyzeDatabaseInput {
  connection_id: string;
  pattern: string;
  delimiter: string;
  max_keys: number;
}

export interface AnalysisProgress {
  scanned: number;
  processed: number;
  max_keys: number;
  truncated: boolean;
}

export interface TypeSummary {
  type: string;
  total: number;
}

export interface AnalysisSummary {
  total: number;
  observed: number;
  types: TypeSummary[];
}

export interface AnalysisKey {
  key: string;
  key_type: string;
  length: number | null;
  memory_bytes: number | null;
  ttl_seconds: number | null;
}

export interface NamespaceSummary {
  namespace: string;
  keys: number;
  memory_bytes: number;
  types: TypeSummary[];
}

export interface ExpirationGroup {
  label: string;
  keys: number;
  memory_bytes: number;
}

export interface DatabaseAnalysisReport {
  database: number;
  pattern: string;
  delimiter: string;
  progress: AnalysisProgress;
  total_keys: AnalysisSummary;
  total_memory: AnalysisSummary;
  top_keys_by_length: AnalysisKey[];
  top_keys_by_memory: AnalysisKey[];
  top_namespaces_by_keys: NamespaceSummary[];
  top_namespaces_by_memory: NamespaceSummary[];
  expiration_groups: ExpirationGroup[];
  node_results: NodeAnalysisResult[];
  failed_nodes: NodeFailure[];
}

export interface NodeAnalysisResult {
  node_id: string;
  endpoint: ConnectionEndpoint;
  report: DatabaseAnalysisReport;
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

export type ScanCursor = number | string;

export interface ScanKeysInput {
  connection_id: string;
  cursor: ScanCursor;
  pattern: string;
  count: number;
  key_type: string | null;
}

export interface ScanPage {
  cursor: ScanCursor;
  keys: KeySummary[];
  has_more: boolean;
  node_failures: NodeFailure[];
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

export interface JsonPathInput {
  connection_id: string;
  key: string;
  path: string;
}

export interface SetJsonPathInput extends JsonPathInput {
  value: JsonValue;
}

export interface AppendJsonArrayInput extends JsonPathInput {
  values: JsonValue[];
}

export interface JsonPathValue {
  key: string;
  path: string;
  found: boolean;
  value: JsonValue | null;
  ttl_ms: number;
}

export interface JsonMutationResult {
  key: string;
  path: string;
  affected: number;
  new_length: number | null;
  ttl_ms: number;
}

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

export type ArrayCreateMode = "contiguous" | "sparse";

export interface ArrayElement {
  index: string;
  value: string;
}

export interface ArrayCell {
  index: string;
  value: string | null;
}

export interface ArraySummary {
  key: string;
  length: string;
  count: string;
  next_index: string;
}

export interface ArrayRange {
  cells: ArrayCell[];
  start: string;
  end: string;
  has_more: boolean;
}

export interface ArrayScan {
  elements: ArrayElement[];
  next_start: string | null;
  has_more: boolean;
}

export interface ArraySearchResult {
  elements: ArrayElement[];
  total: string;
}

export interface ArrayAggregateResult {
  operation: string;
  value: string;
}

export interface ArrayMutationResult {
  affected: number;
  key_exists: boolean;
  next_index: string | null;
}

export interface ArrayKeyInput {
  connection_id: string;
  key: string;
}

export interface CreateArrayInput extends ArrayKeyInput {
  mode: ArrayCreateMode;
  start_index: string | null;
  values: string[];
  elements: ArrayElement[];
  ttl_ms: number | null;
}

export interface ArrayRangeInput extends ArrayKeyInput {
  start: string;
  end: string;
}

export interface ArrayScanInput extends ArrayKeyInput {
  start: string | null;
  end: string | null;
  limit: number;
}

export interface ArrayElementInput extends ArrayKeyInput {
  index: string;
}

export interface ArrayMultiGetInput extends ArrayKeyInput {
  indices: string[];
}

export interface SetArrayElementInput extends ArrayKeyInput {
  index: string;
  value: string;
}

export interface AppendArrayInput extends ArrayKeyInput {
  values: string[];
}

export interface DeleteArrayElementsInput extends ArrayKeyInput {
  indices: string[];
}

export interface DeleteArrayRangeInput extends ArrayRangeInput {}

export interface ArrayPredicate {
  criteria: string;
  value: string;
}

export interface SearchArrayInput extends ArrayKeyInput {
  start: string | null;
  end: string | null;
  predicates: ArrayPredicate[];
  combinator: string | null;
  nocase: boolean;
  with_values: boolean;
  limit: number;
}

export type ArrayAggregateOperation =
  | "SUM"
  | "MIN"
  | "MAX"
  | "AND"
  | "OR"
  | "XOR"
  | "MATCH"
  | "USED";

export interface AggregateArrayInput extends ArrayKeyInput {
  operation: ArrayAggregateOperation;
  start: string | null;
  end: string | null;
  values: string[];
  limit: number;
}

export interface VectorSetElementPayload {
  name: string;
  vector_values: number[] | null;
  vector_fp32_base64: string | null;
  attributes: JsonValue | null;
}

export interface VectorSetKeyInput {
  connection_id: string;
  key: string;
}

export interface VectorSetElementInput extends VectorSetKeyInput {
  element: string;
}

export interface VectorSetSummary {
  key: string;
  total: string;
  dimension: number | null;
  quantization: string | null;
}

export interface VectorSetElement {
  name: string;
  score: number | null;
  vector_base64: string | null;
  attributes: JsonValue | null;
}

export interface VectorSetPage {
  elements: VectorSetElement[];
  cursor: string | null;
  has_more: boolean;
}

export interface VectorSimilarityMatch {
  name: string;
  score: number;
  attributes: JsonValue | null;
}

export interface VectorSimilarityResult {
  matches: VectorSimilarityMatch[];
  has_more: boolean;
}

export interface CreateVectorSetInput extends VectorSetKeyInput {
  dimension: number;
  quantization: string | null;
  elements: VectorSetElementPayload[];
  ttl_ms: number | null;
}

export interface AddVectorSetElementsInput extends VectorSetKeyInput {
  elements: VectorSetElementPayload[];
}

export interface ListVectorSetElementsInput extends VectorSetKeyInput {
  start: string | null;
  end: string | null;
  limit: number;
}

export interface SetVectorSetAttributesInput extends VectorSetKeyInput {
  element: string;
  attributes: JsonValue;
}

export interface DeleteVectorSetElementsInput extends VectorSetKeyInput {
  elements: string[];
}

export interface VectorSimilarityQueryInput extends VectorSetKeyInput {
  by_element: string | null;
  by_vector: number[] | null;
  by_vector_base64: string | null;
  count: number;
  with_attributes: boolean;
}

export type RedisValue =
  | { String: { value: string } }
  | { Hash: { fields: Array<{ field: string; value: string }> } }
  | { List: { items: string[] } }
  | { Set: { members: string[] } }
  | { SortedSet: { members: Array<{ member: string; score: number }> } }
  | { Json: { value: JsonValue } }
  | { Stream: { entries: StreamEntry[] } }
  | { Array: { length: string; count: string } }
  | {
      VectorSet: {
        total: string;
        dimension: number | null;
        quantization: string | null;
      };
    };

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

export type WorkbenchResultFormat = CommandDisplayFormat | "tree" | "table";

export interface DeleteCommandHistoryInput {
  connection_id: string;
  command: string;
  created_at: string;
}

export interface ClearCommandHistoryInput {
  connection_id: string;
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

export interface ClaimStreamPendingEntriesInput {
  connection_id: string;
  key: string;
  group: string;
  consumer: string;
  min_idle_ms: number;
  entries: string[];
}
