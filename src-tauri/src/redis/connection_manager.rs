use std::{collections::HashMap, sync::Arc};

use ::redis::{Client, ClientTlsConfig, TlsCertificates, Value};
use tokio::sync::{Mutex, MutexGuard, RwLock};

use crate::{
    domain::{
        normalize_key_type, parse_info_sections, parse_keyspace_line, validate_certificate_pem,
        validate_private_key_pem, AcknowledgeStreamPendingEntriesInput, AddVectorSetElementsInput,
        AggregateArrayInput, AnalyzeDatabaseInput, AppendArrayInput, AppendJsonArrayInput,
        ArrayKeyInput, ArrayMultiGetInput, ArrayRangeInput, ArrayScanInput, ClusterNodeRole,
        ClusterTopology, CommandDefinition, CommandExecutionItem, CommandResult,
        ConnectionEndpoint, ConnectionInfo, ConnectionProfile, ConnectionTarget, CreateArrayInput,
        CreateKeyInput, CreateSearchIndexInput, CreateStreamConsumerGroupInput,
        CreateVectorSetInput, DatabaseAnalysisReport, DatabaseOverview, DeleteArrayElementsInput,
        DeleteArrayRangeInput, DeleteJsonPathInput, DeleteKeysInput,
        DeleteStreamConsumerGroupInput, DeleteStreamConsumerInput, DeleteVectorSetElementsInput,
        ExecuteCommandsInput, ExportKeysInput, ExportedKey, GetJsonPathInput,
        GetKeySearchIndexesInput, GetSlowLogsInput, GetStreamConsumerGroupsInput,
        GetStreamConsumersInput, GetStreamPendingEntriesInput, HashEntry, ImportKeysInput,
        InstanceDetails, InstanceOverview, JsonMutationResult, JsonPathValue, KeyInfo,
        KeyInfoInput, KeySearchIndexSummary, KeySummary, KeyValue, ListSearchIndexesResult,
        ModuleCapabilities, NodeFailure, ProfilerSession, PubSubSession, PublishPubSubInput,
        RedisValue, RenameKeyInput, ScanCursor, ScanKeysInput, ScanPage, SearchIndexInfo,
        SearchIndexInput, SearchQueryInput, SearchQueryResult, SelectDatabaseInput,
        SetArrayElementInput, SetJsonPathInput, SetKeyInput, SetKeyTtlInput,
        SetVectorSetAttributesInput, SlowLogConfig, SlowLogEntry, SortedSetEntry,
        StartProfilerInput, StartPubSubInput, StopProfilerInput, StopPubSubInput, StreamConsumer,
        StreamConsumerGroup, StreamEntry, StreamPendingEntry, UpdateSlowLogConfigInput,
        VectorSetElement, VectorSetElementInput, VectorSetKeyInput, VectorSetPage,
        VectorSetSummary, VectorSimilarityMatch, VectorSimilarityQueryInput,
        VectorSimilarityResult,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
};

use super::{
    apply_node_info_results,
    array::{
        build_array_aggregate_command, build_array_append_command, build_array_delete_command,
        build_array_delete_range_command, build_array_multi_get_command, build_array_range_command,
        build_array_scan_command, build_array_search_command, build_array_set_command,
        build_create_array_command, parse_array_aggregate, parse_array_multi_get,
        parse_array_range, parse_array_scan, parse_array_search_with_values, parse_array_summary,
    },
    capabilities::{
        build_command_info_command, is_array_command_set_supported,
        is_vector_set_command_set_supported, parse_command_info, parse_vector_set_info_summary,
    },
    cluster_node_info_command,
    database_analysis::{
        allocate_primary_key_limits, analyze_connection, analyze_connection_accumulator,
        load_instance_details, merge_node_reports, parse_module_list, NodeAnalysisState,
    },
    fan_out_cluster_nodes,
    json_ops::{
        append_json_array_path, delete_json_path_value, json_path_uses_legacy_syntax,
        parse_module_capabilities, read_json_path, write_json_path,
    },
    key_ops::{
        decode_json_value, decode_stream_entry, encode_json_value, encode_stream_entry,
        ensure_cluster_scan_page_size, load_key_summaries,
    },
    observability::{
        map_pubsub_error, parse_slow_log_config_reply, parse_slow_log_reply, ProfilerManager,
        PubSubManager,
    },
    parse_cluster_info, parse_cluster_node_info_reply, parse_cluster_nodes,
    parse_cluster_shards_for_tls,
    routed_connection::{RoutedClient, RoutedConnection},
    scan_cluster,
    search::{
        build_create_search_index_command, parse_max_search_results, parse_search_index_info,
        parse_search_index_list, parse_search_query,
    },
    standalone_transport::{StandaloneClient, TlsClientMaterial, TunneledClient},
    stream_groups::{
        parse_stream_consumer_groups, parse_stream_consumers, parse_stream_pending_entries,
    },
    tokenize_command,
    vector_set::{
        build_vadd_command, build_vemb_command, build_vgetattr_command, build_vrange_command,
        build_vrem_command, build_vsetattr_command, build_vsim_command,
        parse_vector_set_attributes, parse_vector_set_element, parse_vector_set_info,
        parse_vector_set_page, parse_vsim_reply,
    },
    ClusterNodeConnectionFactory, ClusterNodeInfoResult, ClusterScanBackend, ClusterScanNode,
};

const CLUSTER_NODE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[allow(async_fn_in_trait)]
pub trait RedisOperations: Send + Sync {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        secrets: &ConnectionSecrets,
    ) -> Result<ConnectionInfo, AppError>;
    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError>;
    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError>;
    async fn get_slow_logs(&self, input: GetSlowLogsInput) -> Result<Vec<SlowLogEntry>, AppError>;
    async fn clear_slow_logs(&self, connection_id: &str) -> Result<(), AppError>;
    async fn get_slow_log_config(&self, connection_id: &str) -> Result<SlowLogConfig, AppError>;
    async fn update_slow_log_config(
        &self,
        input: UpdateSlowLogConfigInput,
    ) -> Result<SlowLogConfig, AppError>;
    async fn publish_pub_sub(&self, input: PublishPubSubInput) -> Result<u64, AppError>;
    async fn scan_keys(&self, input: crate::domain::ScanKeysInput) -> Result<ScanPage, AppError>;
    async fn get_key(&self, connection_id: &str, key: &str) -> Result<KeyValue, AppError>;
    async fn create_array(&self, input: CreateArrayInput) -> Result<KeyValue, AppError>;
    async fn get_array_summary(
        &self,
        input: ArrayKeyInput,
    ) -> Result<crate::domain::ArraySummary, AppError>;
    async fn get_array_range(
        &self,
        input: ArrayRangeInput,
    ) -> Result<crate::domain::ArrayRange, AppError>;
    async fn scan_array(&self, input: ArrayScanInput)
        -> Result<crate::domain::ArrayScan, AppError>;
    async fn get_array_elements(
        &self,
        input: ArrayMultiGetInput,
    ) -> Result<Vec<Option<String>>, AppError>;
    async fn set_array_element(
        &self,
        input: SetArrayElementInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError>;
    async fn append_array_elements(
        &self,
        input: AppendArrayInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError>;
    async fn delete_array_elements(
        &self,
        input: DeleteArrayElementsInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError>;
    async fn delete_array_range(
        &self,
        input: DeleteArrayRangeInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError>;
    async fn search_array(
        &self,
        input: crate::domain::SearchArrayInput,
    ) -> Result<crate::domain::ArraySearchResult, AppError>;
    async fn aggregate_array(
        &self,
        input: AggregateArrayInput,
    ) -> Result<crate::domain::ArrayAggregateResult, AppError>;
    async fn create_vector_set(&self, input: CreateVectorSetInput) -> Result<KeyValue, AppError>;
    async fn add_vector_set_elements(
        &self,
        input: AddVectorSetElementsInput,
    ) -> Result<(), AppError>;
    async fn get_vector_set_summary(
        &self,
        input: VectorSetKeyInput,
    ) -> Result<VectorSetSummary, AppError>;
    async fn list_vector_set_elements(
        &self,
        input: crate::domain::ListVectorSetElementsInput,
    ) -> Result<VectorSetPage, AppError>;
    async fn get_vector_set_element(
        &self,
        input: VectorSetElementInput,
    ) -> Result<VectorSetElement, AppError>;
    async fn set_vector_set_attributes(
        &self,
        input: SetVectorSetAttributesInput,
    ) -> Result<VectorSetElement, AppError>;
    async fn delete_vector_set_attributes(
        &self,
        input: VectorSetElementInput,
    ) -> Result<(), AppError>;
    async fn delete_vector_set_elements(
        &self,
        input: DeleteVectorSetElementsInput,
    ) -> Result<u64, AppError>;
    async fn search_vector_set(
        &self,
        input: VectorSimilarityQueryInput,
    ) -> Result<VectorSimilarityResult, AppError>;
    async fn download_vector_embedding(
        &self,
        input: VectorSetElementInput,
    ) -> Result<String, AppError>;
    async fn get_module_capabilities(
        &self,
        connection_id: &str,
    ) -> Result<ModuleCapabilities, AppError>;
    async fn list_search_indexes(
        &self,
        connection_id: &str,
    ) -> Result<ListSearchIndexesResult, AppError>;
    async fn create_search_index(&self, input: CreateSearchIndexInput) -> Result<(), AppError>;
    async fn get_search_index(&self, input: SearchIndexInput) -> Result<SearchIndexInfo, AppError>;
    async fn delete_search_index(&self, input: SearchIndexInput) -> Result<(), AppError>;
    async fn search_keys(&self, input: SearchQueryInput) -> Result<SearchQueryResult, AppError>;
    async fn get_key_search_indexes(
        &self,
        input: GetKeySearchIndexesInput,
    ) -> Result<Vec<KeySearchIndexSummary>, AppError>;
    async fn get_json_path(&self, input: GetJsonPathInput) -> Result<JsonPathValue, AppError>;
    async fn set_json_path(&self, input: SetJsonPathInput) -> Result<JsonMutationResult, AppError>;
    async fn append_json_array(
        &self,
        input: AppendJsonArrayInput,
    ) -> Result<JsonMutationResult, AppError>;
    async fn delete_json_path(
        &self,
        input: DeleteJsonPathInput,
    ) -> Result<JsonMutationResult, AppError>;
    async fn set_key(&self, input: SetKeyInput) -> Result<KeyValue, AppError>;
    async fn create_key(&self, input: CreateKeyInput) -> Result<KeyValue, AppError>;
    async fn rename_key(&self, input: RenameKeyInput) -> Result<KeyValue, AppError>;
    async fn delete_key(&self, connection_id: &str, key: &str) -> Result<(), AppError>;
    async fn delete_keys(&self, input: DeleteKeysInput) -> Result<u64, AppError>;
    async fn set_key_ttl(&self, input: SetKeyTtlInput) -> Result<i64, AppError>;
    async fn get_key_info(&self, input: KeyInfoInput) -> Result<KeyInfo, AppError>;
    async fn get_stream_consumer_groups(
        &self,
        input: GetStreamConsumerGroupsInput,
    ) -> Result<Vec<StreamConsumerGroup>, AppError>;
    async fn create_stream_consumer_group(
        &self,
        input: CreateStreamConsumerGroupInput,
    ) -> Result<(), AppError>;
    async fn delete_stream_consumer_group(
        &self,
        input: DeleteStreamConsumerGroupInput,
    ) -> Result<u64, AppError>;
    async fn get_stream_consumers(
        &self,
        input: GetStreamConsumersInput,
    ) -> Result<Vec<StreamConsumer>, AppError>;
    async fn get_stream_pending_entries(
        &self,
        input: GetStreamPendingEntriesInput,
    ) -> Result<Vec<StreamPendingEntry>, AppError>;
    async fn acknowledge_stream_pending_entries(
        &self,
        input: AcknowledgeStreamPendingEntriesInput,
    ) -> Result<u64, AppError>;
    async fn delete_stream_consumer(
        &self,
        input: DeleteStreamConsumerInput,
    ) -> Result<u64, AppError>;
    async fn export_keys(&self, input: ExportKeysInput) -> Result<Vec<ExportedKey>, AppError>;
    async fn import_keys(&self, input: ImportKeysInput) -> Result<u64, AppError>;
    async fn get_instance_overview(
        &self,
        connection_id: &str,
    ) -> Result<InstanceOverview, AppError>;
    async fn get_instance_details(&self, connection_id: &str) -> Result<InstanceDetails, AppError>;
    async fn get_cluster_topology(&self, connection_id: &str) -> Result<ClusterTopology, AppError>;
    async fn refresh_cluster_topology(
        &self,
        connection_id: &str,
    ) -> Result<ClusterTopology, AppError>;
    async fn analyze_database(
        &self,
        input: AnalyzeDatabaseInput,
    ) -> Result<DatabaseAnalysisReport, AppError>;
    async fn get_database_overview(
        &self,
        connection_id: &str,
    ) -> Result<Vec<DatabaseOverview>, AppError>;
    async fn select_database(
        &self,
        input: SelectDatabaseInput,
    ) -> Result<ConnectionProfile, AppError>;
    async fn execute_command(
        &self,
        connection_id: &str,
        input: &str,
    ) -> Result<CommandResult, AppError>;
    async fn execute_commands(
        &self,
        input: ExecuteCommandsInput,
    ) -> Result<Vec<CommandExecutionItem>, AppError>;
    fn command_catalog(&self) -> Vec<CommandDefinition>;
}

pub struct RedisService {
    profiles: Arc<dyn ProfileRepository>,
    secrets: Arc<dyn SecretStore>,
    profile_transaction: Mutex<()>,
    active: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
    capabilities: Arc<RwLock<HashMap<String, ModuleCapabilities>>>,
    generations: Arc<RwLock<HashMap<String, u64>>>,
    pubsub: Arc<PubSubManager>,
    profiler: Arc<ProfilerManager>,
    #[cfg(test)]
    test_ssh_transport: Option<super::ssh::SshTransport>,
    #[cfg(test)]
    test_node_scoped_snapshot_gate: Option<Arc<NodeScopedSnapshotGate>>,
}

#[cfg(test)]
#[derive(Default)]
struct NodeScopedSnapshotGate {
    captured: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

#[derive(Clone)]
struct ConnectionHandle {
    client: RoutedClient,
    profile: ConnectionProfile,
    target: ConnectionTarget,
    cluster_node_factory: Option<Arc<ClusterNodeConnectionFactory>>,
    _ssh: Option<super::ssh::SshTransport>,
}

#[derive(Clone)]
struct ActiveConnectionSnapshot {
    token: u64,
    client: RoutedClient,
    profile: ConnectionProfile,
    target: ConnectionTarget,
    cluster_node_factory: Option<Arc<ClusterNodeConnectionFactory>>,
}

struct CapabilityConnection {
    capabilities: ModuleCapabilities,
    connection: RoutedConnection,
}

#[derive(Clone)]
struct RedisClusterScanBackend {
    factory: Arc<ClusterNodeConnectionFactory>,
}

impl ClusterScanBackend for RedisClusterScanBackend {
    fn scan_node<'a>(
        &'a self,
        node: &'a ClusterScanNode,
        cursor: u64,
        pattern: &'a str,
        count: usize,
    ) -> futures_util::future::BoxFuture<'a, Result<(u64, Vec<Vec<u8>>), AppError>> {
        Box::pin(async move {
            let mut connection = self
                .factory
                .connection_for_node_id(&node.node_id, &node.endpoint)
                .await?;
            let (next_cursor, keys) = ::redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(count)
                .query_async::<(u64, Vec<Vec<u8>>)>(&mut connection)
                .await
                .map_err(|_| AppError::ClusterNodeUnavailable)?;
            if keys.iter().any(|key| std::str::from_utf8(key).is_err()) {
                return Err(AppError::ClusterNodeUnavailable);
            }
            Ok((next_cursor, keys))
        })
    }
}

impl RedisService {
    pub fn new(profiles: Arc<dyn ProfileRepository>, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            profiles,
            secrets,
            profile_transaction: Mutex::new(()),
            active: Arc::new(RwLock::new(HashMap::new())),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            generations: Arc::new(RwLock::new(HashMap::new())),
            pubsub: Arc::new(PubSubManager::new()),
            profiler: Arc::new(ProfilerManager::new()),
            #[cfg(test)]
            test_ssh_transport: None,
            #[cfg(test)]
            test_node_scoped_snapshot_gate: None,
        }
    }

    #[cfg(test)]
    fn with_test_ssh_transport(mut self, transport: super::ssh::SshTransport) -> Self {
        self.test_ssh_transport = Some(transport);
        self
    }

    #[cfg(test)]
    fn with_test_node_scoped_snapshot_gate(mut self, gate: Arc<NodeScopedSnapshotGate>) -> Self {
        self.test_node_scoped_snapshot_gate = Some(gate);
        self
    }

    async fn connect_handle(
        &self,
        profile: &ConnectionProfile,
        secrets: &ConnectionSecrets,
    ) -> Result<(ConnectionHandle, Option<ConnectionEndpoint>), AppError> {
        #[cfg(test)]
        if let Some(transport) = self.test_ssh_transport.as_ref() {
            return connect_handle_with_transport(profile, secrets, Some(transport.clone())).await;
        }
        connect_handle_with_transport(profile, secrets, None).await
    }

    // Shared with commands so profile and keyring writes, including rollback, are atomic
    // with respect to connection snapshots and publication. Lock order is transaction,
    // generation, capabilities, active; never wait for this lock while holding the others.
    pub(crate) async fn profile_transaction(&self) -> MutexGuard<'_, ()> {
        self.profile_transaction.lock().await
    }

    fn connection_secrets(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<ConnectionSecrets, AppError> {
        if profile.has_password
            || profile.has_ca_certificate
            || profile.has_client_certificate
            || profile
                .sentinel
                .as_ref()
                .is_some_and(|sentinel| sentinel.has_password)
            || profile.ssh.is_some()
        {
            Ok(self.secrets.read(&profile.id)?.unwrap_or_default())
        } else {
            Ok(ConnectionSecrets::default())
        }
    }

    async fn connection_snapshot(
        &self,
        connection_id: &str,
    ) -> Result<(ConnectionProfile, ConnectionSecrets), AppError> {
        let _transaction = self.profile_transaction().await;
        let profile = self
            .profiles
            .load()?
            .into_iter()
            .find(|profile| profile.id == connection_id)
            .ok_or(AppError::InvalidConnection)?;
        profile.validate()?;
        let secrets = self.connection_secrets(&profile)?;
        Ok((profile, secrets))
    }

    pub async fn claim_stream_pending_entries(
        &self,
        input: crate::domain::ClaimStreamPendingEntriesInput,
    ) -> Result<Vec<String>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        ::redis::cmd("XCLAIM")
            .arg(&input.key)
            .arg(&input.group)
            .arg(&input.consumer)
            .arg(input.min_idle_ms)
            .arg(&input.entries)
            .arg("JUSTID")
            .query_async::<Vec<String>>(&mut connection)
            .await
            .map_err(map_command_error)
    }

    pub async fn start_pub_sub(
        &self,
        app: tauri::AppHandle,
        input: StartPubSubInput,
    ) -> Result<PubSubSession, AppError> {
        input.validate()?;
        let (snapshot, _) = self.active_snapshot(&input.connection_id).await?;
        let client = snapshot.client.standalone_client()?.clone();
        let mut pubsub = client.pubsub().await?;
        for topic in input.normalized_topics() {
            if topic.pattern {
                pubsub
                    .psubscribe(topic.name)
                    .await
                    .map_err(map_pubsub_error)?;
            } else {
                pubsub
                    .subscribe(topic.name)
                    .await
                    .map_err(map_pubsub_error)?;
            }
        }
        let connection_id = input.connection_id.clone();
        self.register_observability_if_current(&connection_id, snapshot.token, || {
            self.pubsub.start(app, pubsub, input)
        })
        .await
    }

    /// Browser collection values are placeholders; the dedicated editor reads bounded pages.
    /// Full reads remain available to explicit export and legacy operations through get_key.
    pub async fn get_browser_key(
        &self,
        connection_id: &str,
        key: &str,
    ) -> Result<KeyValue, AppError> {
        if key.is_empty() || key.len() > 16 * 1024 {
            return Err(AppError::InvalidInput);
        }
        let mut connection = self.connection(connection_id).await?;
        read_key_mode(&mut connection, key, true).await
    }

    pub async fn rename_browser_key(&self, input: RenameKeyInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let renamed: i64 = ::redis::cmd("RENAMENX")
            .arg(&input.key)
            .arg(&input.new_key)
            .query_async(&mut connection)
            .await
            .map_err(map_command_error)?;
        if renamed == 0 {
            return Err(AppError::CommandFailed);
        }
        read_key_mode(&mut connection, &input.new_key, true).await
    }

    pub async fn stop_pub_sub(&self, input: StopPubSubInput) -> Result<(), AppError> {
        self.pubsub.stop(input)
    }

    pub async fn start_profiler(
        &self,
        app: tauri::AppHandle,
        input: StartProfilerInput,
    ) -> Result<ProfilerSession, AppError> {
        input.validate()?;
        let (snapshot, _) = self.active_snapshot(&input.connection_id).await?;
        let client = snapshot.client.standalone_client()?.clone();
        let monitor = client.monitor_stream().await?;
        let connection_id = input.connection_id.clone();
        self.register_observability_if_current(&connection_id, snapshot.token, || {
            self.profiler.start(app, monitor, input)
        })
        .await
    }

    pub async fn stop_profiler(&self, input: StopProfilerInput) -> Result<(), AppError> {
        self.profiler.stop(input)
    }

    async fn active_snapshot(
        &self,
        connection_id: &str,
    ) -> Result<(ActiveConnectionSnapshot, Option<ModuleCapabilities>), AppError> {
        // Keep this acquisition order aligned with every lifecycle writer. Holding the
        // generation guard until the handle is cloned makes the token, cache and handle one
        // indivisible view without retaining any lock across network I/O.
        let generations = self.generations.read().await;
        let capabilities = self.capabilities.read().await;
        let active = self.active.read().await;
        let handle = active
            .get(connection_id)
            .ok_or(AppError::ConnectionFailed)?;
        let snapshot = ActiveConnectionSnapshot {
            token: generations.get(connection_id).copied().unwrap_or(0),
            client: handle.client.clone(),
            profile: handle.profile.clone(),
            target: handle.target.clone(),
            cluster_node_factory: handle.cluster_node_factory.clone(),
        };
        Ok((snapshot, capabilities.get(connection_id).cloned()))
    }

    async fn client(&self, connection_id: &str) -> Result<RoutedClient, AppError> {
        Ok(self.active_snapshot(connection_id).await?.0.client)
    }

    pub async fn standalone_client(
        &self,
        connection_id: &str,
    ) -> Result<StandaloneClient, AppError> {
        self.client(connection_id)
            .await?
            .standalone_client()
            .cloned()
    }

    pub async fn connection_target(
        &self,
        connection_id: &str,
    ) -> Result<ConnectionTarget, AppError> {
        Ok(self.active_snapshot(connection_id).await?.0.target)
    }

    async fn node_scoped_connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        let (snapshot, _) = self.active_snapshot(connection_id).await?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            return Err(AppError::UnsupportedFeature);
        }
        #[cfg(test)]
        if let Some(gate) = self.test_node_scoped_snapshot_gate.as_ref() {
            gate.captured.notify_one();
            gate.resume.notified().await;
        }
        snapshot.client.connection().await
    }

    pub async fn start_analysis_task(
        &self,
        manager: &super::analysis_tasks::AnalysisTaskManager,
        input: super::analysis_tasks::StartAnalysisTaskInput,
    ) -> Result<super::analysis_tasks::AnalysisTask, AppError> {
        input.validate()?;
        let (snapshot, _) = self.active_snapshot(&input.analysis.connection_id).await?;
        if snapshot.profile.database != input.database {
            return Err(AppError::OperationCancelled);
        }
        let is_cluster = matches!(snapshot.target, ConnectionTarget::Cluster(_));
        let tls = snapshot.profile.tls;
        let database = snapshot.profile.database;
        let client = snapshot.client;
        let factory = snapshot.cluster_node_factory;
        let generation = super::analysis_tasks::AnalysisGeneration::new(
            snapshot.token,
            self.generations.clone(),
        );
        let analysis = input.analysis.clone();
        manager.start(input, generation, move |control| async move {
            control.checkpoint().await?;
            let mut connection = client.connection().await?;
            control.checkpoint().await?;
            if !is_cluster {
                let (accumulator, progress) =
                    super::database_analysis::analyze_connection_observed(
                        &mut connection,
                        database,
                        &analysis,
                        false,
                        Some(&control),
                        "instance",
                    )
                    .await?;
                control.node_finished("instance");
                return Ok(accumulator.finish(
                    progress.scanned,
                    progress.processed,
                    progress.truncated,
                ));
            }
            let factory = factory.ok_or(AppError::ClusterNodeUnavailable)?;
            let mut primaries = load_cluster_nodes(&mut connection, tls)
                .await?
                .into_iter()
                .filter(|node| node.role == ClusterNodeRole::Primary)
                .collect::<Vec<_>>();
            primaries.sort_by(|left, right| left.id.cmp(&right.id));
            control.checkpoint().await?;
            let quotas = allocate_primary_key_limits(analysis.max_keys, primaries.len())?;
            control.set_nodes_total(primaries.len());
            let results = fan_out_cluster_nodes(
                primaries.into_iter().zip(quotas).collect(),
                |(node, quota)| {
                    let factory = factory.clone();
                    let control = control.clone();
                    let mut node_input = analysis.clone();
                    node_input.max_keys = quota;
                    async move {
                        let result = async {
                            control.checkpoint().await?;
                            let mut connection = factory
                                .connection_for_node_id(&node.id, &node.endpoint)
                                .await?;
                            let (accumulator, progress) =
                                super::database_analysis::analyze_connection_observed(
                                    &mut connection,
                                    0,
                                    &node_input,
                                    true,
                                    Some(&control),
                                    &node.id,
                                )
                                .await?;
                            Ok::<_, AppError>(NodeAnalysisState {
                                node_id: node.id.clone(),
                                endpoint: node.endpoint.clone(),
                                accumulator,
                                progress,
                            })
                        }
                        .await;
                        control.node_finished(&node.id);
                        result.map_err(|_| NodeFailure {
                            node_id: node.id,
                            code: AppError::ClusterNodeUnavailable.code().to_owned(),
                        })
                    }
                },
            )
            .await;
            control.checkpoint().await?;
            let mut nodes = Vec::new();
            let mut failures = Vec::new();
            for result in results {
                match result {
                    Ok(node) => nodes.push(node),
                    Err(failure) => failures.push(failure),
                }
            }
            let mut report = merge_node_reports(nodes, failures);
            report.database = database;
            report.pattern = analysis.pattern;
            report.delimiter = analysis.delimiter;
            report.progress.max_keys = analysis.max_keys;
            Ok(report)
        })
    }

    pub async fn start_bulk_delete(
        &self,
        manager: &super::bulk_tasks::BulkTaskManager,
        input: super::bulk_tasks::StartBulkDeleteInput,
    ) -> Result<super::bulk_tasks::BulkTask, AppError> {
        let input = input.normalize()?;
        let (snapshot, _) = self.active_snapshot(&input.connection_id).await?;
        let mut connection = snapshot.client.connection().await?;
        let target = match &mut connection {
            RoutedConnection::Standalone(connection) => {
                super::bulk_tasks::BulkDeleteTarget::Single(connection.clone())
            }
            RoutedConnection::Cluster(_) => {
                let nodes = load_cluster_nodes(&mut connection, snapshot.profile.tls).await?;
                let primaries: Vec<_> = nodes
                    .into_iter()
                    .filter(|node| node.role == ClusterNodeRole::Primary)
                    .collect();
                let factory = snapshot
                    .cluster_node_factory
                    .clone()
                    .ok_or(AppError::ClusterNodeUnavailable)?;
                let results = fan_out_cluster_nodes(primaries, |node| {
                    let factory = factory.clone();
                    async move {
                        let connection = factory.connection(&node.endpoint).await;
                        (node.slots, connection)
                    }
                })
                .await;
                let mut owners = HashMap::new();
                let mut connections = Vec::new();
                for (ranges, connection) in results {
                    for range in ranges {
                        for number in range.start..=range.end {
                            let slot = ::redis::cluster_routing::Slot::new(number)
                                .ok_or(AppError::ClusterTopologyFailed)?;
                            if owners.insert(slot, connections.len()).is_some() {
                                return Err(AppError::ClusterTopologyFailed);
                            }
                        }
                    }
                    connections.push(connection);
                }
                if owners.len() != 16_384 {
                    return Err(AppError::ClusterTopologyFailed);
                }
                super::bulk_tasks::BulkDeleteTarget::Cluster {
                    owners: Arc::new(owners),
                    nodes: Arc::new(connections),
                }
            }
        };
        let generations = self.generations.clone();
        let connection_id = input.connection_id.clone();
        self.ensure_generation_current(&connection_id, snapshot.token)
            .await?;
        manager.start(input, move |key| {
            let target = target.clone();
            let generations = generations.clone();
            let connection_id = connection_id.clone();
            async move {
                if generations
                    .read()
                    .await
                    .get(&connection_id)
                    .copied()
                    .unwrap_or(0)
                    != snapshot.token
                {
                    return Err(AppError::OperationCancelled);
                }
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    target.delete(&key).await
                })
                .await
                .map_err(|_| AppError::CommandFailed)?
            }
        })
    }

    pub async fn routed_connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        self.client(connection_id).await?.connection().await
    }

    pub(crate) async fn connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        self.routed_connection(connection_id).await
    }

    async fn capability_connection(
        &self,
        connection_id: &str,
    ) -> Result<CapabilityConnection, AppError> {
        let (snapshot, cached) = self.active_snapshot(connection_id).await?;
        let mut connection = snapshot.client.connection().await?;
        let capabilities = match cached {
            Some(capabilities) => capabilities,
            None => {
                let capabilities = probe_module_capabilities(&mut connection).await?;
                let _ = self
                    .cache_capabilities_if_current(
                        connection_id,
                        snapshot.token,
                        capabilities.clone(),
                    )
                    .await;
                capabilities
            }
        };
        Ok(CapabilityConnection {
            capabilities,
            connection,
        })
    }

    async fn register_observability_if_current<T>(
        &self,
        connection_id: &str,
        token: u64,
        register: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let generations = self.generations.read().await;
        if generations.get(connection_id).copied().unwrap_or(0) != token {
            return Err(AppError::OperationCancelled);
        }
        register()
    }

    async fn ensure_generation_current(
        &self,
        connection_id: &str,
        token: u64,
    ) -> Result<(), AppError> {
        let generations = self.generations.read().await;
        if generations.get(connection_id).copied().unwrap_or(0) == token {
            Ok(())
        } else {
            Err(AppError::OperationCancelled)
        }
    }

    async fn load_live_cluster_topology(
        &self,
        connection_id: &str,
    ) -> Result<ClusterTopology, AppError> {
        validate_connection_id(connection_id)?;
        let (snapshot, _) = self.active_snapshot(connection_id).await?;
        self.load_live_cluster_topology_snapshot(connection_id, snapshot)
            .await
    }

    async fn load_live_cluster_topology_snapshot(
        &self,
        connection_id: &str,
        snapshot: ActiveConnectionSnapshot,
    ) -> Result<ClusterTopology, AppError> {
        if !matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            return Err(AppError::UnsupportedFeature);
        }
        let factory = snapshot
            .cluster_node_factory
            .clone()
            .ok_or(AppError::ClusterNodeUnavailable)?;
        let mut routed = snapshot.client.connection().await?;
        let mut nodes = load_cluster_nodes(&mut routed, snapshot.profile.tls).await?;
        let cluster_info = ::redis::cmd("CLUSTER")
            .arg("INFO")
            .query_async::<String>(&mut routed)
            .await
            .map_err(|_| AppError::ClusterTopologyFailed)?;
        let summary = parse_cluster_info(&cluster_info)?;

        let results = fan_out_cluster_nodes(nodes.clone(), |node| {
            let factory = factory.clone();
            async move {
                let node_id = node.id.clone();
                let operation = async {
                    let mut connection = factory
                        .connection_for_node_id(&node.id, &node.endpoint)
                        .await?;
                    let reply = match cluster_node_info_command()
                        .query_async::<Value>(&mut connection)
                        .await
                    {
                        Err(error) if is_multi_section_info_unsupported(&error) => {
                            // Redis 6 accepts at most one section. Keep this fallback on
                            // the same connection and inside the node's overall deadline.
                            ::redis::cmd("INFO")
                                .query_async::<Value>(&mut connection)
                                .await
                        }
                        result => result,
                    }
                    .map_err(|_| AppError::ClusterNodeUnavailable)?;
                    let info = parse_cluster_node_info_reply(reply)?;
                    Ok::<_, AppError>(ClusterNodeInfoResult::success(
                        &node.id,
                        node.endpoint,
                        info,
                    ))
                };
                match tokio::time::timeout(CLUSTER_NODE_TIMEOUT, operation).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(_)) | Err(_) => ClusterNodeInfoResult::failure(node_id),
                }
            }
        })
        .await;
        let failures = apply_node_info_results(&mut nodes, results);
        for node in &mut nodes {
            if let Some(endpoint) = factory.connection_endpoint(&node.id) {
                node.connection_endpoint = Some(endpoint);
            }
        }
        self.ensure_generation_current(connection_id, snapshot.token)
            .await?;
        Ok(ClusterTopology {
            summary,
            nodes,
            failures,
        })
    }

    async fn cluster_database_overview(
        &self,
        connection_id: &str,
        snapshot: ActiveConnectionSnapshot,
    ) -> Result<Vec<DatabaseOverview>, AppError> {
        let factory = snapshot
            .cluster_node_factory
            .clone()
            .ok_or(AppError::ClusterNodeUnavailable)?;
        let mut routed = snapshot.client.connection().await?;
        let primaries = load_cluster_nodes(&mut routed, snapshot.profile.tls)
            .await?
            .into_iter()
            .filter(|node| node.role == ClusterNodeRole::Primary)
            .collect::<Vec<_>>();
        if primaries.is_empty() {
            return Err(AppError::ClusterTopologyFailed);
        }
        let nodes = fan_out_cluster_nodes(primaries, |node| {
            let factory = factory.clone();
            async move {
                let operation = async {
                    let mut connection = factory
                        .connection_for_node_id(&node.id, &node.endpoint)
                        .await?;
                    let reply = ::redis::cmd("INFO")
                        .arg("keyspace")
                        .query_async::<Value>(&mut connection)
                        .await;
                    if let Ok(reply) = reply {
                        let info = parse_cluster_node_info_reply(reply)?;
                        let sections = parse_info_sections(&info);
                        if let Some(keyspace) = sections.get("Keyspace") {
                            return match keyspace.get("db0") {
                                Some(line) => parse_keyspace_line("db0", line),
                                None => Ok(DatabaseOverview {
                                    database: 0,
                                    key_count: Some(0),
                                    expires: Some(0),
                                    avg_ttl_ms: None,
                                }),
                            };
                        }
                    }
                    let key_count = ::redis::cmd("DBSIZE")
                        .query_async::<u64>(&mut connection)
                        .await
                        .map_err(|_| AppError::ClusterNodeUnavailable)?;
                    Ok(DatabaseOverview {
                        database: 0,
                        key_count: Some(key_count),
                        expires: None,
                        avg_ttl_ms: None,
                    })
                };
                tokio::time::timeout(CLUSTER_NODE_TIMEOUT, operation)
                    .await
                    .ok()
                    .and_then(Result::ok)
            }
        })
        .await;
        let result = DatabaseOverview::from_cluster_primaries(&nodes);
        self.ensure_generation_current(connection_id, snapshot.token)
            .await?;
        Ok(vec![result])
    }

    async fn analyze_cluster_database(
        &self,
        connection_id: &str,
        snapshot: ActiveConnectionSnapshot,
        input: &AnalyzeDatabaseInput,
    ) -> Result<DatabaseAnalysisReport, AppError> {
        let factory = snapshot
            .cluster_node_factory
            .clone()
            .ok_or(AppError::ClusterNodeUnavailable)?;
        let mut routed = snapshot.client.connection().await?;
        let mut primaries = load_cluster_nodes(&mut routed, snapshot.profile.tls)
            .await?
            .into_iter()
            .filter(|node| node.role == ClusterNodeRole::Primary)
            .collect::<Vec<_>>();
        if primaries.is_empty() || primaries.len() > 128 {
            return Err(AppError::ClusterTopologyFailed);
        }
        // CLUSTER SHARDS order can change between reads; assign remainder slots by stable node ID.
        primaries.sort_by(|left, right| left.id.cmp(&right.id));
        let quotas = allocate_primary_key_limits(input.max_keys, primaries.len())?;
        let results = fan_out_cluster_nodes(
            primaries.into_iter().zip(quotas).collect(),
            |(node, quota)| {
                let factory = factory.clone();
                let mut node_input = input.clone();
                node_input.max_keys = quota;
                async move {
                    let result = async {
                        let mut connection = factory
                            .connection_for_node_id(&node.id, &node.endpoint)
                            .await?;
                        let (accumulator, progress) =
                            analyze_connection_accumulator(&mut connection, 0, &node_input, true)
                                .await?;
                        Ok::<_, AppError>(NodeAnalysisState {
                            node_id: node.id.clone(),
                            endpoint: node.endpoint.clone(),
                            accumulator,
                            progress,
                        })
                    }
                    .await;
                    match result {
                        Ok(result) => Ok(result),
                        Err(_) => Err(NodeFailure {
                            node_id: node.id,
                            code: AppError::ClusterNodeUnavailable.code().to_owned(),
                        }),
                    }
                }
            },
        )
        .await;
        let mut node_results = Vec::new();
        let mut failed_nodes = Vec::new();
        for result in results {
            match result {
                Ok(result) => node_results.push(result),
                Err(failure) => failed_nodes.push(failure),
            }
        }
        let mut report = merge_node_reports(node_results, failed_nodes);
        report.database = snapshot.profile.database;
        report.pattern = input.pattern.clone();
        report.delimiter = input.delimiter.clone();
        report.progress.max_keys = input.max_keys;
        self.ensure_generation_current(connection_id, snapshot.token)
            .await?;
        Ok(report)
    }

    pub(super) async fn search_vector_connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        let context = self.capability_connection(connection_id).await?;
        if !context.capabilities.search_compatible() {
            return Err(AppError::UnsupportedFeature);
        }
        crate::domain::search::SearchVectorQueryInput::validate_search_version(
            context.capabilities.search_version.as_deref(),
        )?;
        Ok(context.connection)
    }

    pub(super) async fn search_connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        let context = self.capability_connection(connection_id).await?;
        if !context.capabilities.search_compatible() {
            return Err(AppError::UnsupportedFeature);
        }
        Ok(context.connection)
    }

    async fn array_connection(&self, connection_id: &str) -> Result<RoutedConnection, AppError> {
        let context = self.capability_connection(connection_id).await?;
        if !context.capabilities.array_supported {
            return Err(AppError::UnsupportedFeature);
        }
        Ok(context.connection)
    }

    async fn vector_set_connection(
        &self,
        connection_id: &str,
    ) -> Result<RoutedConnection, AppError> {
        let context = self.capability_connection(connection_id).await?;
        if !context.capabilities.vector_set_supported {
            return Err(AppError::UnsupportedFeature);
        }
        Ok(context.connection)
    }

    async fn inspect_client(client: &RoutedClient) -> Result<ConnectionInfo, AppError> {
        let mut connection = client.connection().await?;
        let pong: String = ::redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map_err(map_connection_error)?;
        if pong != "PONG" {
            return Err(AppError::ConnectionFailed);
        }

        let server_info: Result<String, _> = ::redis::cmd("INFO")
            .arg("server")
            .query_async(&mut connection)
            .await;
        let server_version = server_info
            .ok()
            .and_then(|info| {
                info.lines()
                    .find_map(|line| line.strip_prefix("redis_version:"))
                    .map(str::trim)
                    .filter(|version| !version.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unknown".into());

        Ok(ConnectionInfo {
            server_version,
            resolved_endpoint: None,
        })
    }

    #[cfg(test)]
    async fn profile(&self, connection_id: &str) -> Result<ConnectionProfile, AppError> {
        if let Some(handle) = self.active.read().await.get(connection_id) {
            return Ok(handle.profile.clone());
        }
        let _transaction = self.profile_transaction().await;
        self.profiles
            .load()?
            .into_iter()
            .find(|profile| profile.id == connection_id)
            .ok_or(AppError::InvalidConnection)
    }

    #[cfg(test)]
    async fn capture_capability_token(&self, connection_id: &str) -> u64 {
        self.generations
            .read()
            .await
            .get(connection_id)
            .copied()
            .unwrap_or(0)
    }

    async fn cache_capabilities_if_current(
        &self,
        connection_id: &str,
        token: u64,
        capabilities: ModuleCapabilities,
    ) -> bool {
        let generations = self.generations.write().await;
        if generations.get(connection_id).copied().unwrap_or(0) != token {
            return false;
        }

        self.capabilities
            .write()
            .await
            .insert(connection_id.to_owned(), capabilities);
        true
    }

    async fn bump_connection_generation(&self, connection_id: &str) -> u64 {
        let mut generations = self.generations.write().await;
        bump_generation(&mut generations, connection_id)
    }

    async fn publish_connection_if_current(
        &self,
        connection_id: &str,
        token: u64,
        handle: ConnectionHandle,
        previous_profile: Option<&ConnectionProfile>,
        expected_secrets: &ConnectionSecrets,
    ) -> Result<(), AppError> {
        // Serialize the check, optional persistence, cache invalidation and publication.
        // Network work must stay outside this guard so close/newer attempts can cancel it.
        let _transaction = self.profile_transaction().await;
        let mut generations = self.generations.write().await;
        if generations.get(connection_id).copied().unwrap_or(0) != token {
            return Err(AppError::OperationCancelled);
        }
        let mut capabilities = self.capabilities.write().await;
        let mut active = self.active.write().await;

        // Reload under the transaction lock: neither an edited/deleted profile nor
        // rotated credentials may publish a connection built from an earlier snapshot.
        let old_profiles = self.profiles.load()?;
        let current_profile = old_profiles
            .iter()
            .find(|profile| profile.id == connection_id)
            .filter(|profile| *profile == previous_profile.unwrap_or(&handle.profile))
            .ok_or(AppError::OperationCancelled)?;
        if self.connection_secrets(current_profile)? != *expected_secrets {
            return Err(AppError::OperationCancelled);
        }

        if previous_profile.is_some() {
            let mut profiles = old_profiles.clone();
            let profile = profiles
                .iter_mut()
                .find(|profile| profile.id == connection_id)
                .ok_or(AppError::OperationCancelled)?;
            *profile = handle.profile.clone();
            if self.profiles.save(&profiles).is_err() {
                let _ = self.profiles.save(&old_profiles);
                return Err(AppError::PersistenceFailed);
            }
        }

        // The generation guard stays held across synchronous persistence and publication,
        // so no close or newer operation can invalidate the token between these steps.
        self.pubsub.cancel_connection(connection_id);
        self.profiler.cancel_connection(connection_id);
        // Probes started against the previous active handle during this attempt must
        // also expire, even though they captured the attempt's generation.
        bump_generation(&mut generations, connection_id);
        capabilities.remove(connection_id);
        active.insert(connection_id.to_owned(), handle);
        Ok(())
    }
}

fn bump_generation(generations: &mut HashMap<String, u64>, connection_id: &str) -> u64 {
    let next = generations
        .get(connection_id)
        .copied()
        .unwrap_or(0)
        .wrapping_add(1);
    generations.insert(connection_id.to_owned(), next);
    next
}

async fn probe_module_capabilities(
    connection: &mut RoutedConnection,
) -> Result<ModuleCapabilities, AppError> {
    let module_reply = match ::redis::cmd("MODULE")
        .arg("LIST")
        .query_async::<Value>(connection)
        .await
    {
        Ok(reply) => reply,
        Err(error) if is_unknown_command_error(&error) => Value::Array(Vec::new()),
        Err(error) => return Err(map_command_error(error)),
    };
    let module_capabilities = parse_module_capabilities(module_reply)?;
    let command_reply = match build_command_info_command()
        .query_async::<Value>(connection)
        .await
    {
        Ok(reply) => reply,
        Err(error) if is_unknown_command_error(&error) => Value::Array(Vec::new()),
        Err(error) => return Err(map_command_error(error)),
    };
    let commands = parse_command_info(command_reply)?;
    let mut capabilities = ModuleCapabilities::from_modules_and_commands(
        module_capabilities.modules,
        commands.clone(),
    );
    capabilities.array_supported = is_array_command_set_supported(&commands);
    capabilities.vector_set_supported = is_vector_set_command_set_supported(&commands);
    Ok(capabilities)
}

async fn load_cluster_scan_nodes(
    connection: &mut RoutedConnection,
    tls: bool,
) -> Result<Vec<ClusterScanNode>, AppError> {
    let topology = load_cluster_nodes(connection, tls).await?;
    let primaries = topology
        .into_iter()
        .filter(|node| node.role == ClusterNodeRole::Primary)
        .map(|node| ClusterScanNode {
            node_id: node.id,
            endpoint: node.endpoint,
        })
        .collect::<Vec<_>>();
    if primaries.is_empty() || primaries.len() > 128 {
        return Err(AppError::ClusterTopologyFailed);
    }
    Ok(primaries)
}

async fn load_cluster_nodes(
    connection: &mut RoutedConnection,
    tls: bool,
) -> Result<Vec<crate::domain::ClusterNode>, AppError> {
    let shards = ::redis::cmd("CLUSTER")
        .arg("SHARDS")
        .query_async::<Value>(&mut *connection)
        .await;
    let topology = match shards {
        Ok(reply) => match parse_cluster_shards_for_tls(reply, tls) {
            Ok(nodes) => nodes,
            Err(AppError::UnsupportedFeature) => load_cluster_nodes_fallback(connection).await?,
            Err(_) => return Err(AppError::ClusterTopologyFailed),
        },
        Err(error) if is_unknown_command_error(&error) => {
            load_cluster_nodes_fallback(connection).await?
        }
        Err(_) => return Err(AppError::ClusterTopologyFailed),
    };
    if topology.is_empty() || topology.len() > 128 {
        return Err(AppError::ClusterTopologyFailed);
    }
    Ok(topology)
}

async fn load_cluster_nodes_fallback(
    connection: &mut RoutedConnection,
) -> Result<Vec<crate::domain::ClusterNode>, AppError> {
    let reply = ::redis::cmd("CLUSTER")
        .arg("NODES")
        .query_async::<String>(connection)
        .await
        .map_err(|_| AppError::ClusterTopologyFailed)?;
    parse_cluster_nodes(&reply).map_err(|_| AppError::ClusterTopologyFailed)
}

impl RedisOperations for RedisService {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        secrets: &ConnectionSecrets,
    ) -> Result<ConnectionInfo, AppError> {
        profile.validate()?;
        let (handle, endpoint) = self.connect_handle(profile, secrets).await?;
        let mut info = Self::inspect_client(&handle.client).await?;
        info.resolved_endpoint = endpoint;
        Ok(info)
    }

    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError> {
        let token = self.bump_connection_generation(connection_id).await;
        let (profile, secrets) = self.connection_snapshot(connection_id).await?;
        let (handle, endpoint) = self.connect_handle(&profile, &secrets).await?;
        let mut info = Self::inspect_client(&handle.client).await?;
        info.resolved_endpoint = endpoint;
        self.publish_connection_if_current(connection_id, token, handle, None, &secrets)
            .await?;
        Ok(info)
    }

    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError> {
        let mut generations = self.generations.write().await;
        bump_generation(&mut generations, connection_id);
        let mut capabilities = self.capabilities.write().await;
        let mut active = self.active.write().await;
        self.pubsub.cancel_connection(connection_id);
        self.profiler.cancel_connection(connection_id);
        capabilities.remove(connection_id);
        active.remove(connection_id);
        Ok(())
    }

    async fn get_slow_logs(&self, input: GetSlowLogsInput) -> Result<Vec<SlowLogEntry>, AppError> {
        input.validate()?;
        let mut connection = self.node_scoped_connection(&input.connection_id).await?;
        let count = if input.count == -1 {
            let config = get_slow_log_config_with_connection(&mut connection).await?;
            i64::try_from(config.slowlog_max_len).map_err(|_| AppError::CommandFailed)?
        } else {
            input.count
        };
        let reply = ::redis::cmd("SLOWLOG")
            .arg("GET")
            .arg(count)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_slow_log_reply(reply)
    }

    async fn clear_slow_logs(&self, connection_id: &str) -> Result<(), AppError> {
        validate_connection_id(connection_id)?;
        let mut connection = self.node_scoped_connection(connection_id).await?;
        ::redis::cmd("SLOWLOG")
            .arg("RESET")
            .query_async::<String>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn get_slow_log_config(&self, connection_id: &str) -> Result<SlowLogConfig, AppError> {
        validate_connection_id(connection_id)?;
        let mut connection = self.node_scoped_connection(connection_id).await?;
        get_slow_log_config_with_connection(&mut connection).await
    }

    async fn update_slow_log_config(
        &self,
        input: UpdateSlowLogConfigInput,
    ) -> Result<SlowLogConfig, AppError> {
        input.validate()?;
        let mut connection = self.node_scoped_connection(&input.connection_id).await?;
        if let Some(value) = input.slowlog_max_len {
            ::redis::cmd("CONFIG")
                .arg("SET")
                .arg("slowlog-max-len")
                .arg(value)
                .query_async::<String>(&mut connection)
                .await
                .map_err(map_command_error)?;
        }
        if let Some(value) = input.slowlog_log_slower_than {
            ::redis::cmd("CONFIG")
                .arg("SET")
                .arg("slowlog-log-slower-than")
                .arg(value)
                .query_async::<String>(&mut connection)
                .await
                .map_err(map_command_error)?;
        }
        get_slow_log_config_with_connection(&mut connection).await
    }

    async fn publish_pub_sub(&self, input: PublishPubSubInput) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.node_scoped_connection(&input.connection_id).await?;
        let receivers = ::redis::cmd("PUBLISH")
            .arg(&input.channel)
            .arg(&input.message)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(receivers).map_err(|_| AppError::CommandFailed)
    }

    async fn scan_keys(&self, input: ScanKeysInput) -> Result<ScanPage, AppError> {
        input.validate()?;
        let requested_type = input.key_type.as_deref().and_then(normalize_key_type);
        let (snapshot, _) = self.active_snapshot(&input.connection_id).await?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            let mut routed = snapshot.client.connection().await?;
            let primary_nodes = load_cluster_scan_nodes(&mut routed, snapshot.profile.tls).await?;
            let cluster_cursor = match &input.cursor {
                ScanCursor::Standalone(0) => None,
                ScanCursor::Cluster(cursor) => Some(cursor.as_str()),
                ScanCursor::Standalone(_) => return Err(AppError::InvalidInput),
            };
            let backend = RedisClusterScanBackend {
                factory: snapshot
                    .cluster_node_factory
                    .ok_or(AppError::ClusterNodeUnavailable)?,
            };
            let page = scan_cluster(
                &backend,
                snapshot.token,
                &primary_nodes,
                cluster_cursor,
                &input.pattern,
                input.count,
            )
            .await?;
            let keys = load_key_summaries(&mut routed, page.keys, requested_type).await?;
            let page = ScanPage {
                cursor: ScanCursor::Cluster(page.cursor),
                keys,
                has_more: page.has_more,
                node_failures: page.node_failures,
            };
            ensure_cluster_scan_page_size(&page)?;
            self.ensure_generation_current(&input.connection_id, snapshot.token)
                .await?;
            return Ok(page);
        }
        let standalone_cursor = match input.cursor {
            ScanCursor::Standalone(cursor) => cursor,
            ScanCursor::Cluster(_) => return Err(AppError::InvalidInput),
        };
        let mut connection = snapshot.client.connection().await?;
        let (cursor, keys): (u64, Vec<String>) = ::redis::cmd("SCAN")
            .arg(standalone_cursor)
            .arg("MATCH")
            .arg(&input.pattern)
            .arg("COUNT")
            .arg(input.count)
            .query_async::<(u64, Vec<String>)>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let mut summaries = Vec::with_capacity(keys.len());

        for key in keys {
            let key_type: String = ::redis::cmd("TYPE")
                .arg(&key)
                .query_async::<String>(&mut connection)
                .await
                .map_err(map_command_error)?;
            if requested_type.is_some() && normalize_key_type(&key_type) != requested_type {
                continue;
            }
            let ttl_ms: i64 = ::redis::cmd("PTTL")
                .arg(&key)
                .query_async::<i64>(&mut connection)
                .await
                .map_err(map_command_error)?;
            let size = key_size(&mut connection, &key, &key_type)
                .await
                .ok()
                .flatten();
            let memory_bytes = ::redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(&key)
                .query_async::<Option<u64>>(&mut connection)
                .await
                .ok()
                .flatten();
            let encoding = ::redis::cmd("OBJECT")
                .arg("ENCODING")
                .arg(&key)
                .query_async::<Option<String>>(&mut connection)
                .await
                .ok()
                .flatten();
            let idle_seconds = ::redis::cmd("OBJECT")
                .arg("IDLETIME")
                .arg(&key)
                .query_async::<Option<u64>>(&mut connection)
                .await
                .ok()
                .flatten();
            summaries.push(KeySummary {
                key,
                key_type,
                ttl_ms,
                size,
                memory_bytes,
                encoding,
                idle_seconds,
            });
        }

        Ok(ScanPage {
            cursor: ScanCursor::Standalone(cursor),
            keys: summaries,
            has_more: cursor != 0,
            node_failures: Vec::new(),
        })
    }

    async fn get_key(&self, connection_id: &str, key: &str) -> Result<KeyValue, AppError> {
        let mut connection = self.connection(connection_id).await?;
        read_key(&mut connection, key).await
    }

    async fn create_array(&self, input: CreateArrayInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        let exists: i64 = ::redis::cmd("EXISTS")
            .arg(&input.key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if exists > 0 {
            return Err(AppError::CommandFailed);
        }

        build_create_array_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if let Some(ttl_ms) = input.ttl_ms {
            apply_ttl(&mut connection, &input.key, ttl_ms).await?;
        }
        read_key(&mut connection, &input.key).await
    }

    async fn get_array_summary(
        &self,
        input: ArrayKeyInput,
    ) -> Result<crate::domain::ArraySummary, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let mut pipeline = ::redis::pipe();
        pipeline
            .cmd("ARLEN")
            .arg(&input.key)
            .cmd("ARCOUNT")
            .arg(&input.key)
            .cmd("ARNEXT")
            .arg(&input.key);
        let replies = pipeline
            .query_async::<Vec<Value>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if replies.len() != 3 {
            return Err(AppError::CommandFailed);
        }
        parse_array_summary(
            &input.key,
            replies[0].clone(),
            replies[1].clone(),
            replies[2].clone(),
        )
    }

    async fn get_array_range(
        &self,
        input: ArrayRangeInput,
    ) -> Result<crate::domain::ArrayRange, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_range_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_array_range(reply, &input.start, &input.end)
    }

    async fn scan_array(
        &self,
        input: ArrayScanInput,
    ) -> Result<crate::domain::ArrayScan, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_scan_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_array_scan(reply, input.limit)
    }

    async fn get_array_elements(
        &self,
        input: ArrayMultiGetInput,
    ) -> Result<Vec<Option<String>>, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_multi_get_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_array_multi_get(reply)
    }

    async fn set_array_element(
        &self,
        input: SetArrayElementInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        build_array_set_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(crate::domain::ArrayMutationResult {
            affected: 1,
            key_exists: true,
            next_index: None,
        })
    }

    async fn append_array_elements(
        &self,
        input: AppendArrayInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let length = ::redis::cmd("ARLEN")
            .arg(&input.key)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let index = value_as_decimal_string(&length)?;
        if index == u64::MAX.to_string() {
            return Err(AppError::InvalidInput);
        }
        build_array_append_command(&input.key, &index, &input.values)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(crate::domain::ArrayMutationResult {
            affected: input.values.len() as u64,
            key_exists: true,
            next_index: Some(index),
        })
    }

    async fn delete_array_elements(
        &self,
        input: DeleteArrayElementsInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_delete_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let affected = value_as_decimal_string(&reply)?
            .parse::<u64>()
            .map_err(|_| AppError::CommandFailed)?;
        let key_exists = redis_key_exists(&mut connection, &input.key).await?;
        Ok(crate::domain::ArrayMutationResult {
            affected,
            key_exists,
            next_index: None,
        })
    }

    async fn delete_array_range(
        &self,
        input: DeleteArrayRangeInput,
    ) -> Result<crate::domain::ArrayMutationResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_delete_range_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let affected = value_as_decimal_string(&reply)?
            .parse::<u64>()
            .map_err(|_| AppError::CommandFailed)?;
        let key_exists = redis_key_exists(&mut connection, &input.key).await?;
        Ok(crate::domain::ArrayMutationResult {
            affected,
            key_exists,
            next_index: None,
        })
    }

    async fn search_array(
        &self,
        input: crate::domain::SearchArrayInput,
    ) -> Result<crate::domain::ArraySearchResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let reply = build_array_search_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_array_search_with_values(reply, input.limit, input.with_values)
    }

    async fn aggregate_array(
        &self,
        input: AggregateArrayInput,
    ) -> Result<crate::domain::ArrayAggregateResult, AppError> {
        input.validate()?;
        let mut connection = self.array_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "array").await?;
        let operation = input.operation.clone();
        let reply = build_array_aggregate_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_array_aggregate(reply, &operation)
    }

    async fn create_vector_set(&self, input: CreateVectorSetInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        if redis_key_exists(&mut connection, &input.key).await? {
            return Err(AppError::CommandFailed);
        }

        let mut pipeline = ::redis::pipe();
        for element in &input.elements {
            pipeline.add_command(build_vadd_command(
                &input.key,
                element,
                Some(input.dimension),
            )?);
        }
        if let Some(ttl_ms) = input.ttl_ms {
            validate_ttl(ttl_ms)?;
            pipeline.cmd("PEXPIRE").arg(&input.key).arg(ttl_ms);
        }
        pipeline
            .query_async::<Vec<Value>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        read_key(&mut connection, &input.key).await
    }

    async fn add_vector_set_elements(
        &self,
        input: AddVectorSetElementsInput,
    ) -> Result<(), AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        let expected_dimension = read_vector_set_dimension(&mut connection, &input.key).await?;
        let mut pipeline = ::redis::pipe();
        for element in &input.elements {
            pipeline.add_command(build_vadd_command(&input.key, element, expected_dimension)?);
        }
        pipeline
            .query_async::<Vec<Value>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn get_vector_set_summary(
        &self,
        input: VectorSetKeyInput,
    ) -> Result<VectorSetSummary, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        let mut pipeline = ::redis::pipe();
        pipeline
            .cmd("VCARD")
            .arg(&input.key)
            .cmd("VINFO")
            .arg(&input.key);
        let replies = pipeline
            .query_async::<Vec<Value>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if replies.len() != 2 {
            return Err(AppError::CommandFailed);
        }
        let total = value_as_decimal_string(&replies[0])?;
        let (dimension, quantization) = parse_vector_set_info(replies[1].clone())?;
        Ok(VectorSetSummary {
            key: input.key,
            total,
            dimension,
            quantization,
        })
    }

    async fn list_vector_set_elements(
        &self,
        input: crate::domain::ListVectorSetElementsInput,
    ) -> Result<VectorSetPage, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;

        let (names, pagination_supported) = match build_vrange_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
        {
            Ok(reply) => (parse_vector_set_page(reply, input.limit)?, true),
            Err(error) if is_unknown_command_error(&error) => {
                let reply = ::redis::cmd("VRANDMEMBER")
                    .arg(&input.key)
                    .arg(input.limit)
                    .query_async::<Value>(&mut connection)
                    .await
                    .map_err(map_command_error)?;
                (parse_vector_set_page(reply, input.limit)?, false)
            }
            Err(error) => return Err(map_command_error(error)),
        };
        let cursor = if pagination_supported && names.len() == input.limit {
            names.last().map(|name| format!("({name}"))
        } else {
            None
        };
        let has_more = pagination_supported && cursor.is_some();
        let elements = load_vector_set_page_elements(&mut connection, &input.key, names).await?;
        Ok(VectorSetPage {
            elements,
            cursor,
            has_more,
        })
    }

    async fn get_vector_set_element(
        &self,
        input: VectorSetElementInput,
    ) -> Result<VectorSetElement, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        read_vector_set_element_with_connection(&mut connection, &input.key, &input.element).await
    }

    async fn set_vector_set_attributes(
        &self,
        input: SetVectorSetAttributesInput,
    ) -> Result<VectorSetElement, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        build_vsetattr_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        read_vector_set_element_with_connection(&mut connection, &input.key, &input.element).await
    }

    async fn delete_vector_set_attributes(
        &self,
        input: VectorSetElementInput,
    ) -> Result<(), AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        let mut command = ::redis::cmd("VSETATTR");
        command.arg(&input.key).arg(&input.element).arg("{}");
        command
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn delete_vector_set_elements(
        &self,
        input: DeleteVectorSetElementsInput,
    ) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        let mut pipeline = ::redis::pipe();
        for element in &input.elements {
            let single = DeleteVectorSetElementsInput {
                connection_id: input.connection_id.clone(),
                key: input.key.clone(),
                elements: vec![element.clone()],
            };
            pipeline.add_command(build_vrem_command(&single)?);
        }
        let replies = pipeline
            .query_async::<Vec<Value>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if replies.len() != input.elements.len() {
            return Err(AppError::CommandFailed);
        }
        let mut affected = 0_u64;
        for reply in replies {
            affected = affected
                .checked_add(
                    value_as_decimal_string(&reply)?
                        .parse::<u64>()
                        .map_err(|_| AppError::CommandFailed)?,
                )
                .ok_or(AppError::CommandFailed)?;
        }
        Ok(affected)
    }

    async fn search_vector_set(
        &self,
        input: VectorSimilarityQueryInput,
    ) -> Result<VectorSimilarityResult, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;

        let (mut matches, with_attributes) = match build_vsim_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
        {
            Ok(reply) => (
                parse_vsim_reply(reply, input.with_attributes)?,
                input.with_attributes,
            ),
            Err(error) if input.with_attributes && is_with_attributes_unsupported(&error) => {
                let mut fallback = input.clone();
                fallback.with_attributes = false;
                let reply = build_vsim_command(&fallback)?
                    .query_async::<Value>(&mut connection)
                    .await
                    .map_err(map_command_error)?;
                (parse_vsim_reply(reply, false)?, false)
            }
            Err(error) => return Err(map_command_error(error)),
        };
        if !with_attributes {
            load_vector_set_match_attributes(&mut connection, &input.key, &mut matches).await?;
        }
        Ok(VectorSimilarityResult {
            has_more: matches.len() == input.count as usize,
            matches,
        })
    }

    async fn download_vector_embedding(
        &self,
        input: VectorSetElementInput,
    ) -> Result<String, AppError> {
        input.validate()?;
        let mut connection = self.vector_set_connection(&input.connection_id).await?;
        ensure_existing_key_type(&mut connection, &input.key, "vector-set").await?;
        let reply = build_vemb_command(&input.key, &input.element)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if matches!(reply, Value::Nil) {
            return Err(AppError::KeyNotFound);
        }
        let element = parse_vector_set_element(reply, Value::Nil, &input.element)?;
        element.vector_base64.ok_or(AppError::InvalidInput)
    }

    async fn get_module_capabilities(
        &self,
        connection_id: &str,
    ) -> Result<ModuleCapabilities, AppError> {
        validate_connection_id(connection_id)?;
        let (snapshot, cached) = self.active_snapshot(connection_id).await?;
        if let Some(capabilities) = cached {
            return Ok(capabilities);
        }
        let mut connection = snapshot.client.connection().await?;
        let capabilities = probe_module_capabilities(&mut connection).await?;
        let _ = self
            .cache_capabilities_if_current(connection_id, snapshot.token, capabilities.clone())
            .await;
        Ok(capabilities)
    }

    async fn list_search_indexes(
        &self,
        connection_id: &str,
    ) -> Result<ListSearchIndexesResult, AppError> {
        let input = crate::domain::ListSearchIndexesInput {
            connection_id: connection_id.to_owned(),
        };
        input.validate()?;
        let mut connection = self.search_connection(connection_id).await?;
        let reply = ::redis::cmd("FT._LIST")
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(ListSearchIndexesResult {
            indexes: parse_search_index_list(reply)?,
        })
    }

    async fn create_search_index(&self, input: CreateSearchIndexInput) -> Result<(), AppError> {
        input.validate()?;
        let context = self.capability_connection(&input.connection_id).await?;
        if !context.capabilities.search_compatible() {
            return Err(AppError::UnsupportedFeature);
        }
        input.validate_search_version(context.capabilities.search_version.as_deref())?;
        let mut connection = context.connection;
        build_create_search_index_command(&input)?
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn get_search_index(&self, input: SearchIndexInput) -> Result<SearchIndexInfo, AppError> {
        input.validate()?;
        let mut connection = self.search_connection(&input.connection_id).await?;
        let reply = ::redis::cmd("FT.INFO")
            .arg(&input.index)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_search_index_info(reply)
    }

    async fn delete_search_index(&self, input: SearchIndexInput) -> Result<(), AppError> {
        input.validate()?;
        let mut connection = self.search_connection(&input.connection_id).await?;
        ::redis::cmd("FT.DROPINDEX")
            .arg(&input.index)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn search_keys(&self, input: SearchQueryInput) -> Result<SearchQueryResult, AppError> {
        input.validate()?;
        let mut connection = self.search_connection(&input.connection_id).await?;
        let max_results = ::redis::cmd("FT.CONFIG")
            .arg("GET")
            .arg("MAXSEARCHRESULTS")
            .query_async::<Value>(&mut connection)
            .await
            .ok()
            .and_then(parse_max_search_results);
        let safe_limit = max_results
            .and_then(|value| u32::try_from(value.saturating_sub(input.offset)).ok())
            .map_or(input.limit, |value| input.limit.min(value));
        if safe_limit == 0 {
            return Err(AppError::InvalidInput);
        }
        let mut command = ::redis::cmd("FT.SEARCH");
        command.arg(&input.index).arg(&input.query);
        if !input.include_content {
            command.arg("NOCONTENT");
        }
        let reply = command
            .arg("LIMIT")
            .arg(input.offset)
            .arg(safe_limit)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let mut result =
            parse_search_query(reply, input.offset, safe_limit, input.include_content)?;
        result.max_results = max_results;
        if let Some(max_results) = max_results {
            result.next_offset = result.next_offset.filter(|next| *next < max_results);
        }
        if result.keys.is_empty() {
            return Ok(result);
        }

        let mut pipeline = ::redis::pipe();
        for key in &result.keys {
            pipeline.cmd("TYPE").arg(&key.key);
        }
        let key_types = pipeline
            .query_async::<Vec<String>>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if key_types.len() != result.keys.len() {
            return Err(AppError::CommandFailed);
        }
        for (key, key_type) in result.keys.iter_mut().zip(key_types) {
            key.key_type = key_type;
        }
        Ok(result)
    }

    async fn get_key_search_indexes(
        &self,
        input: GetKeySearchIndexesInput,
    ) -> Result<Vec<KeySearchIndexSummary>, AppError> {
        input.validate()?;
        let mut connection = self.search_connection(&input.connection_id).await?;
        let key_type = ::redis::cmd("TYPE")
            .arg(&input.key)
            .query_async::<String>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let normalized_key_type = key_type.to_ascii_lowercase();
        if !matches!(
            normalized_key_type.as_str(),
            "hash" | "rejson-rl" | "rejson-rs" | "json"
        ) {
            return Ok(Vec::new());
        }

        let list_reply = ::redis::cmd("FT._LIST")
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let indexes = parse_search_index_list(list_reply)?;
        let mut matches = Vec::new();
        for chunk in indexes.chunks(32) {
            let mut pipeline = ::redis::pipe();
            for index in chunk {
                pipeline.cmd("FT.INFO").arg(&index.name);
            }
            let replies = pipeline
                .query_async::<Vec<Value>>(&mut connection)
                .await
                .map_err(map_command_error)?;
            if replies.len() != chunk.len() {
                return Err(AppError::CommandFailed);
            }
            for (index, reply) in chunk.iter().zip(replies) {
                let Ok(info) = parse_search_index_info(reply) else {
                    continue;
                };
                if search_index_covers_key(&info, &input.key, &normalized_key_type) {
                    matches.push(KeySearchIndexSummary {
                        name: index.name.clone(),
                        key_type: info.key_type,
                        prefixes: info.prefixes,
                    });
                }
            }
        }
        Ok(matches)
    }

    async fn get_json_path(&self, input: GetJsonPathInput) -> Result<JsonPathValue, AppError> {
        input.validate()?;
        let context = self.capability_connection(&input.connection_id).await?;
        if !context.capabilities.json_supported {
            return Err(AppError::UnsupportedDataType);
        }
        let legacy = json_path_uses_legacy_syntax(&context.capabilities);
        let mut connection = context.connection;
        read_json_path(&mut connection, input, legacy).await
    }

    async fn set_json_path(&self, input: SetJsonPathInput) -> Result<JsonMutationResult, AppError> {
        input.validate()?;
        let context = self.capability_connection(&input.connection_id).await?;
        if !context.capabilities.json_supported {
            return Err(AppError::UnsupportedDataType);
        }
        let legacy = json_path_uses_legacy_syntax(&context.capabilities);
        let mut connection = context.connection;
        write_json_path(&mut connection, input, legacy).await
    }

    async fn append_json_array(
        &self,
        input: AppendJsonArrayInput,
    ) -> Result<JsonMutationResult, AppError> {
        input.validate()?;
        let context = self.capability_connection(&input.connection_id).await?;
        if !context.capabilities.json_supported {
            return Err(AppError::UnsupportedDataType);
        }
        let legacy = json_path_uses_legacy_syntax(&context.capabilities);
        let mut connection = context.connection;
        append_json_array_path(&mut connection, input, legacy).await
    }

    async fn delete_json_path(
        &self,
        input: DeleteJsonPathInput,
    ) -> Result<JsonMutationResult, AppError> {
        input.validate()?;
        let context = self.capability_connection(&input.connection_id).await?;
        if !context.capabilities.json_supported {
            return Err(AppError::UnsupportedDataType);
        }
        let legacy = json_path_uses_legacy_syntax(&context.capabilities);
        let mut connection = context.connection;
        delete_json_path_value(&mut connection, input, legacy).await
    }

    async fn set_key(&self, input: SetKeyInput) -> Result<KeyValue, AppError> {
        let mut connection = self.connection(&input.connection_id).await?;
        write_key(&mut connection, &input.key, &input.value).await?;
        read_key(&mut connection, &input.key).await
    }

    async fn create_key(&self, input: CreateKeyInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let exists: i64 = ::redis::cmd("EXISTS")
            .arg(&input.key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if exists > 0 {
            return Err(AppError::CommandFailed);
        }

        write_key(&mut connection, &input.key, &input.value).await?;
        if let Some(ttl_ms) = input.ttl_ms {
            apply_ttl(&mut connection, &input.key, ttl_ms).await?;
        }
        read_key(&mut connection, &input.key).await
    }

    async fn rename_key(&self, input: RenameKeyInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let renamed: i64 = ::redis::cmd("RENAMENX")
            .arg(&input.key)
            .arg(&input.new_key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if renamed == 0 {
            return Err(AppError::CommandFailed);
        }
        read_key(&mut connection, &input.new_key).await
    }

    async fn delete_key(&self, connection_id: &str, key: &str) -> Result<(), AppError> {
        let mut connection = self.connection(connection_id).await?;
        ::redis::cmd("DEL")
            .arg(key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn delete_keys(&self, input: DeleteKeysInput) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let deleted: i64 = ::redis::cmd("DEL")
            .arg(&input.keys)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(deleted).map_err(|_| AppError::CommandFailed)
    }

    async fn set_key_ttl(&self, input: SetKeyTtlInput) -> Result<i64, AppError> {
        validate_ttl(input.ttl_ms)?;
        let mut connection = self.connection(&input.connection_id).await?;
        let updated: i64 = ::redis::cmd("PEXPIRE")
            .arg(&input.key)
            .arg(input.ttl_ms)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if updated == 0 {
            return Err(AppError::CommandFailed);
        }
        ::redis::cmd("PTTL")
            .arg(&input.key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)
    }

    async fn get_key_info(&self, input: KeyInfoInput) -> Result<KeyInfo, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        read_key_info(&mut connection, &input.key).await
    }

    async fn get_stream_consumer_groups(
        &self,
        input: GetStreamConsumerGroupsInput,
    ) -> Result<Vec<StreamConsumerGroup>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let reply = ::redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(&input.key)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_stream_consumer_groups(reply)
    }

    async fn create_stream_consumer_group(
        &self,
        input: CreateStreamConsumerGroupInput,
    ) -> Result<(), AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        ::redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&input.key)
            .arg(&input.name)
            .arg(&input.last_delivered_id)
            .query_async::<String>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn delete_stream_consumer_group(
        &self,
        input: DeleteStreamConsumerGroupInput,
    ) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let deleted = ::redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(&input.key)
            .arg(&input.name)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(deleted).map_err(|_| AppError::CommandFailed)
    }

    async fn get_stream_consumers(
        &self,
        input: GetStreamConsumersInput,
    ) -> Result<Vec<StreamConsumer>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let reply = ::redis::cmd("XINFO")
            .arg("CONSUMERS")
            .arg(&input.key)
            .arg(&input.group)
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_stream_consumers(reply)
    }

    async fn get_stream_pending_entries(
        &self,
        input: GetStreamPendingEntriesInput,
    ) -> Result<Vec<StreamPendingEntry>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut command = ::redis::cmd("XPENDING");
        command
            .arg(&input.key)
            .arg(&input.group)
            .arg("-")
            .arg("+")
            .arg(input.count);
        if let Some(consumer) = input.consumer.as_deref() {
            command.arg(consumer);
        }
        let reply = command
            .query_async::<Value>(&mut connection)
            .await
            .map_err(map_command_error)?;
        parse_stream_pending_entries(reply)
    }

    async fn acknowledge_stream_pending_entries(
        &self,
        input: AcknowledgeStreamPendingEntriesInput,
    ) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let acknowledged = ::redis::cmd("XACK")
            .arg(&input.key)
            .arg(&input.group)
            .arg(&input.entries)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(acknowledged).map_err(|_| AppError::CommandFailed)
    }

    async fn delete_stream_consumer(
        &self,
        input: DeleteStreamConsumerInput,
    ) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let pending = ::redis::cmd("XGROUP")
            .arg("DELCONSUMER")
            .arg(&input.key)
            .arg(&input.group)
            .arg(&input.consumer)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(pending).map_err(|_| AppError::CommandFailed)
    }

    async fn export_keys(&self, input: ExportKeysInput) -> Result<Vec<ExportedKey>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut exported = Vec::with_capacity(input.keys.len());
        for key in input.keys {
            let value = read_key(&mut connection, &key).await?;
            exported.push(ExportedKey {
                key,
                ttl_ms: value.ttl_ms,
                value: value.value,
            });
        }
        Ok(exported)
    }

    async fn import_keys(&self, input: ImportKeysInput) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut imported = 0_u64;
        for entry in input.entries {
            let exists: i64 = ::redis::cmd("EXISTS")
                .arg(&entry.key)
                .query_async::<i64>(&mut connection)
                .await
                .map_err(map_command_error)?;
            if exists > 0 {
                continue;
            }

            write_key(&mut connection, &entry.key, &entry.value).await?;
            if entry.ttl_ms >= 0 {
                apply_ttl(&mut connection, &entry.key, entry.ttl_ms).await?;
            }
            imported += 1;
        }
        Ok(imported)
    }

    async fn get_instance_overview(
        &self,
        connection_id: &str,
    ) -> Result<InstanceOverview, AppError> {
        let (snapshot, _) = self.active_snapshot(connection_id).await?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            let token = snapshot.token;
            let topology = self
                .load_live_cluster_topology_snapshot(connection_id, snapshot)
                .await?;
            let overview = InstanceOverview::from_cluster_topology(&topology);
            self.ensure_generation_current(connection_id, token).await?;
            return Ok(overview);
        }
        let mut connection = snapshot.client.connection().await?;
        let info = ::redis::cmd("INFO")
            .query_async::<String>(&mut connection)
            .await
            .unwrap_or_default();
        let sections = parse_info_sections(&info);

        let modules = ::redis::cmd("MODULE")
            .arg("LIST")
            .query_async::<Value>(&mut connection)
            .await
            .map(parse_module_list)
            .unwrap_or_default();

        InstanceOverview::from_info_and_modules(&sections, modules)
    }

    async fn get_instance_details(&self, connection_id: &str) -> Result<InstanceDetails, AppError> {
        let (snapshot, _) = self.active_snapshot(connection_id).await?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            return Err(AppError::UnsupportedFeature);
        }
        let mut connection = snapshot.client.connection().await?;
        load_instance_details(&mut connection).await
    }

    async fn get_cluster_topology(&self, connection_id: &str) -> Result<ClusterTopology, AppError> {
        self.load_live_cluster_topology(connection_id).await
    }

    async fn refresh_cluster_topology(
        &self,
        connection_id: &str,
    ) -> Result<ClusterTopology, AppError> {
        self.load_live_cluster_topology(connection_id).await
    }

    async fn analyze_database(
        &self,
        input: AnalyzeDatabaseInput,
    ) -> Result<DatabaseAnalysisReport, AppError> {
        input.validate()?;
        let (snapshot, _) = self.active_snapshot(&input.connection_id).await?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            let connection_id = input.connection_id.clone();
            return self
                .analyze_cluster_database(&connection_id, snapshot, &input)
                .await;
        }
        let mut connection = snapshot.client.connection().await?;
        analyze_connection(&mut connection, snapshot.profile.database, &input).await
    }

    async fn get_database_overview(
        &self,
        connection_id: &str,
    ) -> Result<Vec<DatabaseOverview>, AppError> {
        let (snapshot, _) = self.active_snapshot(connection_id).await?;
        snapshot.profile.validate()?;
        if matches!(snapshot.target, ConnectionTarget::Cluster(_)) {
            return self
                .cluster_database_overview(connection_id, snapshot)
                .await;
        }
        let mut connection = snapshot.client.connection().await?;
        let info = ::redis::cmd("INFO")
            .arg("keyspace")
            .query_async::<String>(&mut connection)
            .await;

        if let Ok(info) = info {
            let sections = parse_info_sections(&info);
            let mut databases = sections
                .get("Keyspace")
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|(database, _)| database.starts_with("db"))
                        .map(|(database, line)| parse_keyspace_line(database, line))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            databases.sort_by_key(|database| database.database);
            return Ok(databases);
        }

        let key_count: u64 = ::redis::cmd("DBSIZE")
            .query_async::<u64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(vec![DatabaseOverview {
            database: snapshot.profile.database,
            key_count: Some(key_count),
            expires: None,
            avg_ttl_ms: None,
        }])
    }

    async fn select_database(
        &self,
        input: SelectDatabaseInput,
    ) -> Result<ConnectionProfile, AppError> {
        input.validate()?;
        let token = self.bump_connection_generation(&input.connection_id).await;
        let (old_profile, secrets) = self.connection_snapshot(&input.connection_id).await?;
        if old_profile.cluster.is_some() && input.database != 0 {
            return Err(AppError::UnsupportedFeature);
        }
        let mut new_profile = old_profile.clone();
        new_profile.database = input.database;
        let (handle, _) = self.connect_handle(&new_profile, &secrets).await?;
        Self::inspect_client(&handle.client).await?;

        self.publish_connection_if_current(
            &input.connection_id,
            token,
            handle,
            Some(&old_profile),
            &secrets,
        )
        .await?;
        Ok(new_profile)
    }

    async fn execute_command(
        &self,
        connection_id: &str,
        input: &str,
    ) -> Result<CommandResult, AppError> {
        let mut connection = self.connection(connection_id).await?;
        execute_tokenized_command(&mut connection, input).await
    }

    async fn execute_commands(
        &self,
        input: ExecuteCommandsInput,
    ) -> Result<Vec<CommandExecutionItem>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut items = Vec::with_capacity(input.commands.len());
        for raw_command in input.commands {
            let command = raw_command.trim().to_owned();
            match execute_tokenized_command(&mut connection, &command).await {
                Ok(result) => items.push(CommandExecutionItem {
                    command,
                    result: Some(result),
                    error_code: None,
                }),
                Err(error) => {
                    items.push(CommandExecutionItem {
                        command,
                        result: None,
                        error_code: Some(error.code().to_owned()),
                    });
                    if !input.continue_on_error {
                        break;
                    }
                }
            }
        }
        Ok(items)
    }

    fn command_catalog(&self) -> Vec<CommandDefinition> {
        crate::domain::command_catalog()
    }
}

