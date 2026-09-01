use crate::error::AppError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GetStreamEntriesInput {
    pub connection_id: String,
    pub key: String,
    pub start: String,
    pub end: String,
    pub cursor: Option<String>,
    pub count: u32,
    pub reverse: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamEntryField {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamEntryRecord {
    pub id: String,
    pub fields: Vec<StreamEntryField>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StreamEntriesPage {
    pub entries: Vec<StreamEntryRecord>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AddStreamEntryInput {
    pub connection_id: String,
    pub key: String,
    pub id: String,
    pub fields: Vec<StreamEntryField>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteStreamEntriesInput {
    pub connection_id: String,
    pub key: String,
    pub ids: Vec<String>,
}

pub const MAX_STREAM_ENTRIES: u32 = 500;
const MAX_STREAM_FIELDS: usize = 500;
const MAX_STREAM_WRITE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn parse_stream_entry_id(id: &str) -> Result<(u64, u64), AppError> {
    let (ms, sequence) = id.split_once('-').ok_or(AppError::InvalidInput)?;
    let parse = |part: &str| {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AppError::InvalidInput);
        }
        part.parse::<u64>().map_err(|_| AppError::InvalidInput)
    };
    Ok((parse(ms)?, parse(sequence)?))
}

fn validate_target(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty()
        || connection_id.len() > 4_096
        || key.is_empty()
        || key.len() > 65_536
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

impl GetStreamEntriesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_target(&self.connection_id, &self.key)?;
        if !(1..=MAX_STREAM_ENTRIES).contains(&self.count) {
            return Err(AppError::InvalidInput);
        }
        let start = if self.start == "-" {
            (0, 0)
        } else {
            parse_stream_entry_id(&self.start)?
        };
        let end = if self.end == "+" {
            (u64::MAX, u64::MAX)
        } else {
            parse_stream_entry_id(&self.end)?
        };
        if start > end {
            return Err(AppError::InvalidInput);
        }
        if let Some(cursor) = &self.cursor {
            let cursor = parse_stream_entry_id(cursor)?;
            if cursor < start || cursor > end {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(())
    }
}
impl AddStreamEntryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_target(&self.connection_id, &self.key)?;
        if self.id != "*" && parse_stream_entry_id(&self.id)? == (0, 0) {
            return Err(AppError::InvalidInput);
        }
        if self.fields.is_empty() || self.fields.len() > MAX_STREAM_FIELDS {
            return Err(AppError::InvalidInput);
        }
        let bytes = self
            .fields
            .iter()
            .try_fold(0usize, |total, item| {
                total
                    .checked_add(item.field.len())?
                    .checked_add(item.value.len())
            })
            .ok_or(AppError::InvalidInput)?;
        if bytes > MAX_STREAM_WRITE_BYTES {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
impl DeleteStreamEntriesInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_target(&self.connection_id, &self.key)?;
        if self.ids.is_empty() || self.ids.len() > MAX_STREAM_ENTRIES as usize {
            return Err(AppError::InvalidInput);
        }
        for id in &self.ids {
            parse_stream_entry_id(id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn query() -> GetStreamEntriesInput {
        GetStreamEntriesInput {
            connection_id: "local".into(),
            key: "events".into(),
            start: "-".into(),
            end: "+".into(),
            cursor: None,
            count: 100,
            reverse: false,
        }
    }
    #[test]
    fn stream_page_rejects_unbounded_reads_and_invalid_exact_ids() {
        assert!(query().validate().is_ok());
        for count in [0, 501, u32::MAX] {
            let mut input = query();
            input.count = count;
            assert_eq!(input.validate(), Err(AppError::InvalidInput));
        }
        for id in [
            "1",
            "*",
            "1-*",
            "-1-0",
            "1-0-1",
            "18446744073709551616-0",
            "1- 0",
            "(1-0",
        ] {
            let mut input = query();
            input.cursor = Some(id.into());
            assert_eq!(input.validate(), Err(AppError::InvalidInput), "{id}");
        }
    }
    #[test]
    fn stream_range_uses_both_exact_u64_parts_and_checks_cursor_range() {
        let mut input = query();
        input.start = "9007199254740993-18446744073709551614".into();
        input.end = "9007199254740993-18446744073709551615".into();
        input.cursor = Some(input.start.clone());
        assert!(input.validate().is_ok());
        input.start = input.end.clone();
        input.end = "9007199254740993-18446744073709551614".into();
        assert_eq!(input.validate(), Err(AppError::InvalidInput));
        input = query();
        input.end = "2-0".into();
        input.cursor = Some("3-0".into());
        assert_eq!(input.validate(), Err(AppError::InvalidInput));
    }
    #[test]
    fn stream_mutations_require_bounded_explicit_ids_and_fields() {
        let mut add = AddStreamEntryInput {
            connection_id: "local".into(),
            key: "events".into(),
            id: "*".into(),
            fields: vec![StreamEntryField {
                field: "".into(),
                value: "".into(),
            }],
        };
        assert!(add.validate().is_ok());
        add.fields.clear();
        assert_eq!(add.validate(), Err(AppError::InvalidInput));
        add.fields.push(StreamEntryField {
            field: "x".into(),
            value: "y".into(),
        });
        add.id = "0-0".into();
        assert_eq!(add.validate(), Err(AppError::InvalidInput));
        add.id = "1-*".into();
        assert_eq!(add.validate(), Err(AppError::InvalidInput));
        let mut delete = DeleteStreamEntriesInput {
            connection_id: "local".into(),
            key: "events".into(),
            ids: vec!["1-0".into()],
        };
        assert!(delete.validate().is_ok());
        delete.ids = vec![];
        assert_eq!(delete.validate(), Err(AppError::InvalidInput));
        delete.ids = vec!["1-0".into(); 501];
        assert_eq!(delete.validate(), Err(AppError::InvalidInput));
        delete.ids = vec!["*".into()];
        assert_eq!(delete.validate(), Err(AppError::InvalidInput));
    }
}
