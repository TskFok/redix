use ::redis::{Cmd, Value};

use crate::{
    domain::{
        normalize_array_index, AggregateArrayInput, ArrayAggregateOperation, ArrayCell,
        ArrayElement, ArrayRange, ArrayScan, ArraySearchResult, ArraySummary, CreateArrayInput,
        DeleteArrayElementsInput, DeleteArrayRangeInput, SearchArrayInput,
    },
    error::AppError,
};

const MAX_ARRAY_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn parse_array_summary(
    key: &str,
    length: Value,
    count: Value,
    next_index: Value,
) -> Result<ArraySummary, AppError> {
    if key.trim().is_empty() {
        return Err(AppError::CommandFailed);
    }
    Ok(ArraySummary {
        key: key.to_owned(),
        length: required_decimal(length)?,
        count: required_decimal(count)?,
        next_index: optional_decimal(next_index)?.unwrap_or_else(|| "0".to_owned()),
    })
}

pub(crate) fn parse_array_range(
    value: Value,
    start: &str,
    end: &str,
) -> Result<ArrayRange, AppError> {
    let start = normalize_array_index(start)?;
    let end = normalize_array_index(end)?;
    let start_number = start.parse::<u64>().map_err(|_| AppError::InvalidInput)?;
    let end_number = end.parse::<u64>().map_err(|_| AppError::InvalidInput)?;
    let span = if start_number >= end_number {
        start_number
            .checked_sub(end_number)
            .and_then(|value| value.checked_add(1))
    } else {
        end_number
            .checked_sub(start_number)
            .and_then(|value| value.checked_add(1))
    }
    .ok_or(AppError::InvalidInput)?;
    if span > crate::domain::MAX_ARRAY_ELEMENTS_PER_READ as u64 {
        return Err(AppError::InvalidInput);
    }

    let values = array_values(value)?;
    if values.len() != span as usize {
        return Err(AppError::CommandFailed);
    }
    let step = if start_number <= end_number {
        1_i8
    } else {
        -1_i8
    };
    let mut index = start_number;
    let mut cells = Vec::with_capacity(values.len());
    for value in values {
        cells.push(ArrayCell {
            index: index.to_string(),
            value: optional_text(value)?,
        });
        index = if step == 1 {
            index.checked_add(1).ok_or(AppError::CommandFailed)?
        } else {
            index.checked_sub(1).ok_or(AppError::CommandFailed)?
        };
    }
    Ok(ArrayRange {
        cells,
        start,
        end,
        has_more: false,
    })
}

