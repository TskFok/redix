use crate::{
    domain::{collection::*, RedisBytes},
    error::AppError,
    redis::RedisService,
};
use ::redis::Value;

impl RedisService {
    pub async fn get_collection_page(
        &self,
        input: CollectionPageInput,
    ) -> Result<CollectionPage, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let (type_name, length_command, mut read_command) = match input.kind {
            CollectionKind::Hash => ("hash", "HLEN", "HSCAN"),
            CollectionKind::List => ("list", "LLEN", "LRANGE"),
            CollectionKind::Set => ("set", "SCARD", "SSCAN"),
            CollectionKind::Zset => ("zset", "ZCARD", "ZSCAN"),
        };
        if input.kind == CollectionKind::Zset {
            read_command = match input.order {
                CollectionOrder::Scan => "ZSCAN",
                CollectionOrder::ScoreAsc => "ZRANGE",
                CollectionOrder::ScoreDesc => "ZREVRANGE",
            };
        }
        let mut read = ::redis::cmd(read_command);
        read.arg(&input.key);
        if input.kind == CollectionKind::List || input.order != CollectionOrder::Scan {
            let start = decimal_u64(&input.cursor)?;
            read.arg(start).arg(start + input.count as u64 - 1);
            if input.kind == CollectionKind::Zset {
                read.arg("WITHSCORES");
            }
        } else {
            read.arg(&input.cursor)
                .arg("MATCH")
                .arg(&input.pattern)
                .arg("COUNT")
                .arg(input.count);
        }
        // Metadata and the page describe one server snapshot; no full collection read.
        let (actual_type, ttl, total, value): (String, i64, u64, Value) = ::redis::pipe()
            .atomic()
            .cmd("TYPE")
            .arg(&input.key)
            .cmd("PTTL")
            .arg(&input.key)
            .cmd(length_command)
            .arg(&input.key)
            .add_command(read)
            .query_async(&mut connection)
            .await
            .map_err(collection_command_error)?;
        if actual_type == "none" {
            return Err(AppError::KeyNotFound);
        }
        if actual_type != type_name {
            return Err(AppError::UnsupportedDataType);
        }
        let mut page = parse_page(value, &input, total, ttl)?;
        if input.kind == CollectionKind::Hash {
            let mut command = ::redis::cmd("HPTTL");
            command.arg(&input.key).arg("FIELDS");
            if page.entries.is_empty() {
                // An empty MATCH page still reports capability without scanning more fields.
                command.arg(1).arg("");
            } else {
                command.arg(page.entries.len());
                for entry in &page.entries {
                    command.arg(&entry.id);
                }
            }
            match command.query_async::<Vec<i64>>(&mut connection).await {
                Ok(ttls) => {
                    if page.entries.is_empty() {
                        if ttls.len() != 1 {
                            return Err(AppError::CommandFailed);
                        }
                        page.hash_field_ttl_supported = Some(true);
                    } else {
                        apply_hash_ttls(&mut page, ttls)?;
                    }
                }
                Err(error) if unavailable_hash_ttl(&error) => {
                    page.hash_field_ttl_supported = Some(false);
                }
                Err(error) => return Err(collection_command_error(error)),
            }
        }
        ensure_serialized_size(&page)?;
        Ok(page)
    }

    pub async fn get_list_entry(
        &self,
        input: ListIndexInput,
    ) -> Result<Option<CollectionEntry>, AppError> {
        input.validate()?;
        let index = decimal_i64(&input.index)?;
        let mut connection = self.connection(&input.connection_id).await?;
        // Keep i64 indexing and length in native Redis/Rust integers, outside Lua doubles.
        let (actual_type, total, value): (String, u64, Value) = ::redis::pipe()
            .atomic()
            .cmd("TYPE")
            .arg(&input.key)
            .cmd("LLEN")
            .arg(&input.key)
            .cmd("LINDEX")
            .arg(&input.key)
            .arg(index)
            .query_async(&mut connection)
            .await
            .map_err(collection_command_error)?;
        if actual_type == "none" {
            return Err(AppError::KeyNotFound);
        }
        if actual_type != "list" {
            return Err(AppError::UnsupportedDataType);
        }
        parse_list_entry(value, index, total)
    }

    pub async fn mutate_collection(&self, input: CollectionMutationInput) -> Result<(), AppError> {
        let (script, arguments) = mutation_script(&input)?;
        let mut connection = self
            .collection_write_connection(&input.connection_id, input.key.as_bytes())
            .await?;
        // One atomic script first checks existence and type. Native HSET/SADD/ZADD/
        // PUSH commands would otherwise recreate a key that expired between page read and write.
        let result: i64 = ::redis::cmd("EVAL")
            .arg(script)
            .arg(1)
            .arg(&input.key)
            .arg(arguments)
            .query_async(&mut connection)
            .await
            .map_err(collection_command_error)?;
        match result {
            2 => Ok(()),
            0 => Err(AppError::KeyNotFound),
            1 => Err(AppError::UnsupportedDataType),
            3 => Err(AppError::UnsupportedFeature),
            4 => Err(AppError::KeyNotFound),
            _ => Err(AppError::CommandFailed),
        }
    }
}