async fn execute_tokenized_command(
    connection: &mut RoutedConnection,
    input: &str,
) -> Result<CommandResult, AppError> {
    let arguments = tokenize_command(input)?;
    connection.validate_user_command(&arguments)?;
    let mut command = ::redis::cmd(&arguments[0]);
    command.arg(&arguments[1..]);
    let value: Value = command
        .query_async::<Value>(connection)
        .await
        .map_err(map_command_error)?;
    command_result(value)
}

async fn get_slow_log_config_with_connection(
    connection: &mut RoutedConnection,
) -> Result<SlowLogConfig, AppError> {
    let reply = ::redis::cmd("CONFIG")
        .arg("GET")
        .arg("slowlog-*")
        .query_async::<Value>(connection)
        .await
        .map_err(map_command_error)?;
    parse_slow_log_config_reply(reply)
}

fn search_index_covers_key(info: &SearchIndexInfo, key: &str, redis_key_type: &str) -> bool {
    let expected_type = match redis_key_type {
        "hash" => "hash",
        "rejson-rl" | "rejson-rs" | "json" => "json",
        _ => return false,
    };
    let index_type = info.key_type.trim().to_ascii_lowercase();
    let type_matches = match expected_type {
        "hash" => index_type == "hash",
        "json" => index_type == "json",
        _ => false,
    };
    type_matches
        && (info.prefixes.is_empty() || info.prefixes.iter().any(|prefix| key.starts_with(prefix)))
}

