use crate::{
    domain::{
        normalize_key_type, HashEntry, KeySummary, RedisValue, ScanCursor, ScanPage,
        SortedSetEntry, StreamEntry, StreamField,
    },
    error::AppError,
};

use super::{
    connection_manager::{key_size_command, map_command_error},
    RoutedConnection,
};
use ::redis::aio::ConnectionLike;
use futures_util::{stream, StreamExt, TryStreamExt};

const MAX_CLUSTER_SCAN_PAGE_BYTES: usize = 4 * 1024 * 1024;
const SUMMARY_BATCH_SIZE: usize = 500;
const SUMMARY_CLUSTER_CONCURRENCY: usize = 8;

pub(crate) async fn collect_scan_pages<F, Fut>(mut scan: F) -> Result<Vec<KeySummary>, AppError>
where
    F: FnMut(ScanCursor) -> Fut,
    Fut: std::future::Future<Output = Result<ScanPage, AppError>>,
{
    let mut cursor = ScanCursor::default();
    let mut visited_cursors = std::collections::HashSet::from([cursor.clone()]);
    let mut seen_keys = std::collections::HashSet::new();
    let mut keys = Vec::new();
    loop {
        let page = scan(cursor).await?;
        if !page.node_failures.is_empty() {
            return Err(AppError::ClusterNodeUnavailable);
        }
        for key in page.keys {
            if seen_keys.insert(key.key.clone()) {
                keys.push(key);
            }
        }
        // Empty filtered pages do not mean SCAN has completed. Cluster cursors are opaque.
        if !page.has_more {
            return Ok(keys);
        }
        if !visited_cursors.insert(page.cursor.clone()) {
            return Err(AppError::CommandFailed);
        }
        cursor = page.cursor;
    }
}