pub(crate) fn parse_array_scan(value: Value, limit: usize) -> Result<ArrayScan, AppError> {
    if !(1..=crate::domain::MAX_ARRAY_ELEMENTS_PER_READ).contains(&limit) {
        return Err(AppError::InvalidInput);
    }
    let value = unwrap_attributes(value);
    let entries = match value {
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    let mut elements = Vec::new();
    if entries.first().is_some_and(is_pair_container) {
        for entry in entries {
            let pair = match unwrap_attributes(entry) {
                Value::Array(pair) | Value::Set(pair) if pair.len() == 2 => pair,
                _ => return Err(AppError::CommandFailed),
            };
            if let (Some(index), Some(value)) = (
                optional_decimal(pair[0].clone())?,
                optional_text(pair[1].clone())?,
            ) {
                elements.push(ArrayElement { index, value });
            }
        }
    } else {
        if entries.len() % 2 != 0 {
            return Err(AppError::CommandFailed);
        }
        for pair in entries.chunks_exact(2) {
            if let (Some(index), Some(value)) = (
                optional_decimal(pair[0].clone())?,
                optional_text(pair[1].clone())?,
            ) {
                elements.push(ArrayElement { index, value });
            }
        }
    }
    if elements.len() > limit {
        return Err(AppError::CommandFailed);
    }
    Ok(ArrayScan {
        elements,
        next_start: None,
        has_more: false,
    })
}

pub(crate) fn parse_array_search(
    value: Value,
    limit: usize,
) -> Result<ArraySearchResult, AppError> {
    if !(1..=crate::domain::MAX_ARRAY_ELEMENTS_PER_READ).contains(&limit) {
        return Err(AppError::InvalidInput);
    }
    let values = match unwrap_attributes(value) {
        Value::Array(values) | Value::Set(values) => values,
        _ => return Err(AppError::CommandFailed),
    };
    let mut elements = Vec::new();
    if values.first().is_some_and(is_pair_container) {
        for entry in values {
            let pair = match unwrap_attributes(entry) {
                Value::Array(pair) | Value::Set(pair) if pair.len() >= 1 && pair.len() <= 2 => pair,
                _ => return Err(AppError::CommandFailed),
            };
            let Some(index) = optional_decimal(pair[0].clone())? else {
                continue;
            };
            let value = pair
                .get(1)
                .cloned()
                .map(optional_text)
                .transpose()?
                .flatten()
                .unwrap_or_default();
            elements.push(ArrayElement { index, value });
        }
    } else if values.len() % 2 == 0
        && values
            .chunks_exact(2)
            .all(|pair| optional_decimal(pair[0].clone()).ok().flatten().is_some())
    {
        for pair in values.chunks_exact(2) {
            let Some(index) = optional_decimal(pair[0].clone())? else {
                continue;
            };
            elements.push(ArrayElement {
                index,
                value: optional_text(pair[1].clone())?.unwrap_or_default(),
            });
        }
    } else {
        for value in values {
            if let Some(index) = optional_decimal(value)? {
                elements.push(ArrayElement {
                    index,
                    value: String::new(),
                });
            }
        }
    }
    if elements.len() > limit {
        return Err(AppError::CommandFailed);
    }
    Ok(ArraySearchResult {
        total: elements.len().to_string(),
        elements,
    })
}

pub(crate) fn parse_array_aggregate(
    value: Value,
    operation: &ArrayAggregateOperation,
) -> Result<crate::domain::ArrayAggregateResult, AppError> {
    let value = match unwrap_attributes(value) {
        Value::Nil => String::new(),
        value => optional_text(value)?.ok_or(AppError::CommandFailed)?,
    };
    Ok(crate::domain::ArrayAggregateResult {
        operation: operation_name(operation).to_owned(),
        value,
    })
}

pub(crate) fn build_create_array_command(input: &CreateArrayInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd(match input.mode {
        crate::domain::ArrayCreateMode::Contiguous => "ARSET",
        crate::domain::ArrayCreateMode::Sparse => "ARMSET",
    });
    command.arg(&input.key);
    match input.mode {
        crate::domain::ArrayCreateMode::Contiguous => {
            command.arg(
                input
                    .start_index
                    .as_deref()
                    .map(normalize_array_index)
                    .transpose()?
                    .unwrap_or_else(|| "0".to_owned()),
            );
            for value in &input.values {
                command.arg(value);
            }
        }
        crate::domain::ArrayCreateMode::Sparse => {
            for element in &input.elements {
                command.arg(normalize_array_index(&element.index)?);
                command.arg(&element.value);
            }
        }
    }
    Ok(command)
}

pub(crate) fn build_array_get_command(key: &str, index: &str) -> Result<Cmd, AppError> {
    let index = normalize_array_index(index)?;
    let mut command = ::redis::cmd("ARGET");
    command.arg(key).arg(index);
    Ok(command)
}

pub(crate) fn build_array_multi_get_command(
    input: &crate::domain::ArrayMultiGetInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARMGET");
    command.arg(&input.key);
    for index in &input.indices {
        command.arg(normalize_array_index(index)?);
    }
    Ok(command)
}

pub(crate) fn build_array_range_command(
    input: &crate::domain::ArrayRangeInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARGETRANGE");
    command
        .arg(&input.key)
        .arg(normalize_array_index(&input.start)?)
        .arg(normalize_array_index(&input.end)?);
    Ok(command)
}

pub(crate) fn build_array_scan_command(
    input: &crate::domain::ArrayScanInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARSCAN");
    command.arg(&input.key);
    command.arg(
        input
            .start
            .as_deref()
            .map(normalize_array_index)
            .transpose()?
            .unwrap_or_else(|| "0".to_owned()),
    );
    command.arg(
        input
            .end
            .as_deref()
            .map(normalize_array_index)
            .transpose()?
            .unwrap_or_else(|| "+".to_owned()),
    );
    command.arg("LIMIT").arg(input.limit);
    Ok(command)
}

pub(crate) fn build_array_set_command(
    input: &crate::domain::SetArrayElementInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARSET");
    command
        .arg(&input.key)
        .arg(normalize_array_index(&input.index)?)
        .arg(&input.value);
    Ok(command)
}

pub(crate) fn build_array_append_command(
    key: &str,
    index: &str,
    values: &[String],
) -> Result<Cmd, AppError> {
    let index = normalize_array_index(index)?;
    if key.trim().is_empty()
        || values.is_empty()
        || values.len() > crate::domain::MAX_ARRAY_BATCH_ELEMENTS
    {
        return Err(AppError::InvalidInput);
    }
    if values
        .iter()
        .any(|value| value.len() > crate::domain::MAX_ARRAY_VALUE_BYTES)
    {
        return Err(AppError::InvalidInput);
    }
    let mut command = ::redis::cmd("ARSET");
    command.arg(key).arg(index);
    for value in values {
        command.arg(value);
    }
    Ok(command)
}

pub(crate) fn build_array_delete_command(
    input: &DeleteArrayElementsInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARDEL");
    command.arg(&input.key);
    for index in &input.indices {
        command.arg(normalize_array_index(index)?);
    }
    Ok(command)
}

pub(crate) fn build_array_delete_range_command(
    input: &DeleteArrayRangeInput,
) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARDELRANGE");
    command
        .arg(&input.key)
        .arg(normalize_array_index(&input.start)?)
        .arg(normalize_array_index(&input.end)?);
    Ok(command)
}

