use crate::error::AppError;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::{Read, Write};

pub const MAX_STRING_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CODEC_TEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueFormat {
    Utf8,
    Json,
    Ascii,
    Hex,
    Binary,
    Base64,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueCompression {
    #[default]
    None,
    Gzip,
    Zlib,
    Deflate,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetStringValueInput {
    pub connection_id: String,
    pub key: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StringValue {
    pub base64: String,
    pub total_bytes: u64,
    pub ttl_ms: i64,
    pub truncated: bool,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetStringValueInput {
    pub connection_id: String,
    pub key: String,
    pub base64: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StringValueSaved {
    pub byte_length: usize,
    pub ttl_ms: i64,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecodeStringValueInput {
    pub base64: String,
    pub format: ValueFormat,
    #[serde(default)]
    pub compression: ValueCompression,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecodedStringValue {
    pub text: String,
    pub byte_length: usize,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncodeStringValueInput {
    pub text: String,
    pub format: ValueFormat,
    #[serde(default)]
    pub compression: ValueCompression,
}

impl GetStringValueInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.connection_id.len() > 4096
            || self.key.is_empty()
            || self.key.len() > 65536
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
impl SetStringValueInput {
    pub fn bytes(&self) -> Result<Vec<u8>, AppError> {
        GetStringValueInput {
            connection_id: self.connection_id.clone(),
            key: self.key.clone(),
        }
        .validate()?;
        decode_base64(&self.base64)
    }
}

fn decode_base64(value: &str) -> Result<Vec<u8>, AppError> {
    if value.len() > MAX_STRING_BYTES.div_ceil(3) * 4 {
        return Err(AppError::InvalidInput);
    }
    let bytes = STANDARD.decode(value).map_err(|_| AppError::InvalidInput)?;
    if bytes.len() > MAX_STRING_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(bytes)
}

fn decompress(bytes: Vec<u8>, compression: ValueCompression) -> Result<Vec<u8>, AppError> {
    if compression == ValueCompression::None {
        return Ok(bytes);
    }
    let mut output = Vec::new();
    if compression == ValueCompression::Gzip {
        flate2::read::MultiGzDecoder::new(bytes.as_slice())
            .take(MAX_STRING_BYTES as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|_| AppError::InvalidInput)?;
    } else {
        // The fixed capacity is an output budget; StreamEnd also rejects truncated
        // streams that a reader adapter could otherwise treat as a short EOF.
        output.reserve_exact(MAX_STRING_BYTES + 1);
        let mut decoder = flate2::Decompress::new(compression == ValueCompression::Zlib);
        let status = decoder
            .decompress_vec(&bytes, &mut output, flate2::FlushDecompress::Finish)
            .map_err(|_| AppError::InvalidInput)?;
        if status != flate2::Status::StreamEnd || decoder.total_in() != bytes.len() as u64 {
            return Err(AppError::InvalidInput);
        }
    }
    if output.len() > MAX_STRING_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(output)
}

pub fn decode_value(input: DecodeStringValueInput) -> Result<DecodedStringValue, AppError> {
    let bytes = decompress(decode_base64(&input.base64)?, input.compression)?;
    let byte_length = bytes.len();
    let text = match input.format {
        ValueFormat::Utf8 | ValueFormat::Ascii | ValueFormat::Json => {
            if input.format == ValueFormat::Ascii && !bytes.is_ascii() {
                return Err(AppError::InvalidInput);
            }
            let text = String::from_utf8(bytes).map_err(|_| AppError::InvalidInput)?;
            if input.format == ValueFormat::Json {
                let _: serde_json::Value =
                    serde_json::from_str(&text).map_err(|_| AppError::InvalidInput)?;
            }
            text
        }
        ValueFormat::Base64 => STANDARD.encode(bytes),
        ValueFormat::Hex | ValueFormat::Binary => {
            let width = if input.format == ValueFormat::Hex {
                3
            } else {
                9
            };
            if bytes.len().saturating_mul(width) > MAX_CODEC_TEXT_BYTES {
                return Err(AppError::InvalidInput);
            }
            let mut text = String::with_capacity(bytes.len() * width);
            for (index, byte) in bytes.into_iter().enumerate() {
                if index > 0 {
                    text.push(' ');
                }
                if input.format == ValueFormat::Hex {
                    use std::fmt::Write;
                    write!(text, "{byte:02X}").map_err(|_| AppError::InvalidInput)?;
                } else {
                    use std::fmt::Write;
                    write!(text, "{byte:08b}").map_err(|_| AppError::InvalidInput)?;
                }
            }
            text
        }
    };
    Ok(DecodedStringValue { text, byte_length })
}

pub fn encode_value(input: EncodeStringValueInput) -> Result<String, AppError> {
    if input.text.len() > MAX_CODEC_TEXT_BYTES {
        return Err(AppError::InvalidInput);
    }
    let bytes = match input.format {
        ValueFormat::Utf8 | ValueFormat::Ascii | ValueFormat::Json => {
            if input.format == ValueFormat::Ascii && !input.text.is_ascii() {
                return Err(AppError::InvalidInput);
            }
            if input.format == ValueFormat::Json {
                let _: serde_json::Value =
                    serde_json::from_str(&input.text).map_err(|_| AppError::InvalidInput)?;
            }
            input.text.into_bytes()
        }
        ValueFormat::Base64 => decode_base64(&input.text)?,
        ValueFormat::Hex | ValueFormat::Binary => {
            let digits: Vec<u8> = input
                .text
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            let width = if input.format == ValueFormat::Hex {
                2
            } else {
                8
            };
            if digits.len() % width != 0 || digits.len() / width > MAX_STRING_BYTES {
                return Err(AppError::InvalidInput);
            }
            digits
                .chunks_exact(width)
                .map(|chunk| {
                    let radix = if width == 2 { 16 } else { 2 };
                    if !chunk.iter().all(|byte| {
                        if width == 2 {
                            byte.is_ascii_hexdigit()
                        } else {
                            *byte == b'0' || *byte == b'1'
                        }
                    }) {
                        return Err(AppError::InvalidInput);
                    }
                    u8::from_str_radix(
                        std::str::from_utf8(chunk).map_err(|_| AppError::InvalidInput)?,
                        radix,
                    )
                    .map_err(|_| AppError::InvalidInput)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    if bytes.len() > MAX_STRING_BYTES {
        return Err(AppError::InvalidInput);
    }
    let bytes = match input.compression {
        ValueCompression::None => bytes,
        ValueCompression::Gzip => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(&bytes)
                .map_err(|_| AppError::InvalidInput)?;
            encoder.finish().map_err(|_| AppError::InvalidInput)?
        }
        ValueCompression::Zlib => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(&bytes)
                .map_err(|_| AppError::InvalidInput)?;
            encoder.finish().map_err(|_| AppError::InvalidInput)?
        }
        ValueCompression::Deflate => {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(&bytes)
                .map_err(|_| AppError::InvalidInput)?;
            encoder.finish().map_err(|_| AppError::InvalidInput)?
        }
    };
    if bytes.len() > MAX_STRING_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    fn decode(bytes: &[u8], format: ValueFormat) -> Result<String, AppError> {
        decode_value(DecodeStringValueInput {
            base64: STANDARD.encode(bytes),
            format,
            compression: ValueCompression::None,
        })
        .map(|result| result.text)
    }
    fn encode(text: &str, format: ValueFormat) -> Result<Vec<u8>, AppError> {
        encode_value(EncodeStringValueInput {
            text: text.into(),
            format,
            compression: ValueCompression::None,
        })
        .map(|value| STANDARD.decode(value).unwrap())
    }
    #[test]
    fn binary_formats_round_trip_all_bytes_without_utf8_conversion() {
        let bytes: Vec<u8> = (0..=255).collect();
        for format in [ValueFormat::Hex, ValueFormat::Binary, ValueFormat::Base64] {
            let text = decode(&bytes, format).unwrap();
            assert_eq!(encode(&text, format).unwrap(), bytes);
        }
        assert_eq!(
            decode(&[0, 255, 128], ValueFormat::Hex).unwrap(),
            "00 FF 80"
        );
        assert_eq!(
            decode(&[0, 255], ValueFormat::Binary).unwrap(),
            "00000000 11111111"
        );
    }
    #[test]
    fn text_formats_are_strict_and_json_retains_exact_large_numbers() {
        assert_eq!(
            decode("你好\0".as_bytes(), ValueFormat::Utf8).unwrap(),
            "你好\0"
        );
        assert!(decode(&[255], ValueFormat::Utf8).is_err());
        assert!(decode(&[128], ValueFormat::Ascii).is_err());
        assert!(encode("你好", ValueFormat::Ascii).is_err());
        let json = "{\"n\":18446744073709551616,\"precise\":0.123456789012345678901}";
        assert_eq!(decode(json.as_bytes(), ValueFormat::Json).unwrap(), json);
        assert_eq!(encode(json, ValueFormat::Json).unwrap(), json.as_bytes());
        assert!(decode(b"{invalid}", ValueFormat::Json).is_err());
        assert!(encode("01", ValueFormat::Json).is_err());
    }
    #[test]
    fn empty_values_are_valid_and_malformed_binary_input_is_rejected() {
        for format in [
            ValueFormat::Utf8,
            ValueFormat::Ascii,
            ValueFormat::Hex,
            ValueFormat::Binary,
            ValueFormat::Base64,
        ] {
            assert_eq!(encode("", format).unwrap(), Vec::<u8>::new());
        }
        for text in ["0", "0G", "0xAA"] {
            assert!(encode(text, ValueFormat::Hex).is_err());
        }
        for text in ["1", "0000000x", "100000000"] {
            assert!(encode(text, ValueFormat::Binary).is_err());
        }
        for text in ["!", "a", "Zg===", "Zg"] {
            assert!(encode(text, ValueFormat::Base64).is_err());
        }
    }
    #[test]
    fn compression_round_trips_and_rejects_corruption_and_expansion_over_limit() {
        use std::io::Write;
        for compression in [
            ValueCompression::Gzip,
            ValueCompression::Zlib,
            ValueCompression::Deflate,
        ] {
            let base64 = encode_value(EncodeStringValueInput {
                text: "压缩内容".into(),
                format: ValueFormat::Utf8,
                compression,
            })
            .unwrap();
            let result = decode_value(DecodeStringValueInput {
                base64,
                format: ValueFormat::Utf8,
                compression,
            })
            .unwrap();
            assert_eq!(result.text, "压缩内容");
            assert_eq!(result.byte_length, "压缩内容".len());
            assert!(decode_value(DecodeStringValueInput {
                base64: STANDARD.encode(b"bad"),
                format: ValueFormat::Utf8,
                compression
            })
            .is_err());
        }
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&vec![b'a'; MAX_STRING_BYTES + 1]).unwrap();
        let base64 = STANDARD.encode(gzip.finish().unwrap());
        assert!(decode_value(DecodeStringValueInput {
            base64,
            format: ValueFormat::Utf8,
            compression: ValueCompression::Gzip
        })
        .is_err());
    }
    #[test]
    fn target_and_payload_limits_reject_before_redis_work() {
        assert!(GetStringValueInput {
            connection_id: "local".into(),
            key: " ".into()
        }
        .validate()
        .is_ok());
        assert!(GetStringValueInput {
            connection_id: "".into(),
            key: "k".into()
        }
        .validate()
        .is_err());
        assert!(GetStringValueInput {
            connection_id: "local".into(),
            key: "".into()
        }
        .validate()
        .is_err());
        assert!(SetStringValueInput {
            connection_id: "local".into(),
            key: "k".into(),
            base64: STANDARD.encode(vec![0; MAX_STRING_BYTES + 1])
        }
        .bytes()
        .is_err());
        assert!(decode_value(DecodeStringValueInput {
            base64: "x".repeat(MAX_CODEC_TEXT_BYTES + 1),
            format: ValueFormat::Base64,
            compression: ValueCompression::None
        })
        .is_err());
    }
}
