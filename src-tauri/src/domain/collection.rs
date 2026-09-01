use crate::error::AppError;

pub const MAX_COLLECTION_PAGE_HINT: usize = 500;
pub const MAX_COLLECTION_PAGE_ENTRIES: usize = 2_000;
pub const MAX_COLLECTION_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COLLECTION_VALUE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionKind {
    Hash,
    List,
    Set,
    Zset,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionPageInput {
    pub connection_id: String,
    pub key: String,
    pub kind: CollectionKind,
    pub cursor: String,
    pub count: usize,
    pub pattern: String,
}

impl CollectionPageInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_key(&self.connection_id, &self.key)?;
        let cursor = decimal_u64(&self.cursor)?;
        if !(1..=MAX_COLLECTION_PAGE_HINT).contains(&self.count)
            || self.pattern.len() > 4096
            || (self.kind == CollectionKind::List
                && (self.pattern != "*"
                    || cursor
                        .checked_add(self.count as u64)
                        .is_none_or(|end| end > i64::MAX as u64)))
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollectionEntry {
    pub id: String,
    pub value: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollectionPage {
    pub entries: Vec<CollectionEntry>,
    pub next_cursor: String,
    pub has_more: bool,
    pub total: String,
    pub ttl_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum CollectionMutation {
    HashSet { field: String, value: String },
    HashDelete { field: String },
    SetAdd { member: String },
    SetRemove { member: String },
    ZsetAdd { member: String, score: f64 },
    ZsetRemove { member: String },
    ListSet { index: String, value: String },
    ListAppend { value: String, prepend: bool },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionMutationInput {
    pub connection_id: String,
    pub key: String,
    pub mutation: CollectionMutation,
}

impl CollectionMutationInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_key(&self.connection_id, &self.key)?;
        let bounded = |text: &str| {
            if text.len() <= MAX_COLLECTION_VALUE_BYTES {
                Ok(())
            } else {
                Err(AppError::InvalidInput)
            }
        };
        match &self.mutation {
            CollectionMutation::HashSet { field, value } => {
                bounded(field)?;
                bounded(value)
            }
            CollectionMutation::HashDelete { field } => bounded(field),
            CollectionMutation::SetAdd { member }
            | CollectionMutation::SetRemove { member }
            | CollectionMutation::ZsetRemove { member } => bounded(member),
            CollectionMutation::ZsetAdd { member, score } => {
                bounded(member)?;
                if score.is_finite() {
                    Ok(())
                } else {
                    Err(AppError::InvalidInput)
                }
            }
            CollectionMutation::ListSet { index, value } => {
                if decimal_u64(index)? > i64::MAX as u64 {
                    return Err(AppError::InvalidInput);
                }
                bounded(value)
            }
            CollectionMutation::ListAppend { value, .. } => bounded(value),
        }
    }
}

pub(crate) fn decimal_u64(value: &str) -> Result<u64, AppError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::InvalidInput);
    }
    value.parse().map_err(|_| AppError::InvalidInput)
}

fn validate_key(connection_id: &str, key: &str) -> Result<(), AppError> {
    if connection_id.trim().is_empty()
        || connection_id.len() > 4096
        || key.is_empty()
        || key.len() > 65536
    {
        Err(AppError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> CollectionPageInput {
        CollectionPageInput {
            connection_id: "local".into(),
            key: "key".into(),
            kind: CollectionKind::Hash,
            cursor: "0".into(),
            count: 500,
            pattern: "*".into(),
        }
    }

    #[test]
    fn collection_rejects_invalid_page_bounds_without_rounding_u64_cursor() {
        for count in [0, 501, usize::MAX] {
            assert_eq!(
                CollectionPageInput { count, ..page() }.validate(),
                Err(AppError::InvalidInput)
            );
        }
        for cursor in ["-1", "1.5", "18446744073709551616", " 0", "+1", ""] {
            assert!(CollectionPageInput {
                cursor: cursor.into(),
                ..page()
            }
            .validate()
            .is_err());
        }
        assert!(CollectionPageInput {
            cursor: "18446744073709551615".into(),
            ..page()
        }
        .validate()
        .is_ok());
        assert!(CollectionPageInput {
            kind: CollectionKind::List,
            cursor: "9223372036854775807".into(),
            ..page()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn collection_limits_inputs_and_rejects_nonfinite_scores() {
        assert!(CollectionPageInput {
            pattern: "x".repeat(4097),
            ..page()
        }
        .validate()
        .is_err());
        assert!(CollectionPageInput {
            key: "".into(),
            ..page()
        }
        .validate()
        .is_err());
        assert!(CollectionPageInput {
            kind: CollectionKind::List,
            pattern: "match*".into(),
            ..page()
        }
        .validate()
        .is_err());
        for mutation in [
            CollectionMutation::ZsetAdd {
                member: "m".into(),
                score: f64::NAN,
            },
            CollectionMutation::ZsetAdd {
                member: "m".into(),
                score: f64::INFINITY,
            },
            CollectionMutation::ListSet {
                index: "-1".into(),
                value: "v".into(),
            },
            CollectionMutation::ListAppend {
                value: "v".repeat(MAX_COLLECTION_VALUE_BYTES + 1),
                prepend: false,
            },
        ] {
            assert!(CollectionMutationInput {
                connection_id: "local".into(),
                key: "key".into(),
                mutation
            }
            .validate()
            .is_err());
        }
        assert!(CollectionMutationInput {
            connection_id: "local".into(),
            key: "key".into(),
            mutation: CollectionMutation::HashSet {
                field: "".into(),
                value: "".into()
            }
        }
        .validate()
        .is_ok());
    }
}
