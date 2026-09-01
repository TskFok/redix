use crate::{domain::collection::*, error::AppError, redis::RedisService};
use ::redis::Value;

impl RedisService {
    pub async fn get_collection_page(
        &self,
        input: CollectionPageInput,
    ) -> Result<CollectionPage, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let (type_name, length_command, read_command) = match input.kind {
            CollectionKind::Hash => ("hash", "HLEN", "HSCAN"),
            CollectionKind::List => ("list", "LLEN", "LRANGE"),
            CollectionKind::Set => ("set", "SCARD", "SSCAN"),
            CollectionKind::Zset => ("zset", "ZCARD", "ZSCAN"),
        };
        let mut read = ::redis::cmd(read_command);
        read.arg(&input.key);
        if input.kind == CollectionKind::List {
            let start = decimal_u64(&input.cursor)?;
            read.arg(start).arg(start + input.count as u64 - 1);
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
            .map_err(|_| AppError::CommandFailed)?;
        if actual_type == "none" {
            return Err(AppError::KeyNotFound);
        }
        if actual_type != type_name {
            return Err(AppError::UnsupportedDataType);
        }
        parse_page(value, &input, total, ttl)
    }

    pub async fn mutate_collection(&self, input: CollectionMutationInput) -> Result<(), AppError> {
        let (script, arguments) = mutation_script(&input)?;
        let mut connection = self.connection(&input.connection_id).await?;
        // One atomic script first checks existence and type. Native HSET/SADD/ZADD/
        // PUSH commands would otherwise recreate a key that expired between page read and write.
        let result: i64 = ::redis::cmd("EVAL")
            .arg(script)
            .arg(1)
            .arg(&input.key)
            .arg(arguments)
            .query_async(&mut connection)
            .await
            .map_err(|_| AppError::CommandFailed)?;
        match result {
            2 => Ok(()),
            0 => Err(AppError::KeyNotFound),
            1 => Err(AppError::UnsupportedDataType),
            _ => Err(AppError::CommandFailed),
        }
    }
}

fn mutation_script(
    input: &CollectionMutationInput,
) -> Result<(&'static str, Vec<String>), AppError> {
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
    let (expected, script, values): (&str, &'static str, Vec<String>) = match &input.mutation {
        CollectionMutation::HashSet { field, value } => (
            "hash",
            checked_script!("redis.call('HSET',KEYS[1],ARGV[2],ARGV[3])"),
            vec![field.clone(), value.clone()],
        ),
        CollectionMutation::HashDelete { field } => (
            "hash",
            checked_script!("redis.call('HDEL',KEYS[1],ARGV[2])"),
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
            vec![score.to_string(), member.clone()],
        ),
        CollectionMutation::ZsetRemove { member } => (
            "zset",
            checked_script!("redis.call('ZREM',KEYS[1],ARGV[2])"),
            vec![member.clone()],
        ),
        CollectionMutation::ListSet { index, value } => (
            "list",
            checked_script!("redis.call('LSET',KEYS[1],ARGV[2],ARGV[3])"),
            vec![index.clone(), value.clone()],
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
    };
    debug_assert!(script.starts_with(PREFIX));
    let mut arguments = vec![expected.to_string()];
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
    let (next_cursor, values) = if input.kind == CollectionKind::List {
        let values = array(value)?;
        if values.len() > input.count {
            return Err(AppError::CommandFailed);
        }
        let end = decimal_u64(&input.cursor)? + values.len() as u64;
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
    let paired = matches!(input.kind, CollectionKind::Hash | CollectionKind::Zset);
    if (paired && values.len() % 2 != 0)
        || values.len() > MAX_COLLECTION_PAGE_ENTRIES * if paired { 2 } else { 1 }
    {
        return Err(AppError::CommandFailed);
    }
    let mut entries = Vec::new();
    let mut values = values.into_iter();
    while let Some(raw) = values.next() {
        let id = text(raw)?;
        let (id, value, score) = match input.kind {
            CollectionKind::Hash => (
                id,
                text(values.next().ok_or(AppError::CommandFailed)?)?,
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
                (decimal_u64(&input.cursor)? + entries.len() as u64).to_string(),
                id,
                None,
            ),
            CollectionKind::Set => (id.clone(), id, None),
        };
        entries.push(CollectionEntry { id, value, score });
    }
    let page = CollectionPage {
        entries,
        has_more: next_cursor != "0",
        next_cursor,
        total: total.to_string(),
        ttl_ms,
    };
    // Include JSON escaping and repeated member identities in the IPC byte budget.
    if serde_json::to_vec(&page)
        .map_err(|_| AppError::CommandFailed)?
        .len()
        > MAX_COLLECTION_RESPONSE_BYTES
    {
        return Err(AppError::CommandFailed);
    }
    Ok(page)
}

fn array(value: Value) -> Result<Vec<Value>, AppError> {
    match value {
        Value::Array(values) => Ok(values),
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
        assert_eq!(page.entries[2].id, "c");
        assert_eq!(page.next_cursor, "18446744073709551615");
        assert!(page.has_more);
    }
    #[test]
    fn collection_oversized_or_binary_scan_fails_without_returning_a_partial_cursor() {
        for values in [
            vec![text("x"); 2001],
            vec![Value::BulkString(vec![b'a'; 4 * 1024 * 1024 + 1])],
            vec![Value::BulkString(vec![0xff])],
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
        assert_eq!(page.entries[0].id, "500");
        assert_eq!(page.entries[1].id, "501");
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
