use crate::{domain::SearchDocumentField, error::AppError};

pub const MAX_AGGREGATE_LOAD_FIELDS: usize = 16;
pub const MAX_AGGREGATE_GROUPS: usize = 8;
pub const MAX_AGGREGATE_REDUCERS: usize = 8;
pub const MAX_AGGREGATE_SORTS: usize = 4;
pub const MAX_AGGREGATE_COLUMNS: usize = 32;
pub const MAX_AGGREGATE_PAGE: u32 = 100;
pub const MAX_AGGREGATE_OFFSET: u64 = 10_000;
pub const MAX_AGGREGATE_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_AGGREGATE_CELL_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AggregateFunction {
    Count,
    Sum,
    Min,
    Max,
    Avg,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AggregateReducer {
    pub function: AggregateFunction,
    pub field: Option<String>,
    pub alias: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AggregateDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AggregateSort {
    pub field: String,
    pub direction: AggregateDirection,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchAggregateInput {
    pub connection_id: String,
    pub index: String,
    pub query: String,
    pub load_fields: Vec<String>,
    pub group_by: Vec<String>,
    pub reducers: Vec<AggregateReducer>,
    pub sort_by: Vec<AggregateSort>,
    pub offset: u64,
    pub limit: u32,
}

impl SearchAggregateInput {
    pub fn validate(&self) -> Result<(), AppError> {
        use std::collections::HashSet;
        if self.connection_id.trim().is_empty() || self.connection_id.len() > 256 {
            return Err(AppError::InvalidConnection);
        }
        if self.index.trim().is_empty()
            || self.index.len() > 256
            || self.query.trim().is_empty()
            || self.query.len() > 4096
            || !(1..=MAX_AGGREGATE_PAGE).contains(&self.limit)
            || self.offset > MAX_AGGREGATE_OFFSET
            || self.load_fields.len() > MAX_AGGREGATE_LOAD_FIELDS
            || self.group_by.len() > MAX_AGGREGATE_GROUPS
            || self.reducers.len() > MAX_AGGREGATE_REDUCERS
            || self.sort_by.len() > MAX_AGGREGATE_SORTS
        {
            return Err(AppError::InvalidInput);
        }
        for names in [&self.load_fields, &self.group_by] {
            let mut seen = HashSet::new();
            for name in names {
                validate_field(name)?;
                if !seen.insert(name) {
                    return Err(AppError::InvalidInput);
                }
            }
        }
        let grouped = !self.group_by.is_empty() || !self.reducers.is_empty();
        let mut outputs: HashSet<&str> = if grouped {
            &self.group_by
        } else {
            &self.load_fields
        }
        .iter()
        .map(String::as_str)
        .collect();
        for reducer in &self.reducers {
            validate_field(&reducer.alias)?;
            if !outputs.insert(&reducer.alias) {
                return Err(AppError::InvalidInput);
            }
            match (&reducer.function, &reducer.field) {
                (AggregateFunction::Count, None) => (),
                (AggregateFunction::Count, Some(_)) | (_, None) => {
                    return Err(AppError::InvalidInput)
                }
                (_, Some(field)) => validate_field(field)?,
            }
        }
        let mut sorted = HashSet::new();
        for sort in &self.sort_by {
            validate_field(&sort.field)?;
            if !outputs.contains(sort.field.as_str()) || !sorted.insert(&sort.field) {
                return Err(AppError::InvalidInput);
            }
        }
        if self.loaded_fields().len() > MAX_AGGREGATE_LOAD_FIELDS || outputs.is_empty() {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }

    // Non-sortable schema fields must be explicitly loaded before GROUPBY/REDUCE.
    pub(crate) fn loaded_fields(&self) -> Vec<&str> {
        let mut fields = Vec::new();
        for field in self.load_fields.iter().chain(&self.group_by).chain(
            self.reducers
                .iter()
                .filter_map(|reducer| reducer.field.as_ref()),
        ) {
            if !fields.contains(&field.as_str()) {
                fields.push(field.as_str());
            }
        }
        fields
    }
}

fn validate_field(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '*' | ','))
    {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AggregateRow {
    pub fields: Vec<SearchDocumentField>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SearchAggregateResult {
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub columns: Vec<String>,
    pub rows: Vec<AggregateRow>,
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn input() -> SearchAggregateInput {
        SearchAggregateInput {
            connection_id: "local".into(),
            index: "idx".into(),
            query: "*".into(),
            load_fields: vec!["price".into(), "category".into()],
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
            offset: 0,
            limit: 20,
        }
    }
    #[test]
    fn aggregate_validates_limits_and_typed_references() {
        assert_eq!(input().validate(), Ok(()));
        let invalid = |change: fn(&mut SearchAggregateInput)| {
            let mut value = input();
            change(&mut value);
            assert_eq!(value.validate(), Err(AppError::InvalidInput));
        };
        invalid(|v| v.limit = 0);
        invalid(|v| v.limit = MAX_AGGREGATE_PAGE + 1);
        invalid(|v| v.offset = MAX_AGGREGATE_OFFSET + 1);
        invalid(|v| v.load_fields = vec!["*".into()]);
        invalid(|v| v.load_fields = vec!["price".into(); 17]);
        invalid(|v| v.group_by.push("category".into()));
        invalid(|v| v.reducers[0].alias = "category".into());
        invalid(|v| v.reducers[0].field = None);
        invalid(|v| v.reducers[0].function = AggregateFunction::Count);
        invalid(|v| v.sort_by[0].field = "price".into());
        invalid(|v| v.load_fields[0] = "@price".into());
        invalid(|v| v.load_fields[0] = "price\r\nLIMIT 0 999".into());
        invalid(|v| v.query = "a".repeat(4097));
    }
    #[test]
    fn aggregate_dto_rejects_raw_commands_and_unknown_reducers() {
        let mut input = serde_json::to_value(input()).unwrap();
        input["command"] = "FLUSHALL".into();
        assert!(serde_json::from_value::<SearchAggregateInput>(input).is_err());
        assert!(serde_json::from_str::<AggregateFunction>("\"random_sample\"").is_err());
    }
}
