use crate::{domain::value_codec::*, error::AppError, redis::RedisService};
use base64::{engine::general_purpose::STANDARD, Engine};

// Checks and replacement are atomic. KEEPTTL preserves the existing expiry exactly,
// including permanent keys, instead of reconstructing it using rounded milliseconds.
const WRITE_STRING: &str = "local t=redis.call('TYPE',KEYS[1]); if type(t)=='table' then t=t['ok'] end; if t=='none' then return {0,-2} end; if t~='string' then return {1,-1} end; redis.call('SET',KEYS[1],ARGV[1],'KEEPTTL'); return {2,redis.call('PTTL',KEYS[1])}";

impl RedisService {
    pub async fn get_string_value(
        &self,
        input: GetStringValueInput,
    ) -> Result<StringValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let type_name: String = ::redis::cmd("TYPE")
            .arg(&input.key)
            .query_async(&mut connection)
            .await
            .map_err(|_| AppError::CommandFailed)?;
        check_type(&type_name)?;
        // Reads require neither EVAL nor MULTI permission. This is deliberately a
        // non-atomic preview: recheck type/expiry and reject inconsistent lengths.
        let (total_bytes, ttl_ms, bytes, current_type): (u64, i64, Vec<u8>, String) =
            ::redis::pipe()
                .cmd("STRLEN")
                .arg(&input.key)
                .cmd("PTTL")
                .arg(&input.key)
                .cmd("GETRANGE")
                .arg(&input.key)
                .arg(0)
                .arg(MAX_STRING_BYTES)
                .cmd("TYPE")
                .arg(&input.key)
                .query_async(&mut connection)
                .await
                .map_err(|_| AppError::CommandFailed)?;
        check_type(&current_type)?;
        if ttl_ms == -2 {
            return Err(AppError::KeyNotFound);
        }
        if bytes.len() > MAX_STRING_BYTES + 1 || bytes.len() as u64 > total_bytes || ttl_ms < -1 {
            return Err(AppError::CommandFailed);
        }
        // One extra byte detects a value that grew beyond the preview limit between
        // STRLEN and GETRANGE. Never enable saving a silently partial range.
        let truncated = bytes.len() > MAX_STRING_BYTES || total_bytes > bytes.len() as u64;
        Ok(StringValue {
            base64: STANDARD.encode(&bytes[..bytes.len().min(MAX_STRING_BYTES)]),
            total_bytes,
            ttl_ms,
            truncated,
        })
    }
    pub async fn set_string_value(
        &self,
        input: SetStringValueInput,
    ) -> Result<StringValueSaved, AppError> {
        let bytes = input.bytes()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let (status, ttl_ms): (i64, i64) = ::redis::cmd("EVAL")
            .arg(WRITE_STRING)
            .arg(1)
            .arg(&input.key)
            .arg(&bytes)
            .query_async(&mut connection)
            .await
            .map_err(|_| AppError::CommandFailed)?;
        check_status(status)?;
        Ok(StringValueSaved {
            byte_length: bytes.len(),
            ttl_ms,
        })
    }
}

fn check_type(type_name: &str) -> Result<(), AppError> {
    match type_name {
        "string" => Ok(()),
        "none" => Err(AppError::KeyNotFound),
        _ => Err(AppError::UnsupportedDataType),
    }
}

fn check_status(status: i64) -> Result<(), AppError> {
    match status {
        2 => Ok(()),
        0 => Err(AppError::KeyNotFound),
        1 => Err(AppError::UnsupportedDataType),
        _ => Err(AppError::CommandFailed),
    }
}