pub(crate) fn ensure_cluster_scan_page_size(
    page: &crate::domain::ScanPage,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(page).map_err(|_| AppError::InvalidInput)?;
    if bytes.len() > MAX_CLUSTER_SCAN_PAGE_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

pub(crate) async fn load_key_summaries(
    connection: &mut RoutedConnection,
    keys: Vec<Vec<u8>>,
    requested_type: Option<&str>,
) -> Result<Vec<KeySummary>, AppError> {
    let keys = keys
        .into_iter()
        .enumerate()
        .map(|(index, key)| {
            String::from_utf8(key)
                .map(|key| (index, key))
                .map_err(|_| AppError::InvalidInput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut summaries = Vec::with_capacity(keys.len());
    for batch in keys.chunks(SUMMARY_BATCH_SIZE) {
        match connection {
            RoutedConnection::Standalone(_) => {
                summaries.extend(load_summary_batch(connection, batch, requested_type).await?);
            }
            RoutedConnection::Cluster(cluster) => {
                // redis-rs routes an async pipeline to one slot. Even keys on the
                // same primary must be grouped by slot to avoid CROSSSLOT.
                let mut groups = std::collections::HashMap::<_, Vec<_>>::new();
                for entry in batch {
                    groups
                        .entry(::redis::cluster_routing::Slot::for_key(entry.1.as_bytes()))
                        .or_default()
                        .push(entry.clone());
                }
                let results =
                    stream::iter(groups.into_values())
                        .map(|group| {
                            let mut connection = RoutedConnection::Cluster(cluster.clone());
                            async move {
                                load_summary_batch(&mut connection, &group, requested_type).await
                            }
                        })
                        .buffer_unordered(SUMMARY_CLUSTER_CONCURRENCY)
                        .try_collect::<Vec<_>>()
                        .await?;
                summaries.extend(results.into_iter().flatten());
            }
        }
    }
    summaries.sort_unstable_by_key(|(index, _)| *index);
    Ok(summaries.into_iter().map(|(_, summary)| summary).collect())
}

async fn load_summary_batch(
    connection: &mut RoutedConnection,
    keys: &[(usize, String)],
    requested_type: Option<&str>,
) -> Result<Vec<(usize, KeySummary)>, AppError> {
    let mut types = ::redis::pipe();
    types.ignore_errors();
    for (_, key) in keys {
        types.cmd("TYPE").arg(key);
    }
    let key_types = query_summary_pipeline(connection, &types).await?;
    if key_types.len() != keys.len() {
        return Err(AppError::CommandFailed);
    }

    let mut metadata = ::redis::pipe();
    // MEMORY / OBJECT and type-dependent lengths are optional. Keep their
    // individual errors in the replies instead of failing the whole pipeline.
    metadata.ignore_errors();
    let mut summaries = Vec::with_capacity(keys.len());
    for ((index, key), key_type) in keys.iter().zip(key_types) {
        let key_type: String = required_summary_value(key_type)?;
        if requested_type.is_some() && normalize_key_type(&key_type) != requested_type {
            continue;
        }
        metadata.cmd("PTTL").arg(key);
        if let Some(command) = key_size_command(&key_type) {
            metadata.cmd(command).arg(key);
        }
        metadata
            .cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .cmd("OBJECT")
            .arg("ENCODING")
            .arg(key)
            .cmd("OBJECT")
            .arg("IDLETIME")
            .arg(key);
        summaries.push((
            *index,
            KeySummary {
                key: key.clone(),
                key_type,
                ttl_ms: -2,
                size: None,
                memory_bytes: None,
                encoding: None,
                idle_seconds: None,
            },
        ));
    }
    if summaries.is_empty() {
        return Ok(summaries);
    }
    let replies = query_summary_pipeline(connection, &metadata).await?;
    if replies.len() != metadata.len() {
        return Err(AppError::CommandFailed);
    }
    let mut replies = replies.into_iter();
    for (_, summary) in &mut summaries {
        summary.ttl_ms = required_summary_value(next_summary_reply(&mut replies)?)?;
        if key_size_command(&summary.key_type).is_some() {
            summary.size = ::redis::from_redis_value::<u64>(next_summary_reply(&mut replies)?).ok();
        }
        summary.memory_bytes =
            ::redis::from_redis_value::<Option<u64>>(next_summary_reply(&mut replies)?)
                .ok()
                .flatten();
        summary.encoding =
            ::redis::from_redis_value::<Option<String>>(next_summary_reply(&mut replies)?)
                .ok()
                .flatten();
        summary.idle_seconds =
            ::redis::from_redis_value::<Option<u64>>(next_summary_reply(&mut replies)?)
                .ok()
                .flatten();
    }
    Ok(summaries)
}

async fn query_summary_pipeline(
    connection: &mut RoutedConnection,
    pipeline: &::redis::Pipeline,
) -> Result<Vec<::redis::Value>, AppError> {
    match pipeline
        .query_async::<Vec<::redis::Value>>(connection)
        .await
    {
        Err(error)
            if matches!(connection, RoutedConnection::Cluster(_))
                && matches!(
                    error.kind(),
                    ::redis::ErrorKind::Server(
                        ::redis::ServerErrorKind::Ask | ::redis::ServerErrorKind::Moved
                    )
                ) =>
        {
            // redis-rs 1.5 sends ASKING only once before replaying a pipeline,
            // but Redis permits only the next command on an IMPORTING node.
            // After redirect retries are exhausted, route these read-only
            // commands individually. Preserve optional server errors for parsing.
            let mut replies = Vec::with_capacity(pipeline.len());
            for command in pipeline.cmd_iter() {
                replies.push(
                    connection
                        .req_packed_command(command)
                        .await
                        .map_err(map_command_error)?,
                );
            }
            Ok(replies)
        }
        result => result.map_err(map_command_error),
    }
}

fn required_summary_value<T: ::redis::FromRedisValue>(
    value: ::redis::Value,
) -> Result<T, AppError> {
    let value = value.extract_error().map_err(map_command_error)?;
    ::redis::from_redis_value(value).map_err(|error| map_command_error(error.into()))
}

fn next_summary_reply(
    replies: &mut std::vec::IntoIter<::redis::Value>,
) -> Result<::redis::Value, AppError> {
    replies.next().ok_or(AppError::CommandFailed)
}

pub fn decode_stream_entry(id: &str, entries: Vec<String>) -> Result<StreamEntry, AppError> {
    if id.trim().is_empty() || entries.is_empty() || entries.len() % 2 != 0 {
        return Err(AppError::CommandFailed);
    }

    let fields = entries
        .chunks_exact(2)
        .map(|pair| {
            if pair[0].trim().is_empty() {
                return Err(AppError::CommandFailed);
            }
            Ok(StreamField {
                field: pair[0].clone(),
                value: pair[1].clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let entry = StreamEntry {
        id: id.to_owned(),
        fields,
    };
    RedisValue::Stream {
        entries: vec![entry.clone()],
    }
    .validate()?;
    Ok(entry)
}

pub fn encode_stream_entry(entry: &StreamEntry) -> Result<Vec<String>, AppError> {
    RedisValue::Stream {
        entries: vec![entry.clone()],
    }
    .validate()?;

    let mut values = Vec::with_capacity(entry.fields.len() * 2 + 1);
    values.push(entry.id.clone());
    for field in &entry.fields {
        values.push(field.field.clone());
        values.push(field.value.clone());
    }
    Ok(values)
}

pub fn decode_json_value(raw: &str) -> Result<serde_json::Value, AppError> {
    serde_json::from_str(raw).map_err(|_| AppError::CommandFailed)
}

pub fn encode_json_value(value: &serde_json::Value) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| AppError::CommandFailed)
}

pub fn decode_key_value(key_type: &str, entries: Vec<String>) -> Result<RedisValue, AppError> {
    match key_type {
        "string" => match entries.as_slice() {
            [value] => Ok(RedisValue::String {
                value: value.clone(),
            }),
            _ => Err(AppError::CommandFailed),
        },
        "hash" => entries
            .into_iter()
            .map(|entry| {
                let (field, value) = entry.split_once('=').ok_or(AppError::CommandFailed)?;
                Ok(HashEntry {
                    field: field.to_owned(),
                    value: value.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(|fields| RedisValue::Hash { fields }),
        "list" => Ok(RedisValue::List { items: entries }),
        "set" => Ok(RedisValue::Set { members: entries }),
        "zset" => entries
            .into_iter()
            .map(|entry| {
                let (member, score) = entry.rsplit_once('=').ok_or(AppError::CommandFailed)?;
                let score = score.parse::<f64>().map_err(|_| AppError::CommandFailed)?;
                if !score.is_finite() {
                    return Err(AppError::CommandFailed);
                }
                Ok(SortedSetEntry {
                    member: member.to_owned(),
                    score,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(|members| RedisValue::SortedSet { members }),
        _ => Err(AppError::UnsupportedDataType),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_json_value, decode_key_value, decode_stream_entry, encode_json_value,
        ensure_cluster_scan_page_size,
    };
    use crate::domain::{
        HashEntry, KeySummary, RedisValue, ScanCursor, ScanPage, SortedSetEntry, StreamEntry,
        StreamField,
    };
    use crate::error::AppError;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    type Reply = (Vec<String>, String);

    fn reply(args: &[&str], response: &str) -> Reply {
        (
            args.iter().map(|arg| (*arg).to_owned()).collect(),
            response.to_owned(),
        )
    }

    // Withhold a stage's replies until every command arrives. A serial client
    // cannot complete this exchange; no wall-clock performance assertion is needed.
    async fn pipelined_connection(
        stages: Vec<Vec<Reply>>,
    ) -> (crate::redis::RoutedConnection, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            for mut stage in stages {
                let mut responses = String::new();
                while !stage.is_empty() {
                    let mut line = String::new();
                    assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                    let count = line
                        .trim()
                        .strip_prefix('*')
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    let mut args = Vec::new();
                    for _ in 0..count {
                        line.clear();
                        socket.read_line(&mut line).await.unwrap();
                        let size = line
                            .trim()
                            .strip_prefix('$')
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        let mut value = vec![0; size + 2];
                        socket.read_exact(&mut value).await.unwrap();
                        args.push(String::from_utf8(value[..size].to_vec()).unwrap());
                    }
                    // redis-rs sends client identification during connection setup.
                    if args.first().is_some_and(|arg| arg == "CLIENT") {
                        socket.get_mut().write_all(b"+OK\r\n").await.unwrap();
                        continue;
                    }
                    let index = stage
                        .iter()
                        .position(|(expected, _)| *expected == args)
                        .unwrap_or_else(|| panic!("unexpected command: {args:?}"));
                    responses.push_str(&stage.remove(index).1);
                }
                socket
                    .get_mut()
                    .write_all(responses.as_bytes())
                    .await
                    .unwrap();
            }
        });
        let client = ::redis::Client::open(format!("redis://{address}/")).unwrap();
        let connection =
            crate::redis::RoutedClient::Standalone(crate::redis::StandaloneClient::Direct(client))
                .connection()
                .await
                .unwrap();
        (connection, server)
    }

    #[tokio::test]
    async fn summaries_pipeline_batches_types_and_preserves_optional_errors_and_alignment() {
        let stages = vec![
            vec![
                reply(&["TYPE", "a"], "+string\r\n"),
                reply(&["TYPE", "b"], "+hash\r\n"),
                reply(&["TYPE", "c"], "+ReJSON-RL\r\n"),
            ],
            vec![
                reply(&["PTTL", "a"], ":1234\r\n"),
                reply(&["STRLEN", "a"], ":11\r\n"),
                reply(&["MEMORY", "USAGE", "a"], ":72\r\n"),
                reply(&["OBJECT", "ENCODING", "a"], "+embstr\r\n"),
                reply(&["OBJECT", "IDLETIME", "a"], ":5\r\n"),
                reply(&["PTTL", "b"], ":-1\r\n"),
                reply(&["HLEN", "b"], "-WRONGTYPE changed during scan\r\n"),
                reply(&["MEMORY", "USAGE", "b"], "-NOPERM denied\r\n"),
                reply(&["OBJECT", "ENCODING", "b"], "$-1\r\n"),
                reply(&["OBJECT", "IDLETIME", "b"], "-ERR LFU policy\r\n"),
                reply(&["PTTL", "c"], ":-2\r\n"),
                reply(&["MEMORY", "USAGE", "c"], "$-1\r\n"),
                reply(&["OBJECT", "ENCODING", "c"], "$-1\r\n"),
                reply(&["OBJECT", "IDLETIME", "c"], "$-1\r\n"),
            ],
        ];
        let (mut connection, server) = pipelined_connection(stages).await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::load_key_summaries(
                &mut connection,
                vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
                None,
            ),
        )
        .await;
        server.abort();
        assert_eq!(
            result.expect("metadata queries must be pipelined").unwrap(),
            vec![
                KeySummary {
                    key: "a".into(),
                    key_type: "string".into(),
                    ttl_ms: 1234,
                    size: Some(11),
                    memory_bytes: Some(72),
                    encoding: Some("embstr".into()),
                    idle_seconds: Some(5)
                },
                KeySummary {
                    key: "b".into(),
                    key_type: "hash".into(),
                    ttl_ms: -1,
                    size: None,
                    memory_bytes: None,
                    encoding: None,
                    idle_seconds: None
                },
                KeySummary {
                    key: "c".into(),
                    key_type: "ReJSON-RL".into(),
                    ttl_ms: -2,
                    size: None,
                    memory_bytes: None,
                    encoding: None,
                    idle_seconds: None
                },
            ]
        );
    }

    #[tokio::test]
    async fn summaries_pipeline_filters_types_before_reading_metadata() {
        let (mut connection, server) = pipelined_connection(vec![
            vec![
                reply(&["TYPE", "a"], "+string\r\n"),
                reply(&["TYPE", "b"], "+ReJSON-RL\r\n"),
            ],
            vec![
                reply(&["PTTL", "b"], ":-1\r\n"),
                reply(&["MEMORY", "USAGE", "b"], ":200\r\n"),
                reply(&["OBJECT", "ENCODING", "b"], "+raw\r\n"),
                reply(&["OBJECT", "IDLETIME", "b"], ":0\r\n"),
            ],
        ])
        .await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::load_key_summaries(
                &mut connection,
                vec![b"a".to_vec(), b"b".to_vec()],
                Some("json"),
            ),
        )
        .await;
        server.abort();
        let summaries = result
            .expect("TYPE must be pipelined before filtering")
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].key, "b");
        assert_eq!(summaries[0].size, None);
        assert_eq!(summaries[0].memory_bytes, Some(200));
    }

    #[tokio::test]
    async fn summaries_pipeline_still_rejects_required_metadata_errors() {
        let (mut connection, server) = pipelined_connection(vec![
            vec![reply(&["TYPE", "a"], "+string\r\n")],
            vec![
                reply(&["PTTL", "a"], "-NOPERM denied\r\n"),
                reply(&["STRLEN", "a"], ":1\r\n"),
                reply(&["MEMORY", "USAGE", "a"], ":72\r\n"),
                reply(&["OBJECT", "ENCODING", "a"], "+embstr\r\n"),
                reply(&["OBJECT", "IDLETIME", "a"], ":0\r\n"),
            ],
        ])
        .await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::load_key_summaries(&mut connection, vec![b"a".to_vec()], None),
        )
        .await;
        server.abort();
        assert_eq!(
            result.expect("metadata queries must be pipelined"),
            Err(AppError::CommandFailed)
        );
    }

    #[tokio::test]
    async fn summaries_pipeline_does_not_filter_away_type_errors() {
        let (mut connection, server) = pipelined_connection(vec![vec![
            reply(&["TYPE", "a"], "+string\r\n"),
            reply(&["TYPE", "b"], "-NOPERM denied\r\n"),
        ]])
        .await;
        let result = super::load_key_summaries(
            &mut connection,
            vec![b"a".to_vec(), b"b".to_vec()],
            Some("hash"),
        )
        .await;
        server.abort();
        assert_eq!(result, Err(AppError::CommandFailed));
    }

    #[tokio::test]
    async fn summaries_pipeline_splits_large_scan_pages_without_losing_keys() {
        let keys = (0..501)
            .map(|index| format!("key:{index}"))
            .collect::<Vec<_>>();
        let mut stages = Vec::new();
        for start in [0, 500] {
            let end = (start + 500).min(keys.len());
            stages.push(
                keys[start..end]
                    .iter()
                    .map(|key| reply(&["TYPE", key], "+string\r\n"))
                    .collect(),
            );
            let mut metadata = Vec::new();
            for (index, key) in keys.iter().enumerate().take(end).skip(start) {
                metadata.extend([
                    reply(&["PTTL", key], ":-1\r\n"),
                    reply(&["STRLEN", key], &format!(":{}\r\n", index + 1)),
                    reply(&["MEMORY", "USAGE", key], ":72\r\n"),
                    reply(&["OBJECT", "ENCODING", key], "+raw\r\n"),
                    reply(&["OBJECT", "IDLETIME", key], ":0\r\n"),
                ]);
            }
            stages.push(metadata);
        }
        let (mut connection, server) = pipelined_connection(stages).await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            super::load_key_summaries(
                &mut connection,
                keys.iter().map(|key| key.as_bytes().to_vec()).collect(),
                None,
            ),
        )
        .await;
        server.abort();
        let summaries = result
            .expect("large SCAN pages must use bounded pipelines")
            .unwrap();
        assert_eq!(summaries.len(), 501);
        for (index, summary) in summaries.iter().enumerate() {
            assert_eq!(summary.key, keys[index]);
            assert_eq!(summary.size, Some(index as u64 + 1));
        }
    }

    fn scan_page(cursor: ScanCursor, keys: &[&str], has_more: bool) -> ScanPage {
        ScanPage {
            cursor,
            keys: keys
                .iter()
                .map(|key| KeySummary {
                    key: (*key).into(),
                    key_type: "string".into(),
                    ttl_ms: -1,
                    size: Some(1),
                    memory_bytes: None,
                    encoding: None,
                    idle_seconds: None,
                })
                .collect(),
            has_more,
            node_failures: Vec::new(),
        }
    }

    async fn collect_pages(
        pages: Vec<Result<ScanPage, AppError>>,
    ) -> Result<Vec<KeySummary>, AppError> {
        let mut pages = pages.into_iter();
        super::collect_scan_pages(|_| {
            std::future::ready(
                pages
                    .next()
                    .expect("scan must stop at completion or failure"),
            )
        })
        .await
    }

    #[tokio::test]
    async fn scan_all_keys_continues_after_empty_pages_and_deduplicates_all_pages() {
        let keys = collect_pages(vec![
            Ok(scan_page(7.into(), &["a", "a"], true)),
            Ok(scan_page(3.into(), &[], true)),
            Ok(scan_page(0.into(), &["a", "b"], false)),
        ])
        .await
        .unwrap();
        assert_eq!(
            keys.iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[tokio::test]
    async fn scan_all_keys_forwards_opaque_cluster_cursors_and_stops_on_has_more() {
        let mut requests = Vec::new();
        let mut pages = vec![
            scan_page(ScanCursor::Cluster("cluster:first".into()), &[], true),
            scan_page(ScanCursor::Cluster("cluster:done".into()), &["key"], false),
        ]
        .into_iter();
        let keys = super::collect_scan_pages(|cursor| {
            requests.push(cursor);
            std::future::ready(Ok(pages.next().unwrap()))
        })
        .await
        .unwrap();
        assert_eq!(
            requests,
            [
                ScanCursor::Standalone(0),
                ScanCursor::Cluster("cluster:first".into())
            ]
        );
        assert_eq!(keys[0].key, "key");
    }

    #[tokio::test]
    async fn scan_all_keys_rejects_node_failures_even_on_the_final_page() {
        let mut failed_page = scan_page(0.into(), &["partial"], false);
        failed_page.node_failures.push(crate::domain::NodeFailure {
            node_id: "unavailable".into(),
            code: "CLUSTER_NODE_UNAVAILABLE".into(),
        });
        assert_eq!(
            collect_pages(vec![Ok(failed_page)]).await,
            Err(AppError::ClusterNodeUnavailable)
        );
    }

    #[tokio::test]
    async fn scan_all_keys_discards_prior_pages_when_a_later_scan_fails() {
        assert_eq!(
            collect_pages(vec![
                Ok(scan_page(7.into(), &["partial"], true)),
                Err(AppError::ConnectionFailed),
            ])
            .await,
            Err(AppError::ConnectionFailed)
        );
    }

    #[tokio::test]
    async fn scan_all_keys_rejects_stalled_and_cyclic_cursors() {
        for cursors in [vec![0], vec![7, 7], vec![7, 3, 7]] {
            let pages = cursors
                .into_iter()
                .map(|cursor| Ok(scan_page(cursor.into(), &[], true)))
                .collect();
            assert_eq!(collect_pages(pages).await, Err(AppError::CommandFailed));
        }
    }

    #[tokio::test]
    async fn scan_all_keys_does_not_limit_the_combined_result_to_one_page_size() {
        let first = "a".repeat(3 * 1024 * 1024);
        let second = "b".repeat(3 * 1024 * 1024);
        let keys = collect_pages(vec![
            Ok(scan_page(1.into(), &[&first], true)),
            Ok(scan_page(0.into(), &[&second], false)),
        ])
        .await
        .unwrap();
        assert_eq!(keys.len(), 2);
        assert!(serde_json::to_vec(&keys).unwrap().len() > 4 * 1024 * 1024);
    }

    #[test]
    fn cluster_scan_page_rejects_serialized_output_over_four_mibibytes() {
        let page = ScanPage {
            cursor: ScanCursor::Cluster("cluster:cursor".into()),
            keys: vec![KeySummary {
                key: "x".repeat(4 * 1024 * 1024),
                key_type: "string".into(),
                ttl_ms: -1,
                size: Some(1),
                memory_bytes: None,
                encoding: None,
                idle_seconds: None,
            }],
            has_more: true,
            node_failures: Vec::new(),
        };

        assert_eq!(
            ensure_cluster_scan_page_size(&page).unwrap_err(),
            AppError::InvalidInput
        );
    }

    #[test]
    fn maps_unknown_redis_type_to_unsupported_data_type() {
        let error = decode_key_value("vector", vec![]).unwrap_err();

        assert_eq!(error.code(), "UNSUPPORTED_DATA_TYPE");
    }

    #[test]
    fn decodes_scalar_and_collection_values() {
        assert_eq!(
            decode_key_value("string", vec!["hello".into()]).unwrap(),
            RedisValue::String {
                value: "hello".into()
            }
        );
        assert_eq!(
            decode_key_value("hash", vec!["name=redix".into()]).unwrap(),
            RedisValue::Hash {
                fields: vec![HashEntry {
                    field: "name".into(),
                    value: "redix".into()
                }]
            }
        );
        assert_eq!(
            decode_key_value("list", vec!["a".into(), "b".into()]).unwrap(),
            RedisValue::List {
                items: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(
            decode_key_value("set", vec!["a".into(), "b".into()]).unwrap(),
            RedisValue::Set {
                members: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(
            decode_key_value("zset", vec!["redix=1.5".into()]).unwrap(),
            RedisValue::SortedSet {
                members: vec![SortedSetEntry {
                    member: "redix".into(),
                    score: 1.5
                }]
            }
        );
    }

    #[test]
    fn rejects_malformed_value_entries_without_exposing_input() {
        let error = decode_key_value("zset", vec!["member=not-a-score".into()]).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn decodes_stream_entries_from_flattened_redis_fields() {
        assert_eq!(
            decode_stream_entry(
                "1710000000000-0",
                vec![
                    "event".into(),
                    "created".into(),
                    "owner".into(),
                    "redix".into()
                ],
            )
            .unwrap(),
            StreamEntry {
                id: "1710000000000-0".into(),
                fields: vec![
                    StreamField {
                        field: "event".into(),
                        value: "created".into(),
                    },
                    StreamField {
                        field: "owner".into(),
                        value: "redix".into(),
                    },
                ],
            }
        );
    }

    #[test]
    fn rejects_odd_stream_fields_without_exposing_field_values() {
        let error = decode_stream_entry("1-0", vec!["event".into()]).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn parses_and_serializes_json_as_a_valid_document() {
        let value = decode_json_value(r#"{"name":"Alice","items":[1,true]}"#).unwrap();

        assert_eq!(
            value,
            serde_json::json!({"name": "Alice", "items": [1, true]})
        );
        let encoded = encode_json_value(&value).unwrap();
        assert_eq!(decode_json_value(&encoded).unwrap(), value);
    }

    #[test]
    fn rejects_invalid_json_as_command_failure() {
        let error = decode_json_value(r#"{"name":"#).unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
        assert!(!error.to_string().contains("name"));
    }
}
