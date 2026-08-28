use ::redis::{Cmd, Value};

use crate::{
    domain::{
        CreateSearchIndexInput, SearchFieldType, SearchIndexAttribute, SearchIndexInfo,
        SearchIndexSummary, SearchKeyResult, SearchKeyType, SearchQueryResult,
        MAX_SEARCH_ATTRIBUTES, MAX_SEARCH_INDEXES, MAX_SEARCH_NAME_BYTES, MAX_SEARCH_PAGE,
        MAX_SEARCH_PREFIXES, MAX_SEARCH_RESPONSE_BYTES,
    },
    error::AppError,
};

pub(crate) fn parse_search_index_list(value: Value) -> Result<Vec<SearchIndexSummary>, AppError> {
    ensure_response_size(&value)?;
    let values = match unwrap_attribute(value) {
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    let mut indexes = Vec::with_capacity(values.len().min(MAX_SEARCH_INDEXES));
    for value in values {
        let Some(name) = value_to_string(&value, MAX_SEARCH_NAME_BYTES) else {
            return Err(AppError::CommandFailed);
        };
        if name.is_empty() {
            continue;
        }
        if indexes.len() == MAX_SEARCH_INDEXES {
            break;
        }
        indexes.push(SearchIndexSummary { name });
    }
    Ok(indexes)
}

pub(crate) fn parse_search_index_info(value: Value) -> Result<SearchIndexInfo, AppError> {
    ensure_response_size(&value)?;
    let pairs = object_pairs(value)?;
    let mut index_name = None;
    let mut key_type = None;
    let mut prefixes = Vec::new();
    let mut attributes = Vec::new();
    let mut num_docs = None;
    let mut num_terms = None;
    let mut num_records = None;
    let mut total_index_memory_bytes = None;

    for (key, value) in pairs {
        let Some(key) = value_to_string(&key, MAX_SEARCH_NAME_BYTES) else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "index_name" => {
                index_name = Some(required_string(value, MAX_SEARCH_NAME_BYTES)?);
            }
            "index_definition" => {
                let (definition_key_type, definition_prefixes) = parse_index_definition(value)?;
                if definition_key_type.is_some() {
                    key_type = definition_key_type;
                }
                prefixes = definition_prefixes;
            }
            "key_type" => {
                key_type = Some(required_string(value, MAX_SEARCH_NAME_BYTES)?);
            }
            "prefixes" => prefixes = parse_prefixes(value)?,
            "attributes" => attributes = parse_attributes(value)?,
            "num_docs" => num_docs = parse_optional_u64(&value),
            "num_terms" => num_terms = parse_optional_u64(&value),
            "num_records" => num_records = parse_optional_u64(&value),
            "total_index_memory_bytes" => total_index_memory_bytes = parse_optional_u64(&value),
            _ => {}
        }
    }

    let Some(index_name) = index_name else {
        return Err(AppError::CommandFailed);
    };
    let Some(key_type) = key_type else {
        return Err(AppError::CommandFailed);
    };

    Ok(SearchIndexInfo {
        index_name,
        key_type,
        prefixes,
        attributes,
        num_docs,
        num_terms,
        num_records,
        total_index_memory_bytes,
    })
}

