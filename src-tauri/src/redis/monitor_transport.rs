use std::pin::Pin;

use futures_util::Stream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::error::AppError;

use super::standalone_transport::BoxedRedisStream;

const MAX_ACK_LINE_BYTES: usize = 8 * 1024;
const MAX_MONITOR_LINE_BYTES: usize = 256 * 1024;

pub type MonitorLineStream = Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>>;

pub(crate) async fn monitor_stream(
    stream: BoxedRedisStream,
    redis_info: &redis::RedisConnectionInfo,
) -> Result<MonitorLineStream, AppError> {
    let mut reader = BufReader::new(stream);
    if let Some(password) = redis_info.password() {
        let mut auth = redis::cmd("AUTH");
        if let Some(username) = redis_info.username() {
            auth.arg(username);
        }
        auth.arg(password);
        write_command(reader.get_mut(), &auth).await?;
        require_ok(&mut reader, true).await?;
    }
    if redis_info.db() != 0 {
        let mut select = redis::cmd("SELECT");
        select.arg(redis_info.db());
        write_command(reader.get_mut(), &select).await?;
        require_ok(&mut reader, false).await?;
    }
    write_command(reader.get_mut(), &redis::cmd("MONITOR")).await?;
    require_ok(&mut reader, false).await?;

    Ok(Box::pin(futures_util::stream::unfold(
        Some(reader),
        |state| async move {
            let mut reader = state?;
            match read_monitor_line(&mut reader).await {
                Ok(Some(line)) => Some((Ok(line), Some(reader))),
                Ok(None) => None,
                Err(error) => Some((Err(error), None)),
            }
        },
    )))
}

async fn write_command(
    stream: &mut BoxedRedisStream,
    command: &redis::Cmd,
) -> Result<(), AppError> {
    stream
        .write_all(&command.get_packed_command())
        .await
        .map_err(|_| AppError::ConnectionFailed)
}

async fn require_ok(
    reader: &mut BufReader<BoxedRedisStream>,
    authenticating: bool,
) -> Result<(), AppError> {
    let line = read_bounded_line(reader, MAX_ACK_LINE_BYTES)
        .await?
        .ok_or(AppError::ConnectionFailed)?;
    if line == b"+OK\r\n" {
        return Ok(());
    }
    if authenticating && line.first() == Some(&b'-') {
        Err(AppError::AuthenticationFailed)
    } else {
        Err(AppError::CommandFailed)
    }
}

async fn read_monitor_line(
    reader: &mut BufReader<BoxedRedisStream>,
) -> Result<Option<String>, AppError> {
    let Some(line) = read_bounded_line(reader, MAX_MONITOR_LINE_BYTES).await? else {
        return Ok(None);
    };
    if line.first() != Some(&b'+') || !line.ends_with(b"\r\n") {
        return Err(AppError::CommandFailed);
    }
    String::from_utf8(line[1..line.len() - 2].to_vec())
        .map(Some)
        .map_err(|_| AppError::CommandFailed)
}

async fn read_bounded_line<R>(
    reader: &mut R,
    payload_limit: usize,
) -> Result<Option<Vec<u8>>, AppError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let (take, complete) = {
            let available = reader
                .fill_buf()
                .await
                .map_err(|_| AppError::ConnectionFailed)?;
            if available.is_empty() {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err(AppError::CommandFailed)
                };
            }
            match available.iter().position(|byte| *byte == b'\n') {
                Some(index) => {
                    line.extend_from_slice(&available[..=index]);
                    (index + 1, true)
                }
                None => {
                    line.extend_from_slice(available);
                    (available.len(), false)
                }
            }
        };
        reader.consume(take);
        if line.len() > payload_limit.saturating_add(3) {
            return Err(AppError::CommandFailed);
        }
        if complete {
            return line
                .ends_with(b"\r\n")
                .then_some(Some(line))
                .ok_or(AppError::CommandFailed);
        }
    }
}
