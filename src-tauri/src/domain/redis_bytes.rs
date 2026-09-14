use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A Redis bulk string. Identity and command arguments always use the original bytes.
#[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RedisBytes(Vec<u8>);

impl RedisBytes {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn utf8(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }
}
impl AsRef<[u8]> for RedisBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl From<Vec<u8>> for RedisBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}
impl From<&[u8]> for RedisBytes {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}
impl<const N: usize> From<&[u8; N]> for RedisBytes {
    fn from(value: &[u8; N]) -> Self {
        Self(value.to_vec())
    }
}
impl From<String> for RedisBytes {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}
impl From<&str> for RedisBytes {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}
impl From<&String> for RedisBytes {
    fn from(value: &String) -> Self {
        Self::from(value.as_str())
    }
}
impl From<&RedisBytes> for RedisBytes {
    fn from(value: &RedisBytes) -> Self {
        value.clone()
    }
}
impl PartialEq<str> for RedisBytes {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}
impl PartialEq<&str> for RedisBytes {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}
impl PartialEq<String> for RedisBytes {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}
impl PartialEq<RedisBytes> for str {
    fn eq(&self, other: &RedisBytes) -> bool {
        other == self
    }
}
impl PartialEq<RedisBytes> for &str {
    fn eq(&self, other: &RedisBytes) -> bool {
        other == *self
    }
}
impl PartialEq<RedisBytes> for String {
    fn eq(&self, other: &RedisBytes) -> bool {
        other == self
    }
}

impl std::fmt::Display for RedisBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Escapes controls and invalid UTF-8 for diagnostics only; never use this as a key.
        match self.utf8() {
            Ok(text) => write!(f, "{}", text.escape_debug()),
            Err(_) => write!(f, "base64:{}", STANDARD.encode(&self.0)),
        }
    }
}
impl Serialize for RedisBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.utf8() {
            Ok(text) => serializer.serialize_str(text),
            Err(_) => {
                use serde::ser::SerializeStruct;
                let mut object = serializer.serialize_struct("RedisBytes", 1)?;
                object.serialize_field("base64", &STANDARD.encode(&self.0))?;
                object.end()
            }
        }
    }
}
impl<'de> Deserialize<'de> for RedisBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Encoded {
            base64: String,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Text(String),
            Encoded(Encoded),
        }
        match Wire::deserialize(deserializer)? {
            Wire::Text(text) => Ok(text.into()),
            Wire::Encoded(encoded) => STANDARD
                .decode(encoded.base64)
                .map(Self)
                .map_err(serde::de::Error::custom),
        }
    }
}
impl ::redis::ToRedisArgs for RedisBytes {
    fn write_redis_args<W: ?Sized + ::redis::RedisWrite>(&self, out: &mut W) {
        out.write_arg(self.as_bytes());
    }
}
impl ::redis::FromRedisValue for RedisBytes {
    fn from_redis_value(value: ::redis::Value) -> Result<Self, ::redis::ParsingError> {
        match value {
            ::redis::Value::BulkString(bytes) => Ok(Self(bytes)),
            ::redis::Value::SimpleString(text) | ::redis::Value::VerbatimString { text, .. } => {
                Ok(text.into())
            }
            ::redis::Value::Attribute { data, .. } => Self::from_redis_value(*data),
            _ => Err("Expected a Redis bulk string".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::redis::{FromRedisValue, ToRedisArgs};

    #[test]
    fn wire_and_redis_arguments_round_trip_all_bytes() {
        for bytes in [
            vec![],
            b" \0\n".to_vec(),
            "中文".as_bytes().to_vec(),
            (0..=255).collect(),
        ] {
            let value = RedisBytes::from(bytes.clone());
            let json = serde_json::to_value(&value).unwrap();
            assert_eq!(serde_json::from_value::<RedisBytes>(json).unwrap(), value);
            assert_eq!(value.to_redis_args(), vec![bytes.clone()]);
            assert_eq!(
                RedisBytes::from_redis_value(::redis::Value::BulkString(bytes)).unwrap(),
                value
            );
        }
        assert!(RedisBytes::from_redis_value(::redis::Value::Nil).is_err());
        assert!(RedisBytes::from_redis_value(::redis::Value::Array(vec![])).is_err());
        assert_eq!(
            serde_json::to_value(RedisBytes::from("text")).unwrap(),
            "text"
        );
        assert_eq!(
            serde_json::to_value(RedisBytes::from(vec![255, 0])).unwrap(),
            serde_json::json!({"base64":"/wA="})
        );
    }

    #[test]
    fn wire_rejects_malformed_or_ambiguous_base64() {
        for json in [
            serde_json::json!({"base64":"Zg"}),
            serde_json::json!({"base64":"Zh=="}),
            serde_json::json!({"base64":"!"}),
            serde_json::json!({"base64":"Zg==", "text":"other"}),
            serde_json::json!({"base64":42}),
        ] {
            assert!(serde_json::from_value::<RedisBytes>(json).is_err());
        }
        let from_base64: RedisBytes =
            serde_json::from_value(serde_json::json!({"base64":"Zg=="})).unwrap();
        assert_eq!(from_base64, "f");
        assert_eq!(serde_json::to_value(from_base64).unwrap(), "f");
    }

    #[test]
    fn identity_does_not_depend_on_diagnostic_display() {
        use std::collections::HashSet;
        let values = HashSet::from([
            RedisBytes::from(vec![255]),
            RedisBytes::from("base64:/w=="),
            RedisBytes::from("\0"),
            RedisBytes::from("\\0"),
        ]);
        assert_eq!(values.len(), 4);
        assert!(!RedisBytes::from("\n\0").to_string().contains('\n'));
    }
}