pub(crate) fn parse_search_query(
    value: Value,
    offset: u64,
    limit: u32,
) -> Result<SearchQueryResult, AppError> {
    ensure_response_size(&value)?;
    let values = match unwrap_attribute(value) {
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    let Some(total) = values.first().and_then(parse_optional_u64) else {
        return Err(AppError::CommandFailed);
    };
    let safe_limit = usize::try_from(limit)
        .unwrap_or(MAX_SEARCH_PAGE as usize)
        .min(MAX_SEARCH_PAGE as usize);
    if values.len().saturating_sub(1) > safe_limit {
        return Err(AppError::CommandFailed);
    }

    let mut keys = Vec::with_capacity(values.len().saturating_sub(1));
    for value in values.into_iter().skip(1) {
        let Some(key) = value_to_string(&value, MAX_SEARCH_NAME_BYTES.max(512)) else {
            return Err(AppError::CommandFailed);
        };
        keys.push(SearchKeyResult {
            key,
            key_type: String::new(),
        });
    }
    if keys.len() > 200 {
        return Err(AppError::CommandFailed);
    }

    let next_offset = if offset.saturating_add(keys.len() as u64) < total {
        Some(offset.saturating_add(keys.len() as u64))
    } else {
        None
    };
    Ok(SearchQueryResult {
        total,
        offset,
        next_offset,
        max_results: None,
        keys,
    })
}

pub(crate) fn parse_max_search_results(value: Value) -> Option<u64> {
    let value = unwrap_attribute(value);
    if let Value::Array(values) = &value {
        if values.len() == 1 && matches!(values[0], Value::Array(_) | Value::Map(_)) {
            return parse_max_search_results(values[0].clone());
        }
    }
    let pairs = object_pairs(value).ok()?;
    for (key, value) in pairs {
        let key = value_to_string(&key, MAX_SEARCH_NAME_BYTES)?;
        if key.eq_ignore_ascii_case("maxsearchresults") {
            return parse_optional_u64(&value).filter(|value| *value > 0);
        }
    }
    None
}

pub(crate) fn build_create_search_index_command(
    input: &CreateSearchIndexInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("FT.CREATE");
    command.arg(&input.index).arg("ON");
    command.arg(match input.key_type {
        SearchKeyType::Hash => "HASH",
        SearchKeyType::Json => "JSON",
    });
    if !input.prefixes.is_empty() {
        command.arg("PREFIX").arg(input.prefixes.len());
        for prefix in &input.prefixes {
            command.arg(prefix);
        }
    }
    command.arg("SCHEMA");
    for field in &input.fields {
        command.arg(&field.name).arg(match field.field_type {
            SearchFieldType::Text => "TEXT",
            SearchFieldType::Tag => "TAG",
            SearchFieldType::Numeric => "NUMERIC",
            SearchFieldType::Geo => "GEO",
            SearchFieldType::Geoshape => "GEOSHAPE",
            SearchFieldType::Vector => "VECTOR",
        });
    }
    Ok(command)
}

pub(crate) fn redis_value_size(value: &Value) -> usize {
    match value {
        Value::Nil | Value::Okay => 0,
        Value::Int(_) | Value::Double(_) | Value::Boolean(_) => 8,
        Value::BulkString(bytes) => bytes.len(),
        Value::SimpleString(text) => text.len(),
        Value::Array(values) | Value::Set(values) => values
            .iter()
            .map(redis_value_size)
            .sum::<usize>()
            .saturating_add(values.len() * 8),
        Value::Map(entries) => entries
            .iter()
            .map(|(key, value)| redis_value_size(key).saturating_add(redis_value_size(value)))
            .sum::<usize>()
            .saturating_add(entries.len() * 16),
        Value::Attribute { data, attributes } => redis_value_size(data).saturating_add(
            attributes
                .iter()
                .map(|(key, value)| redis_value_size(key).saturating_add(redis_value_size(value)))
                .sum::<usize>(),
        ),
        Value::VerbatimString { text, .. } => text.len(),
        Value::BigNumber(number) => number.to_string().len(),
        Value::Push { data, .. } => data.iter().map(redis_value_size).sum(),
        Value::ServerError(_) => 0,
        _ => 0,
    }
}

fn ensure_response_size(value: &Value) -> Result<(), AppError> {
    (redis_value_size(value) <= MAX_SEARCH_RESPONSE_BYTES)
        .then_some(())
        .ok_or(AppError::CommandFailed)
}

fn unwrap_attribute(mut value: Value) -> Value {
    while let Value::Attribute { data, .. } = value {
        value = *data;
    }
    value
}

fn object_pairs(value: Value) -> Result<Vec<(Value, Value)>, AppError> {
    match unwrap_attribute(value) {
        Value::Map(entries) => Ok(entries),
        Value::Array(values) if values.len() % 2 == 0 => Ok(values
            .chunks_exact(2)
            .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
            .collect()),
        _ => Err(AppError::CommandFailed),
    }
}

fn required_string(value: Value, max_bytes: usize) -> Result<String, AppError> {
    value_to_string(&value, max_bytes)
        .filter(|value| !value.is_empty())
        .ok_or(AppError::CommandFailed)
}

fn value_to_string(value: &Value, max_bytes: usize) -> Option<String> {
    let value = match value {
        Value::Attribute { data, .. } => return value_to_string(data, max_bytes),
        Value::BulkString(bytes) => String::from_utf8(bytes.clone()).ok()?,
        Value::SimpleString(text) | Value::VerbatimString { text, .. } => text.clone(),
        Value::Okay => "OK".into(),
        Value::Int(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::BigNumber(number) => number.to_string(),
        _ => return None,
    };
    (value.len() <= max_bytes).then_some(value)
}

fn parse_optional_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int(number) => u64::try_from(*number).ok(),
        _ => value_to_string(value, MAX_SEARCH_NAME_BYTES)?
            .parse::<u64>()
            .ok(),
    }
}