pub(crate) async fn key_size(
    connection: &mut RoutedConnection,
    key: &str,
    key_type: &str,
) -> Result<Option<u64>, AppError> {
    let command = match key_type {
        "string" => "STRLEN",
        "hash" => "HLEN",
        "list" => "LLEN",
        "set" => "SCARD",
        "zset" => "ZCARD",
        "stream" => "XLEN",
        "array" => "ARLEN",
        "vectorset" | "vector-set" => "VCARD",
        "ReJSON-RL" | "ReJSON-RS" | "JSON" => return Ok(None),
        _ => return Ok(None),
    };
    ::redis::cmd(command)
        .arg(key)
        .query_async::<u64>(connection)
        .await
        .map(Some)
        .map_err(map_command_error)
}

async fn ensure_existing_key_type(
    connection: &mut RoutedConnection,
    key: &str,
    expected: &str,
) -> Result<(), AppError> {
    let key_type: String = ::redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    if key_type == "none" {
        return Err(AppError::KeyNotFound);
    }
    if normalize_key_type(&key_type) != Some(expected) {
        return Err(AppError::UnsupportedDataType);
    }
    Ok(())
}

async fn redis_key_exists(connection: &mut RoutedConnection, key: &str) -> Result<bool, AppError> {
    let exists: i64 = ::redis::cmd("EXISTS")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    Ok(exists > 0)
}

