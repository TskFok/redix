use std::collections::HashMap;

use ::redis::Value;

use crate::{
    domain::{
        parse_info_sections, AnalysisAccumulator, AnalysisKeyMetadata, AnalysisProgress,
        AnalyzeDatabaseInput, DatabaseAnalysisReport, InstanceDetails, ModuleSummary,
        NodeAnalysisResult, NodeFailure,
    },
    error::AppError,
};

use super::{connection_manager::map_command_error, RoutedConnection};

const METADATA_BATCH_SIZE: usize = 500;

pub fn allocate_primary_key_limits(
    max_keys: u64,
    primary_count: usize,
) -> Result<Vec<u64>, AppError> {
    if primary_count == 0 || primary_count > 128 {
        return Err(AppError::ClusterTopologyFailed);
    }
    let count = u64::try_from(primary_count).map_err(|_| AppError::ClusterTopologyFailed)?;
    let base = max_keys / count;
    let remainder =
        usize::try_from(max_keys % count).map_err(|_| AppError::ClusterTopologyFailed)?;
    Ok((0..primary_count)
        .map(|index| base + u64::from(index < remainder))
        .collect())
}

/// Internal, untruncated node observations. Only finished reports cross IPC.
#[derive(Debug)]
pub struct NodeAnalysisState {
    pub node_id: String,
    pub endpoint: crate::domain::ConnectionEndpoint,
    pub accumulator: AnalysisAccumulator,
    pub progress: crate::domain::AnalysisProgress,
}

pub fn merge_node_reports(
    mut nodes: Vec<NodeAnalysisState>,
    mut failed_nodes: Vec<NodeFailure>,
) -> DatabaseAnalysisReport {
    nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    failed_nodes.sort_by(|left, right| {
        left.node_id
            .cmp(&right.node_id)
            .then_with(|| left.code.cmp(&right.code))
    });
    let mut aggregate = AnalysisAccumulator::new(0, "*".to_owned(), ":".to_owned(), 0);
    let mut scanned = 0_u64;
    let mut processed = 0_u64;
    let mut truncated = !failed_nodes.is_empty();
    let mut node_results = Vec::with_capacity(nodes.len());
    for node in nodes {
        scanned = scanned.saturating_add(node.progress.scanned);
        processed = processed.saturating_add(node.progress.processed);
        truncated |= node.progress.truncated;
        aggregate.merge(&node.accumulator);
        node_results.push(NodeAnalysisResult {
            node_id: node.node_id,
            endpoint: node.endpoint,
            report: Box::new(node.accumulator.finish(
                node.progress.scanned,
                node.progress.processed,
                node.progress.truncated,
            )),
        });
    }
    let mut report = aggregate.finish(scanned, processed, truncated);
    if let Some(first) = node_results.first() {
        report.database = first.report.database;
        report.pattern = first.report.pattern.clone();
        report.delimiter = first.report.delimiter.clone();
    }
    report.node_results = node_results;
    report.failed_nodes = failed_nodes;
    report
}

pub(crate) async fn load_instance_details(
    connection: &mut RoutedConnection,
) -> Result<InstanceDetails, AppError> {
    let info = ::redis::cmd("INFO")
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    let mut sections = parse_info_sections(&info);
    let command_stats_info = command_stats_info_command()
        .query_async::<String>(connection)
        .await
        .ok();
    merge_command_stats_sections(&mut sections, command_stats_info.as_deref());
    let modules = ::redis::cmd("MODULE")
        .arg("LIST")
        .query_async::<Value>(connection)
        .await
        .map(parse_module_list)
        .unwrap_or_default();
    InstanceDetails::from_info_and_modules(&sections, modules)
}

fn command_stats_info_command() -> ::redis::Cmd {
    let mut command = ::redis::cmd("INFO");
    command.arg("commandstats");
    command
}

