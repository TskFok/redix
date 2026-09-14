use crate::error::AppError;

pub const MAX_COLLECTION_PAGE_HINT: usize = 500;
pub const MAX_COLLECTION_PAGE_ENTRIES: usize = 2_000;
pub const MAX_COLLECTION_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COLLECTION_VALUE_BYTES: usize = 1024 * 1024;
// Redis hash-field expiry has a tighter absolute-time bound than key expiry.
// A 100-year relative limit stays safely below that bound and is exact in JS.
pub const MAX_HASH_FIELD_TTL_MS: u64 = 3_153_600_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionKind {
    Hash,
    List,
    Set,
    Zset,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionOrder {
    #[default]
    Scan,
    ScoreAsc,
    ScoreDesc,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionPageInput {
    pub connection_id: String,
    pub key: String,
    pub kind: CollectionKind,
    pub cursor: String,
    pub count: usize,
    pub pattern: String,
    #[serde(default)]
    pub order: CollectionOrder,
}

impl CollectionPageInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_key(&self.connection_id, &self.key)?;
        let cursor = decimal_u64(&self.cursor)?;
        if !(1..=MAX_COLLECTION_PAGE_HINT).contains(&self.count)
            || self.pattern.len() > 4096
            || (self.order != CollectionOrder::Scan && self.kind != CollectionKind::Zset)
            || ((self.kind == CollectionKind::List || self.order != CollectionOrder::Scan)
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
    pub ttl_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollectionPage {
    pub entries: Vec<CollectionEntry>,
    pub next_cursor: String,
    pub has_more: bool,
    pub total: String,
    pub ttl_ms: i64,
    pub hash_field_ttl_supported: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum CollectionMutation {
    HashSet { field: String, value: String },
    HashDelete { field: String },
    HashExpire { field: String, ttl_ms: String },
    HashPersist { field: String },
    SetAdd { member: String },
    SetRemove { member: String },
    ZsetAdd { member: String, score: f64 },
    ZsetRemove { member: String },
    ListSet { index: String, value: String },
    ListAppend { value: String, prepend: bool },
    ListTrim { count: String, from_head: bool },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListIndexInput {
    pub connection_id: String,
    pub key: String,
    pub index: String,
}

impl ListIndexInput {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_key(&self.connection_id, &self.key)?;
        decimal_i64(&self.index).map(|_| ())
    }
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
            CollectionMutation::HashDelete { field }
            | CollectionMutation::HashPersist { field } => bounded(field),
            CollectionMutation::HashExpire { field, ttl_ms } => {
                bounded(field)?;
                positive_decimal(ttl_ms, MAX_HASH_FIELD_TTL_MS).map(|_| ())
            }
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
            CollectionMutation::ListTrim { count, .. } => {
                positive_decimal(count, i64::MAX as u64 - 1).map(|_| ())
            }
        }
    }
}

pub(crate) fn decimal_u64(value: &str) -> Result<u64, AppError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::InvalidInput);
    }
    value.parse().map_err(|_| AppError::InvalidInput)
}

pub(crate) fn decimal_i64(value: &str) -> Result<i64, AppError> {
    let magnitude = value.strip_prefix('-').unwrap_or(value);
    decimal_u64(magnitude)?;
    value.parse().map_err(|_| AppError::InvalidInput)
}

fn positive_decimal(value: &str, max: u64) -> Result<u64, AppError> {
    let value = decimal_u64(value)?;
    if value == 0 || value > max {
        Err(AppError::InvalidInput)
    } else {
        Ok(value)
    }
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

    #[test]
    fn collection_list_lookup_accepts_signed_i64_without_float_rounding() {
        for (index, valid) in [
            ("0", true),
            ("-1", true),
            ("9007199254740993", true),
            ("9223372036854775807", true),
            ("-9223372036854775808", true),
            ("9223372036854775808", false),
            ("-9223372036854775809", false),
            ("", false),
            ("+1", false),
            ("--1", false),
            ("1.0", false),
            (" 1", false),
        ] {
            assert_eq!(
                ListIndexInput {
                    connection_id: "local".into(),
                    key: "key".into(),
                    index: index.into()
                }
                .validate()
                .is_ok(),
                valid,
                "{index}"
            );
        }
    }

    #[test]
    fn collection_score_order_requires_zset_unfiltered_rank_paging() {
        for (kind, pattern, cursor, valid) in [
            ("zset", "*", "0", true),
            ("hash", "*", "0", false),
            ("list", "*", "0", false),
            ("zset", "member*", "0", false),
            ("zset", "*", "9223372036854775807", false),
        ] {
            let request: CollectionPageInput = serde_json::from_value(serde_json::json!({
                "connection_id": "local", "key": "key", "kind": kind,
                "cursor": cursor, "count": 2, "pattern": pattern, "order": "score_desc"
            }))
            .unwrap();
            assert_eq!(
                request.validate().is_ok(),
                valid,
                "{kind}/{pattern}/{cursor}"
            );
        }
    }

    #[test]
    fn collection_new_mutations_validate_decimal_bounds_before_execution() {
        for (operation, number_key, number, valid) in [
            ("hash_expire", "ttl_ms", "1", true),
            ("hash_expire", "ttl_ms", "3153600000000", true),
            ("hash_expire", "ttl_ms", "3153600000001", false),
            ("hash_expire", "ttl_ms", "9007199254740991", false),
            ("hash_expire", "ttl_ms", "9007199254740992", false),
            ("hash_expire", "ttl_ms", "0", false),
            ("hash_expire", "ttl_ms", "-1", false),
            ("list_trim", "count", "1", true),
            ("list_trim", "count", "9223372036854775806", true),
            ("list_trim", "count", "9223372036854775807", false),
            ("list_trim", "count", "0", false),
            ("list_trim", "count", "+1", false),
        ] {
            let mut mutation =
                serde_json::json!({"operation": operation, "field": "f", "from_head": true});
            mutation[number_key] = number.into();
            let parsed = serde_json::from_value::<CollectionMutationInput>(serde_json::json!({
                "connection_id": "local", "key": "key", "mutation": mutation
            }));
            assert!(parsed.is_ok(), "operation {operation} must be recognized");
            assert_eq!(
                parsed.unwrap().validate().is_ok(),
                valid,
                "{operation}/{number}"
            );
        }
    }

    fn page() -> CollectionPageInput {
        CollectionPageInput {
            connection_id: "local".into(),
            key: "key".into(),
            kind: CollectionKind::Hash,
            cursor: "0".into(),
            count: 500,
            pattern: "*".into(),
            order: CollectionOrder::Scan,
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