async fn read_vector_set_dimension(
    connection: &mut RoutedConnection,
    key: &str,
) -> Result<Option<u32>, AppError> {
    let reply = ::redis::cmd("VINFO")
        .arg(key)
        .query_async::<Value>(connection)
        .await
        .map_err(map_command_error)?;
    parse_vector_set_info(reply).map(|(dimension, _)| dimension)
}

async fn read_vector_set_element_with_connection(
    connection: &mut RoutedConnection,
    key: &str,
    element: &str,
) -> Result<VectorSetElement, AppError> {
    let mut pipeline = ::redis::pipe();
    pipeline
        .add_command(build_vemb_command(key, element)?)
        .add_command(build_vgetattr_command(key, element)?);
    let replies = pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;
    if replies.len() != 2 {
        return Err(AppError::CommandFailed);
    }
    if matches!(replies[0], Value::Nil) && matches!(replies[1], Value::Nil) {
        return Err(AppError::KeyNotFound);
    }
    parse_vector_set_element(replies[0].clone(), replies[1].clone(), element)
}

async fn load_vector_set_page_elements(
    connection: &mut RoutedConnection,
    key: &str,
    names: Vec<String>,
) -> Result<Vec<VectorSetElement>, AppError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut pipeline = ::redis::pipe();
    for name in &names {
        pipeline.add_command(build_vgetattr_command(key, name)?);
    }
    let replies = pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;
    if replies.len() != names.len() {
        return Err(AppError::CommandFailed);
    }
    names
        .into_iter()
        .zip(replies)
        .map(|(name, reply)| {
            Ok(VectorSetElement {
                name,
                score: None,
                vector_base64: None,
                attributes: parse_vector_set_attributes(reply)?,
            })
        })
        .collect()
}

