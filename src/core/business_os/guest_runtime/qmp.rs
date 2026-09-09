// Origin: CTOX
// License: AGPL-3.0-only

//! Bounded local QEMU monitor adapter. The native lifecycle owner must select
//! the private endpoint and authorize each effect. This module grants no VM
//! ownership, starts no VM, and registers no model-facing tool or transport.
//!
//! Protocol: https://www.qemu.org/docs/master/interop/qmp-spec.html

use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufStream};

const MAX_FRAME_BYTES: usize = 65_536;
const MAX_SKIPPED_FRAMES: usize = 64;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum QmpError {
    Unavailable,
    InvalidGreeting,
    ConnectionUnusable,
    /// Bytes may have reached QEMU. Never retry the effect automatically.
    UnknownOutcome,
    /// An explicit correlated QEMU error; does not promise absence of effects.
    CommandFailed,
}

impl fmt::Display for QmpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "local QEMU monitor is unavailable",
            Self::InvalidGreeting => "QEMU monitor greeting is invalid",
            Self::ConnectionUnusable => "QEMU monitor connection must be replaced",
            Self::UnknownOutcome => "QEMU command outcome is unknown; do not repeat automatically",
            Self::CommandFailed => "QEMU reported a command failure",
        })
    }
}
impl std::error::Error for QmpError {}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(super) struct QemuStatus {
    pub running: bool,
    /// Kept as a status string so future QEMU states are not called 'stopped'.
    pub status: String,
}

pub(super) struct QmpClient<S> {
    stream: BufStream<S>,
    next_id: u64,
    usable: bool,
    timeout: Duration,
}