pub(crate) fn build_array_search_command(input: &SearchArrayInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("ARGREP");
    command
        .arg(&input.key)
        .arg(input.start.as_deref().unwrap_or("-"))
        .arg(input.end.as_deref().unwrap_or("+"));
    for predicate in &input.predicates {
        command.arg(predicate.criteria.trim().to_ascii_uppercase());
        command.arg(&predicate.value);
    }
    if input.predicates.len() > 1 {
        if let Some(combinator) = input.combinator.as_deref() {
            command.arg(combinator.trim().to_ascii_uppercase());
        }
    }
    if input.nocase {
        command.arg("NOCASE");
    }
    if input.with_values {
        command.arg("WITHVALUES");
    }
    command.arg("LIMIT").arg(input.limit);
    Ok(command)
}

pub(crate) fn build_array_aggregate_command(input: &AggregateArrayInput) -> Result<Cmd, AppError> {
    input.validate()?;
    let mut command = ::redis::cmd("AROP");
    command
        .arg(&input.key)
        .arg(input.start.as_deref().unwrap_or("0"))
        .arg(input.end.as_deref().unwrap_or("-1"))
        .arg(operation_name(&input.operation));
    if matches!(input.operation, ArrayAggregateOperation::Match) {
        let value = input.values.first().ok_or(AppError::InvalidInput)?;
        command.arg(value);
    }
    Ok(command)
}

pub(crate) fn build_array_insert_command(
    key: &str,
    index: &str,
    values: &[String],
) -> Result<Cmd, AppError> {
    let index = normalize_array_index(index)?;
    if key.trim().is_empty() || values.is_empty() {
        return Err(AppError::InvalidInput);
    }
    let mut command = ::redis::cmd("ARINSERT");
    command.arg(key).arg(index);
    for value in values {
        command.arg(value);
    }
    Ok(command)
}

pub(crate) fn build_array_ring_command(key: &str, start: &str, end: &str) -> Result<Cmd, AppError> {
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }
    let mut command = ::redis::cmd("ARRING");
    command
        .arg(key)
        .arg(normalize_array_index(start)?)
        .arg(normalize_array_index(end)?);
    Ok(command)
}

pub(crate) fn build_array_info_command(key: &str) -> Result<Cmd, AppError> {
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }
    let mut command = ::redis::cmd("ARINFO");
    command.arg(key);
    Ok(command)
}

fn array_values(value: Value) -> Result<Vec<Value>, AppError> {
    ensure_response_size(&value)?;
    match unwrap_attributes(value) {
        Value::Array(values) | Value::Set(values) => Ok(values),
        _ => Err(AppError::CommandFailed),
    }
}

fn unwrap_attributes(mut value: Value) -> Value {
    while let Value::Attribute { data, .. } = value {
        value = *data;
    }
    value
}

fn is_pair_container(value: &Value) -> bool {
    matches!(unwrap_attributes(value.clone()), Value::Array(values) | Value::Set(values) if values.len() >= 1)
}

fn required_decimal(value: Value) -> Result<String, AppError> {
    optional_decimal(value)?.ok_or(AppError::CommandFailed)
}

fn optional_decimal(value: Value) -> Result<Option<String>, AppError> {
    let value = unwrap_attributes(value);
    let Some(text) = optional_text(value)? else {
        return Ok(None);
    };
    normalize_array_index(&text)
        .map(Some)
        .map_err(|_| AppError::CommandFailed)
}