async fn load_vector_set_match_attributes(
    connection: &mut RoutedConnection,
    key: &str,
    matches: &mut [VectorSimilarityMatch],
) -> Result<(), AppError> {
    if matches.is_empty() {
        return Ok(());
    }
    let mut pipeline = ::redis::pipe();
    for item in matches.iter() {
        pipeline.add_command(build_vgetattr_command(key, &item.name)?);
    }
    let replies = pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;
    if replies.len() != matches.len() {
        return Err(AppError::CommandFailed);
    }
    for (item, reply) in matches.iter_mut().zip(replies) {
        item.attributes = parse_vector_set_attributes(reply)?;
    }
    Ok(())
}

async fn apply_ttl(
    connection: &mut RoutedConnection,
    key: &str,
    ttl_ms: i64,
) -> Result<i64, AppError> {
    validate_ttl(ttl_ms)?;
    let updated: i64 = ::redis::cmd("PEXPIRE")
        .arg(key)
        .arg(ttl_ms)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    if updated == 0 {
        return Err(AppError::CommandFailed);
    }
    ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)
}

async fn read_key_info(connection: &mut RoutedConnection, key: &str) -> Result<KeyInfo, AppError> {
    let key_type: String = ::redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    if key_type == "none" {
        return Err(AppError::KeyNotFound);
    }
    let ttl_ms: i64 = ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    let size = key_size(connection, key, &key_type).await?;
    let memory_bytes = ::redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .query_async::<Option<u64>>(connection)
        .await
        .ok()
        .flatten();
    let encoding = ::redis::cmd("OBJECT")
        .arg("ENCODING")
        .arg(key)
        .query_async::<Option<String>>(connection)
        .await
        .ok()
        .flatten();
    let idle_seconds = ::redis::cmd("OBJECT")
        .arg("IDLETIME")
        .arg(key)
        .query_async::<Option<u64>>(connection)
        .await
        .ok()
        .flatten();

    Ok(KeyInfo {
        key: key.to_owned(),
        key_type,
        ttl_ms,
        size,
        memory_bytes,
        encoding,
        idle_seconds,
    })
}

async fn read_key(connection: &mut RoutedConnection, key: &str) -> Result<KeyValue, AppError> {
    read_key_mode(connection, key, false).await
}

