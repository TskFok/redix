use crate::{domain::search_aggregate::*, error::AppError};
use ::redis::{Cmd, Value};
use std::collections::HashSet;

impl super::RedisService {
    pub async fn aggregate_search(
        &self,
        input: SearchAggregateInput,
    ) -> Result<SearchAggregateResult, AppError> {
        input.validate()?;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut connection = self.search_connection(&input.connection_id).await?;
            let value = build_aggregate_command(&input)?
                .query_async(&mut connection)
                .await
                .map_err(super::connection_manager::map_command_error)?;
            parse_aggregate(value, input.offset, input.limit)
        })
        .await
        .map_err(|_| AppError::CommandFailed)?
    }
}

pub(crate) fn build_aggregate_command(input: &SearchAggregateInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("FT.AGGREGATE");
    command.arg(&input.index).arg(&input.query);
    let loaded = input.loaded_fields();
    if !loaded.is_empty() {
        command.arg("LOAD").arg(loaded.len());
        for field in loaded {
            command.arg(format!("@{field}"));
        }
    }
    if !input.group_by.is_empty() || !input.reducers.is_empty() {
        command.arg("GROUPBY").arg(input.group_by.len());
        for field in &input.group_by {
            command.arg(format!("@{field}"));
        }
        for reducer in &input.reducers {
            let function = match reducer.function {
                AggregateFunction::Count => "COUNT",
                AggregateFunction::Sum => "SUM",
                AggregateFunction::Min => "MIN",
                AggregateFunction::Max => "MAX",
                AggregateFunction::Avg => "AVG",
            };
            command
                .arg("REDUCE")
                .arg(function)
                .arg(usize::from(reducer.field.is_some()));
            if let Some(field) = &reducer.field {
                command.arg(format!("@{field}"));
            }
            command.arg("AS").arg(&reducer.alias);
        }
    }
    if !input.sort_by.is_empty() {
        command.arg("SORTBY").arg(input.sort_by.len() * 2);
        for sort in &input.sort_by {
            command
                .arg(format!("@{}", sort.field))
                .arg(match sort.direction {
                    AggregateDirection::Asc => "ASC",
                    AggregateDirection::Desc => "DESC",
                });
        }
        command
            .arg("MAX")
            .arg(input.offset + u64::from(input.limit) + 1);
    }
    command.arg("LIMIT").arg(input.offset).arg(input.limit + 1);
    Ok(command)
}

