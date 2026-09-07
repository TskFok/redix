use ::redis::{Cmd, Value};
use std::collections::HashSet;

use crate::{
    domain::{
        CreateSearchIndexInput, SearchDocumentField, SearchFieldType, SearchIndexAttribute,
        SearchIndexInfo, SearchIndexSummary, SearchKeyResult, SearchKeyType, SearchQueryResult,
        MAX_SEARCH_ATTRIBUTES, MAX_SEARCH_FIELDS, MAX_SEARCH_FIELD_BYTES, MAX_SEARCH_INDEXES,
        MAX_SEARCH_KEY_BYTES, MAX_SEARCH_NAME_BYTES, MAX_SEARCH_OFFSET, MAX_SEARCH_PAGE,
        MAX_SEARCH_PREFIXES, MAX_SEARCH_RESPONSE_BYTES,
    },
    error::AppError,
};

impl super::RedisService {
    pub async fn search_vector_index(
        &self,
        input: crate::domain::SearchVectorQueryInput,
    ) -> Result<crate::domain::SearchVectorQueryResult, AppError> {
        input.validate()?;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut connection = self.search_vector_connection(&input.connection_id).await?;
            let info = ::redis::cmd("FT.INFO")
                .arg(&input.index)
                .query_async(&mut connection)
                .await
                .map_err(super::connection_manager::map_command_error)?;
            let info = parse_search_index_info(info)?;
            let attribute = info
                .attributes
                .iter()
                .find(|attribute| {
                    attribute
                        .query_name
                        .as_ref()
                        .unwrap_or(&attribute.identifier)
                        == &input.field
                        && attribute.field_type.eq_ignore_ascii_case("VECTOR")
                        && !attribute.no_index
                })
                .ok_or(AppError::InvalidInput)?;
            let schema = attribute
                .vector
                .as_ref()
                .ok_or(AppError::UnsupportedFeature)?;
            // Avoid overwriting a real document field with the generated distance alias.
            let mut score = "__redix_distance".to_string();
            while info.attributes.iter().any(|attribute| {
                attribute
                    .query_name
                    .as_ref()
                    .unwrap_or(&attribute.identifier)
                    == &score
            }) {
                score.push('_');
            }
            let value = build_vector_search_command(&input, schema, &score)?
                .query_async(&mut connection)
                .await
                .map_err(super::connection_manager::map_command_error)?;
            parse_vector_search_result(value, input.count, &score, &schema.distance_metric)
        })
        .await
        .map_err(|_| AppError::CommandFailed)?
    }
}

fn build_vector_search_command(
    input: &crate::domain::SearchVectorQueryInput,
    schema: &crate::domain::SearchVectorFieldInfo,
    score: &str,
) -> Result<Cmd, AppError> {
    if !crate::domain::safe_vector_field(score) {
        return Err(AppError::InvalidInput);
    }
    let bytes = input.encode_vector(schema)?;
    let mut command = ::redis::cmd("FT.SEARCH");
    command
        .arg(&input.index)
        .arg(format!(
            "({})=>[KNN {} @{} $__redix_vector AS {}]",
            input.filter.trim(),
            input.count,
            input.field,
            score
        ))
        .arg("PARAMS")
        .arg(2)
        .arg("__redix_vector")
        .arg(bytes)
        .arg("SORTBY")
        .arg(score)
        .arg("ASC")
        .arg("RETURN")
        .arg(1)
        .arg(score)
        .arg("LIMIT")
        .arg(0)
        .arg(input.count)
        .arg("TIMEOUT")
        .arg(4000)
        .arg("DIALECT")
        .arg(2);
    Ok(command)
}

fn parse_vector_search_result(
    value: Value,
    count: u32,
    score: &str,
    metric: &str,
) -> Result<crate::domain::SearchVectorQueryResult, AppError> {
    ensure_response_size(&value)?;
    let value = unwrap_attribute(value);
    if let Value::Map(pairs) = &value {
        for (key, value) in pairs {
            if value_to_string(key, MAX_SEARCH_NAME_BYTES)
                .is_some_and(|name| matches!(name.as_str(), "warning" | "warnings" | "error"))
            {
                match value {
                    Value::Nil => (),
                    Value::Array(values) if values.is_empty() => (),
                    _ => return Err(AppError::CommandFailed),
                }
            }
        }
    }
    let result = parse_search_query(value, 0, count, true)?;
    if result.total > u64::from(count) {
        return Err(AppError::CommandFailed);
    }
    let mut matches: Vec<crate::domain::SearchVectorMatch> = Vec::with_capacity(result.keys.len());
    for key in result.keys {
        let fields = key.fields.ok_or(AppError::CommandFailed)?;
        if fields.len() != 1 || fields[0].name != score {
            return Err(AppError::CommandFailed);
        }
        let value = &fields[0].value;
        let distance = value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
            .filter(|value| value.is_finite())
            .ok_or(AppError::CommandFailed)?;
        if matches
            .last()
            .is_some_and(|previous| previous.distance > distance)
        {
            return Err(AppError::CommandFailed);
        }
        matches.push(crate::domain::SearchVectorMatch {
            key: key.key,
            distance,
        });
    }
    Ok(crate::domain::SearchVectorQueryResult {
        returned: matches.len(),
        matches,
        count,
        distance_metric: metric.into(),
    })
}

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
    include_content: bool,
) -> Result<SearchQueryResult, AppError> {
    ensure_response_size(&value)?;
    if offset > MAX_SEARCH_OFFSET || !(1..=MAX_SEARCH_PAGE).contains(&limit) {
        return Err(AppError::CommandFailed);
    }
    let (total, keys) = match unwrap_attribute(value) {
        Value::Array(values) => parse_search_resp2(values, limit as usize, include_content)?,
        Value::Map(pairs) => parse_search_resp3(pairs, limit as usize, include_content)?,
        _ => return Err(AppError::CommandFailed),
    };
    let next = offset.saturating_add(keys.len() as u64);
    if total > 9_007_199_254_740_991 || (!keys.is_empty() && next > total) {
        return Err(AppError::CommandFailed);
    }
    let mut seen = HashSet::new();
    if keys.iter().any(|key| !seen.insert(key.key.as_str())) {
        return Err(AppError::CommandFailed);
    }
    let next_offset =
        (!keys.is_empty() && next < total && next <= MAX_SEARCH_OFFSET).then_some(next);
    Ok(SearchQueryResult {
        total,
        offset,
        next_offset,
        max_results: None,
        keys,
    })
}