fn parse_prefixes(value: Value) -> Result<Vec<String>, AppError> {
    let values = match unwrap_attribute(value) {
        Value::Nil => return Ok(Vec::new()),
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    if values.len() > MAX_SEARCH_PREFIXES {
        return Err(AppError::CommandFailed);
    }
    values
        .into_iter()
        .map(|value| value_to_string(&value, MAX_SEARCH_NAME_BYTES).ok_or(AppError::CommandFailed))
        .collect()
}

fn parse_index_definition(value: Value) -> Result<(Option<String>, Vec<String>), AppError> {
    let mut key_type = None;
    let mut prefixes = Vec::new();
    for (key, value) in object_pairs(value)? {
        let Some(key) = value_to_string(&key, MAX_SEARCH_NAME_BYTES) else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "key_type" => key_type = Some(required_string(value, MAX_SEARCH_NAME_BYTES)?),
            "prefixes" => prefixes = parse_prefixes(value)?,
            _ => {}
        }
    }
    Ok((key_type, prefixes))
}

fn parse_attributes(value: Value) -> Result<Vec<SearchIndexAttribute>, AppError> {
    let values = match unwrap_attribute(value) {
        Value::Nil => return Ok(Vec::new()),
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    if values.len() > MAX_SEARCH_ATTRIBUTES {
        return Err(AppError::CommandFailed);
    }
    values.into_iter().map(parse_attribute).collect()
}

fn parse_attribute(value: Value) -> Result<SearchIndexAttribute, AppError> {
    let pairs = attribute_pairs(value)?;
    let mut identifier = None;
    let mut field_type = None;
    let mut sortable = false;
    let mut no_index = false;
    let mut attribute_name = None;
    for (key, value) in pairs {
        let Some(key) = value_to_string(&key, MAX_SEARCH_NAME_BYTES) else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "identifier" => identifier = Some(required_string(value, MAX_SEARCH_NAME_BYTES)?),
            "attribute" => attribute_name = value_to_string(&value, MAX_SEARCH_NAME_BYTES),
            "type" | "field_type" => {
                field_type = Some(required_string(value, MAX_SEARCH_NAME_BYTES)?)
            }
            "sortable" => sortable = parse_bool(&value),
            "no_index" | "noindex" => no_index = parse_bool(&value),
            _ => {}
        }
    }
    let identifier = identifier
        .or_else(|| attribute_name.clone())
        .ok_or(AppError::CommandFailed)?;
    let field_type = field_type
        .or(attribute_name)
        .ok_or(AppError::CommandFailed)?;
    Ok(SearchIndexAttribute {
        identifier,
        field_type,
        sortable,
        no_index,
    })
}