async fn read_key_mode(
    connection: &mut RoutedConnection,
    key: &str,
    preview: bool,
) -> Result<KeyValue, AppError> {
    let key_type: String = ::redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    if key_type == "none" {
        return Err(AppError::KeyNotFound);
    }

    if preview {
        let value = match key_type.as_str() {
            "string" => Some(RedisValue::String {
                value: String::new(),
            }),
            "hash" => Some(RedisValue::Hash { fields: vec![] }),
            "list" => Some(RedisValue::List { items: vec![] }),
            "set" => Some(RedisValue::Set { members: vec![] }),
            "zset" => Some(RedisValue::SortedSet { members: vec![] }),
            "stream" => Some(RedisValue::Stream { entries: vec![] }),
            _ => None,
        };
        if let Some(value) = value {
            let ttl_ms = ::redis::cmd("PTTL")
                .arg(key)
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
            if ttl_ms == -2 {
                return Err(AppError::KeyNotFound);
            }
            return Ok(KeyValue {
                key: key.into(),
                key_type,
                ttl_ms,
                value,
            });
        }
    }
    let value = match key_type.as_str() {
        "string" => RedisValue::String {
            value: ::redis::cmd("GET")
                .arg(key)
                .query_async::<String>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "hash" => RedisValue::Hash {
            fields: ::redis::cmd("HGETALL")
                .arg(key)
                .query_async::<Vec<(String, String)>>(connection)
                .await
                .map_err(map_command_error)?
                .into_iter()
                .map(|(field, value)| HashEntry { field, value })
                .collect(),
        },
        "list" => RedisValue::List {
            items: ::redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .query_async::<Vec<String>>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "set" => RedisValue::Set {
            members: ::redis::cmd("SMEMBERS")
                .arg(key)
                .query_async::<Vec<String>>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "zset" => RedisValue::SortedSet {
            members: ::redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async::<Vec<(String, f64)>>(connection)
                .await
                .map_err(map_command_error)?
                .into_iter()
                .map(|(member, score)| SortedSetEntry { member, score })
                .collect(),
        },
        "stream" => {
            let reply: ::redis::streams::StreamRangeReply = ::redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(500)
                .query_async::<::redis::streams::StreamRangeReply>(connection)
                .await
                .map_err(map_command_error)?;
            let entries = reply
                .ids
                .into_iter()
                .map(stream_entry_from_reply)
                .collect::<Result<Vec<_>, AppError>>()?;
            RedisValue::Stream { entries }
        }
        "array" => {
            let mut pipeline = ::redis::pipe();
            pipeline
                .cmd("ARLEN")
                .arg(key)
                .cmd("ARCOUNT")
                .arg(key)
                .cmd("ARNEXT")
                .arg(key);
            let replies = pipeline
                .query_async::<Vec<Value>>(connection)
                .await
                .map_err(map_command_error)?;
            if replies.len() != 3 {
                return Err(AppError::CommandFailed);
            }
            RedisValue::Array {
                length: value_as_decimal_string(&replies[0])?,
                count: value_as_decimal_string(&replies[1])?,
            }
        }
        "vectorset" | "vector-set" => {
            let mut pipeline = ::redis::pipe();
            pipeline.cmd("VCARD").arg(key).cmd("VINFO").arg(key);
            let replies = pipeline
                .query_async::<Vec<Value>>(connection)
                .await
                .map_err(map_command_error)?;
            if replies.len() != 2 {
                return Err(AppError::CommandFailed);
            }
            let (dimension, quantization) = parse_vector_set_info_summary(replies[1].clone())?;
            RedisValue::VectorSet {
                total: value_as_decimal_string(&replies[0])?,
                dimension,
                quantization,
            }
        }
        "ReJSON-RL" | "ReJSON-RS" | "JSON" => {
            let raw: String = ::redis::cmd("JSON.GET")
                .arg(key)
                .arg(".")
                .query_async::<String>(connection)
                .await
                .map_err(map_json_command_error)?;
            RedisValue::Json {
                value: decode_json_value(&raw)?,
            }
        }
        _ => return Err(AppError::UnsupportedDataType),
    };
    let ttl_ms = ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    Ok(KeyValue {
        key: key.to_owned(),
        key_type,
        ttl_ms,
        value,
    })
}

fn value_as_decimal_string(value: &Value) -> Result<String, AppError> {
    let value = match value {
        Value::Int(value) => u64::try_from(*value)
            .map_err(|_| AppError::CommandFailed)?
            .to_string(),
        Value::BulkString(value) => {
            String::from_utf8(value.clone()).map_err(|_| AppError::CommandFailed)?
        }
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => value.clone(),
        Value::Attribute { data, .. } => return value_as_decimal_string(data),
        _ => return Err(AppError::CommandFailed),
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::CommandFailed);
    }
    value
        .parse::<u64>()
        .map(|number| number.to_string())
        .map_err(|_| AppError::CommandFailed)
}

fn stream_entry_from_reply(entry: ::redis::streams::StreamId) -> Result<StreamEntry, AppError> {
    let id = entry.id;
    let mut fields = Vec::with_capacity(entry.map.len());
    for (field, value) in entry.map {
        let value =
            ::redis::from_redis_value::<String>(value).map_err(|_| AppError::CommandFailed)?;
        fields.push((field, value));
    }
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    decode_stream_entry(
        &id,
        fields
            .into_iter()
            .flat_map(|(field, value)| [field, value])
            .collect(),
    )
}

async fn write_key(
    connection: &mut RoutedConnection,
    key: &str,
    value: &RedisValue,
) -> Result<(), AppError> {
    value.validate()?;
    match value {
        RedisValue::String { value } => {
            ::redis::cmd("SET")
                .arg(key)
                .arg(value)
                .query_async::<String>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Hash { fields } => {
            if fields.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("HSET");
            command.arg(key);
            for entry in fields {
                command.arg(&entry.field).arg(&entry.value);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::List { items } => {
            if items.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("RPUSH");
            command.arg(key);
            for item in items {
                command.arg(item);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Set { members } => {
            if members.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("SADD");
            command.arg(key);
            for member in members {
                command.arg(member);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::SortedSet { members } => {
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("ZADD");
            command.arg(key);
            for entry in members {
                command.arg(entry.score).arg(&entry.member);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Json { value } => {
            let encoded = encode_json_value(value)?;
            ::redis::cmd("JSON.SET")
                .arg(key)
                .arg(".")
                .arg(encoded)
                .query_async::<String>(connection)
                .await
                .map_err(map_json_command_error)?;
        }
        RedisValue::Stream { entries } => {
            replace_collection(connection, key).await?;
            for entry in entries {
                let encoded = encode_stream_entry(entry)?;
                ::redis::cmd("XADD")
                    .arg(key)
                    .arg(&encoded)
                    .query_async::<String>(connection)
                    .await
                    .map_err(map_command_error)?;
            }
        }
        RedisValue::Array { .. } | RedisValue::VectorSet { .. } => {
            return Err(AppError::UnsupportedFeature);
        }
    }
    Ok(())
}

async fn replace_collection(connection: &mut RoutedConnection, key: &str) -> Result<(), AppError> {
    ::redis::cmd("DEL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map(|_| ())
        .map_err(map_command_error)
}

pub(crate) fn command_result(value: Value) -> Result<CommandResult, AppError> {
    let kind = match &value {
        Value::Nil => "null",
        Value::Int(_) | Value::Double(_) => "number",
        Value::Boolean(_) => "boolean",
        Value::Array(_) | Value::Set(_) | Value::Push { .. } => "array",
        Value::Map(_) => "array",
        Value::BulkString(_)
        | Value::SimpleString(_)
        | Value::Okay
        | Value::VerbatimString { .. }
        | Value::BigNumber(_) => "string",
        Value::Attribute { data, .. } => return command_result(*data.clone()),
        Value::ServerError(error) if error.kind() == Some(::redis::ServerErrorKind::CrossSlot) => {
            return Err(AppError::CrossSlot);
        }
        Value::ServerError(_) => return Err(AppError::CommandFailed),
        _ => "unknown",
    }
    .to_owned();
    Ok(CommandResult {
        kind,
        value: value_to_json(value)?,
    })
}

fn value_to_json(value: Value) -> Result<serde_json::Value, AppError> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Int(value) => Ok(value.into()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or(AppError::CommandFailed),
        Value::Boolean(value) => Ok(value.into()),
        Value::BulkString(value) => Ok(String::from_utf8_lossy(&value).into_owned().into()),
        Value::SimpleString(value) => Ok(value.into()),
        Value::Okay => Ok("OK".into()),
        Value::Array(values) | Value::Set(values) => values
            .into_iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| {
                Ok(serde_json::json!([
                    value_to_json(key)?,
                    value_to_json(value)?
                ]))
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(serde_json::Value::Array),
        Value::Attribute { data, .. } => value_to_json(*data),
        Value::VerbatimString { text, .. } => Ok(text.into()),
        Value::BigNumber(value) => Ok(value.to_string().into()),
        Value::Push { data, .. } => data
            .into_iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::ServerError(error) if error.kind() == Some(::redis::ServerErrorKind::CrossSlot) => {
            Err(AppError::CrossSlot)
        }
        Value::ServerError(_) => Err(AppError::CommandFailed),
        _ => Err(AppError::CommandFailed),
    }
}

pub fn connection_url(
    profile: &ConnectionProfile,
    password: Option<&str>,
) -> Result<String, AppError> {
    connection_url_with_database(profile, password, profile.database)
}

async fn connect_handle_with_transport(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
    injected_ssh: Option<super::ssh::SshTransport>,
) -> Result<(ConnectionHandle, Option<ConnectionEndpoint>), AppError> {
    profile.validate()?;
    let target = ConnectionTarget::try_from(profile)?;
    let cluster_tls = if matches!(target, ConnectionTarget::Cluster(_)) && profile.tls {
        Some(tls_client_material(profile, secrets)?)
    } else {
        None
    };
    let cluster_node_factory = if matches!(target, ConnectionTarget::Cluster(_)) {
        Some(Arc::new(ClusterNodeConnectionFactory::new(
            profile.username.clone(),
            secrets.password.clone(),
            cluster_tls.clone(),
        )?))
    } else {
        None
    };
    let ssh = if injected_ssh.is_some() {
        injected_ssh
    } else if let Some(config) = profile.ssh.as_ref() {
        Some(super::ssh::SshTransport::connect(config, secrets).await?)
    } else {
        None
    };
    let (client, endpoint) =
        discover_client(profile, secrets, &target, ssh.as_ref(), cluster_tls).await?;
    Ok((
        ConnectionHandle {
            client,
            profile: profile.clone(),
            target,
            cluster_node_factory,
            _ssh: ssh,
        },
        endpoint,
    ))
}

async fn discover_client(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
    target: &ConnectionTarget,
    ssh: Option<&super::ssh::SshTransport>,
    cluster_tls: Option<TlsClientMaterial>,
) -> Result<(RoutedClient, Option<ConnectionEndpoint>), AppError> {
    match target {
        ConnectionTarget::Standalone => Ok((
            RoutedClient::Standalone(build_standalone_client(profile, secrets, ssh).await?),
            None,
        )),
        ConnectionTarget::Cluster(cluster) => Ok((
            RoutedClient::cluster(
                cluster,
                profile.username.as_deref(),
                secrets.password.as_deref(),
                cluster_tls,
            )
            .await?,
            None,
        )),
        ConnectionTarget::Sentinel(sentinel) => {
            discover_sentinel_client(profile, secrets, sentinel, ssh).await
        }
    }
}

async fn discover_sentinel_client(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
    sentinel: &crate::domain::SentinelConfig,
    ssh: Option<&super::ssh::SshTransport>,
) -> Result<(RoutedClient, Option<ConnectionEndpoint>), AppError> {
    let mut last_error = AppError::ConnectionFailed;
    for node in &sentinel.nodes {
        let attempt = async {
            let mut seed_profile = profile.clone();
            seed_profile.sentinel = None;
            seed_profile.host = node.host.clone();
            seed_profile.port = node.port;
            seed_profile.username = sentinel.username.clone();
            seed_profile.database = 0;
            seed_profile.tls = sentinel.tls;
            let mut seed_secrets = secrets.clone();
            seed_secrets.password = secrets.sentinel_password.clone();
            let seed = build_standalone_client(&seed_profile, &seed_secrets, ssh).await?;
            let mut connection = seed.connection().await?;
            let address = ::redis::cmd("SENTINEL")
                .arg("GET-MASTER-ADDR-BY-NAME")
                .arg(&sentinel.master_name)
                .query_async::<Option<(String, u16)>>(&mut connection)
                .await
                .map_err(map_connection_error)?
                .ok_or(AppError::ConnectionFailed)?;
            let endpoint = crate::domain::ConnectionEndpoint {
                host: address.0,
                port: address.1,
            };
            let mut master_profile = profile.clone();
            master_profile.sentinel = None;
            master_profile.host = endpoint.host.clone();
            master_profile.port = endpoint.port;
            let master = build_standalone_client(&master_profile, secrets, ssh).await?;
            let mut connection = master.connection().await?;
            let role = ::redis::cmd("ROLE")
                .query_async::<Vec<Value>>(&mut connection)
                .await
                .map_err(map_connection_error)?;
            if role
                .first()
                .and_then(|value| ::redis::from_redis_value::<String>(value.clone()).ok())
                .as_deref()
                != Some("master")
            {
                return Err(AppError::ConnectionFailed);
            }
            Ok((RoutedClient::Standalone(master), Some(endpoint)))
        }
        .await;
        match attempt {
            Ok(client) => return Ok(client),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn build_standalone_client(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
    ssh: Option<&super::ssh::SshTransport>,
) -> Result<StandaloneClient, AppError> {
    let direct = build_client(profile, secrets)?;
    let Some(ssh) = ssh else {
        return Ok(StandaloneClient::Direct(direct));
    };
    let original_endpoint = ConnectionEndpoint {
        host: profile.host.clone(),
        port: profile.port,
    };
    let tls = profile
        .tls
        .then(|| tls_client_material(profile, secrets))
        .transpose()?;
    TunneledClient::from_ssh_transport(
        direct
            .get_connection_info()
            .redis_settings()
            .clone()
            .set_skip_set_lib_name(),
        original_endpoint,
        tls,
        ssh,
    )
    .await
    .map(StandaloneClient::Tunneled)
}

fn tls_client_material(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
) -> Result<TlsClientMaterial, AppError> {
    let root_cert = if let Some(certificate) = secrets.ca_certificate.as_deref() {
        validate_certificate_pem(certificate)?;
        Some(certificate.as_bytes().to_vec())
    } else {
        None
    };
    let (client_cert, client_key) = match (
        secrets.client_certificate.as_deref(),
        secrets.client_key.as_deref(),
    ) {
        (Some(certificate), Some(key)) => {
            validate_certificate_pem(certificate)?;
            validate_private_key_pem(key)?;
            (
                Some(certificate.as_bytes().to_vec()),
                Some(key.as_bytes().to_vec()),
            )
        }
        (None, None) => (None, None),
        _ => return Err(AppError::InvalidInput),
    };
    Ok(TlsClientMaterial {
        root_cert,
        client_cert,
        client_key,
        verify_server_cert: profile.verify_server_cert,
    })
}

fn build_client(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
) -> Result<Client, AppError> {
    build_client_with_database(profile, secrets, profile.database)
}

fn build_client_with_database(
    profile: &ConnectionProfile,
    secrets: &ConnectionSecrets,
    database: u8,
) -> Result<Client, AppError> {
    let url = if database == profile.database {
        connection_url(profile, secrets.password.as_deref())?
    } else {
        connection_url_with_database(profile, secrets.password.as_deref(), database)?
    };
    if !profile.tls {
        return Client::open(url).map_err(|_| AppError::InvalidConnection);
    }

    let root_cert = if let Some(certificate) = secrets.ca_certificate.as_deref() {
        validate_certificate_pem(certificate)?;
        Some(certificate.as_bytes().to_vec())
    } else {
        None
    };
    let client_tls = match (
        secrets.client_certificate.as_deref(),
        secrets.client_key.as_deref(),
    ) {
        (Some(certificate), Some(key)) => {
            validate_certificate_pem(certificate)?;
            validate_private_key_pem(key)?;
            Some(ClientTlsConfig {
                client_cert: certificate.as_bytes().to_vec(),
                client_key: key.as_bytes().to_vec(),
            })
        }
        (None, None) => None,
        _ => return Err(AppError::InvalidInput),
    };

    Client::build_with_tls(
        url,
        TlsCertificates {
            client_tls,
            root_cert,
        },
    )
    .map_err(|_| AppError::InvalidInput)
}

pub fn connection_url_with_database(
    profile: &ConnectionProfile,
    password: Option<&str>,
    database: u8,
) -> Result<String, AppError> {
    profile.validate()?;
    if database > 15 {
        return Err(AppError::InvalidConnection);
    }
    let host = standalone_host(&profile.host)?;
    let credentials = match (profile.username.as_deref(), password) {
        (Some(username), Some(password)) => {
            format!("{}:{}@", percent_encode(username), percent_encode(password))
        }
        (Some(username), None) => format!("{}@", percent_encode(username)),
        (None, Some(password)) => format!(":{}@", percent_encode(password)),
        (None, None) => String::new(),
    };
    let scheme = if profile.tls { "rediss" } else { "redis" };
    let insecure_fragment = if profile.tls && !profile.verify_server_cert {
        "#insecure"
    } else {
        ""
    };
    Ok(format!(
        "{scheme}://{credentials}{host}:{}/{}{insecure_fragment}",
        profile.port, database
    ))
}

pub fn validate_ttl(ttl_ms: i64) -> Result<(), AppError> {
    if ttl_ms < 0 {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

pub(super) fn standalone_host(host: &str) -> Result<String, AppError> {
    let host = host.trim();
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(format!("[{host}]"));
    }
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    {
        return Err(AppError::InvalidConnection);
    }
    Ok(host.to_owned())
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn map_connection_error(error: ::redis::RedisError) -> AppError {
    if error.kind() == ::redis::ErrorKind::AuthenticationFailed {
        AppError::AuthenticationFailed
    } else {
        AppError::ConnectionFailed
    }
}

fn validate_connection_id(connection_id: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty() {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

pub(crate) fn map_command_error(error: ::redis::RedisError) -> AppError {
    match error.kind() {
        ::redis::ErrorKind::Server(::redis::ServerErrorKind::CrossSlot) => AppError::CrossSlot,
        ::redis::ErrorKind::AuthenticationFailed => AppError::AuthenticationFailed,
        ::redis::ErrorKind::Io => AppError::ConnectionFailed,
        _ => AppError::CommandFailed,
    }
}

fn is_unknown_command_error(error: &::redis::RedisError) -> bool {
    error.detail().is_some_and(|detail| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("unknown command") || detail.contains("unknown subcommand")
    })
}

fn is_multi_section_info_unsupported(error: &::redis::RedisError) -> bool {
    error.code() == Some("ERR")
        && error.detail().is_some_and(|detail| {
            detail == "syntax error"
                || detail.eq_ignore_ascii_case("wrong number of arguments for 'info' command")
        })
}

fn is_with_attributes_unsupported(error: &::redis::RedisError) -> bool {
    error.detail().is_some_and(|detail| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("withattribs")
            && (detail.contains("unknown")
                || detail.contains("unsupported")
                || detail.contains("not supported")
                || detail.contains("syntax")
                || detail.contains("invalid"))
    })
}

pub(crate) fn map_json_command_error(error: ::redis::RedisError) -> AppError {
    if error.kind() == ::redis::ErrorKind::Server(::redis::ServerErrorKind::CrossSlot) {
        return AppError::CrossSlot;
    }
    let unsupported = error.detail().is_some_and(|detail| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("unknown command")
            || detail.contains("unknown subcommand")
            || detail.contains("module command")
    });
    if unsupported {
        AppError::UnsupportedDataType
    } else {
        map_command_error(error)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::oneshot;

    use crate::{
        domain::{
            ClusterConfig, ConnectionEndpoint, ConnectionProfile, ConnectionTarget,
            GetSlowLogsInput, GetStreamConsumerGroupsInput, GetStreamPendingEntriesInput,
            ModuleCapabilities, ModuleSummary, PublishPubSubInput, SelectDatabaseInput,
            SentinelConfig, StopProfilerInput,
        },
        error::AppError,
        persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    };

    use super::{
        build_client, build_standalone_client, command_result, connection_url,
        discover_sentinel_client, validate_ttl, ConnectionHandle, RedisClusterScanBackend,
        RedisOperations, RedisService,
    };

    struct EmptyProfiles;

    impl ProfileRepository for EmptyProfiles {
        fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
            Ok(Vec::new())
        }

        fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
            Ok(())
        }
    }

    struct EmptySecrets;

    impl SecretStore for EmptySecrets {
        fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(None)
        }

        fn write(
            &self,
            _connection_id: &str,
            _secrets: &ConnectionSecrets,
        ) -> Result<(), AppError> {
            Ok(())
        }

        fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    fn valid_profile() -> ConnectionProfile {
        ConnectionProfile {
            ssh: None,
            sentinel: None,
            cluster: None,
            id: "local".into(),
            name: "Local".into(),
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            database: 0,
            has_password: false,
            tls: false,
            verify_server_cert: true,
            ca_certificate_name: None,
            client_certificate_name: None,
            has_ca_certificate: false,
            has_client_certificate: false,
        }
    }

    async fn spawn_cluster_scan_node(cursor: u64, keys: Vec<Vec<u8>>) -> ConnectionEndpoint {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            loop {
                let mut line = String::new();
                if stream.read_line(&mut line).await.unwrap() == 0 {
                    return;
                }
                let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                let mut args = Vec::with_capacity(count);
                for _ in 0..count {
                    line.clear();
                    stream.read_line(&mut line).await.unwrap();
                    let length: usize = line.trim().strip_prefix('$').unwrap().parse().unwrap();
                    let mut data = vec![0_u8; length + 2];
                    stream.read_exact(&mut data).await.unwrap();
                    args.push(data[..length].to_vec());
                }
                if args.first().map(Vec::as_slice) == Some(b"SCAN") {
                    let mut reply = format!(
                        "*2\r\n${}\r\n{}\r\n*{}\r\n",
                        cursor.to_string().len(),
                        cursor,
                        keys.len()
                    )
                    .into_bytes();
                    for key in &keys {
                        reply.extend_from_slice(format!("${}\r\n", key.len()).as_bytes());
                        reply.extend_from_slice(key);
                        reply.extend_from_slice(b"\r\n");
                    }
                    stream.get_mut().write_all(&reply).await.unwrap();
                } else {
                    stream.get_mut().write_all(b"+OK\r\n").await.unwrap();
                }
            }
        });
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port,
        }
    }

    #[tokio::test]
    async fn production_cluster_backend_keeps_cursor_when_a_node_returns_binary_keys() {
        let visible_endpoint = spawn_cluster_scan_node(17, vec![b"visible".to_vec()]).await;
        let binary_endpoint = spawn_cluster_scan_node(23, vec![vec![0xff, 0, b'k']]).await;
        let nodes = vec![
            crate::redis::ClusterScanNode {
                node_id: "visible-node".into(),
                endpoint: visible_endpoint,
            },
            crate::redis::ClusterScanNode {
                node_id: "binary-node".into(),
                endpoint: binary_endpoint,
            },
        ];
        let initial = crate::redis::ClusterScanState::new(
            5,
            vec![
                crate::redis::NodeScanCursor {
                    node_id: "visible-node".into(),
                    cursor: 0,
                },
                crate::redis::NodeScanCursor {
                    node_id: "binary-node".into(),
                    cursor: 41,
                },
            ],
        )
        .encode()
        .unwrap();
        let backend = RedisClusterScanBackend {
            factory: Arc::new(
                crate::redis::ClusterNodeConnectionFactory::new(None, None, None).unwrap(),
            ),
        };

        let page = crate::redis::scan_cluster(&backend, 5, &nodes, Some(&initial), "*", 100)
            .await
            .unwrap();
        assert_eq!(page.keys, vec![b"visible".to_vec()]);
        assert_eq!(page.node_failures.len(), 1);
        assert_eq!(page.node_failures[0].node_id, "binary-node");
        assert_eq!(
            page.node_failures[0].code,
            AppError::ClusterNodeUnavailable.code()
        );
        let known =
            std::collections::HashSet::from(["visible-node".to_owned(), "binary-node".to_owned()]);
        let state = crate::redis::ClusterScanState::decode(&page.cursor, 5, &known).unwrap();
        assert_eq!(state.cursor_for("visible-node"), Some(17));
        assert_eq!(state.cursor_for("binary-node"), Some(41));
    }

    #[tokio::test]
    async fn production_cluster_backend_records_only_successful_endpoints_in_shared_factory() {
        let successful_endpoint = spawn_cluster_scan_node(0, vec![b"visible".to_vec()]).await;
        let factory =
            Arc::new(crate::redis::ClusterNodeConnectionFactory::new(None, None, None).unwrap());
        let backend = RedisClusterScanBackend {
            factory: factory.clone(),
        };
        let successful_node = crate::redis::ClusterScanNode {
            node_id: "recorded-node".into(),
            endpoint: successful_endpoint.clone(),
        };

        let page = crate::redis::scan_cluster(
            &backend,
            6,
            std::slice::from_ref(&successful_node),
            None,
            "*",
            100,
        )
        .await
        .unwrap();
        assert!(page.node_failures.is_empty());
        assert_eq!(
            factory.connection_endpoint("recorded-node"),
            Some(successful_endpoint.clone())
        );

        let failed_node = crate::redis::ClusterScanNode {
            node_id: "recorded-node".into(),
            endpoint: ConnectionEndpoint {
                host: "?".into(),
                port: successful_endpoint.port,
            },
        };
        let failed_page = crate::redis::scan_cluster(
            &backend,
            7,
            std::slice::from_ref(&failed_node),
            None,
            "*",
            100,
        )
        .await
        .unwrap();
        assert_eq!(failed_page.node_failures.len(), 1);
        assert_eq!(
            factory.connection_endpoint("recorded-node"),
            Some(successful_endpoint)
        );
        assert_eq!(factory.connection_endpoint("unknown-node"), None);
    }

    const TEST_TLS_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIBxTCCAWugAwIBAgIUJrqzrus0OeHJc4plw8TswbjYNhAwCgYIKoZIzj0EAwIw
GDEWMBQGA1UEAwwNUmVkaXggVGVzdCBDQTAeFw0yNjA5MDIwOTIzMDRaFw0zNjA4
MzAwOTIzMDRaMBkxFzAVBgNVBAMMDmNhY2hlLmludGVybmFsMFkwEwYHKoZIzj0C
AQYIKoZIzj0DAQcDQgAE9vYoE+AAk3CmXwFFM/EtIcDg4oscPWEiTb+FOm0VsiIu
973FLq4/eXTDtzxbZ/TlTgxUYq0+zKgSTNzo7bZ7E6OBkTCBjjAMBgNVHRMBAf8E
AjAAMA4GA1UdDwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEFBQcDATAZBgNVHREE
EjAQgg5jYWNoZS5pbnRlcm5hbDAdBgNVHQ4EFgQUW2ST5EeDywrEQXCq6V/szpaX
SnkwHwYDVR0jBBgwFoAU8665SzPKr5cvfi3TMbORTiT7Av4wCgYIKoZIzj0EAwID
SAAwRQIhAIf4cSkcsInz1Gs/XcFflIkmZzkM5ikRYN598hQP19WIAiBxGKqXQQmD
kOq6G4FuxZ3fU3d5yW+BLVwSneRiIaCbww==
-----END CERTIFICATE-----
"#;
    const TEST_TLS_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgmIYd7cs7Gxzdq0rS
Ymnw0YzaAQkbf2ywPQ+isYIgv++hRANCAAT29igT4ACTcKZfAUUz8S0hwODiixw9
YSJNv4U6bRWyIi73vcUurj95dMO3PFtn9OVODFRirT7MqBJM3OjttnsT
-----END PRIVATE KEY-----
"#;

    async fn spawn_tls_sentinel_fixture(response: String) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
        let certs = rustls_pemfile::certs(&mut TEST_TLS_CERT.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let key = rustls_pemfile::private_key(&mut TEST_TLS_KEY.as_bytes())
            .unwrap()
            .unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_names = Arc::new(Mutex::new(Vec::new()));
        let observed = server_names.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let config = Arc::new(config.clone());
                let observed = observed.clone();
                let response = response.clone();
                tokio::spawn(async move {
                    let Ok(start) = tokio_rustls::LazyConfigAcceptor::new(
                        rustls::server::Acceptor::default(),
                        socket,
                    )
                    .await
                    else {
                        return;
                    };
                    observed.lock().unwrap().push(
                        start
                            .client_hello()
                            .server_name()
                            .unwrap_or_default()
                            .to_owned(),
                    );
                    let Ok(mut stream) = start.into_stream(config).await else {
                        return;
                    };
                    let mut buffer = [0_u8; 4096];
                    loop {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(_) => stream.write_all(response.as_bytes()).await.unwrap(),
                        }
                    }
                });
            }
        });
        (address, server_names)
    }

    async fn spawn_recording_sentinel_fixture(
        discovered: Option<ConnectionEndpoint>,
        primary: bool,
    ) -> (SocketAddr, Arc<Mutex<Vec<Vec<String>>>>, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let observed_commands = Arc::clone(&commands);
        let observed_active = Arc::clone(&active);
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                observed_active.fetch_add(1, Ordering::SeqCst);
                let commands = Arc::clone(&observed_commands);
                let active = Arc::clone(&observed_active);
                let discovered = discovered.clone();
                tokio::spawn(async move {
                    struct ActiveGuard(Arc<AtomicUsize>);
                    impl Drop for ActiveGuard {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _active = ActiveGuard(active);
                    let mut stream = BufReader::new(stream);
                    loop {
                        let mut line = String::new();
                        if stream.read_line(&mut line).await.unwrap() == 0 {
                            return;
                        }
                        let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                        let mut args = Vec::with_capacity(count);
                        for _ in 0..count {
                            line.clear();
                            stream.read_line(&mut line).await.unwrap();
                            let length: usize =
                                line.trim().strip_prefix('$').unwrap().parse().unwrap();
                            let mut data = vec![0; length + 2];
                            stream.read_exact(&mut data).await.unwrap();
                            args.push(String::from_utf8(data[..length].to_vec()).unwrap());
                        }
                        commands.lock().unwrap().push(args.clone());
                        let response = match args.first().map(String::as_str) {
                            Some("AUTH") => "+OK\r\n".into(),
                            Some("SENTINEL") => discovered.as_ref().map_or_else(
                                || "$-1\r\n".into(),
                                |endpoint| {
                                    format!(
                                        "*2\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
                                        endpoint.host.len(),
                                        endpoint.host,
                                        endpoint.port.to_string().len(),
                                        endpoint.port
                                    )
                                },
                            ),
                            Some("ROLE") if primary => "*1\r\n$6\r\nmaster\r\n".into(),
                            Some("PING") => "+PONG\r\n".into(),
                            Some("INFO") => "$21\r\nredis_version:7.0.0\r\n\r\n".into(),
                            _ => "+OK\r\n".into(),
                        };
                        stream
                            .get_mut()
                            .write_all(response.as_bytes())
                            .await
                            .unwrap();
                    }
                });
            }
        });
        (address, commands, active)
    }

    async fn wait_for_connection_count(active: &AtomicUsize, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while active.load(Ordering::SeqCst) != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn sentinel_over_ssh_tls_forwards_each_target_and_uses_original_sni_without_direct_dials()
    {
        let (unavailable_sentinel_address, unavailable_sni) =
            spawn_tls_sentinel_fixture("$-1\r\n".into()).await;
        let (sentinel_address, sentinel_sni) =
            spawn_tls_sentinel_fixture("*2\r\n$16\r\nprimary.internal\r\n$4\r\n6379\r\n".into())
                .await;
        let (primary_address, primary_sni) =
            spawn_tls_sentinel_fixture("*1\r\n$6\r\nmaster\r\n".into()).await;
        let unavailable_sentinel_endpoint = ConnectionEndpoint {
            host: "sentinel-one.internal".into(),
            port: 26379,
        };
        let sentinel_endpoint = ConnectionEndpoint {
            host: "sentinel-two.internal".into(),
            port: 26379,
        };
        let primary_endpoint = ConnectionEndpoint {
            host: "primary.internal".into(),
            port: 6379,
        };
        let (transport, backend) = crate::redis::ssh::test_forwarding_transport(vec![
            (
                unavailable_sentinel_endpoint.clone(),
                unavailable_sentinel_address,
            ),
            (sentinel_endpoint.clone(), sentinel_address),
            (primary_endpoint.clone(), primary_address),
        ]);
        let mut profile = valid_profile();
        profile.host = "sentinel-one.internal".into();
        profile.port = 26379;
        profile.tls = true;
        profile.verify_server_cert = false;
        profile.ssh = Some(
            serde_json::from_value(serde_json::json!({
                "host": "jump.internal",
                "port": 22,
                "username": "operator"
            }))
            .unwrap(),
        );
        let sentinel = SentinelConfig {
            master_name: "redix-primary".into(),
            nodes: vec![
                unavailable_sentinel_endpoint.clone(),
                sentinel_endpoint.clone(),
            ],
            username: None,
            has_password: false,
            tls: true,
        };
        profile.sentinel = Some(sentinel.clone());

        let result = discover_sentinel_client(
            &profile,
            &ConnectionSecrets::default(),
            &sentinel,
            Some(&transport),
        )
        .await;
        assert!(
            result.is_ok(),
            "sentinel discovery failed after forwards {:?}: {:?}",
            backend.forwarded_targets(),
            result.as_ref().err()
        );
        let (client, endpoint) = result.unwrap();
        assert_eq!(endpoint, Some(primary_endpoint.clone()));
        assert!(client.standalone_client().is_ok());
        assert_eq!(
            backend.forwarded_targets(),
            vec![
                unavailable_sentinel_endpoint,
                sentinel_endpoint,
                primary_endpoint
            ]
        );
        assert_eq!(
            unavailable_sni.lock().unwrap().as_slice(),
            ["sentinel-one.internal"]
        );
        assert_eq!(
            sentinel_sni.lock().unwrap().as_slice(),
            ["sentinel-two.internal"]
        );
        assert_eq!(primary_sni.lock().unwrap().as_slice(), ["primary.internal"]);
    }

    #[tokio::test]
    async fn sentinel_ssh_service_publication_keeps_credentials_and_forward_lifetimes_separate() {
        let failed_seed_endpoint = ConnectionEndpoint {
            host: "sentinel-one.internal".into(),
            port: 26379,
        };
        let seed_endpoint = ConnectionEndpoint {
            host: "sentinel-two.internal".into(),
            port: 26379,
        };
        let primary_endpoint = ConnectionEndpoint {
            host: "primary.internal".into(),
            port: 6379,
        };
        let (failed_seed_address, failed_seed_commands, failed_seed_active) =
            spawn_recording_sentinel_fixture(None, false).await;
        let (seed_address, seed_commands, _) =
            spawn_recording_sentinel_fixture(Some(primary_endpoint.clone()), false).await;
        let (primary_address, primary_commands, _) =
            spawn_recording_sentinel_fixture(None, true).await;
        let (transport, backend) = crate::redis::ssh::test_forwarding_transport(vec![
            (failed_seed_endpoint.clone(), failed_seed_address),
            (seed_endpoint.clone(), seed_address),
            (primary_endpoint.clone(), primary_address),
        ]);
        let mut profile = valid_profile();
        profile.host = failed_seed_endpoint.host.clone();
        profile.port = failed_seed_endpoint.port;
        profile.username = Some("redis-user".into());
        profile.has_password = true;
        profile.ssh = Some(
            serde_json::from_value(serde_json::json!({
                "host": "jump.internal",
                "port": 22,
                "username": "operator"
            }))
            .unwrap(),
        );
        profile.sentinel = Some(SentinelConfig {
            master_name: "redix-primary".into(),
            nodes: vec![failed_seed_endpoint.clone(), seed_endpoint.clone()],
            username: Some("sentinel-user".into()),
            has_password: true,
            tls: false,
        });
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![profile])));
        let secrets = Arc::new(MutableSecrets(Mutex::new(Some(ConnectionSecrets {
            password: Some("redis-pass".into()),
            sentinel_password: Some("sentinel-pass".into()),
            ..ConnectionSecrets::default()
        }))));
        let mut service =
            RedisService::new(profiles, secrets).with_test_ssh_transport(transport.clone());

        service.open_connection("local").await.unwrap();
        wait_for_connection_count(failed_seed_active.as_ref(), 0).await;
        let primary_local_endpoint = {
            let active = service.active.read().await;
            let handle = active.get("local").unwrap();
            let crate::redis::RoutedClient::Standalone(crate::redis::StandaloneClient::Tunneled(
                client,
            )) = &handle.client
            else {
                panic!("sentinel SSH publication must retain the tunneled primary")
            };
            client.local_endpoint.clone()
        };
        drop(service.test_ssh_transport.take());
        drop(transport);
        assert_eq!(
            service
                .execute_command("local", "PING")
                .await
                .unwrap()
                .value,
            serde_json::json!("PONG")
        );

        for commands in [&failed_seed_commands, &seed_commands] {
            let commands = commands.lock().unwrap();
            assert!(commands.iter().any(|command| {
                command
                    == &[
                        "AUTH".to_owned(),
                        "sentinel-user".to_owned(),
                        "sentinel-pass".to_owned(),
                    ]
            }));
            assert!(!commands.iter().flatten().any(|arg| arg == "redis-pass"));
        }
        let primary_commands = primary_commands.lock().unwrap();
        assert!(primary_commands.iter().any(|command| {
            command
                == &[
                    "AUTH".to_owned(),
                    "redis-user".to_owned(),
                    "redis-pass".to_owned(),
                ]
        }));
        assert!(!primary_commands
            .iter()
            .flatten()
            .any(|arg| arg == "sentinel-pass"));
        drop(primary_commands);
        let forwarded = backend.forwarded_targets();
        assert_eq!(forwarded[0], failed_seed_endpoint);
        assert_eq!(forwarded[1], seed_endpoint);
        assert!(forwarded[2..]
            .iter()
            .all(|target| target == &primary_endpoint));

        assert_eq!(
            tokio::net::TcpListener::bind((
                primary_local_endpoint.host.as_str(),
                primary_local_endpoint.port,
            ))
            .await
            .unwrap_err()
            .kind(),
            std::io::ErrorKind::AddrInUse,
        );
        service.close_connection("local").await.unwrap();
        // Windows 连接已关闭的端口可能等待数秒；重绑直接验证监听器已释放。
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                match tokio::net::TcpListener::bind((
                    primary_local_endpoint.host.as_str(),
                    primary_local_endpoint.port,
                ))
                .await
                {
                    Ok(listener) => break listener,
                    Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                    Err(error) => panic!("failed to rebind primary SSH forward listener: {error}"),
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("closing the handle must release the primary SSH forward listener");
    }

    #[tokio::test]
    async fn standalone_ssh_tls_keeps_the_original_endpoint_for_sni_and_owns_its_forward() {
        let original = ConnectionEndpoint {
            host: "cache.internal".into(),
            port: 6380,
        };
        let (_unused_listener, address) = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            (listener, address)
        };
        let (transport, _) =
            crate::redis::ssh::test_forwarding_transport(vec![(original.clone(), address)]);
        let mut profile = valid_profile();
        profile.host = original.host.clone();
        profile.port = original.port;
        profile.tls = true;
        profile.verify_server_cert = false;
        profile.ssh = Some(
            serde_json::from_value(serde_json::json!({
                "host": "jump.internal",
                "port": 22,
                "username": "operator"
            }))
            .unwrap(),
        );

        let client =
            build_standalone_client(&profile, &ConnectionSecrets::default(), Some(&transport))
                .await
                .unwrap();
        let crate::redis::StandaloneClient::Tunneled(client) = client else {
            panic!("SSH + TLS must use the tunneled standalone path");
        };
        assert_eq!(client.original_endpoint, original);
        assert_eq!(client.local_endpoint.host, "127.0.0.1");
        assert!(client.tls.is_some());
    }

    struct MutableProfiles(Mutex<Vec<ConnectionProfile>>);

    impl ProfileRepository for MutableProfiles {
        fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError> {
            *self.0.lock().unwrap() = profiles.to_vec();
            Ok(())
        }
    }

    struct MutableSecrets(Mutex<Option<ConnectionSecrets>>);

    impl SecretStore for MutableSecrets {
        fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn write(&self, _: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
            *self.0.lock().unwrap() = Some(secrets.clone());
            Ok(())
        }

        fn delete(&self, _: &str) -> Result<(), AppError> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn ssh_connection_snapshot_loads_migrated_local_paths_from_secret_store() {
        let mut profile = valid_profile();
        profile.ssh = Some(
            serde_json::from_value(serde_json::json!({
                "host": "jump.example", "port": 22, "username": "operator",
                "has_identity_file": true, "has_known_hosts_file": true
            }))
            .unwrap(),
        );
        let expected = ConnectionSecrets {
            ssh_identity_file: Some("/private/key".into()),
            ssh_known_hosts_file: Some("/private/known_hosts".into()),
            ..Default::default()
        };
        let service = RedisService::new(
            Arc::new(MutableProfiles(Mutex::new(vec![profile.clone()]))),
            Arc::new(MutableSecrets(Mutex::new(Some(expected.clone())))),
        );
        assert_eq!(service.connection_secrets(&profile).unwrap(), expected);
    }

    // Pause the real client's PING so tests can order lifecycle operations without sleeps.
    struct PausedRedis {
        port: u16,
        ping: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl PausedRedis {
        async fn start() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let (ping_tx, ping) = oneshot::channel();
            let (resume, resume_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut gate = Some((ping_tx, resume_rx));
                loop {
                    let mut line = String::new();
                    if stream.read_line(&mut line).await.unwrap() == 0 {
                        break;
                    }
                    let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                    let mut args = Vec::new();
                    for _ in 0..count {
                        line.clear();
                        stream.read_line(&mut line).await.unwrap();
                        let length: usize = line.trim().strip_prefix('$').unwrap().parse().unwrap();
                        let mut data = vec![0; length + 2];
                        stream.read_exact(&mut data).await.unwrap();
                        args.push(String::from_utf8(data[..length].to_vec()).unwrap());
                    }
                    let response = match args[0].as_str() {
                        "PING" => {
                            let (ping_tx, resume_rx) = gate.take().unwrap();
                            ping_tx.send(()).unwrap();
                            resume_rx.await.unwrap();
                            "+PONG\r\n"
                        }
                        "INFO" => "$21\r\nredis_version:7.0.0\r\n\r\n",
                        _ => "+OK\r\n",
                    };
                    stream.write_all(response.as_bytes()).await.unwrap();
                }
            });
            Self {
                port,
                ping,
                resume: Some(resume),
                task,
            }
        }

        async fn wait_for_ping(&mut self) {
            tokio::time::timeout(Duration::from_secs(5), &mut self.ping)
                .await
                .unwrap()
                .unwrap();
        }

        fn resume(&mut self) {
            self.resume.take().unwrap().send(()).unwrap();
        }

        fn profile(&self) -> ConnectionProfile {
            let mut profile = valid_profile();
            profile.port = self.port;
            profile
        }
    }

    impl Drop for PausedRedis {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    #[tokio::test]
    async fn lifecycle_close_cancels_an_open_waiting_for_redis() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles, Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let opening = tokio::spawn(async move { opening_service.open_connection("local").await });
        server.wait_for_ping().await;

        service.close_connection("local").await.unwrap();
        server.resume();

        assert_eq!(opening.await.unwrap(), Err(AppError::OperationCancelled));
        assert!(service.client("local").await.is_err());
        assert!(!service.capabilities.read().await.contains_key("local"));
    }

    #[tokio::test]
    async fn lifecycle_open_rejects_a_profile_edited_during_connection() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let opening = tokio::spawn(async move { opening_service.open_connection("local").await });
        server.wait_for_ping().await;
        let mut edited = server.profile();
        edited.name = "Edited".into();
        profiles.save(&[edited]).unwrap();
        server.resume();

        assert_eq!(opening.await.unwrap(), Err(AppError::OperationCancelled));
        assert!(service.client("local").await.is_err());
    }

    async fn assert_rotation_cancels_connection(select_database: bool) {
        let mut server = PausedRedis::start().await;
        let mut profile = server.profile();
        profile.has_password = true;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![profile])));
        let secrets = Arc::new(MutableSecrets(Mutex::new(Some(ConnectionSecrets {
            password: Some("original".into()),
            ..ConnectionSecrets::default()
        }))));
        let service = Arc::new(RedisService::new(profiles.clone(), secrets.clone()));
        let opening_service = Arc::clone(&service);
        let opening = tokio::spawn(async move {
            if select_database {
                opening_service
                    .select_database(SelectDatabaseInput {
                        connection_id: "local".into(),
                        database: 2,
                    })
                    .await
                    .map(|_| ())
            } else {
                opening_service.open_connection("local").await.map(|_| ())
            }
        });
        server.wait_for_ping().await;
        secrets
            .write(
                "local",
                &ConnectionSecrets {
                    password: Some("rotated".into()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
        server.resume();

        assert_eq!(opening.await.unwrap(), Err(AppError::OperationCancelled));
        assert!(service.client("local").await.is_err());
        assert_eq!(profiles.load().unwrap()[0].database, 0);
    }

    #[tokio::test]
    async fn lifecycle_open_rejects_credentials_rotated_during_connection() {
        assert_rotation_cancels_connection(false).await;
    }

    #[tokio::test]
    async fn lifecycle_open_snapshot_waits_for_profile_transaction_rollback() {
        let mut server = PausedRedis::start().await;
        let original = server.profile();
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![original.clone()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let transaction = service.profile_transaction().await;
        let mut uncommitted = original.clone();
        uncommitted.name = "Uncommitted".into();
        profiles.save(&[uncommitted]).unwrap();
        let opening_service = Arc::clone(&service);
        let opening = tokio::spawn(async move { opening_service.open_connection("local").await });
        let ping_arrived = tokio::time::timeout(Duration::from_millis(100), &mut server.ping).await;
        profiles.save(&[original.clone()]).unwrap();
        drop(transaction);
        if ping_arrived.is_err() {
            server.wait_for_ping().await;
        }
        server.resume();
        opening.await.unwrap().unwrap();

        assert_eq!(service.profile("local").await.unwrap(), original);
    }

    #[tokio::test]
    async fn lifecycle_database_publication_waits_for_profile_transaction_rollback() {
        let mut server = PausedRedis::start().await;
        let original = server.profile();
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![original.clone()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let selecting_service = Arc::clone(&service);
        let mut selecting = tokio::spawn(async move {
            selecting_service
                .select_database(SelectDatabaseInput {
                    connection_id: "local".into(),
                    database: 2,
                })
                .await
        });
        server.wait_for_ping().await;
        let transaction = service.profile_transaction().await;
        server.resume();
        let early_result = tokio::time::timeout(Duration::from_millis(100), &mut selecting).await;
        profiles.save(&[original]).unwrap();
        drop(transaction);
        match early_result {
            Ok(result) => {
                result.unwrap().unwrap();
            }
            Err(_) => {
                selecting.await.unwrap().unwrap();
            }
        }

        assert_eq!(profiles.load().unwrap()[0].database, 2);
        assert_eq!(service.profile("local").await.unwrap().database, 2);
    }

    #[tokio::test]
    async fn lifecycle_close_can_cancel_publication_waiting_for_profile_transaction() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles, Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let mut opening =
            tokio::spawn(async move { opening_service.open_connection("local").await });
        server.wait_for_ping().await;
        let transaction = service.profile_transaction().await;
        server.resume();
        let early_result = tokio::time::timeout(Duration::from_millis(100), &mut opening).await;
        let close_result =
            tokio::time::timeout(Duration::from_secs(1), service.close_connection("local")).await;
        drop(transaction);

        close_result
            .expect(
                "publication must not hold the generation lock while waiting for the transaction",
            )
            .unwrap();
        let result = match early_result {
            Ok(result) => result.unwrap(),
            Err(_) => opening.await.unwrap(),
        };
        assert_eq!(result, Err(AppError::OperationCancelled));
        assert!(service.client("local").await.is_err());
    }

    #[tokio::test]
    async fn lifecycle_database_selection_rejects_credentials_rotated_during_connection() {
        assert_rotation_cancels_connection(true).await;
    }

    #[tokio::test]
    async fn lifecycle_old_open_cannot_replace_a_newer_connection() {
        let mut first = PausedRedis::start().await;
        let mut second = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![first.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let older = tokio::spawn(async move { opening_service.open_connection("local").await });
        first.wait_for_ping().await;

        profiles.save(&[second.profile()]).unwrap();
        let opening_service = Arc::clone(&service);
        let newer = tokio::spawn(async move { opening_service.open_connection("local").await });
        second.wait_for_ping().await;
        second.resume();
        newer.await.unwrap().unwrap();
        first.resume();

        assert_eq!(older.await.unwrap(), Err(AppError::OperationCancelled));
        assert_eq!(service.profile("local").await.unwrap().port, second.port);
    }

    #[tokio::test]
    async fn lifecycle_new_open_cancels_older_attempt_before_it_finishes_connecting() {
        let mut first = PausedRedis::start().await;
        let mut second = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![first.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let older = tokio::spawn(async move { opening_service.open_connection("local").await });
        first.wait_for_ping().await;
        profiles.save(&[second.profile()]).unwrap();
        let opening_service = Arc::clone(&service);
        let newer = tokio::spawn(async move { opening_service.open_connection("local").await });
        second.wait_for_ping().await;

        first.resume();
        assert_eq!(older.await.unwrap(), Err(AppError::OperationCancelled));
        assert!(service.client("local").await.is_err());
        second.resume();
        newer.await.unwrap().unwrap();
        assert_eq!(service.profile("local").await.unwrap().port, second.port);
    }

    #[tokio::test]
    async fn lifecycle_publication_expires_capability_probes_started_during_open() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles, Arc::new(EmptySecrets)));
        let opening_service = Arc::clone(&service);
        let opening = tokio::spawn(async move { opening_service.open_connection("local").await });
        server.wait_for_ping().await;
        let token = service.capture_capability_token("local").await;
        server.resume();
        opening.await.unwrap().unwrap();

        assert!(
            !service
                .cache_capabilities_if_current(
                    "local",
                    token,
                    ModuleCapabilities::from_modules(Vec::new()),
                )
                .await
        );
        assert!(!service.capabilities.read().await.contains_key("local"));
    }

    #[tokio::test]
    async fn lifecycle_close_prevents_pending_database_selection_from_restoring_deleted_profile() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let selecting_service = Arc::clone(&service);
        let selection = tokio::spawn(async move {
            selecting_service
                .select_database(SelectDatabaseInput {
                    connection_id: "local".into(),
                    database: 2,
                })
                .await
        });
        server.wait_for_ping().await;

        profiles.save(&[]).unwrap();
        service.close_connection("local").await.unwrap();
        server.resume();

        assert_eq!(selection.await.unwrap(), Err(AppError::OperationCancelled));
        assert!(profiles.load().unwrap().is_empty());
        assert!(service.client("local").await.is_err());
    }

    #[tokio::test]
    async fn lifecycle_database_selection_does_not_overwrite_profile_edits() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let selecting_service = Arc::clone(&service);
        let selection = tokio::spawn(async move {
            selecting_service
                .select_database(SelectDatabaseInput {
                    connection_id: "local".into(),
                    database: 2,
                })
                .await
        });
        server.wait_for_ping().await;
        let mut edited = server.profile();
        edited.name = "Updated while connecting".into();
        profiles.save(&[edited.clone()]).unwrap();
        server.resume();

        assert_eq!(selection.await.unwrap(), Err(AppError::OperationCancelled));
        assert_eq!(profiles.load().unwrap(), vec![edited]);
        assert!(service.client("local").await.is_err());
    }

    #[tokio::test]
    async fn lifecycle_database_selection_preserves_unrelated_profile_edits() {
        let mut server = PausedRedis::start().await;
        let profiles = Arc::new(MutableProfiles(Mutex::new(vec![server.profile()])));
        let service = Arc::new(RedisService::new(profiles.clone(), Arc::new(EmptySecrets)));
        let selecting_service = Arc::clone(&service);
        let selection = tokio::spawn(async move {
            selecting_service
                .select_database(SelectDatabaseInput {
                    connection_id: "local".into(),
                    database: 2,
                })
                .await
        });
        server.wait_for_ping().await;
        let mut unrelated = valid_profile();
        unrelated.id = "other".into();
        profiles
            .save(&[server.profile(), unrelated.clone()])
            .unwrap();
        server.resume();

        assert_eq!(selection.await.unwrap().unwrap().database, 2);
        let saved = profiles.load().unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[1], unrelated);
        assert_eq!(saved[0].database, 2);
    }

    #[test]
    fn builds_a_standalone_url_with_encoded_credentials_and_database() {
        let mut profile = valid_profile();
        profile.username = Some("user name".into());
        profile.database = 3;

        let url = connection_url(&profile, Some("p@ss word")).unwrap();

        assert_eq!(url, "redis://user%20name:p%40ss%20word@127.0.0.1:6379/3");
    }

    #[test]
    fn builds_rediss_url_when_tls_is_enabled() {
        let mut profile = valid_profile();
        profile.tls = true;

        assert_eq!(
            connection_url(&profile, Some("secret")).unwrap(),
            "rediss://:secret@127.0.0.1:6379/0"
        );
    }

    #[test]
    fn appends_insecure_marker_only_when_server_verification_is_disabled() {
        let mut profile = valid_profile();
        profile.tls = true;
        profile.verify_server_cert = false;

        assert!(connection_url(&profile, None)
            .unwrap()
            .ends_with("/0#insecure"));
    }

    #[test]
    fn builds_tls_client_without_network_io() {
        let mut profile = valid_profile();
        profile.tls = true;

        assert!(build_client(&profile, &ConnectionSecrets::default()).is_ok());
    }

    #[test]
    fn rejects_incomplete_mtls_material_before_building_a_client() {
        let mut profile = valid_profile();
        profile.tls = true;
        let secrets = ConnectionSecrets {
            client_certificate: Some("certificate".into()),
            ..ConnectionSecrets::default()
        };

        assert_eq!(
            build_client(&profile, &secrets).unwrap_err(),
            AppError::InvalidInput
        );
    }

    #[test]
    fn rejects_negative_ttl_without_exposing_the_value() {
        let error = validate_ttl(-1).unwrap_err();

        assert_eq!(error, AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn serializes_redis_map_as_pair_array_with_array_kind() {
        let result = command_result(::redis::Value::Map(vec![(
            ::redis::Value::SimpleString("field".into()),
            ::redis::Value::Int(1),
        )]))
        .unwrap();

        assert_eq!(result.kind, "array");
        assert_eq!(result.value, serde_json::json!([["field", 1]]));
    }

    #[tokio::test]
    async fn rejects_observability_operations_without_active_connection() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));

        let error = service
            .get_slow_logs(GetSlowLogsInput {
                connection_id: "local".into(),
                count: 50,
            })
            .await
            .unwrap_err();
        assert_eq!(error, AppError::ConnectionFailed);

        let error = service
            .publish_pub_sub(PublishPubSubInput {
                connection_id: "local".into(),
                channel: "events".into(),
                message: "hello".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error, AppError::ConnectionFailed);
    }

    #[tokio::test]
    async fn node_scoped_operation_dials_the_same_snapshot_that_passed_the_cluster_gate() {
        let (original_address, original_commands, _) =
            spawn_recording_sentinel_fixture(None, false).await;
        let (replacement_address, replacement_commands, _) =
            spawn_recording_sentinel_fixture(None, false).await;
        let gate = Arc::new(super::NodeScopedSnapshotGate::default());
        let service = Arc::new(
            RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets))
                .with_test_node_scoped_snapshot_gate(Arc::clone(&gate)),
        );
        let handle = |address: SocketAddr, target: ConnectionTarget| {
            let mut profile = valid_profile();
            profile.port = address.port();
            if let ConnectionTarget::Cluster(cluster) = &target {
                profile.cluster = Some(cluster.clone());
            }
            ConnectionHandle {
                client: crate::redis::RoutedClient::Standalone(
                    crate::redis::StandaloneClient::Direct(
                        ::redis::Client::open(format!("redis://{address}/")).unwrap(),
                    ),
                ),
                profile,
                target,
                cluster_node_factory: None,
                _ssh: None,
            }
        };
        service.active.write().await.insert(
            "local".into(),
            handle(original_address, ConnectionTarget::Standalone),
        );

        let operation_service = Arc::clone(&service);
        let operation =
            tokio::spawn(async move { operation_service.clear_slow_logs("local").await });
        tokio::time::timeout(Duration::from_secs(1), gate.captured.notified())
            .await
            .expect("node-scoped operation must pause after its target snapshot");
        service.active.write().await.insert(
            "local".into(),
            handle(
                replacement_address,
                ConnectionTarget::Cluster(ClusterConfig {
                    nodes: vec![ConnectionEndpoint {
                        host: "127.0.0.1".into(),
                        port: replacement_address.port(),
                    }],
                    read_from_replicas: false,
                }),
            ),
        );
        gate.resume.notify_one();

        tokio::time::timeout(Duration::from_secs(1), operation)
            .await
            .expect("node-scoped operation must finish after the snapshot gate resumes")
            .unwrap()
            .unwrap();
        let forbidden = vec!["SLOWLOG".to_owned(), "RESET".to_owned()];
        assert_eq!(
            original_commands
                .lock()
                .unwrap()
                .iter()
                .filter(|command| command == &&forbidden)
                .count(),
            1
        );
        assert_eq!(
            replacement_commands
                .lock()
                .unwrap()
                .iter()
                .filter(|command| command == &&forbidden)
                .count(),
            0
        );
        assert_eq!(
            service.clear_slow_logs("local").await,
            Err(AppError::UnsupportedFeature)
        );
        assert!(!replacement_commands
            .lock()
            .unwrap()
            .iter()
            .any(|command| command == &forbidden));
    }

    #[tokio::test]
    async fn rejects_stream_group_operations_without_active_connection() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));

        let error = service
            .get_stream_consumer_groups(GetStreamConsumerGroupsInput {
                connection_id: "local".into(),
                key: "events".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error, AppError::ConnectionFailed);

        let error = service
            .get_stream_pending_entries(GetStreamPendingEntriesInput {
                connection_id: "local".into(),
                key: "events".into(),
                group: "workers".into(),
                count: 0,
                consumer: None,
            })
            .await
            .unwrap_err();
        assert_eq!(error, AppError::InvalidConnection);
    }

    #[tokio::test]
    async fn stopping_a_missing_profiler_session_is_idempotent() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));

        service
            .stop_profiler(StopProfilerInput {
                connection_id: "local".into(),
                session_id: "missing-session".into(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn close_connection_removes_cached_module_capabilities() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));
        service.capabilities.write().await.insert(
            "cached".into(),
            ModuleCapabilities::from_modules(vec![ModuleSummary {
                name: "RedisJSON".into(),
                version: Some("2.0.0".into()),
            }]),
        );

        service.close_connection("cached").await.unwrap();

        assert!(!service.capabilities.read().await.contains_key("cached"));
    }

    #[tokio::test]
    async fn capability_cache_ignores_stale_probe_results_after_session_bump() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));
        let capabilities = ModuleCapabilities::from_modules(vec![ModuleSummary {
            name: "RedisJSON".into(),
            version: Some("2.0.0".into()),
        }]);

        let token = service.capture_capability_token("cached").await;
        service.bump_connection_generation("cached").await;

        assert!(
            !service
                .cache_capabilities_if_current("cached", token, capabilities.clone())
                .await
        );
        assert!(!service.capabilities.read().await.contains_key("cached"));
    }

    #[tokio::test]
    async fn capability_cache_reuses_snapshot_when_generation_is_unchanged() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));
        let capabilities = ModuleCapabilities::from_modules(vec![ModuleSummary {
            name: "RedisJSON".into(),
            version: Some("2.0.0".into()),
        }]);

        let token = service.capture_capability_token("cached").await;
        assert!(
            service
                .cache_capabilities_if_current("cached", token, capabilities.clone())
                .await
        );
        assert_eq!(
            service.capabilities.read().await.get("cached"),
            Some(&capabilities)
        );
    }

    #[tokio::test]
    async fn capability_session_bump_cannot_pass_a_blocked_cache_write() {
        let service = Arc::new(RedisService::new(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
        ));
        let capabilities = ModuleCapabilities::from_modules(vec![ModuleSummary {
            name: "RedisJSON".into(),
            version: Some("2.0.0".into()),
        }]);
        let token = service.capture_capability_token("cached").await;
        let capabilities_guard = service.capabilities.write().await;
        let initial_generation_guard = service.generations.write().await;

        let cache_service = Arc::clone(&service);
        let cache_capabilities = capabilities.clone();
        let cache_task = tokio::spawn(async move {
            cache_service
                .cache_capabilities_if_current("cached", token, cache_capabilities)
                .await
        });

        drop(initial_generation_guard);
        tokio::task::yield_now().await;

        let (bump_started_tx, mut bump_started_rx) = oneshot::channel();
        let bump_service = Arc::clone(&service);
        let bump_task = tokio::spawn(async move {
            let next = bump_service.bump_connection_generation("cached").await;
            let _ = bump_started_tx.send(());
            next
        });

        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        assert!(
            matches!(
                bump_started_rx.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "generation bump must wait until the pending capability cache write finishes"
        );

        drop(capabilities_guard);

        assert!(cache_task.await.unwrap());
        assert_eq!(bump_task.await.unwrap(), 1);
        service.capabilities.write().await.remove("cached");
        assert!(!service.capabilities.read().await.contains_key("cached"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn observability_registration_and_close_are_serialized_by_generation() {
        let service = Arc::new(RedisService::new(
            Arc::new(EmptyProfiles),
            Arc::new(EmptySecrets),
        ));
        let token = service.bump_connection_generation("observed").await;
        let entered = Arc::new(std::sync::Barrier::new(2));
        let resume = Arc::new(std::sync::Barrier::new(2));
        let start_service = Arc::clone(&service);
        let start_entered = Arc::clone(&entered);
        let start_resume = Arc::clone(&resume);
        let start = tokio::spawn(async move {
            start_service
                .register_observability_if_current("observed", token, || {
                    start_entered.wait();
                    start_resume.wait();
                    Ok(())
                })
                .await
        });
        entered.wait();

        let close_service = Arc::clone(&service);
        let mut close =
            tokio::spawn(async move { close_service.close_connection("observed").await });
        assert!(tokio::time::timeout(Duration::from_millis(100), &mut close)
            .await
            .is_err());
        resume.wait();

        start.await.unwrap().unwrap();
        close.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn stale_observability_prepare_cannot_register_after_replace() {
        let service = RedisService::new(Arc::new(EmptyProfiles), Arc::new(EmptySecrets));
        let token = service.bump_connection_generation("observed").await;
        service.bump_connection_generation("observed").await;
        let registered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let attempted = Arc::clone(&registered);

        assert_eq!(
            service
                .register_observability_if_current("observed", token, || {
                    attempted.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
                .await,
            Err(AppError::OperationCancelled)
        );
        assert!(!registered.load(std::sync::atomic::Ordering::SeqCst));
    }
}