fn optional_text(value: Value) -> Result<Option<String>, AppError> {
    match unwrap_attributes(value) {
        Value::Nil => Ok(None),
        Value::BulkString(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AppError::CommandFailed),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => Ok(Some(value)),
        Value::Int(value) => Ok(Some(value.to_string())),
        Value::Double(value) if value.is_finite() => Ok(Some(value.to_string())),
        Value::Boolean(value) => Ok(Some(value.to_string())),
        Value::BigNumber(value) => Ok(Some(value.to_string())),
        _ => Err(AppError::CommandFailed),
    }
}

fn operation_name(operation: &ArrayAggregateOperation) -> &'static str {
    match operation {
        ArrayAggregateOperation::Sum => "SUM",
        ArrayAggregateOperation::Min => "MIN",
        ArrayAggregateOperation::Max => "MAX",
        ArrayAggregateOperation::And => "AND",
        ArrayAggregateOperation::Or => "OR",
        ArrayAggregateOperation::Xor => "XOR",
        ArrayAggregateOperation::Match => "MATCH",
        ArrayAggregateOperation::Used => "USED",
    }
}

fn ensure_response_size(value: &Value) -> Result<(), AppError> {
    if redis_value_size(value) > MAX_ARRAY_RESPONSE_BYTES {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

fn redis_value_size(value: &Value) -> usize {
    match value {
        Value::Nil | Value::Okay => 0,
        Value::Int(_) | Value::Double(_) | Value::Boolean(_) => 8,
        Value::BulkString(value) => value.len(),
        Value::SimpleString(value) => value.len(),
        Value::VerbatimString { text, .. } => text.len(),
        Value::BigNumber(value) => value.to_string().len(),
        Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => values
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
                .sum(),
        ),
        Value::ServerError(value) => value.to_string().len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use ::redis::{Cmd, Value};

    use crate::domain::{
        AggregateArrayInput, ArrayAggregateOperation, ArrayCell, ArrayCreateMode, ArrayElement,
        ArrayElementInput, ArrayMultiGetInput, ArrayPredicate, ArrayRangeInput, ArrayScanInput,
        CreateArrayInput, DeleteArrayElementsInput, DeleteArrayRangeInput, SearchArrayInput,
        SetArrayElementInput,
    };

    use super::{
        build_array_aggregate_command, build_array_append_command, build_array_delete_command,
        build_array_delete_range_command, build_array_get_command, build_array_info_command,
        build_array_insert_command, build_array_multi_get_command, build_array_range_command,
        build_array_ring_command, build_array_scan_command, build_array_search_command,
        build_array_set_command, build_create_array_command, parse_array_aggregate,
        parse_array_range, parse_array_scan, parse_array_search,
    };

    fn assert_command_name(command: &Cmd, name: &str) {
        let packed = String::from_utf8(command.get_packed_command()).unwrap();
        assert!(
            packed.contains(&format!("${}\r\n{}\r\n", name.len(), name)),
            "command {name} was not found in {packed:?}"
        );
    }

    #[test]
    fn parses_sparse_range_without_losing_indexes() {
        let reply = Value::Array(vec![
            Value::BulkString(b"first".to_vec()),
            Value::Nil,
            Value::BulkString(b"third".to_vec()),
        ]);
        let result = parse_array_range(reply, "10", "12").unwrap();
        assert_eq!(
            result.cells,
            vec![
                ArrayCell {
                    index: "10".into(),
                    value: Some("first".into())
                },
                ArrayCell {
                    index: "11".into(),
                    value: None
                },
                ArrayCell {
                    index: "12".into(),
                    value: Some("third".into())
                },
            ]
        );
    }

    #[test]
    fn builds_array_search_without_command_string_interpolation() {
        let input = SearchArrayInput {
            connection_id: "local".into(),
            key: "arr".into(),
            start: None,
            end: None,
            predicates: vec![ArrayPredicate {
                criteria: "CONTAINS".into(),
                value: "a b".into(),
            }],
            combinator: None,
            nocase: true,
            with_values: true,
            limit: 50,
        };
        let command = build_array_search_command(&input).unwrap();
        assert_eq!(
            command.get_packed_command(),
            b"*10\r\n$6\r\nARGREP\r\n$3\r\narr\r\n$1\r\n-\r\n$1\r\n+\r\n$8\r\nCONTAINS\r\n$3\r\na b\r\n$6\r\nNOCASE\r\n$10\r\nWITHVALUES\r\n$5\r\nLIMIT\r\n$2\r\n50\r\n".to_vec()
        );
    }

    #[test]
    fn parses_resp3_array_variants_and_preserves_empty_results() {
        let scan = parse_array_scan(
            Value::Attribute {
                data: Box::new(Value::Set(vec![
                    Value::Array(vec![Value::Int(12), Value::BulkString(b"value".to_vec())]),
                    Value::Array(vec![Value::Int(13), Value::Nil]),
                ])),
                attributes: vec![],
            },
            10,
        )
        .unwrap();
        assert_eq!(scan.elements.len(), 1);
        assert_eq!(scan.elements[0].index, "12");

        let search = parse_array_search(Value::Array(vec![Value::Int(99)]), 10).unwrap();
        assert_eq!(search.total, "1");
        assert_eq!(search.elements[0].value, "");

        let aggregate = parse_array_aggregate(Value::Nil, &ArrayAggregateOperation::Sum).unwrap();
        assert_eq!(aggregate.value, "");
        assert_eq!(aggregate.operation, "SUM");
    }

    #[test]
    fn builds_all_array_commands_with_fixed_command_names() {
        let create_contiguous = build_create_array_command(&CreateArrayInput {
            connection_id: "local".into(),
            key: "arr".into(),
            mode: ArrayCreateMode::Contiguous,
            start_index: Some("2".into()),
            values: vec!["a".into(), "b".into()],
            elements: vec![],
            ttl_ms: None,
        })
        .unwrap();
        assert_command_name(&create_contiguous, "ARSET");

        let create_sparse = build_create_array_command(&CreateArrayInput {
            connection_id: "local".into(),
            key: "arr".into(),
            mode: ArrayCreateMode::Sparse,
            start_index: None,
            values: vec![],
            elements: vec![ArrayElement {
                index: "10".into(),
                value: "a".into(),
            }],
            ttl_ms: None,
        })
        .unwrap();
        assert_command_name(&create_sparse, "ARMSET");

        let get = build_array_get_command("arr", "2").unwrap();
        assert_command_name(&get, "ARGET");
        let multi_get = build_array_multi_get_command(&ArrayMultiGetInput {
            connection_id: "local".into(),
            key: "arr".into(),
            indices: vec!["1".into(), "2".into()],
        })
        .unwrap();
        assert_command_name(&multi_get, "ARMGET");
        let range = build_array_range_command(&ArrayRangeInput {
            connection_id: "local".into(),
            key: "arr".into(),
            start: "1".into(),
            end: "2".into(),
        })
        .unwrap();
        assert_command_name(&range, "ARGETRANGE");
        let scan = build_array_scan_command(&ArrayScanInput {
            connection_id: "local".into(),
            key: "arr".into(),
            start: Some("1".into()),
            end: Some("2".into()),
            limit: 10,
        })
        .unwrap();
        assert_command_name(&scan, "ARSCAN");
        let set = build_array_set_command(&SetArrayElementInput {
            connection_id: "local".into(),
            key: "arr".into(),
            index: "2".into(),
            value: "b".into(),
        })
        .unwrap();
        assert_command_name(&set, "ARSET");
        let append = build_array_append_command("arr", "3", &["c".into()]).unwrap();
        assert_command_name(&append, "ARSET");
        let delete = build_array_delete_command(&DeleteArrayElementsInput {
            connection_id: "local".into(),
            key: "arr".into(),
            indices: vec!["2".into()],
        })
        .unwrap();
        assert_command_name(&delete, "ARDEL");
        let delete_range = build_array_delete_range_command(&DeleteArrayRangeInput {
            connection_id: "local".into(),
            key: "arr".into(),
            start: "1".into(),
            end: "2".into(),
        })
        .unwrap();
        assert_command_name(&delete_range, "ARDELRANGE");
        let aggregate = build_array_aggregate_command(&AggregateArrayInput {
            connection_id: "local".into(),
            key: "arr".into(),
            operation: ArrayAggregateOperation::Match,
            start: Some("0".into()),
            end: Some("2".into()),
            values: vec!["b".into()],
            limit: 10,
        })
        .unwrap();
        assert_command_name(&aggregate, "AROP");
        let insert = build_array_insert_command("arr", "1", &["x".into()]).unwrap();
        assert_command_name(&insert, "ARINSERT");
        let ring = build_array_ring_command("arr", "0", "2").unwrap();
        assert_command_name(&ring, "ARRING");
        let info = build_array_info_command("arr").unwrap();
        assert_command_name(&info, "ARINFO");

        let element = ArrayElementInput {
            connection_id: "local".into(),
            key: "arr".into(),
            index: "1".into(),
        };
        assert!(element.validate().is_ok());
    }
}