fn mutation_script(
    input: &CollectionMutationInput,
) -> Result<(&'static str, Vec<RedisBytes>), AppError> {
    input.validate()?;
    const PREFIX: &str = "local t=redis.call('TYPE',KEYS[1]); if type(t)=='table' then t=t['ok'] end; if t=='none' then return 0 end; if t~=ARGV[1] then return 1 end; ";
    macro_rules! checked_script {
        ($command:literal) => {
            concat!(
                "local t=redis.call('TYPE',KEYS[1]); if type(t)=='table' then t=t['ok'] end; if t=='none' then return 0 end; if t~=ARGV[1] then return 1 end; ",
                $command,
                "; return 2"
            )
        };
    }
    let (expected, script, values): (&str, &'static str, Vec<RedisBytes>) = match &input.mutation {
        CollectionMutation::HashSet { field, value } => (
            "hash",
            // HSET clears a field's expiration. Read and restore it atomically on
            // Redis 7.4+, and retain ordinary editing on older Redis versions.
            checked_script!(r#"
                local ttl=redis.pcall('HPTTL',KEYS[1],'FIELDS',1,ARGV[2])
                if ttl.err then
                    local e=string.lower(ttl.err)
                    if not string.find(e,'unknown command',1,true) and not string.find(e,'unknown redis command',1,true) then return 3 end
                elseif ttl[1]>=0 and not redis.acl_check_cmd('HPEXPIRE',KEYS[1],string.format('%.0f',ttl[1]),'FIELDS',1,ARGV[2]) then
                    return 3
                end
                -- HPTTL reports logical expiry without deleting the last field.
                -- HEXISTS performs lazy expiry so the key cannot be recreated below.
                if not ttl.err and ttl[1]==-2 then redis.call('HEXISTS',KEYS[1],ARGV[2]) end
                if redis.call('EXISTS',KEYS[1])==0 then return 0 end
                redis.call('HSET',KEYS[1],ARGV[2],ARGV[3])
                if not ttl.err and ttl[1]>=0 then
                    redis.call('HPEXPIRE',KEYS[1],string.format('%.0f',ttl[1]),'FIELDS',1,ARGV[2])
                end
            "#),
            vec![field.clone(), value.clone()],
        ),
        CollectionMutation::HashDelete { field } => (
            "hash",
            checked_script!("redis.call('HDEL',KEYS[1],ARGV[2])"),
            vec![field.clone()],
        ),
        CollectionMutation::HashExpire { field, ttl_ms } => (
            "hash",
            checked_script!("if redis.call('HEXISTS',KEYS[1],ARGV[2])==0 then return 4 end; local result=redis.call('HPEXPIRE',KEYS[1],ARGV[3],'FIELDS',1,ARGV[2]); if result[1]==-2 then return 4 end; if result[1]~=1 then return 5 end"),
            vec![field.clone(), decimal_u64(ttl_ms)?.to_string().into()],
        ),
        CollectionMutation::HashPersist { field } => (
            "hash",
            checked_script!("if redis.call('HEXISTS',KEYS[1],ARGV[2])==0 then return 4 end; local result=redis.call('HPERSIST',KEYS[1],'FIELDS',1,ARGV[2]); if result[1]==-2 then return 4 end; if result[1]~=1 and result[1]~=-1 then return 5 end"),
            vec![field.clone()],
        ),
        CollectionMutation::SetAdd { member } => (
            "set",
            checked_script!("redis.call('SADD',KEYS[1],ARGV[2])"),
            vec![member.clone()],
        ),
        CollectionMutation::SetRemove { member } => (
            "set",
            checked_script!("redis.call('SREM',KEYS[1],ARGV[2])"),
            vec![member.clone()],
        ),
        CollectionMutation::ZsetAdd { member, score } => (
            "zset",
            checked_script!("redis.call('ZADD',KEYS[1],ARGV[2],ARGV[3])"),
            vec![score.to_string().into(), member.clone()],
        ),
        CollectionMutation::ZsetRemove { member } => (
            "zset",
            checked_script!("redis.call('ZREM',KEYS[1],ARGV[2])"),
            vec![member.clone()],
        ),
        CollectionMutation::ListSet { index, value } => (
            "list",
            checked_script!("redis.call('LSET',KEYS[1],ARGV[2],ARGV[3])"),
            vec![index.clone().into(), value.clone()],
        ),
        CollectionMutation::ListAppend { value, prepend } => (
            "list",
            if *prepend {
                checked_script!("redis.call('LPUSH',KEYS[1],ARGV[2])")
            } else {
                checked_script!("redis.call('RPUSH',KEYS[1],ARGV[2])")
            },
            vec![value.clone()],
        ),
        CollectionMutation::ListTrim { count, from_head } => (
            "list",
            if *from_head {
                checked_script!("redis.call('LTRIM',KEYS[1],ARGV[2],-1)")
            } else {
                checked_script!("redis.call('LTRIM',KEYS[1],0,ARGV[2])")
            },
            vec![if *from_head {
                decimal_u64(count)?.to_string()
            } else {
                format!("-{}", decimal_u64(count)? + 1)
            }.into()],
        ),
    };
    debug_assert!(script.starts_with(PREFIX));
    let mut arguments = vec![RedisBytes::from(expected)];
    arguments.extend(values);
    Ok((script, arguments))
}

fn parse_page(
    value: Value,
    input: &CollectionPageInput,
    total: u64,
    ttl_ms: i64,
) -> Result<CollectionPage, AppError> {
    input.validate()?;
    // SCAN COUNT is only a hint. Reject an oversized reply as a whole, never truncate
    // and return its cursor (which would silently skip unseen members).
    ensure_response_size(&value, 0, &mut 0, &mut 0)?;
    let paired = matches!(input.kind, CollectionKind::Hash | CollectionKind::Zset);
    let (next_cursor, values) =
        if input.kind == CollectionKind::List || input.order != CollectionOrder::Scan {
            let values = array(value)?;
            let entry_count = values.len() / if paired { 2 } else { 1 };
            if entry_count > input.count {
                return Err(AppError::CommandFailed);
            }
            let end = decimal_u64(&input.cursor)? + entry_count as u64;
            (
                (if end < total && !values.is_empty() {
                    end
                } else {
                    0
                })
                .to_string(),
                values,
            )
        } else {
            let mut outer = array(value)?.into_iter();
            let cursor = text(outer.next().ok_or(AppError::CommandFailed)?)?;
            let cursor = decimal_u64(&cursor)
                .map_err(|_| AppError::CommandFailed)?
                .to_string();
            let values = array(outer.next().ok_or(AppError::CommandFailed)?)?;
            if outer.next().is_some() {
                return Err(AppError::CommandFailed);
            }
            (cursor, values)
        };
    if (paired && values.len() % 2 != 0)
        || values.len() > MAX_COLLECTION_PAGE_ENTRIES * if paired { 2 } else { 1 }
    {
        return Err(AppError::CommandFailed);
    }
    let mut entries = Vec::new();
    let mut values = values.into_iter();
    while let Some(raw) = values.next() {
        let id = payload(raw)?;
        let (id, value, score) = match input.kind {
            CollectionKind::Hash => (
                id,
                payload(values.next().ok_or(AppError::CommandFailed)?)?,
                None,
            ),
            CollectionKind::Zset => {
                let score = text(values.next().ok_or(AppError::CommandFailed)?)?
                    .parse::<f64>()
                    .map_err(|_| AppError::CommandFailed)?;
                if !score.is_finite() {
                    return Err(AppError::CommandFailed);
                }
                (id.clone(), id, Some(score))
            }
            CollectionKind::List => (
                (decimal_u64(&input.cursor)? + entries.len() as u64)
                    .to_string()
                    .into(),
                id,
                None,
            ),
            CollectionKind::Set => (id.clone(), id, None),
        };
        entries.push(CollectionEntry {
            id,
            value,
            score,
            ttl_ms: None,
        });
    }
    let page = CollectionPage {
        entries,
        has_more: next_cursor != "0",
        next_cursor,
        total: total.to_string(),
        ttl_ms,
        hash_field_ttl_supported: None,
    };
    ensure_serialized_size(&page)?;
    Ok(page)
}

fn ensure_serialized_size(value: &impl serde::Serialize) -> Result<(), AppError> {
    // Include JSON escaping and repeated member identities in the IPC byte budget.
    if serde_json::to_vec(value)
        .map_err(|_| AppError::CommandFailed)?
        .len()
        > MAX_COLLECTION_RESPONSE_BYTES
    {
        return Err(AppError::CommandFailed);
    }
    Ok(())
}

fn apply_hash_ttls(page: &mut CollectionPage, ttls: Vec<i64>) -> Result<(), AppError> {
    if ttls.len() != page.entries.len() || ttls.iter().any(|ttl| *ttl < -2) {
        return Err(AppError::CommandFailed);
    }
    for (entry, ttl) in page.entries.iter_mut().zip(ttls) {
        entry.ttl_ms = Some(ttl);
    }
    page.entries.retain(|entry| entry.ttl_ms != Some(-2));
    page.hash_field_ttl_supported = Some(true);
    ensure_serialized_size(page)
}

fn parse_list_entry(
    value: Value,
    index: i64,
    total: u64,
) -> Result<Option<CollectionEntry>, AppError> {
    if value == Value::Nil {
        return Ok(None);
    }
    let total = i64::try_from(total).map_err(|_| AppError::CommandFailed)?;
    let absolute = if index < 0 {
        total.checked_add(index)
    } else {
        Some(index)
    }
    .filter(|absolute| *absolute >= 0 && *absolute < total)
    .ok_or(AppError::CommandFailed)?;
    ensure_response_size(&value, 0, &mut 0, &mut 0)?;
    let value = payload(value)?;
    if value.len() > MAX_COLLECTION_VALUE_BYTES {
        return Err(AppError::CommandFailed);
    }
    let entry = CollectionEntry {
        id: absolute.to_string().into(),
        value,
        score: None,
        ttl_ms: None,
    };
    ensure_serialized_size(&entry)?;
    Ok(Some(entry))
}

fn unavailable_hash_ttl(error: &::redis::RedisError) -> bool {
    let detail = error.to_string().to_ascii_lowercase();
    detail.contains("unknown command")
        || detail.contains("unknown redis command")
        || detail.contains("noperm")
        || detail.contains("no permissions")
        || detail.contains("permission denied")
}

fn collection_command_error(error: ::redis::RedisError) -> AppError {
    if unavailable_hash_ttl(&error) {
        AppError::UnsupportedFeature
    } else if error.code() == Some("WRONGTYPE")
        || error
            .into_server_errors()
            .is_some_and(|errors| errors.iter().any(|(_, error)| error.code() == "WRONGTYPE"))
    {
        AppError::UnsupportedDataType
    } else {
        AppError::CommandFailed
    }
}

fn array(value: Value) -> Result<Vec<Value>, AppError> {
    match value {
        Value::Array(values) => Ok(values),
        _ => Err(AppError::CommandFailed),
    }
}

fn payload(value: Value) -> Result<RedisBytes, AppError> {
    match value {
        Value::BulkString(bytes) => Ok(bytes.into()),
        Value::SimpleString(value) => Ok(value.into()),
        _ => Err(AppError::CommandFailed),
    }
}

fn text(value: Value) -> Result<String, AppError> {
    match value {
        Value::BulkString(bytes) => String::from_utf8(bytes).map_err(|_| AppError::CommandFailed),
        Value::SimpleString(value) => Ok(value),
        Value::Int(value) => Ok(value.to_string()),
        Value::Double(value) => Ok(value.to_string()),
        _ => Err(AppError::CommandFailed),
    }
}

fn ensure_response_size(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    bytes: &mut usize,
) -> Result<(), AppError> {
    *nodes += 1;
    if depth > 3 || *nodes > MAX_COLLECTION_PAGE_ENTRIES * 2 + 8 {
        return Err(AppError::CommandFailed);
    }
    let added = match value {
        Value::BulkString(value) => value.len(),
        Value::SimpleString(value) => value.len(),
        Value::Array(values) => {
            if values.len() > MAX_COLLECTION_PAGE_ENTRIES * 2 {
                return Err(AppError::CommandFailed);
            }
            for value in values {
                ensure_response_size(value, depth + 1, nodes, bytes)?;
            }
            0
        }
        Value::Int(_) | Value::Double(_) => 24,
        _ => return Err(AppError::CommandFailed),
    };
    *bytes = bytes.checked_add(added).ok_or(AppError::CommandFailed)?;
    if *bytes > MAX_COLLECTION_RESPONSE_BYTES {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_binary_pages_preserve_raw_identifiers_values_and_list_lookup() {
        for kind in [
            CollectionKind::Hash,
            CollectionKind::Set,
            CollectionKind::Zset,
            CollectionKind::List,
        ] {
            let mut values = vec![Value::BulkString(vec![0xff, 0])];
            if kind == CollectionKind::Hash {
                values.push(Value::BulkString(vec![0xfe, 0]));
            } else if kind == CollectionKind::Zset {
                values.push(text("1.5"));
            }
            let value = if kind == CollectionKind::List {
                Value::Array(values)
            } else {
                Value::Array(vec![text("12"), Value::Array(values)])
            };
            let page = parse_page(value, &input(kind), 20, -1);
            assert!(page.is_ok(), "binary {kind:?} reply must be accepted");
            let page = serde_json::to_value(page.unwrap()).unwrap();
            assert_eq!(
                page["entries"][0]["id"],
                if kind == CollectionKind::List {
                    serde_json::json!("0")
                } else {
                    serde_json::json!({"base64": "/wA="})
                }
            );
            assert_eq!(
                page["entries"][0]["value"],
                serde_json::json!({"base64": if kind == CollectionKind::Hash { "/gA=" } else { "/wA=" }})
            );
        }
        let entry = parse_list_entry(Value::BulkString(vec![0xff, 0]), -1, 2);
        assert!(entry.is_ok(), "binary LINDEX reply must be accepted");
        assert_eq!(
            serde_json::to_value(entry.unwrap()).unwrap()["value"],
            serde_json::json!({"base64": "/wA="})
        );
    }

    #[test]
    fn collection_binary_cursor_and_score_are_rejected_as_invalid_metadata() {
        for (kind, value) in [
            (
                CollectionKind::Set,
                Value::Array(vec![Value::BulkString(vec![0xff]), Value::Array(vec![])]),
            ),
            (
                CollectionKind::Zset,
                Value::Array(vec![
                    text("0"),
                    Value::Array(vec![text("member"), Value::BulkString(vec![0xff])]),
                ]),
            ),
        ] {
            assert_eq!(
                parse_page(value, &input(kind), 1, -1),
                Err(AppError::CommandFailed)
            );
        }
    }

    #[test]
    fn collection_binary_page_budget_includes_base64_and_repeated_member_ids() {
        let page = |size| {
            parse_page(
                Value::Array(vec![
                    text("12"),
                    Value::Array(vec![Value::BulkString(vec![0xff; size])]),
                ]),
                &input(CollectionKind::Set),
                2,
                -1,
            )
        };
        assert!(page(MAX_COLLECTION_VALUE_BYTES).is_ok());
        // The raw member fits the 4 MiB response limit, while its base64 IPC
        // representation repeated in id/value exceeds that same limit.
        assert_eq!(
            page(MAX_COLLECTION_VALUE_BYTES * 2),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn collection_binary_mutations_send_original_bytes_to_lua() {
        let request = serde_json::from_value::<CollectionMutationInput>(serde_json::json!({
            "connection_id": "local", "key": {"base64": "/wA="}, "mutation": {
                "operation": "hash_set", "field": {"base64": "/gA="}, "value": {"base64": "/QA="}
            }
        }));
        assert!(request.is_ok(), "binary mutation must deserialize");
        let (_, args) = mutation_script(&request.unwrap()).unwrap();
        assert_eq!(args[1].as_bytes(), &[0xfe, 0]);
        assert_eq!(args[2].as_bytes(), &[0xfd, 0]);
    }

    #[test]
    fn collection_numeric_mutation_arguments_use_redis_integer_format_without_lua_rounding() {
        for (mutation, argument_index, expected) in [
            (
                CollectionMutation::HashExpire {
                    field: "f".into(),
                    ttl_ms: "00042".into(),
                },
                2,
                "42",
            ),
            (
                CollectionMutation::ListTrim {
                    count: "00002".into(),
                    from_head: true,
                },
                1,
                "2",
            ),
            (
                CollectionMutation::ListTrim {
                    count: "9223372036854775806".into(),
                    from_head: false,
                },
                1,
                "-9223372036854775807",
            ),
        ] {
            let (_, arguments) = mutation_script(&CollectionMutationInput {
                connection_id: "local".into(),
                key: "key".into(),
                mutation,
            })
            .unwrap();
            assert_eq!(arguments[argument_index].as_bytes(), expected.as_bytes());
        }
    }

    #[test]
    fn collection_list_index_results_preserve_integers_beyond_javascript_and_lua_precision() {
        for (index, total, expected) in [
            (
                9_007_199_254_740_993,
                9_007_199_254_740_995,
                "9007199254740993",
            ),
            (-2, 9_007_199_254_740_995, "9007199254740993"),
            (-1, i64::MAX as u64, "9223372036854775806"),
        ] {
            let entry = parse_list_entry(text("v"), index, total).unwrap().unwrap();
            assert_eq!(entry.id.as_bytes(), expected.as_bytes());
        }
        assert_eq!(parse_list_entry(Value::Nil, i64::MIN, 1), Ok(None));
        assert_eq!(
            parse_list_entry(text("impossible"), -2, 1),
            Err(AppError::CommandFailed)
        );
        assert_eq!(
            parse_list_entry(text(&"x".repeat(MAX_COLLECTION_VALUE_BYTES + 1)), 0, 1),
            Err(AppError::CommandFailed)
        );
        assert_eq!(
            parse_list_entry(text(&"\0".repeat(MAX_COLLECTION_VALUE_BYTES)), 0, 1),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn collection_hash_ttl_batch_maps_persistent_fields_and_filters_expired_fields() {
        let mut page = parse_page(
            Value::Array(vec![
                text("42"),
                Value::Array(vec![
                    text("permanent"),
                    text("p"),
                    text("expiring"),
                    text("e"),
                    text("expired"),
                    text("x"),
                ]),
            ]),
            &input(CollectionKind::Hash),
            3,
            -1,
        )
        .unwrap();
        apply_hash_ttls(&mut page, vec![-1, 1200, -2]).unwrap();
        assert_eq!(page.hash_field_ttl_supported, Some(true));
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.entries[0].ttl_ms, Some(-1));
        assert_eq!(page.entries[1].ttl_ms, Some(1200));
        assert_eq!(page.next_cursor, "42");
        assert!(page.has_more);
        assert_eq!(
            apply_hash_ttls(&mut page, vec![-1]),
            Err(AppError::CommandFailed)
        );
        assert_eq!(
            apply_hash_ttls(&mut page, vec![-1, -3]),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn collection_hash_ttl_metadata_remains_within_final_ipc_budget() {
        let mut page = CollectionPage {
            entries: vec![CollectionEntry {
                id: "f".into(),
                value: "".into(),
                score: None,
                ttl_ms: None,
            }],
            next_cursor: "0".into(),
            has_more: false,
            total: "1".into(),
            ttl_ms: -1,
            hash_field_ttl_supported: None,
        };
        let overhead = serde_json::to_vec(&page).unwrap().len();
        page.entries[0].value = "v".repeat(MAX_COLLECTION_RESPONSE_BYTES - overhead).into();
        assert!(ensure_serialized_size(&page).is_ok());
        assert_eq!(
            apply_hash_ttls(&mut page, vec![9_007_199_254_740_991]),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn collection_sorted_zset_parses_rank_page_and_preserves_server_order() {
        let request: CollectionPageInput = serde_json::from_value(serde_json::json!({
            "connection_id": "local", "key": "key", "kind": "zset", "cursor": "2",
            "count": 2, "pattern": "*", "order": "score_desc"
        }))
        .unwrap();
        let page = parse_page(
            Value::Array(vec![text("high"), text("12.5"), text("low"), text("-3")]),
            &request,
            5,
            -1,
        )
        .unwrap();
        assert_eq!(page.entries[0].id.as_bytes(), b"high");
        assert_eq!(page.entries[0].score, Some(12.5));
        assert_eq!(page.entries[1].id.as_bytes(), b"low");
        assert_eq!(page.next_cursor, "4");
        assert!(page.has_more);
    }
    fn text(value: &str) -> Value {
        Value::BulkString(value.as_bytes().to_vec())
    }
    fn input(kind: CollectionKind) -> CollectionPageInput {
        CollectionPageInput {
            connection_id: "local".into(),
            key: "key".into(),
            kind,
            cursor: "0".into(),
            count: 2,
            pattern: "*".into(),
            order: CollectionOrder::Scan,
        }
    }
    #[test]
    fn collection_scan_preserves_large_cursor_and_all_fields_when_count_is_a_hint() {
        let value = Value::Array(vec![
            text("18446744073709551615"),
            Value::Array(vec![
                text("a"),
                text("1"),
                text("b"),
                text("2"),
                text("c"),
                text("3"),
            ]),
        ]);
        let page = parse_page(value, &input(CollectionKind::Hash), 30, 5000).unwrap();
        assert_eq!(page.entries.len(), 3);
        assert_eq!(page.entries[2].id.as_bytes(), b"c");
        assert_eq!(page.next_cursor, "18446744073709551615");
        assert!(page.has_more);
    }
    #[test]
    fn collection_oversized_scan_fails_without_returning_a_partial_cursor() {
        for values in [
            vec![text("x"); 2001],
            vec![Value::BulkString(vec![b'a'; 4 * 1024 * 1024 + 1])],
        ] {
            assert_eq!(
                parse_page(
                    Value::Array(vec![text("12"), Value::Array(values)]),
                    &input(CollectionKind::Set),
                    4000,
                    -1
                ),
                Err(AppError::CommandFailed)
            );
        }
    }
    #[test]
    fn collection_list_page_uses_absolute_indexes_and_exact_end() {
        let mut request = input(CollectionKind::List);
        request.cursor = "500".into();
        let page = parse_page(
            Value::Array(vec![text("before"), text("after")]),
            &request,
            502,
            60000,
        )
        .unwrap();
        assert_eq!(page.entries[0].id.as_bytes(), b"500");
        assert_eq!(page.entries[1].id.as_bytes(), b"501");
        assert!(!page.has_more);
        assert_eq!(page.next_cursor, "0");
    }
    #[test]
    fn collection_empty_scan_page_does_not_end_iteration() {
        let page = parse_page(
            Value::Array(vec![text("13"), Value::Array(vec![])]),
            &input(CollectionKind::Set),
            40,
            -1,
        )
        .unwrap();
        assert!(page.entries.is_empty());
        assert!(page.has_more);
    }
    #[test]
    fn collection_malformed_hash_and_nonfinite_score_fail() {
        for (kind, values) in [
            (CollectionKind::Hash, vec![text("field")]),
            (CollectionKind::Zset, vec![text("member"), text("nan")]),
        ] {
            assert_eq!(
                parse_page(
                    Value::Array(vec![text("0"), Value::Array(values)]),
                    &input(kind),
                    1,
                    -1
                ),
                Err(AppError::CommandFailed)
            );
        }
    }
}