fn parse_search_resp2(
    values: Vec<Value>,
    limit: usize,
    content: bool,
) -> Result<(u64, Vec<SearchKeyResult>), AppError> {
    let mut values = values.into_iter();
    let total = search_total(values.next().ok_or(AppError::CommandFailed)?)?;
    let width = if content { 2 } else { 1 };
    if values.len() % width != 0 || values.len() / width > limit {
        return Err(AppError::CommandFailed);
    }
    let mut keys = Vec::with_capacity(values.len() / width);
    while let Some(value) = values.next() {
        let key = strict_string(value, MAX_SEARCH_KEY_BYTES)?;
        let fields = if content {
            parse_document_fields(values.next().ok_or(AppError::CommandFailed)?)?
        } else {
            None
        };
        keys.push(SearchKeyResult {
            key,
            key_type: String::new(),
            fields,
        });
    }
    Ok((total, keys))
}

fn parse_search_resp3(
    pairs: Vec<(Value, Value)>,
    limit: usize,
    content: bool,
) -> Result<(u64, Vec<SearchKeyResult>), AppError> {
    let mut total = None;
    let mut results = None;
    for (key, value) in pairs {
        match strict_string(key, MAX_SEARCH_NAME_BYTES)?.as_str() {
            "total_results" if total.is_none() => total = Some(search_total(value)?),
            "results" if results.is_none() => results = Some(value),
            "total_results" | "results" => return Err(AppError::CommandFailed),
            _ => {}
        }
    }
    let total = total.ok_or(AppError::CommandFailed)?;
    let Value::Array(documents) = unwrap_attribute(results.ok_or(AppError::CommandFailed)?) else {
        return Err(AppError::CommandFailed);
    };
    if documents.len() > limit {
        return Err(AppError::CommandFailed);
    }
    let mut keys = Vec::with_capacity(documents.len());
    for document in documents {
        let Value::Map(pairs) = unwrap_attribute(document) else {
            return Err(AppError::CommandFailed);
        };
        let mut key = None;
        let mut attributes = None;
        for (name, value) in pairs {
            match strict_string(name, MAX_SEARCH_NAME_BYTES)?.as_str() {
                "id" if key.is_none() => key = Some(strict_string(value, MAX_SEARCH_KEY_BYTES)?),
                "extra_attributes" if attributes.is_none() => attributes = Some(value),
                "id" | "extra_attributes" => return Err(AppError::CommandFailed),
                _ => {}
            }
        }
        let fields = if content {
            parse_document_fields(attributes.unwrap_or(Value::Nil))?
        } else {
            None
        };
        keys.push(SearchKeyResult {
            key: key.ok_or(AppError::CommandFailed)?,
            key_type: String::new(),
            fields,
        });
    }
    Ok((total, keys))
}

fn search_total(value: Value) -> Result<u64, AppError> {
    match unwrap_attribute(value) {
        Value::Int(total) => u64::try_from(total).map_err(|_| AppError::CommandFailed),
        _ => Err(AppError::CommandFailed),
    }
}

fn strict_string(value: Value, max_bytes: usize) -> Result<String, AppError> {
    match unwrap_attribute(value) {
        Value::BulkString(bytes) if bytes.len() <= max_bytes => {
            String::from_utf8(bytes).map_err(|_| AppError::CommandFailed)
        }
        Value::SimpleString(text) | Value::VerbatimString { text, .. }
            if text.len() <= max_bytes =>
        {
            Ok(text)
        }
        _ => Err(AppError::CommandFailed),
    }
}

fn parse_document_fields(value: Value) -> Result<Option<Vec<SearchDocumentField>>, AppError> {
    let value = unwrap_attribute(value);
    if matches!(value, Value::Nil) {
        return Ok(None);
    }
    let pairs = object_pairs(value)?;
    if pairs.len() > MAX_SEARCH_FIELDS {
        return Err(AppError::CommandFailed);
    }
    let mut seen = HashSet::new();
    let mut fields = Vec::with_capacity(pairs.len());
    for (name, value) in pairs {
        let name = strict_string(name, MAX_SEARCH_NAME_BYTES)?;
        if !seen.insert(name.clone()) || redis_value_size(&value) > MAX_SEARCH_FIELD_BYTES {
            return Err(AppError::CommandFailed);
        }
        fields.push(SearchDocumentField {
            name,
            value: document_json_value(value, 0)?,
        });
    }
    Ok(Some(fields))
}