fn merge_command_stats_sections(
    sections: &mut HashMap<String, HashMap<String, String>>,
    command_stats_info: Option<&str>,
) {
    let Some(command_stats_info) = command_stats_info else {
        return;
    };
    let mut explicit_sections = parse_info_sections(command_stats_info);
    let Some(command_stats) = explicit_sections.remove("Commandstats") else {
        return;
    };
    sections
        .entry("Commandstats".to_string())
        .or_default()
        .extend(command_stats);
}

pub(crate) async fn analyze_connection<C>(
    connection: &mut C,
    database: u8,
    input: &AnalyzeDatabaseInput,
) -> Result<DatabaseAnalysisReport, AppError>
where
    C: ::redis::aio::ConnectionLike + Send + Unpin,
{
    let (accumulator, progress) =
        analyze_connection_accumulator(connection, database, input, false).await?;
    Ok(accumulator.finish(progress.scanned, progress.processed, progress.truncated))
}

pub(crate) async fn analyze_connection_accumulator<C>(
    connection: &mut C,
    database: u8,
    input: &AnalyzeDatabaseInput,
    strict_key_limit: bool,
) -> Result<(AnalysisAccumulator, AnalysisProgress), AppError>
where
    C: ::redis::aio::ConnectionLike + Send + Unpin,
{
    let mut cursor = 0_u64;
    let mut scanned = 0_u64;
    let mut processed = 0_u64;
    let mut accumulator = AnalysisAccumulator::new(
        database,
        input.pattern.clone(),
        input.delimiter.clone(),
        input.max_keys,
    );

    loop {
        let (next_cursor, keys): (u64, Vec<String>) = ::redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&input.pattern)
            .arg("COUNT")
            .arg(500_i64)
            .query_async(connection)
            .await
            .map_err(map_command_error)?;
        let page_plan = if strict_key_limit {
            let remaining = input.max_keys.saturating_sub(scanned);
            let process_count = keys
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            ScanPagePlan {
                process_count,
                truncated: process_count < keys.len()
                    || (next_cursor != 0 && process_count as u64 >= remaining),
            }
        } else {
            scan_page_plan(scanned, keys.len(), next_cursor, input.max_keys)
        };
        scanned = scanned.saturating_add(u64::try_from(keys.len()).unwrap_or(u64::MAX));
        let batch = &keys[..page_plan.process_count];
        for metadata_keys in metadata_key_batches(batch) {
            let metadata = load_key_metadata(connection, metadata_keys).await?;
            processed = processed.saturating_add(accumulate_metadata(&mut accumulator, metadata));
        }
        if page_plan.truncated || next_cursor == 0 {
            return Ok((
                accumulator,
                AnalysisProgress {
                    scanned,
                    processed,
                    max_keys: input.max_keys,
                    truncated: page_plan.truncated,
                },
            ));
        }
        cursor = next_cursor;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScanPagePlan {
    process_count: usize,
    truncated: bool,
}

fn scan_page_plan(
    scanned_before_page: u64,
    page_key_count: usize,
    next_cursor: u64,
    max_keys: u64,
) -> ScanPagePlan {
    if next_cursor == 0 {
        return ScanPagePlan {
            process_count: page_key_count,
            truncated: false,
        };
    }

    let page_key_count_u64 = u64::try_from(page_key_count).unwrap_or(u64::MAX);
    let remaining = max_keys.saturating_sub(scanned_before_page);
    ScanPagePlan {
        process_count: page_key_count.min(usize::try_from(remaining).unwrap_or(usize::MAX)),
        truncated: scanned_before_page.saturating_add(page_key_count_u64) >= max_keys,
    }
}

fn accumulate_metadata(
    accumulator: &mut AnalysisAccumulator,
    metadata: Vec<AnalysisKeyMetadata>,
) -> u64 {
    let mut processed = 0_u64;
    for item in metadata {
        if item.key_type.is_empty() || item.key_type == "none" || item.ttl_seconds == -2 {
            continue;
        }
        accumulator.process(item);
        processed = processed.saturating_add(1);
    }
    processed
}

fn metadata_key_batches(keys: &[String]) -> impl Iterator<Item = &[String]> {
    keys.chunks(METADATA_BATCH_SIZE)
}

async fn load_key_metadata<C>(
    connection: &mut C,
    keys: &[String],
) -> Result<Vec<AnalysisKeyMetadata>, AppError>
where
    C: ::redis::aio::ConnectionLike + Send + Unpin,
{
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut metadata_pipeline = ::redis::pipe();
    for key in keys {
        metadata_pipeline
            .cmd("TYPE")
            .arg(key)
            .cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .cmd("TTL")
            .arg(key);
    }
    let replies = metadata_pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;

    let mut metadata = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let reply_index = index * 3;
            AnalysisKeyMetadata {
                key: key.clone(),
                key_type: value_as_string(replies.get(reply_index)).unwrap_or_default(),
                length: None,
                memory_bytes: value_as_optional(replies.get(reply_index + 1)),
                ttl_seconds: value_as_i64(replies.get(reply_index + 2)).unwrap_or(-2),
            }
        })
        .collect::<Vec<_>>();

    let mut length_pipeline = ::redis::pipe();
    let mut length_indexes = Vec::new();
    for (index, item) in metadata.iter().enumerate() {
        let Some(command) = length_command(&item.key_type) else {
            continue;
        };
        length_pipeline.cmd(command).arg(&item.key);
        length_indexes.push(index);
    }
    if length_indexes.is_empty() {
        return Ok(metadata);
    }

    let replies = length_pipeline
        .query_async::<Vec<Value>>(connection)
        .await
        .map_err(map_command_error)?;
    for (index, reply) in length_indexes.into_iter().zip(replies.iter()) {
        metadata[index].length = value_as_optional(Some(reply));
    }
    Ok(metadata)
}