#[cfg(unix)]
impl QmpClient<tokio::net::UnixStream> {
    /// The native owner provides a private socket for its already-owned child.
    /// No hostname, URL, port or caller-supplied actor is accepted.
    pub(super) async fn connect_local(path: &std::path::Path) -> Result<Self, QmpError> {
        if !path.is_absolute() {
            return Err(QmpError::Unavailable);
        }
        let stream = tokio::time::timeout(OPERATION_TIMEOUT, tokio::net::UnixStream::connect(path))
            .await
            .map_err(|_| QmpError::Unavailable)?
            .map_err(|_| QmpError::Unavailable)?;
        Self::negotiate(stream, OPERATION_TIMEOUT).await
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> QmpClient<S> {
    pub(super) async fn negotiate(stream: S, timeout: Duration) -> Result<Self, QmpError> {
        let mut client = Self {
            stream: BufStream::new(stream),
            next_id: 1,
            usable: true,
            timeout,
        };
        let greeting = tokio::time::timeout(timeout, client.read_frame())
            .await
            .map_err(|_| QmpError::InvalidGreeting)?
            .map_err(|_| QmpError::InvalidGreeting)?;
        let valid = greeting.get("QMP").is_some_and(|qmp| {
            qmp.get("version").is_some_and(Value::is_object)
                && qmp.get("capabilities").is_some_and(Value::is_array)
        });
        if !valid {
            return Err(QmpError::InvalidGreeting);
        }
        client.acknowledge("qmp_capabilities").await?;
        Ok(client)
    }

    pub(super) async fn query_status(&mut self) -> Result<QemuStatus, QmpError> {
        let reply = self.command("query-status").await?;
        let decoded: Result<QemuStatus, _> = serde_json::from_value(reply);
        match decoded {
            Ok(status)
                if !status.status.is_empty()
                    && status.status.len() <= 64
                    && !status.status.chars().any(char::is_control) =>
            {
                Ok(status)
            }
            _ => {
                self.usable = false;
                Err(QmpError::UnknownOutcome)
            }
        }
    }

    pub(super) async fn pause(&mut self) -> Result<(), QmpError> {
        self.acknowledge("stop").await
    }

    pub(super) async fn resume(&mut self) -> Result<(), QmpError> {
        self.acknowledge("cont").await
    }

    /// This only acknowledges the powerdown request. The lifecycle owner must
    /// observe process exit; an unresponsive guest may not shut down.
    pub(super) async fn request_powerdown(&mut self) -> Result<(), QmpError> {
        self.acknowledge("system_powerdown").await
    }

    /// The caller still owns waiting/reaping its captured child PID.
    pub(super) async fn quit(&mut self) -> Result<(), QmpError> {
        let result = self.acknowledge("quit").await;
        self.usable = false;
        result
    }

    async fn acknowledge(&mut self, operation: &'static str) -> Result<(), QmpError> {
        if self.command(operation).await?.is_object() {
            Ok(())
        } else {
            self.usable = false;
            Err(QmpError::UnknownOutcome)
        }
    }

    async fn command(&mut self, operation: &'static str) -> Result<Value, QmpError> {
        if !self.usable {
            return Err(QmpError::ConnectionUnusable);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(QmpError::ConnectionUnusable)?;
        // Poison BEFORE the first await. If this future is dropped after a
        // partial write/read, a later call cannot reuse the uncertain stream.
        self.usable = false;
        let result = tokio::time::timeout(self.timeout, self.exchange(operation, id)).await;
        match result {
            Ok(Ok(value)) => {
                self.usable = true;
                Ok(value)
            }
            Ok(Err(QmpError::CommandFailed)) => {
                self.usable = true;
                Err(QmpError::CommandFailed)
            }
            _ => Err(QmpError::UnknownOutcome),
        }
    }

    async fn exchange(&mut self, operation: &'static str, id: u64) -> Result<Value, QmpError> {
        let mut request = serde_json::to_vec(&json!({"execute": operation, "id": id}))
            .map_err(|_| QmpError::UnknownOutcome)?;
        request.extend_from_slice(b"\r\n");
        self.stream
            .write_all(&request)
            .await
            .map_err(|_| QmpError::UnknownOutcome)?;
        self.stream
            .flush()
            .await
            .map_err(|_| QmpError::UnknownOutcome)?;
        for _ in 0..=MAX_SKIPPED_FRAMES {
            let frame = self.read_frame().await?;
            if frame.get("event").is_some() {
                // Event payloads are not published as authoritative lifecycle
                // state. The caller requests fresh query-status observations.
                if frame.get("id").is_some() || !frame["event"].is_string() {
                    return Err(QmpError::UnknownOutcome);
                }
                continue;
            }
            if frame.get("id").and_then(Value::as_u64) != Some(id) {
                // QMP specifies dropping unrelated replies. The total number,
                // byte length and elapsed time remain bounded.
                continue;
            }
            match (frame.get("return"), frame.get("error")) {
                (Some(value), None) => return Ok(value.clone()),
                (None, Some(error)) if error.is_object() => return Err(QmpError::CommandFailed),
                _ => return Err(QmpError::UnknownOutcome),
            }
        }
        Err(QmpError::UnknownOutcome)
    }

    async fn read_frame(&mut self) -> Result<Value, QmpError> {
        let mut bytes = Vec::new();
        loop {
            let buffer = self
                .stream
                .fill_buf()
                .await
                .map_err(|_| QmpError::UnknownOutcome)?;
            if buffer.is_empty() {
                return Err(QmpError::UnknownOutcome);
            }
            let newline = buffer.iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(buffer.len(), |position| position + 1);
            if bytes.len() + count > MAX_FRAME_BYTES {
                return Err(QmpError::UnknownOutcome);
            }
            bytes.extend_from_slice(&buffer[..count]);
            self.stream.consume(count);
            if newline.is_some() {
                let frame: Value =
                    serde_json::from_slice(&bytes).map_err(|_| QmpError::UnknownOutcome)?;
                return if frame.is_object() {
                    Ok(frame)
                } else {
                    Err(QmpError::UnknownOutcome)
                };
            }
        }
    }
}

#[cfg(test)]
mod tests;