fn document_json_value(value: Value, depth: usize) -> Result<serde_json::Value, AppError> {
    use serde_json::Value as Json;
    if depth > 16 {
        return Err(AppError::CommandFailed);
    }
    Ok(match unwrap_attribute(value) {
        Value::Nil => Json::Null,
        Value::Boolean(value) => Json::Bool(value),
        Value::Int(value) if value.unsigned_abs() <= 9_007_199_254_740_991 => value.into(),
        Value::Int(value) => Json::String(value.to_string()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(Json::Number)
            .ok_or(AppError::CommandFailed)?,
        value @ (Value::BulkString(_) | Value::SimpleString(_) | Value::VerbatimString { .. }) => {
            Json::String(strict_string(value, MAX_SEARCH_FIELD_BYTES)?)
        }
        Value::Array(values) => {
            if values.len() > 1024 {
                return Err(AppError::CommandFailed);
            }
            Json::Array(
                values
                    .into_iter()
                    .map(|value| document_json_value(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            )
        }
        Value::Map(pairs) => {
            if pairs.len() > MAX_SEARCH_FIELDS {
                return Err(AppError::CommandFailed);
            }
            let mut object = serde_json::Map::new();
            for (name, value) in pairs {
                let name = strict_string(name, MAX_SEARCH_NAME_BYTES)?;
                if object.contains_key(&name) {
                    return Err(AppError::CommandFailed);
                }
                object.insert(name, document_json_value(value, depth + 1)?);
            }
            Json::Object(object)
        }
        _ => return Err(AppError::CommandFailed),
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
        command.arg(&field.name);
        if let Some(alias) = &field.alias {
            command.arg("AS").arg(alias);
        }
        command.arg(match field.field_type {
            SearchFieldType::Text => "TEXT",
            SearchFieldType::Tag => "TAG",
            SearchFieldType::Numeric => "NUMERIC",
            SearchFieldType::Geo => "GEO",
            SearchFieldType::Geoshape => "GEOSHAPE",
            SearchFieldType::Vector => "VECTOR",
        });
        if let Some(vector) = &field.vector {
            let arguments = vector.command_arguments();
            command
                .arg(vector.algorithm_name())
                .arg(arguments.len())
                .arg(arguments);
        }
    }
    Ok(command)
}

pub(crate) fn redis_value_size(value: &Value) -> usize {
    let mut pending = vec![(value, 0_usize)];
    let mut size = 0_usize;
    let mut visited = 0_usize;
    while let Some((value, depth)) = pending.pop() {
        visited += 1;
        if depth > 32 || visited > 100_000 {
            return MAX_SEARCH_RESPONSE_BYTES + 1;
        }
        let bytes = match value {
            Value::BulkString(bytes) => bytes.len(),
            Value::SimpleString(text) | Value::VerbatimString { text, .. } => text.len(),
            Value::BigNumber(number) => number.to_string().len(),
            Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => {
                if values.len() + pending.len() > 100_000 {
                    return MAX_SEARCH_RESPONSE_BYTES + 1;
                }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
                values.len().saturating_mul(8)
            }
            Value::Map(pairs) => {
                if pairs.len().saturating_mul(2) + pending.len() > 100_000 {
                    return MAX_SEARCH_RESPONSE_BYTES + 1;
                }
                for (key, value) in pairs {
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
                pairs.len().saturating_mul(16)
            }
            Value::Attribute { data, attributes } => {
                if attributes.len().saturating_mul(2) + pending.len() > 100_000 {
                    return MAX_SEARCH_RESPONSE_BYTES + 1;
                }
                pending.push((data, depth + 1));
                for (key, value) in attributes {
                    pending.push((key, depth + 1));
                    pending.push((value, depth + 1));
                }
                attributes.len().saturating_mul(16)
            }
            _ => 8,
        };
        size = size.saturating_add(bytes);
        if size > MAX_SEARCH_RESPONSE_BYTES {
            return size;
        }
    }
    size
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
    let mut vector_type = None;
    let mut vector_dimension = None;
    let mut vector_metric = None;
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
            "data_type" => vector_type = value_to_string(&value, MAX_SEARCH_NAME_BYTES),
            "dim" => {
                vector_dimension =
                    parse_optional_u64(&value).and_then(|value| u32::try_from(value).ok())
            }
            "distance_metric" => vector_metric = value_to_string(&value, MAX_SEARCH_NAME_BYTES),
            "sortable" => sortable = parse_bool(&value),
            "no_index" | "noindex" => no_index = parse_bool(&value),
            _ => {}
        }
    }
    // Modern FT.INFO reports the source path as identifier and the query alias as attribute.
    // Older responses in supported deployments may use attribute for the field type instead.
    let query_name = field_type.as_ref().and(attribute_name.clone());
    let identifier = identifier
        .or_else(|| attribute_name.clone())
        .ok_or(AppError::CommandFailed)?;
    let field_type = field_type
        .or(attribute_name)
        .ok_or(AppError::CommandFailed)?;
    let vector = if field_type.eq_ignore_ascii_case("VECTOR") {
        vector_type.zip(vector_dimension).zip(vector_metric).map(
            |((data_type, dimension), distance_metric)| crate::domain::SearchVectorFieldInfo {
                data_type: data_type.to_ascii_uppercase(),
                dimension,
                distance_metric: distance_metric.to_ascii_uppercase(),
            },
        )
    } else {
        None
    };
    Ok(SearchIndexAttribute {
        identifier,
        query_name,
        field_type,
        sortable,
        no_index,
        vector,
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
            false,
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

        let empty = parse_search_query(Value::Array(vec![Value::Int(0)]), 0, 200, false).unwrap();
        assert_eq!(empty.total, 0);
        assert!(empty.keys.is_empty());
        assert_eq!(empty.next_offset, None);

        let too_many = Value::Array(
            std::iter::once(Value::Int(201))
                .chain((0..201).map(|index| text(&format!("doc:{index}"))))
                .collect(),
        );
        assert_eq!(
            parse_search_query(too_many, 0, 200, false),
            Err(AppError::CommandFailed)
        );
    }

    #[test]
    fn search_content_resp2_preserves_fields_null_and_expired_documents() {
        let value = Value::Array(vec![
            Value::Int(3),
            text("doc:1"),
            Value::Array(vec![text("name"), text("Alice"), text("empty"), Value::Nil]),
            text("doc:2"),
            Value::Nil,
        ]);
        let result = parse_search_query(value, 0, 2, true).unwrap();
        let encoded = serde_json::to_value(&result).unwrap();
        assert_eq!(
            encoded["keys"][0]["fields"],
            serde_json::json!([
                {"name":"name","value":"Alice"}, {"name":"empty","value":null}
            ])
        );
        assert_eq!(encoded["keys"][1]["fields"], serde_json::Value::Null);
        assert_eq!(result.next_offset, Some(2));
    }

    #[test]
    fn search_content_resp3_preserves_nested_attributes_and_nocontent_ids() {
        let value = Value::Map(vec![
            (text("total_results"), Value::Int(1)),
            (
                text("results"),
                Value::Array(vec![Value::Map(vec![
                    (text("id"), text("doc:1")),
                    (
                        text("extra_attributes"),
                        Value::Map(vec![(
                            text("tags"),
                            Value::Array(vec![text("redis"), Value::Boolean(true)]),
                        )]),
                    ),
                    (text("values"), Value::Array(vec![])),
                ])]),
            ),
        ]);
        let result = parse_search_query(
            Value::Attribute {
                data: Box::new(value),
                attributes: vec![],
            },
            0,
            2,
            true,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap()["keys"][0]["fields"][0]["value"],
            serde_json::json!(["redis", true])
        );

        let ids = Value::Map(vec![
            (text("total_results"), Value::Int(5)),
            (
                text("results"),
                Value::Array(vec![Value::Map(vec![(text("id"), text("doc:2"))])]),
            ),
        ]);
        assert_eq!(
            parse_search_query(ids, 0, 1, false).unwrap().keys[0].key,
            "doc:2"
        );
    }

    #[test]
    fn search_query_empty_page_must_not_repeat_same_offset() {
        let result =
            parse_search_query(Value::Array(vec![Value::Int(100)]), 20, 10, false).unwrap();
        assert_eq!(result.next_offset, None);
    }

    #[test]
    fn search_content_input_is_opt_in_and_roundtrips() {
        let base = serde_json::json!({"connection_id":"local","index":"idx","query":"*","offset":0,"limit":10});
        let default: crate::domain::SearchQueryInput =
            serde_json::from_value(base.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(default).unwrap()["include_content"],
            false
        );
        let mut content = base;
        content["include_content"] = true.into();
        let enabled: crate::domain::SearchQueryInput = serde_json::from_value(content).unwrap();
        assert_eq!(
            serde_json::to_value(enabled).unwrap()["include_content"],
            true
        );
    }

    #[test]
    fn search_content_rejects_malformed_shapes_duplicate_ids_and_noninteger_totals() {
        for (reply, content) in [
            (Value::Array(vec![Value::Double(1.0), text("doc")]), false),
            (
                Value::Array(vec![Value::Int(2), text("same"), text("same")]),
                false,
            ),
            (Value::Array(vec![Value::Int(1), Value::Int(123)]), false),
            (Value::Array(vec![Value::Int(1), text("doc")]), true),
            (
                Value::Array(vec![
                    Value::Int(1),
                    text("doc"),
                    Value::Array(vec![text("orphan")]),
                ]),
                true,
            ),
            (
                Value::Array(vec![
                    Value::Int(1),
                    text("doc"),
                    Value::Array(vec![text("a"), text("1"), text("a"), text("2")]),
                ]),
                true,
            ),
            (
                Value::Map(vec![
                    (text("total_results"), Value::Int(1)),
                    (text("results"), Value::Array(vec![Value::Map(vec![])])),
                ]),
                true,
            ),
            (
                Value::Map(vec![
                    (text("total_results"), Value::Int(1)),
                    (text("total_results"), Value::Int(1)),
                    (text("results"), Value::Array(vec![])),
                ]),
                true,
            ),
        ] {
            assert_eq!(
                parse_search_query(reply, 0, 2, content),
                Err(AppError::CommandFailed)
            );
        }
    }

    #[test]
    fn search_content_bounds_response_fields_bytes_depth_and_page() {
        let document = |fields: Value| Value::Array(vec![Value::Int(1), text("doc"), fields]);
        let fields = Value::Array(
            (0..65)
                .flat_map(|index| [text(&format!("f{index}")), text("value")])
                .collect(),
        );
        assert!(parse_search_query(document(fields), 0, 1, true).is_err());
        assert!(parse_search_query(
            document(Value::Array(vec![
                text("x"),
                text(&"x".repeat(256 * 1024 + 1))
            ])),
            0,
            1,
            true
        )
        .is_err());
        assert!(parse_search_query(
            document(Value::Array(vec![text("x"), Value::BulkString(vec![0xff])])),
            0,
            1,
            true
        )
        .is_err());
        let mut nested = Value::Nil;
        for _ in 0..40 {
            nested = Value::Array(vec![nested]);
        }
        assert!(
            parse_search_query(document(Value::Array(vec![text("x"), nested])), 0, 1, true)
                .is_err()
        );
        let oversized = Value::Array(
            (0..17)
                .flat_map(|index| [text(&format!("f{index}")), text(&"x".repeat(256 * 1024))])
                .collect(),
        );
        assert!(parse_search_query(document(oversized), 0, 1, true).is_err());
        let many = Value::Array(
            std::iter::once(Value::Int(201))
                .chain(
                    (0..201).flat_map(|index| [text(&format!("doc{index}")), Value::Array(vec![])]),
                )
                .collect(),
        );
        assert!(parse_search_query(many, 0, 200, true).is_err());
        let boundary = parse_search_query(
            document(Value::Array(vec![text("x"), text(&"x".repeat(256 * 1024))])),
            0,
            1,
            true,
        )
        .unwrap();
        assert_eq!(
            boundary.keys[0].fields.as_ref().unwrap()[0]
                .value
                .as_str()
                .unwrap()
                .len(),
            256 * 1024
        );
    }

    #[test]
    fn search_content_never_rounds_large_integer_fields() {
        let value = Value::Array(vec![
            Value::Int(1),
            text("doc"),
            Value::Array(vec![text("id"), Value::Int(9_007_199_254_740_993)]),
        ]);
        let result = parse_search_query(value, 0, 1, true).unwrap();
        assert_eq!(
            result.keys[0].fields.as_ref().unwrap()[0].value,
            serde_json::json!("9007199254740993")
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
                    alias: None,
                    name: "title value".into(),
                    field_type: SearchFieldType::Text,
                    vector: None,
                },
                SearchIndexFieldInput {
                    alias: None,
                    name: "labels".into(),
                    field_type: SearchFieldType::Tag,
                    vector: None,
                },
                SearchIndexFieldInput {
                    alias: None,
                    name: "score".into(),
                    field_type: SearchFieldType::Numeric,
                    vector: None,
                },
                SearchIndexFieldInput {
                    alias: None,
                    name: "location".into(),
                    field_type: SearchFieldType::Geo,
                    vector: None,
                },
                SearchIndexFieldInput {
                    alias: None,
                    name: "shape".into(),
                    field_type: SearchFieldType::Geoshape,
                    vector: None,
                },
                SearchIndexFieldInput {
                    alias: None,
                    name: "embedding".into(),
                    field_type: SearchFieldType::Vector,
                    vector: Some(serde_json::from_value(serde_json::json!({"algorithm": "FLAT", "data_type": "FLOAT32", "dimension": 384, "distance_metric": "COSINE"})).unwrap()),
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
                "FLAT",
                "6",
                "TYPE",
                "FLOAT32",
                "DIM",
                "384",
                "DISTANCE_METRIC",
                "COSINE",
            ]
        );
        assert!(!String::from_utf8(command.get_packed_command())
            .unwrap()
            .contains("DD"));
    }

    #[test]
    fn vector_create_emits_complete_hnsw_arguments() {
        let input: CreateSearchIndexInput = serde_json::from_value(serde_json::json!({
            "connection_id": "local", "index": "idx:vector", "key_type": "hash",
            "prefixes": [], "fields": [{"name": "embedding", "field_type": "vector",
                "vector": {"algorithm": "HNSW", "data_type": "FLOAT32", "dimension": 768,
                    "distance_metric": "COSINE", "m": 16, "ef_construction": 200, "ef_runtime": 10}}]
        })).unwrap();
        let command = build_create_search_index_command(&input).unwrap();
        let args = command
            .args_iter()
            .filter_map(|arg| match arg {
                Arg::Simple(value) => Some(String::from_utf8(value.to_vec()).unwrap()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &args[4..],
            &[
                "SCHEMA",
                "embedding",
                "VECTOR",
                "HNSW",
                "12",
                "TYPE",
                "FLOAT32",
                "DIM",
                "768",
                "DISTANCE_METRIC",
                "COSINE",
                "M",
                "16",
                "EF_CONSTRUCTION",
                "200",
                "EF_RUNTIME",
                "10"
            ]
        );
    }

    #[test]
    fn vector_create_rejects_missing_config_and_invalid_dimension() {
        for vector in [
            serde_json::Value::Null,
            serde_json::json!({"algorithm": "FLAT",
            "data_type": "FLOAT32", "dimension": 0, "distance_metric": "L2"}),
        ] {
            let input: CreateSearchIndexInput = serde_json::from_value(serde_json::json!({
                "connection_id": "local", "index": "idx", "key_type": "hash", "prefixes": [],
                "fields": [{"name": "embedding", "field_type": "vector", "vector": vector}]
            }))
            .unwrap();
            assert_eq!(input.validate(), Err(AppError::InvalidInput));
        }
    }

    #[test]
    fn vector_flat_optional_argument_count_and_json_storage_are_correct() {
        let input: CreateSearchIndexInput = serde_json::from_value(serde_json::json!({
            "connection_id": "local", "index": "idx:vector", "key_type": "json", "prefixes": ["docs:"],
            "fields": [{"name": "$.embedding", "field_type": "vector", "vector": {
                "algorithm": "FLAT", "data_type": "FLOAT64", "dimension": 32,
                "distance_metric": "IP", "initial_capacity": 128, "block_size": 64
            }}]
        })).unwrap();
        let command = build_create_search_index_command(&input).unwrap();
        let args = command
            .args_iter()
            .filter_map(|arg| match arg {
                Arg::Simple(value) => Some(String::from_utf8(value.to_vec()).unwrap()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            [
                "FT.CREATE",
                "idx:vector",
                "ON",
                "JSON",
                "PREFIX",
                "1",
                "docs:",
                "SCHEMA",
                "$.embedding",
                "VECTOR",
                "FLAT",
                "10",
                "TYPE",
                "FLOAT64",
                "DIM",
                "32",
                "DISTANCE_METRIC",
                "IP",
                "INITIAL_CAP",
                "128",
                "BLOCK_SIZE",
                "64"
            ]
        );
    }

    #[test]
    fn index_attributes_keep_query_alias_separate_from_json_path() {
        let result = super::parse_attribute(Value::Array(vec![
            text("identifier"),
            text("$.name"),
            text("attribute"),
            text("name"),
            text("type"),
            text("TEXT"),
        ]))
        .unwrap();
        assert_eq!(result.identifier, "$.name");
        assert_eq!(result.query_name.as_deref(), Some("name"));
        let legacy = super::parse_attribute(Value::Array(vec![
            text("identifier"),
            text("name"),
            text("attribute"),
            text("TEXT"),
        ]))
        .unwrap();
        assert_eq!(legacy.query_name, None);
        assert_eq!(legacy.field_type, "TEXT");
    }

    #[tokio::test]
    #[ignore = "需要独立 Redis Stack：设置 REDIX_TEST_REDIS_STACK_URL 后显式运行"]
    async fn vector_indexes_execute_real_flat_and_hnsw_knn_queries() {
        let url = std::env::var("REDIX_TEST_REDIS_STACK_URL")
            .expect("请设置独立测试 Redis 的 REDIX_TEST_REDIS_STACK_URL");
        let client = redis::Client::open(url).unwrap();
        let mut connection = client.get_multiplexed_async_connection().await.unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for algorithm in ["FLAT", "HNSW"] {
            let prefix = format!(
                "redix:vector:test:{}:{nonce}:{algorithm}:",
                std::process::id()
            );
            let key = format!("{prefix}one");
            let index = format!("{prefix}index");
            let input: CreateSearchIndexInput = serde_json::from_value(serde_json::json!({
                "connection_id": "integration", "index": index, "key_type": "hash", "prefixes": [prefix],
                "fields": [{"name": "embedding", "field_type": "vector", "vector": {
                    "algorithm": algorithm, "data_type": "FLOAT32", "dimension": 2, "distance_metric": "L2"
                }}]
            })).unwrap();
            let flow: Result<Value, redis::RedisError> = async {
                build_create_search_index_command(&input)
                    .unwrap()
                    .query_async::<Value>(&mut connection)
                    .await?;
                let vector = [1.0_f32, 0.0_f32]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>();
                redis::cmd("HSET")
                    .arg(&key)
                    .arg("embedding")
                    .arg(&vector)
                    .query_async::<Value>(&mut connection)
                    .await?;
                redis::cmd("FT.SEARCH")
                    .arg(&index)
                    .arg("*=>[KNN 1 @embedding $vector AS score]")
                    .arg("PARAMS")
                    .arg(2)
                    .arg("vector")
                    .arg(&vector)
                    .arg("SORTBY")
                    .arg("score")
                    .arg("NOCONTENT")
                    .arg("DIALECT")
                    .arg(2)
                    .query_async(&mut connection)
                    .await
            }
            .await;
            let drop_result = redis::cmd("FT.DROPINDEX")
                .arg(&index)
                .query_async::<Value>(&mut connection)
                .await;
            let delete_result = redis::cmd("DEL")
                .arg(&key)
                .query_async::<Value>(&mut connection)
                .await;
            let result = flow.expect("VECTOR 索引创建和实际 KNN 查询必须成功");
            drop_result.expect("清理测试索引");
            delete_result.expect("清理测试键");
            assert_eq!(result, Value::Array(vec![Value::Int(1), text(&key)]));
        }
    }
    fn vector_input() -> crate::domain::SearchVectorQueryInput {
        serde_json::from_value(serde_json::json!({"connection_id":"local", "index":"idx", "field":"embedding", "vector":[1.0, -2.0], "count":2, "filter":"@tag:{book}"})).unwrap()
    }

    fn vector_info() -> crate::domain::SearchVectorFieldInfo {
        crate::domain::SearchVectorFieldInfo {
            data_type: "FLOAT32".into(),
            dimension: 2,
            distance_metric: "L2".into(),
        }
    }

    #[test]
    fn typed_vector_params_are_binary_little_endian_and_command_tokens_are_fixed() {
        let command =
            super::build_vector_search_command(&vector_input(), &vector_info(), "__redix_distance")
                .unwrap();
        let args = command
            .args_iter()
            .map(|arg| match arg {
                Arg::Simple(bytes) => bytes.to_vec(),
                _ => panic!("unexpected argument"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            args[2],
            b"(@tag:{book})=>[KNN 2 @embedding $__redix_vector AS __redix_distance]"
        );
        assert_eq!(args[3], b"PARAMS");
        assert_eq!(args[4], b"2");
        assert_eq!(
            args[6],
            [1.0_f32, -2.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        let mut info = vector_info();
        info.data_type = "FLOAT64".into();
        let command =
            super::build_vector_search_command(&vector_input(), &info, "__redix_distance").unwrap();
        let expected = [1.0_f64, -2.0]
            .into_iter()
            .flat_map(f64::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            command.args_iter().nth(6).unwrap(),
            Arg::Simple(expected.as_slice())
        );
    }

    #[test]
    fn typed_vector_rejects_invalid_parameters_and_unknown_schema() {
        let mut input = vector_input();
        for vector in [
            vec![1.0],
            vec![1e100, 0.0],
            vec![f64::NAN, 0.0],
            vec![f64::INFINITY, 0.0],
        ] {
            input.vector = vector;
            assert!(
                super::build_vector_search_command(&input, &vector_info(), "__redix_distance")
                    .is_err()
            );
        }
        for (field, filter, count) in [
            ("embedding]", "*", 2),
            ("embedding", "*)=>[KNN 1 @other $x] (", 2),
            ("embedding", "*", 201),
            ("embedding", "*", 0),
        ] {
            let mut input = vector_input();
            input.field = field.into();
            input.filter = filter.into();
            input.count = count;
            assert!(
                super::build_vector_search_command(&input, &vector_info(), "__redix_distance")
                    .is_err()
            );
        }
        let mut info = vector_info();
        info.data_type = "BFLOAT16".into();
        assert!(
            super::build_vector_search_command(&vector_input(), &info, "__redix_distance").is_err()
        );
        assert!(
            crate::domain::SearchVectorQueryInput::validate_search_version(Some("2.2.0")).is_err()
        );
        assert!(crate::domain::SearchVectorQueryInput::validate_search_version(None).is_err());
        assert!(
            crate::domain::SearchVectorQueryInput::validate_search_version(Some("2.4.0")).is_ok()
        );
    }

    #[test]
    fn typed_vector_schema_keeps_modern_metadata_and_legacy_none() {
        let attribute = super::parse_attribute(Value::Array(vec![
            text("identifier"),
            text("$.embedding"),
            text("attribute"),
            text("embedding"),
            text("type"),
            text("VECTOR"),
            text("data_type"),
            text("FLOAT32"),
            text("dim"),
            Value::Int(2),
            text("distance_metric"),
            text("L2"),
        ]))
        .unwrap();
        assert_eq!(attribute.vector, Some(vector_info()));
        let legacy = super::parse_attribute(Value::Array(vec![
            text("identifier"),
            text("embedding"),
            text("attribute"),
            text("VECTOR"),
        ]))
        .unwrap();
        assert_eq!(legacy.vector, None);
    }

    #[test]
    fn typed_vector_results_require_finite_sorted_distance_and_respect_count() {
        let result = super::parse_vector_search_result(
            Value::Array(vec![
                Value::Int(1),
                text("doc:1"),
                Value::Array(vec![text("score"), text("-0.5")]),
            ]),
            2,
            "score",
            "IP",
        )
        .unwrap();
        assert_eq!(result.matches[0].distance, -0.5);
        for distance in ["NaN", "inf", "garbage"] {
            assert!(super::parse_vector_search_result(
                Value::Array(vec![
                    Value::Int(1),
                    text("doc:1"),
                    Value::Array(vec![text("score"), text(distance)])
                ]),
                2,
                "score",
                "IP"
            )
            .is_err());
        }
        assert!(super::parse_vector_search_result(
            Value::Array(vec![Value::Int(3)]),
            2,
            "score",
            "L2"
        )
        .is_err());
        assert!(super::parse_vector_search_result(
            Value::Array(vec![
                Value::Int(1),
                text("doc:1"),
                Value::Array(vec![text("wrong"), text("1")])
            ]),
            2,
            "score",
            "L2"
        )
        .is_err());
    }

    #[test]
    fn search_create_alias_is_explicit_and_rejects_collisions() {
        let make = |fields: serde_json::Value| {
            serde_json::from_value::<CreateSearchIndexInput>(serde_json::json!({"connection_id":"local", "index":"idx", "key_type":"json", "prefixes":[], "fields":fields})).unwrap()
        };
        let input = make(
            serde_json::json!([{"name":"$.embedding", "alias":"embedding", "field_type":"vector", "vector":{"algorithm":"FLAT", "data_type":"FLOAT32", "dimension":2, "distance_metric":"L2"}}]),
        );
        let command = build_create_search_index_command(&input).unwrap();
        let args = command
            .args_iter()
            .map(|arg| match arg {
                Arg::Simple(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                _ => panic!("unexpected argument"),
            })
            .collect::<Vec<_>>();
        assert_eq!(&args[5..9], &["$.embedding", "AS", "embedding", "VECTOR"]);
        for fields in [
            serde_json::json!([{"name":"$.a", "alias":"same", "field_type":"text"},{"name":"$.b", "alias":"same", "field_type":"text"}]),
            serde_json::json!([{"name":"$.a", "alias":"b", "field_type":"text"},{"name":"b", "field_type":"text"}]),
            serde_json::json!([{"name":"$.a", "alias":"bad]", "field_type":"text"}]),
        ] {
            assert!(make(fields).validate().is_err());
        }
        assert!(
            make(serde_json::json!([{"name":"name", "field_type":"text"}]))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn typed_vector_filter_keeps_native_exclusive_numeric_ranges() {
        let mut input = vector_input();
        input.filter = "(@price:[(100 +inf] @tag:{book})".into();
        assert!(input.validate().is_ok());
        for filter in [
            "@price:[0 1",
            "*)=>[KNN 1 @other $x] (",
            "@tag:{book}=>{$weight:2}",
        ] {
            input.filter = filter.into();
            assert!(input.validate().is_err());
        }
    }

    #[test]
    fn typed_vector_resp3_rejects_partial_warnings_and_unsorted_results() {
        let row = |key: &str, score: &str| {
            Value::Map(vec![
                (text("id"), text(key)),
                (
                    text("extra_attributes"),
                    Value::Map(vec![(text("score"), text(score))]),
                ),
            ])
        };
        let response = |rows: Vec<Value>, warning: Value| {
            Value::Map(vec![
                (text("total_results"), Value::Int(rows.len() as i64)),
                (text("results"), Value::Array(rows)),
                (text("warning"), warning),
            ])
        };
        assert!(super::parse_vector_search_result(
            response(vec![row("a", "0.1"), row("b", "0.5")], Value::Array(vec![])),
            2,
            "score",
            "L2"
        )
        .is_ok());
        assert!(super::parse_vector_search_result(
            response(vec![row("a", "0.5"), row("b", "0.1")], Value::Nil),
            2,
            "score",
            "L2"
        )
        .is_err());
        assert!(super::parse_vector_search_result(
            response(
                vec![row("a", "0.1")],
                Value::Array(vec![text("Timeout limit was reached")])
            ),
            2,
            "score",
            "L2"
        )
        .is_err());
    }

    #[tokio::test]
    #[ignore = "需要独立 Redis Search：设置 REDIX_TEST_REDIS_STACK_URL 后显式运行"]
    async fn typed_vector_query_executes_real_knn_search() {
        let url =
            std::env::var("REDIX_TEST_REDIS_STACK_URL").expect("请设置 REDIX_TEST_REDIS_STACK_URL");
        let client = redis::Client::open(url).unwrap();
        let mut connection = client.get_multiplexed_async_connection().await.unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for (algorithm, data_type, storage) in [
            ("FLAT", "FLOAT32", "hash"),
            ("HNSW", "FLOAT64", "hash"),
            ("FLAT", "FLOAT32", "json"),
        ] {
            let prefix = format!(
                "redix:typed-vector:{}:{nonce}:{algorithm}:{storage}:",
                std::process::id()
            );
            let index = format!("{prefix}index");
            let key = format!("{prefix}one");
            let input: crate::domain::SearchVectorQueryInput = serde_json::from_value(serde_json::json!({"connection_id":"integration", "index":index, "field":"embedding", "vector":[1.0,0.0], "count":2, "filter":"@tag:{book}"})).unwrap();
            let create: CreateSearchIndexInput = serde_json::from_value(serde_json::json!({"connection_id":"integration", "index":index, "key_type":storage, "prefixes":[prefix], "fields":[{"name":if storage == "json" { "$.tag" } else { "tag" }, "alias":"tag", "field_type":"tag"}, {"name":if storage == "json" { "$.embedding" } else { "embedding" }, "alias":"embedding", "field_type":"vector", "vector":{"algorithm":algorithm, "data_type":data_type, "dimension":2, "distance_metric":"L2"}}]})).unwrap();
            let flow: Result<crate::domain::SearchVectorQueryResult, AppError> = async {
                build_create_search_index_command(&create)?
                    .query_async::<Value>(&mut connection)
                    .await
                    .map_err(|_| AppError::CommandFailed)?;
                let raw_info = redis::cmd("FT.INFO")
                    .arg(&index)
                    .query_async(&mut connection)
                    .await
                    .map_err(|_| AppError::CommandFailed)?;
                let info = super::parse_search_index_info(raw_info)?;
                let schema = info
                    .attributes
                    .iter()
                    .find(|field| field.query_name.as_deref() == Some("embedding"))
                    .and_then(|field| field.vector.as_ref())
                    .ok_or(AppError::CommandFailed)?;
                if storage == "json" {
                    redis::cmd("JSON.SET")
                        .arg(&key)
                        .arg("$")
                        .arg(r#"{"embedding":[1.0,0.0],"tag":"book"}"#)
                        .query_async::<Value>(&mut connection)
                        .await
                        .map_err(|_| AppError::CommandFailed)?;
                } else {
                    let bytes = input.encode_vector(schema)?;
                    redis::cmd("HSET")
                        .arg(&key)
                        .arg("embedding")
                        .arg(bytes)
                        .arg("tag")
                        .arg("book")
                        .query_async::<Value>(&mut connection)
                        .await
                        .map_err(|_| AppError::CommandFailed)?;
                }
                let raw = super::build_vector_search_command(&input, schema, "__redix_distance")?
                    .query_async(&mut connection)
                    .await
                    .map_err(|_| AppError::CommandFailed)?;
                super::parse_vector_search_result(
                    raw,
                    input.count,
                    "__redix_distance",
                    &schema.distance_metric,
                )
            }
            .await;
            let drop = redis::cmd("FT.DROPINDEX")
                .arg(&index)
                .query_async::<Value>(&mut connection)
                .await;
            let delete = redis::cmd("DEL")
                .arg(&key)
                .query_async::<Value>(&mut connection)
                .await;
            let result = flow.expect("typed KNN 实际查询失败");
            drop.expect("清理索引");
            delete.expect("清理键");
            assert_eq!(result.returned, 1);
            assert_eq!(result.matches[0].key, key);
            assert_eq!(result.matches[0].distance, 0.0);
        }
    }
}