fn length_command(key_type: &str) -> Option<&'static str> {
    match key_type {
        "string" => Some("STRLEN"),
        "list" => Some("LLEN"),
        "hash" => Some("HLEN"),
        "set" => Some("SCARD"),
        "zset" => Some("ZCARD"),
        "stream" => Some("XLEN"),
        "array" => Some("ARLEN"),
        "vectorset" | "vector-set" => Some("VCARD"),
        _ => None,
    }
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| ::redis::from_redis_value_ref::<String>(value).ok())
}

fn value_as_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|value| ::redis::from_redis_value_ref::<i64>(value).ok())
}

fn value_as_optional(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        ::redis::from_redis_value_ref::<Option<u64>>(value)
            .ok()
            .flatten()
    })
}

pub(crate) fn parse_module_list(value: Value) -> Vec<ModuleSummary> {
    let entries = match value {
        Value::Array(entries) | Value::Set(entries) => entries,
        Value::Attribute { data, .. } => return parse_module_list(*data),
        _ => return Vec::new(),
    };

    entries.into_iter().filter_map(parse_module_entry).collect()
}

pub(crate) fn parse_module_entry(value: Value) -> Option<ModuleSummary> {
    let mut name = None;
    let mut version = None;
    match value {
        Value::Map(entries) => {
            for (key, value) in entries {
                update_module_field(&key, &value, &mut name, &mut version);
            }
        }
        Value::Array(entries) => {
            let mut pairs = entries.chunks_exact(2);
            for pair in &mut pairs {
                update_module_field(&pair[0], &pair[1], &mut name, &mut version);
            }
        }
        Value::Attribute { data, .. } => return parse_module_entry(*data),
        _ => return None,
    }

    name.filter(|name| !name.trim().is_empty())
        .map(|name| ModuleSummary {
            name,
            version: version.filter(|version| !version.trim().is_empty()),
        })
}

