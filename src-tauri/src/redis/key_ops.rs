use crate::{
    domain::{
        normalize_key_type, HashEntry, KeySummary, RedisValue, ScanCursor, ScanPage,
        SortedSetEntry, StreamEntry, StreamField,
    },
    error::AppError,
};

use super::connection_manager::map_command_error;

const MAX_CLUSTER_SCAN_PAGE_BYTES: usize = 4 * 1024 * 1024;

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

/// Redis expects its native TYPE name, not the UI's normalized type label.
pub(crate) fn scan_type_name(value: &str) -> Option<&'static str> {
    if value.trim().eq_ignore_ascii_case("rejson-rs") {
        return Some("ReJSON-RS");
    }
    match normalize_key_type(value)? {
        "json" => Some("ReJSON-RL"),
        "vector-set" => Some("vectorset"),
        key_type => Some(key_type),
    }
}

pub(crate) async fn scan_key_names<C: ::redis::aio::ConnectionLike + Send + Unpin>(
    connection: &mut C,
    cursor: u64,
    pattern: &str,
    count: usize,
    key_type: Option<&str>,
) -> Result<(u64, Vec<Vec<u8>>), AppError> {
    let mut command = ::redis::cmd("SCAN");
    command
        .arg(cursor)
        .arg("MATCH")
        .arg(pattern)
        .arg("COUNT")
        .arg(count);
    if let Some(key_type) = key_type {
        command
            .arg("TYPE")
            .arg(scan_type_name(key_type).ok_or(AppError::InvalidInput)?);
    }
    command
        .query_async(connection)
        .await
        .map_err(map_command_error)
}

/// Types are known only when Redis itself has filtered the scan. Never query each key.
pub(crate) fn key_summaries(
    keys: Vec<Vec<u8>>,
    requested_type: Option<&str>,
) -> Result<Vec<KeySummary>, AppError> {
    keys.into_iter()
        .map(|key| {
            Ok(KeySummary {
                key: key.into(),
                key_type: requested_type
                    .and_then(normalize_key_type)
                    .map(str::to_owned),
            })
        })
        .collect()
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
                value: value.clone().into(),
            }),
            _ => Err(AppError::CommandFailed),
        },
        "hash" => entries
            .into_iter()
            .map(|entry| {
                let (field, value) = entry.split_once('=').ok_or(AppError::CommandFailed)?;
                Ok(HashEntry {
                    field: field.into(),
                    value: value.into(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(|fields| RedisValue::Hash { fields }),
        "list" => Ok(RedisValue::List {
            items: entries.into_iter().map(Into::into).collect(),
        }),
        "set" => Ok(RedisValue::Set {
            members: entries.into_iter().map(Into::into).collect(),
        }),
        "zset" => entries
            .into_iter()
            .map(|entry| {
                let (member, score) = entry.rsplit_once('=').ok_or(AppError::CommandFailed)?;
                let score = score.parse::<f64>().map_err(|_| AppError::CommandFailed)?;
                if !score.is_finite() {
                    return Err(AppError::CommandFailed);
                }
                Ok(SortedSetEntry {
                    member: member.into(),
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
    async fn scan_default_returns_only_names_without_type_or_metadata_commands() {
        let (mut connection, server) = pipelined_connection(vec![vec![reply(
            &["SCAN", "7", "MATCH", "user:*", "COUNT", "100"],
            "*2\r\n$1\r\n0\r\n*2\r\n$6\r\nuser:a\r\n$6\r\nuser:b\r\n",
        )]])
        .await;
        let (cursor, keys) = super::scan_key_names(&mut connection, 7, "user:*", 100, None)
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(
            serde_json::to_value(super::key_summaries(keys, None).unwrap()).unwrap(),
            serde_json::json!([{"key": "user:a"}, {"key": "user:b"}])
        );
    }

    #[tokio::test]
    async fn scan_type_uses_native_names_and_preserves_empty_page_cursors() {
        for (requested, native, normalized) in [
            ("hash", "hash", "hash"),
            (" SORTED-SET ", "zset", "zset"),
            ("json", "ReJSON-RL", "json"),
            ("rejson-rs", "ReJSON-RS", "json"),
            ("vector-set", "vectorset", "vector-set"),
            ("array", "array", "array"),
        ] {
            let (mut connection, server) = pipelined_connection(vec![
                vec![reply(
                    &["SCAN", "0", "MATCH", "*", "COUNT", "1", "TYPE", native],
                    "*2\r\n$2\r\n19\r\n*0\r\n",
                )],
                vec![reply(
                    &["SCAN", "19", "MATCH", "*", "COUNT", "1", "TYPE", native],
                    "*2\r\n$1\r\n0\r\n*1\r\n$1\r\nb\r\n",
                )],
            ])
            .await;
            let (cursor, keys) = super::scan_key_names(&mut connection, 0, "*", 1, Some(requested))
                .await
                .unwrap();
            assert_eq!(cursor, 19);
            assert!(keys.is_empty());
            let (cursor, keys) =
                super::scan_key_names(&mut connection, cursor, "*", 1, Some(requested))
                    .await
                    .unwrap();
            server.await.unwrap();
            assert_eq!(cursor, 0);
            assert_eq!(
                super::key_summaries(keys, Some(requested)).unwrap(),
                vec![KeySummary {
                    key: "b".into(),
                    key_type: Some(normalized.into())
                }]
            );
        }
    }

    #[tokio::test]
    async fn scan_type_error_is_not_silently_replaced_with_an_unfiltered_scan() {
        let (mut connection, server) = pipelined_connection(vec![vec![reply(
            &["SCAN", "0", "MATCH", "*", "COUNT", "100", "TYPE", "hash"],
            "-ERR syntax error\r\n",
        )]])
        .await;
        let result = super::scan_key_names(&mut connection, 0, "*", 100, Some("hash")).await;
        server.await.unwrap();
        assert_eq!(result, Err(AppError::CommandFailed));
    }

    #[test]
    fn key_summaries_preserve_binary_empty_and_whitespace_names() {
        let page = super::key_summaries(vec![vec![0xff, 0], vec![], b" ".to_vec()], None)
            .expect("Redis 键名必须保留任意字节");
        assert_eq!(
            serde_json::to_value(&page).unwrap(),
            serde_json::json!([
                {"key": {"base64": "/wA="}}, {"key": ""}, {"key": " "}
            ])
        );
    }

    fn scan_page(cursor: ScanCursor, keys: &[&str], has_more: bool) -> ScanPage {
        ScanPage {
            cursor,
            keys: keys
                .iter()
                .map(|key| KeySummary {
                    key: (*key).into(),
                    key_type: None,
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
                .map(|item| item.key.utf8().unwrap())
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
                key: "x".repeat(4 * 1024 * 1024).into(),
                key_type: None,
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