pub(crate) fn parse_aggregate(
    value: Value,
    offset: u64,
    limit: u32,
) -> Result<SearchAggregateResult, AppError> {
    if offset > MAX_AGGREGATE_OFFSET || !(1..=MAX_AGGREGATE_PAGE).contains(&limit) {
        return Err(AppError::CommandFailed);
    }
    bound_value(&value, 0, &mut 0, &mut 0)?;
    let (values, resp3) = match unwrap(value) {
        Value::Array(values) => {
            let mut values = values.into_iter();
            // The RESP2 first integer is not a reliable total; only lookahead controls pagination.
            if !matches!(values.next().map(unwrap), Some(Value::Int(_))) {
                return Err(AppError::CommandFailed);
            }
            (values.collect::<Vec<_>>(), false)
        }
        Value::Map(pairs) => {
            let mut results = None;
            let mut seen = HashSet::new();
            for (name, value) in pairs {
                let name = string(name, 256)?;
                if !seen.insert(name.clone()) {
                    return Err(AppError::CommandFailed);
                }
                match name.as_str() {
                    "results" => results = Some(value),
                    "warning" | "warnings" | "error" => match unwrap(value) {
                        Value::Nil => (),
                        Value::Array(values) if values.is_empty() => (),
                        _ => return Err(AppError::CommandFailed),
                    },
                    _ => (),
                }
            }
            let Value::Array(values) = unwrap(results.ok_or(AppError::CommandFailed)?) else {
                return Err(AppError::CommandFailed);
            };
            (values, true)
        }
        _ => return Err(AppError::CommandFailed),
    };
    if values.len() > limit as usize + 1 {
        return Err(AppError::CommandFailed);
    }
    let more = values.len() > limit as usize;
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    for value in values {
        let value = if resp3 {
            let mut attributes = None;
            let mut seen = HashSet::new();
            for (name, value) in pairs(value)? {
                let name = string(name, 256)?;
                if !seen.insert(name.clone()) {
                    return Err(AppError::CommandFailed);
                }
                if name == "extra_attributes" {
                    attributes = Some(value);
                }
            }
            attributes.ok_or(AppError::CommandFailed)?
        } else {
            value
        };
        let mut row_columns = Vec::new();
        let fields = if matches!(unwrap_ref(&value), Value::Nil) {
            Vec::new()
        } else {
            let mut fields = Vec::new();
            let mut names = HashSet::new();
            for (name, value) in pairs(value)? {
                let name = string(name, 256)?;
                if name.is_empty() || !names.insert(name.clone()) {
                    return Err(AppError::CommandFailed);
                }
                if !row_columns.contains(&name) {
                    row_columns.push(name.clone());
                }
                if row_columns.len() > MAX_AGGREGATE_COLUMNS {
                    return Err(AppError::CommandFailed);
                }
                let value = json(value)?;
                if serde_json::to_vec(&value)
                    .map_err(|_| AppError::CommandFailed)?
                    .len()
                    > MAX_AGGREGATE_CELL_BYTES
                {
                    return Err(AppError::CommandFailed);
                }
                fields.push(crate::domain::SearchDocumentField { name, value });
            }
            fields
        };
        if rows.len() < limit as usize {
            for column in row_columns {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            if columns.len() > MAX_AGGREGATE_COLUMNS {
                return Err(AppError::CommandFailed);
            }
            rows.push(AggregateRow { fields });
        }
    }
    let next = offset + rows.len() as u64;
    let result = SearchAggregateResult {
        offset,
        next_offset: (more && next <= MAX_AGGREGATE_OFFSET).then_some(next),
        columns,
        rows,
    };
    if serde_json::to_vec(&result)
        .map_err(|_| AppError::CommandFailed)?
        .len()
        > MAX_AGGREGATE_RESPONSE_BYTES
    {
        return Err(AppError::CommandFailed);
    }
    Ok(result)
}

// Bound the whole reply, including ignored metadata, before recursive conversion or allocation.
fn bound_value(
    value: &Value,
    depth: usize,
    bytes: &mut usize,
    nodes: &mut usize,
) -> Result<(), AppError> {
    *nodes += 1;
    *bytes = bytes.saturating_add(8);
    if depth > 12 || *nodes > 32_768 || *bytes > MAX_AGGREGATE_RESPONSE_BYTES {
        return Err(AppError::CommandFailed);
    }
    match value {
        Value::BulkString(value) => *bytes = bytes.saturating_add(value.len()),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => {
            *bytes = bytes.saturating_add(value.len())
        }
        Value::Array(values) | Value::Set(values) => {
            for value in values {
                bound_value(value, depth + 1, bytes, nodes)?;
            }
        }
        Value::Map(values) => {
            for (key, value) in values {
                bound_value(key, depth + 1, bytes, nodes)?;
                bound_value(value, depth + 1, bytes, nodes)?;
            }
        }
        Value::Attribute { data, attributes } => {
            bound_value(data, depth + 1, bytes, nodes)?;
            for (key, value) in attributes {
                bound_value(key, depth + 1, bytes, nodes)?;
                bound_value(value, depth + 1, bytes, nodes)?;
            }
        }
        Value::Int(_) | Value::Double(_) | Value::Nil | Value::Boolean(_) => (),
        _ => return Err(AppError::CommandFailed),
    }
    if *bytes > MAX_AGGREGATE_RESPONSE_BYTES {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

fn unwrap(mut value: Value) -> Value {
    while let Value::Attribute { data, .. } = value {
        value = *data;
    }
    value
}
fn unwrap_ref(mut value: &Value) -> &Value {
    while let Value::Attribute { data, .. } = value {
        value = data;
    }
    value
}
fn string(value: Value, maximum: usize) -> Result<String, AppError> {
    match unwrap(value) {
        Value::BulkString(bytes) if bytes.len() <= maximum => {
            String::from_utf8(bytes).map_err(|_| AppError::CommandFailed)
        }
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. }
            if value.len() <= maximum =>
        {
            Ok(value)
        }
        _ => Err(AppError::CommandFailed),
    }
}
fn pairs(value: Value) -> Result<Vec<(Value, Value)>, AppError> {
    match unwrap(value) {
        Value::Map(pairs) if pairs.len() <= MAX_AGGREGATE_COLUMNS => Ok(pairs),
        Value::Array(values)
            if values.len() % 2 == 0 && values.len() <= MAX_AGGREGATE_COLUMNS * 2 =>
        {
            let mut values = values.into_iter();
            let mut pairs = Vec::new();
            while let Some(name) = values.next() {
                pairs.push((name, values.next().ok_or(AppError::CommandFailed)?));
            }
            Ok(pairs)
        }
        _ => Err(AppError::CommandFailed),
    }
}
fn json(value: Value) -> Result<serde_json::Value, AppError> {
    use serde_json::Value as Json;
    Ok(match unwrap(value) {
        Value::Nil => Json::Null,
        Value::Boolean(value) => Json::Bool(value),
        Value::Int(value) if value.unsigned_abs() <= 9_007_199_254_740_991 => value.into(),
        Value::Int(value) => value.to_string().into(),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(Json::Number)
            .ok_or(AppError::CommandFailed)?,
        value @ (Value::BulkString(_) | Value::SimpleString(_) | Value::VerbatimString { .. }) => {
            string(value, MAX_AGGREGATE_CELL_BYTES)?.into()
        }
        Value::Array(values) if values.len() <= 256 => {
            Json::Array(values.into_iter().map(json).collect::<Result<_, _>>()?)
        }
        Value::Map(values) if values.len() <= MAX_AGGREGATE_COLUMNS => {
            let mut object = serde_json::Map::new();
            for (name, value) in values {
                let name = string(name, 256)?;
                if object.insert(name, json(value)?).is_some() {
                    return Err(AppError::CommandFailed);
                }
            }
            Json::Object(object)
        }
        _ => return Err(AppError::CommandFailed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(value: &str) -> Value {
        Value::BulkString(value.as_bytes().to_vec())
    }
    fn row(value: &str) -> Value {
        Value::Array(vec![
            text("category"),
            text(value),
            text("amount"),
            text("12"),
        ])
    }
    #[test]
    fn aggregate_builder_uses_only_typed_stages_with_bounded_lookahead() {
        let input = SearchAggregateInput {
            connection_id: "local".into(),
            index: "idx".into(),
            query: "* LIMIT 0 999".into(),
            load_fields: vec!["price".into()],
            group_by: vec!["category".into()],
            reducers: vec![AggregateReducer {
                function: AggregateFunction::Sum,
                field: Some("price".into()),
                alias: "amount".into(),
            }],
            sort_by: vec![AggregateSort {
                field: "amount".into(),
                direction: AggregateDirection::Desc,
            }],
            offset: 20,
            limit: 20,
        };
        let command = build_aggregate_command(&input).unwrap();
        let args: Vec<_> = command
            .args_iter()
            .map(|arg| match arg {
                ::redis::Arg::Simple(bytes) => String::from_utf8_lossy(bytes).to_string(),
                _ => panic!("unexpected cursor"),
            })
            .collect();
        assert_eq!(
            args,
            [
                "FT.AGGREGATE",
                "idx",
                "* LIMIT 0 999",
                "LOAD",
                "2",
                "@price",
                "@category",
                "GROUPBY",
                "1",
                "@category",
                "REDUCE",
                "SUM",
                "1",
                "@price",
                "AS",
                "amount",
                "SORTBY",
                "2",
                "@amount",
                "DESC",
                "MAX",
                "41",
                "LIMIT",
                "20",
                "21"
            ]
        );
    }
    #[test]
    fn aggregate_resp2_uses_lookahead_not_unreliable_total() {
        let page = parse_aggregate(
            Value::Array(vec![Value::Int(0), row("a"), row("b"), row("c")]),
            20,
            2,
        )
        .unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.next_offset, Some(22));
        assert_eq!(page.columns, vec!["category", "amount"]);
        assert_eq!(page.rows[0].fields[0].value, "a");
        let end = parse_aggregate(Value::Array(vec![Value::Int(999), row("a")]), 22, 2).unwrap();
        assert_eq!(end.next_offset, None);
    }
    #[test]
    fn aggregate_lookahead_does_not_add_columns_to_the_visible_page() {
        let reply = Value::Array(vec![
            Value::Int(999),
            row("visible"),
            Value::Array(vec![text("next_page_only"), text("hidden")]),
        ]);
        let result = parse_aggregate(reply, 0, 1).unwrap();
        assert_eq!(result.columns, vec!["category", "amount"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.next_offset, Some(1));
    }
    #[test]
    fn aggregate_resp3_supports_nested_attributes_and_exact_large_numbers() {
        let value = Value::Map(vec![
            (text("total_results"), Value::Int(0)),
            (
                text("results"),
                Value::Array(vec![Value::Map(vec![(
                    text("extra_attributes"),
                    Value::Map(vec![
                        (text("n"), Value::Int(i64::MAX)),
                        (
                            text("nested"),
                            Value::Array(vec![Value::Nil, Value::Boolean(true)]),
                        ),
                    ]),
                )])]),
            ),
        ]);
        let page = parse_aggregate(value, 0, 10).unwrap();
        assert_eq!(page.rows[0].fields[0].value, i64::MAX.to_string());
        assert_eq!(
            page.rows[0].fields[1].value,
            serde_json::json!([null, true])
        );
    }
    #[test]
    fn aggregate_rejects_duplicate_fields_malformed_and_oversized_results() {
        for value in [
            Value::Array(vec![]),
            Value::Array(vec![text("bad")]),
            Value::Array(vec![Value::Int(1), Value::Array(vec![text("n")])]),
            Value::Array(vec![
                Value::Int(1),
                Value::Array(vec![text("n"), text("1"), text("n"), text("2")]),
            ]),
            Value::Array(vec![
                Value::Int(1),
                Value::Array(vec![
                    text("n"),
                    Value::BulkString(vec![b'x'; MAX_AGGREGATE_CELL_BYTES + 1]),
                ]),
            ]),
            Value::Array(vec![Value::Int(4), row("a"), row("b"), row("c")]),
        ] {
            assert_eq!(parse_aggregate(value, 0, 1), Err(AppError::CommandFailed));
        }
    }
    #[test]
    fn aggregate_rejects_depth_column_and_warning_overflows() {
        let mut nested = Value::Nil;
        for _ in 0..20 {
            nested = Value::Array(vec![nested]);
        }
        assert_eq!(
            parse_aggregate(
                Value::Array(vec![Value::Int(1), Value::Array(vec![text("n"), nested])]),
                0,
                1
            ),
            Err(AppError::CommandFailed)
        );
        let pairs = (0..33)
            .flat_map(|i| [text(&format!("c{i}")), Value::Nil])
            .collect();
        assert_eq!(
            parse_aggregate(Value::Array(vec![Value::Int(1), Value::Array(pairs)]), 0, 1),
            Err(AppError::CommandFailed)
        );
        assert_eq!(
            parse_aggregate(
                Value::Map(vec![
                    (text("results"), Value::Array(vec![])),
                    (text("warning"), Value::Array(vec![text("Timeout secret")]))
                ]),
                0,
                1
            ),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn aggregate_rejects_binary_nonfinite_and_total_byte_overflow() {
        for cell in [Value::BulkString(vec![255]), Value::Double(f64::INFINITY)] {
            assert_eq!(
                parse_aggregate(
                    Value::Array(vec![Value::Int(1), Value::Array(vec![text("cell"), cell])]),
                    0,
                    1
                ),
                Err(AppError::CommandFailed)
            );
        }
        let mut values = vec![Value::Int(100)];
        for _ in 0..100 {
            values.push(Value::Array(vec![text("large"), text(&"a".repeat(30_000))]));
        }
        assert_eq!(
            parse_aggregate(Value::Array(values), 0, 100),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn aggregate_handles_empty_expired_and_attribute_wrapped_rows() {
        let wrapped = Value::Attribute {
            data: Box::new(Value::Array(vec![Value::Int(500), Value::Nil, row("last")])),
            attributes: vec![],
        };
        let result = parse_aggregate(wrapped, MAX_AGGREGATE_OFFSET, 1).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert!(result.rows[0].fields.is_empty());
        assert_eq!(result.next_offset, None);
        assert!(parse_aggregate(Value::Array(vec![Value::Int(500)]), 50, 10)
            .unwrap()
            .rows
            .is_empty());
    }

    #[tokio::test]
    #[ignore = "需要独立 Redis Stack：设置 REDIX_TEST_REDIS_STACK_URL 后显式运行"]
    async fn aggregate_redis_stack_resp2_resp3_pagination() {
        let Ok(url) = std::env::var("REDIX_TEST_REDIS_STACK_URL") else {
            eprintln!("skipped: REDIX_TEST_REDIS_STACK_URL is not set");
            return;
        };
        let mut connection = ::redis::Client::open(url)
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        let prefix = format!("redix:aggregate:{}:", uuid::Uuid::new_v4());
        let index = format!("{prefix}idx");
        let keys: Vec<_> = (0..120)
            .map(|number| format!("{prefix}{number:03}"))
            .collect();
        let flow = async {
            ::redis::cmd("FT.CREATE").arg(&index).arg("ON").arg("HASH").arg("PREFIX").arg(1).arg(&prefix).arg("SCHEMA").arg("category").arg("TAG").arg("price").arg("NUMERIC").query_async::<()>(&mut connection).await?;
            let mut pipeline = ::redis::pipe();
            for (number, key) in keys.iter().enumerate() { pipeline.cmd("HSET").arg(key).arg("category").arg(format!("c{number:03}")).arg("price").arg(2).ignore(); }
            pipeline.query_async::<()>(&mut connection).await?;
            let mut reducers = vec![AggregateReducer { function: AggregateFunction::Count, field: None, alias: "count".into() }];
            for (function, alias) in [(AggregateFunction::Sum, "sum"), (AggregateFunction::Min, "min"), (AggregateFunction::Max, "max"), (AggregateFunction::Avg, "avg")] { reducers.push(AggregateReducer { function, field: Some("price".into()), alias: alias.into() }); }
            let mut input = SearchAggregateInput { connection_id: "integration".into(), index: index.clone(), query: "*".into(), load_fields: vec![], group_by: vec!["category".into()], reducers, sort_by: vec![AggregateSort { field: "category".into(), direction: AggregateDirection::Asc }], offset: 0, limit: 50 };
            // Wait for indexing on the explicitly selected temporary instance only.
            let mut ready = false;
            for _ in 0..100 {
                let reply: Value = ::redis::cmd("FT.SEARCH").arg(&index).arg("*").arg("NOCONTENT").arg("LIMIT").arg(0).arg(0).query_async(&mut connection).await?;
                if matches!(reply, Value::Array(ref values) if values.first() == Some(&Value::Int(120))) { ready = true; break; }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert!(ready, "临时文档未在等待上限内完成索引");
            for protocol in [2, 3] {
                ::redis::cmd("HELLO").arg(protocol).query_async::<Value>(&mut connection).await?;
                input.offset = 0;
                let mut observed = Vec::new();
                loop {
                    let reply = build_aggregate_command(&input).unwrap().query_async(&mut connection).await?;
                    let page = parse_aggregate(reply, input.offset, input.limit).unwrap();
                    for row in page.rows {
                        let category = row.fields.iter().find(|field| field.name == "category").unwrap().value.as_str().unwrap().to_owned();
                        observed.push(category);
                        for field in row.fields.iter().filter(|field| field.name != "category") {
                            let expected = if field.name == "count" { 1.0 } else { 2.0 };
                            let actual = field.value.as_f64().or_else(|| field.value.as_str().and_then(|value| value.parse().ok())).unwrap();
                            assert_eq!(actual, expected);
                        }
                    }
                    match page.next_offset { Some(offset) => input.offset = offset, None => break }
                }
                assert_eq!(observed, (0..120).map(|number| format!("c{number:03}")).collect::<Vec<_>>());
            }
            Ok::<(), ::redis::RedisError>(())
        }.await;
        let _ = ::redis::cmd("FT.DROPINDEX")
            .arg(&index)
            .query_async::<Value>(&mut connection)
            .await;
        let _ = ::redis::cmd("DEL")
            .arg(&keys)
            .query_async::<Value>(&mut connection)
            .await;
        flow.unwrap();
    }
}