fn update_module_field(
    key: &Value,
    value: &Value,
    name: &mut Option<String>,
    version: &mut Option<String>,
) {
    let Some(key) = ::redis::from_redis_value_ref::<String>(key).ok() else {
        return;
    };
    match key.as_str() {
        "name" => *name = ::redis::from_redis_value_ref::<String>(value).ok(),
        "ver" | "version" => *version = ::redis::from_redis_value_ref::<String>(value).ok(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_explicit_commandstats_info_command() {
        assert_eq!(
            command_stats_info_command().get_packed_command(),
            b"*2\r\n$4\r\nINFO\r\n$12\r\ncommandstats\r\n".to_vec()
        );
    }

    #[test]
    fn uses_module_length_commands_for_array_and_vector_set() {
        assert_eq!(length_command("array"), Some("ARLEN"));
        assert_eq!(length_command("vectorset"), Some("VCARD"));
        assert_eq!(length_command("vector-set"), Some("VCARD"));
    }

    #[test]
    fn merges_explicit_commandstats_without_losing_default_info() {
        let mut sections = parse_info_sections("# Server\r\nredis_version:7.2.5\r\n");

        merge_command_stats_sections(
            &mut sections,
            Some(
                "# Commandstats\r\ncmdstat_get:calls=4,usec=20,usec_per_call=5.0,rejected_calls=0,failed_calls=1\r\n",
            ),
        );

        let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();
        assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
        assert_eq!(details.command_stats.len(), 1);
        assert_eq!(details.command_stats[0].command, "GET");

        merge_command_stats_sections(&mut sections, None);
        assert_eq!(sections["Server"]["redis_version"], "7.2.5");
        assert_eq!(sections["Commandstats"].len(), 1);
    }

    #[test]
    fn final_cursor_zero_page_is_processed_in_full() {
        let plan = scan_page_plan(900, 250, 0, 1_000);

        assert_eq!(plan.process_count, 250);
        assert!(!plan.truncated);
    }

    #[test]
    fn nonzero_cursor_page_stops_at_encountered_key_limit() {
        let plan = scan_page_plan(900, 250, 42, 1_000);

        assert_eq!(plan.process_count, 100);
        assert!(plan.truncated);
    }

    #[test]
    fn deleted_scan_keys_are_not_processed_or_aggregated() {
        let mut accumulator = AnalysisAccumulator::new(0, "*".into(), ":".into(), 1_000);
        let processed = accumulate_metadata(
            &mut accumulator,
            vec![
                AnalysisKeyMetadata {
                    key: "deleted:type".into(),
                    key_type: "none".into(),
                    length: None,
                    memory_bytes: Some(64),
                    ttl_seconds: -1,
                },
                AnalysisKeyMetadata {
                    key: "deleted:ttl".into(),
                    key_type: "string".into(),
                    length: Some(3),
                    memory_bytes: Some(64),
                    ttl_seconds: -2,
                },
                AnalysisKeyMetadata {
                    key: "present:nil-metadata".into(),
                    key_type: "string".into(),
                    length: None,
                    memory_bytes: None,
                    ttl_seconds: -1,
                },
            ],
        );
        let report = accumulator.finish(3, processed, false);

        assert_eq!(processed, 1);
        assert_eq!(report.progress.scanned, 3);
        assert_eq!(report.progress.processed, 1);
        assert_eq!(report.total_keys.total, 1);
        assert_eq!(report.total_keys.types[0].r#type, "string");
        assert_eq!(report.total_keys.types[0].total, 1);
        assert_eq!(report.total_memory.observed, 0);
        assert_eq!(report.expiration_groups[0].label, "No Expiry");
        assert_eq!(report.expiration_groups[0].keys, 1);
    }

    #[test]
    fn splits_metadata_batches_at_five_hundred_keys() {
        let keys = (0..501)
            .map(|index| format!("analysis:{index}"))
            .collect::<Vec<_>>();

        let batches = metadata_key_batches(&keys).collect::<Vec<_>>();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 500);
        assert_eq!(batches[1].len(), 1);
        assert!(batches.iter().all(|batch| batch.len() <= 500));
    }
}