fn attribute_pairs(value: Value) -> Result<Vec<(Value, Value)>, AppError> {
    match unwrap_attribute(value) {
        Value::Map(entries) => Ok(entries),
        Value::Array(values) => {
            let mut pairs = Vec::new();
            let mut index = 0;
            while index < values.len() {
                let key = values[index].clone();
                let Some(key_name) = value_to_string(&key, MAX_SEARCH_NAME_BYTES) else {
                    return Err(AppError::CommandFailed);
                };
                if matches!(
                    key_name.as_str(),
                    "SORTABLE"
                        | "NOINDEX"
                        | "CASESENSITIVE"
                        | "UNF"
                        | "NOSTEM"
                        | "WITHSUFFIXTRIE"
                        | "INDEXEMPTY"
                        | "INDEXMISSING"
                ) {
                    pairs.push((key, Value::Boolean(true)));
                    index += 1;
                    continue;
                }
                let Some(value) = values.get(index + 1) else {
                    return Err(AppError::CommandFailed);
                };
                pairs.push((key, value.clone()));
                index += 2;
            }
            Ok(pairs)
        }
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_bool(value: &Value) -> bool {
    match value {
        Value::Boolean(value) => *value,
        Value::Int(value) => *value != 0,
        _ => value_to_string(value, MAX_SEARCH_NAME_BYTES).is_some_and(|value| {
            matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
        }),
    }
}

#[cfg(test)]
mod tests {
    use redis::{Arg, Value};

    use crate::domain::{
        CreateSearchIndexInput, SearchFieldType, SearchIndexFieldInput, SearchKeyType,
    };
    use crate::error::AppError;

    use super::{
        build_create_search_index_command, parse_max_search_results, parse_search_index_info,
        parse_search_index_list, parse_search_query,
    };

    fn text(value: &str) -> Value {
        Value::BulkString(value.as_bytes().to_vec())
    }

    fn info_flat() -> Value {
        Value::Array(vec![
            text("index_name"),
            text("idx:users"),
            text("key_type"),
            text("HASH"),
            text("prefixes"),
            Value::Array(vec![text("user:")]),
            text("attributes"),
            Value::Array(vec![Value::Array(vec![
                text("identifier"),
                text("title"),
                text("attribute"),
                text("TEXT"),
                text("sortable"),
                Value::Int(1),
                text("no_index"),
                Value::Int(0),
            ])]),
            text("num_docs"),
            Value::Int(2),
            text("num_terms"),
            Value::Int(4),
            text("num_records"),
            Value::Int(5),
            text("total_index_memory_bytes"),
            Value::Int(128),
            text("unknown_optional"),
            text("ignore me"),
        ])
    }

    #[test]
    fn parse_search_index_list_accepts_resp2_resp3_and_attribute() {
        let resp2 = Value::Array(vec![text("idx_a"), text("idx_b")]);
        let resp3 = Value::Set(vec![text("idx_a"), text("idx_b")]);
        let attribute = Value::Attribute {
            data: Box::new(Value::Array(vec![text("idx_a"), text("idx_b")])),
            attributes: vec![],
        };

        for value in [resp2, resp3, attribute] {
            assert_eq!(
                parse_search_index_list(value).unwrap(),
                vec![
                    crate::domain::SearchIndexSummary {
                        name: "idx_a".into()
                    },
                    crate::domain::SearchIndexSummary {
                        name: "idx_b".into()
                    },
                ]
            );
        }

        let many = Value::Array(
            (0..501)
                .map(|index| text(&format!("idx:{index}")))
                .collect(),
        );
        assert_eq!(parse_search_index_list(many).unwrap().len(), 500);
    }

    #[test]
    fn parse_search_index_info_accepts_flat_map_and_attribute() {
        let flat = parse_search_index_info(info_flat()).unwrap();
        assert_eq!(flat.index_name, "idx:users");
        assert_eq!(flat.key_type, "HASH");
        assert_eq!(flat.prefixes, vec!["user:"]);
        assert_eq!(flat.attributes[0].identifier, "title");
        assert_eq!(flat.attributes[0].field_type, "TEXT");
        assert!(flat.attributes[0].sortable);
        assert!(!flat.attributes[0].no_index);
        assert_eq!(flat.num_docs, Some(2));
        assert_eq!(flat.num_terms, Some(4));
        assert_eq!(flat.num_records, Some(5));
        assert_eq!(flat.total_index_memory_bytes, Some(128));

        let nested = Value::Array(vec![
            text("index_name"),
            text("idx:nested"),
            text("index_definition"),
            Value::Array(vec![
                text("key_type"),
                text("JSON"),
                text("prefixes"),
                Value::Array(vec![text("doc:")]),
                text("default_score"),
                text("1"),
            ]),
            text("attributes"),
            info_flat_attributes(),
            text("num_docs"),
            Value::Int(4),
        ]);
        let nested_info = parse_search_index_info(nested).unwrap();
        assert_eq!(nested_info.index_name, "idx:nested");
        assert_eq!(nested_info.key_type, "JSON");
        assert_eq!(nested_info.prefixes, vec!["doc:"]);
        assert_eq!(nested_info.num_docs, Some(4));

        let map = Value::Map(vec![
            (text("index_name"), text("idx:users")),
            (text("key_type"), text("JSON")),
            (text("prefixes"), Value::Array(vec![text("doc:")])),
            (text("attributes"), info_flat_attributes()),
            (text("num_docs"), Value::Int(3)),
        ]);
        let mapped = parse_search_index_info(Value::Attribute {
            data: Box::new(map),
            attributes: vec![],
        })
        .unwrap();
        assert_eq!(mapped.index_name, "idx:users");
        assert_eq!(mapped.key_type, "JSON");
        assert_eq!(mapped.prefixes, vec!["doc:"]);
        assert_eq!(mapped.attributes.len(), 1);
        assert_eq!(mapped.num_docs, Some(3));

        let flagged = parse_search_index_info(Value::Array(vec![
            text("index_name"),
            text("idx:flags"),
            text("key_type"),
            text("HASH"),
            text("attributes"),
            Value::Array(vec![Value::Array(vec![
                text("identifier"),
                text("title"),
                text("attribute"),
                text("title"),
                text("type"),
                text("TEXT"),
                text("SORTABLE"),
                text("NOINDEX"),
            ])]),
        ]))
        .unwrap();
        assert!(flagged.attributes[0].sortable);
        assert!(flagged.attributes[0].no_index);
    }

    fn info_flat_attributes() -> Value {
        Value::Array(vec![Value::Map(vec![
            (text("identifier"), text("title")),
            (text("attribute"), text("TEXT")),
            (text("sortable"), Value::Boolean(false)),
            (text("no_index"), Value::Boolean(true)),
        ])])
    }

    #[test]
    fn parse_search_index_info_rejects_missing_required_shape_and_caps_attributes() {
        assert_eq!(
            parse_search_index_info(Value::Array(vec![text("key_type"), text("HASH")])),
            Err(AppError::CommandFailed)
        );
        assert_eq!(
            parse_search_index_info(Value::Array(vec![
                text("index_name"),
                text("idx"),
                text("key_type"),
                text("HASH"),
                text("attributes"),
                Value::Array(vec![text("odd")]),
            ])),
            Err(AppError::CommandFailed)
        );

        let too_many = Value::Array(vec![
            text("index_name"),
            text("idx"),
            text("key_type"),
            text("HASH"),
            text("attributes"),
            Value::Array(
                (0..257)
                    .map(|index| {
                        Value::Map(vec![
                            (text("identifier"), text(&format!("field:{index}"))),
                            (text("attribute"), text("TEXT")),
                        ])
                    })
                    .collect(),
            ),
        ]);
        assert_eq!(
            parse_search_index_info(too_many),
            Err(AppError::CommandFailed)
        );

        let oversized = Value::Array(vec![
            text("index_name"),
            text(&"x".repeat(257)),
            text("key_type"),
            text("HASH"),
        ]);
        assert_eq!(
            parse_search_index_info(oversized),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn parse_search_query_accepts_total_and_keys_only() {
        let result = parse_search_query(
            Value::Array(vec![Value::Int(3), text("doc:1"), text("doc:2")]),
            0,
            2,
        )
        .unwrap();
        assert_eq!(result.total, 3);
        assert_eq!(result.offset, 0);
        assert_eq!(result.next_offset, Some(2));
        assert_eq!(
            result
                .keys
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            ["doc:1", "doc:2"]
        );

        let empty = parse_search_query(Value::Array(vec![Value::Int(0)]), 0, 200).unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.keys.is_empty());
        assert_eq!(empty.next_offset, None);

        let too_many = Value::Array(
            std::iter::once(Value::Int(201))
                .chain((0..201).map(|index| text(&format!("doc:{index}"))))
                .collect(),
        );
        assert_eq!(
            parse_search_query(too_many, 0, 200),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn parse_max_search_results_handles_resp2_minus_one_and_map() {
        assert_eq!(
            parse_max_search_results(Value::Array(vec![text("MAXSEARCHRESULTS"), text("100"),])),
            Some(100)
        );
        assert_eq!(
            parse_max_search_results(Value::Array(vec![text("MAXSEARCHRESULTS"), text("-1"),])),
            None
        );
        assert_eq!(
            parse_max_search_results(Value::Map(vec![(
                text("MAXSEARCHRESULTS"),
                Value::Int(100),
            )])),
            Some(100)
        );
        assert_eq!(
            parse_max_search_results(Value::Array(vec![Value::Array(vec![
                text("MAXSEARCHRESULTS"),
                text("250"),
            ])])),
            Some(250)
        );
        assert_eq!(
            parse_max_search_results(Value::Array(vec![text("MAXSEARCHRESULTS"), text("bad")])),
            None
        );
    }

    #[test]
    fn create_search_index_command_keeps_each_input_as_an_argument() {
        let input = CreateSearchIndexInput {
            connection_id: "local".into(),
            index: "idx users".into(),
            key_type: SearchKeyType::Json,
            prefixes: vec!["doc:".into(), "profile:".into()],
            fields: vec![
                SearchIndexFieldInput {
                    name: "title value".into(),
                    field_type: SearchFieldType::Text,
                },
                SearchIndexFieldInput {
                    name: "labels".into(),
                    field_type: SearchFieldType::Tag,
                },
                SearchIndexFieldInput {
                    name: "score".into(),
                    field_type: SearchFieldType::Numeric,
                },
                SearchIndexFieldInput {
                    name: "location".into(),
                    field_type: SearchFieldType::Geo,
                },
                SearchIndexFieldInput {
                    name: "shape".into(),
                    field_type: SearchFieldType::Geoshape,
                },
                SearchIndexFieldInput {
                    name: "embedding".into(),
                    field_type: SearchFieldType::Vector,
                },
            ],
        };
        let command = build_create_search_index_command(&input).unwrap();
        let args = command
            .args_iter()
            .map(|arg: Arg<&[u8]>| match arg {
                Arg::Simple(value) => String::from_utf8(value.to_vec()).unwrap(),
                Arg::Cursor => "<cursor>".into(),
                _ => "<unknown>".into(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "FT.CREATE",
                "idx users",
                "ON",
                "JSON",
                "PREFIX",
                "2",
                "doc:",
                "profile:",
                "SCHEMA",
                "title value",
                "TEXT",
                "labels",
                "TAG",
                "score",
                "NUMERIC",
                "location",
                "GEO",
                "shape",
                "GEOSHAPE",
                "embedding",
                "VECTOR",
            ]
        );
        assert!(!String::from_utf8(command.get_packed_command())
            .unwrap()
            .contains("DD"));
    }
}
