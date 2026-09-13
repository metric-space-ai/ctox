// Origin: CTOX
// License: AGPL-3.0-only

//! Bounded host-to-guest desktop effects over an owned QEMU virtio-serial
//! chardev. This module is not an authorization owner, provisioner, browser
//! data path, or VM capability advertisement.
//!
//! Host ownership matches the existing QMP socket: bind a private unix socket
//! first, then QEMU connects with `server=off`. That inverts the qemu-ga
//! example, which has QEMU listen (`server=on,wait=off`), so this process can
//! refuse any peer other than the owned child.
//!
//! Guest device shape follows qemu-ga: virtio-serial + virtserialport named
//! `org.ctox.guest.desktop`, appearing as `/dev/virtio-ports/org.ctox.guest.desktop`.
//! https://www.qemu.org/docs/master/interop/qemu-ga.html
//!
//! Framing reuses the local IPC convention (u32be length prefix) rather than
//! QMP newlines, because an observation carries a raw PNG. Guest replies are
//! untrusted and bounded. One in-flight request; no automatic replay.

use super::{identifier, GuestDriver, GuestFrame, GuestInput, GUEST_FRAME_LIMIT};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Port name exposed inside the guest as `/dev/virtio-ports/<name>`.
pub(super) const GUEST_DESKTOP_PORT: &str = "org.ctox.guest.desktop";
const GUEST_VIRTIO_PORT_PATH: &str = "/dev/virtio-ports/org.ctox.guest.desktop";
const MAX_CONTROL_BYTES: usize = 65_536;
const CHANNEL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum GuestChannelError {
    Unavailable,
    ConnectionUnusable,
    /// Bytes may have reached the guest. Never retry the effect automatically.
    UnknownOutcome,
    /// Explicit correlated failure; does not leak guest detail.
    Failed,
}

impl fmt::Display for GuestChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "guest desktop channel is unavailable",
            Self::ConnectionUnusable => "guest desktop channel must be replaced",
            Self::UnknownOutcome => "guest desktop outcome is unknown; do not repeat automatically",
            Self::Failed => "guest desktop effect failed",
        })
    }
}
impl std::error::Error for GuestChannelError {}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireRequest {
    Observe { id: u64 },
    Input { id: u64, input: GuestInput },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireReply {
    Observation { id: u64, width: u32, height: u32 },
    Applied { id: u64 },
    Failed { id: u64 },
}

pub(super) struct GuestChannel<S> {
    stream: S,
    next_id: u64,
    usable: bool,
}

impl<S: AsyncRead + AsyncWrite + Unpin> GuestChannel<S> {
    pub(super) fn new(stream: S) -> Self {
        Self {
            stream,
            next_id: 1,
            usable: true,
        }
    }

    pub(super) async fn observe(&mut self) -> Result<GuestFrame, GuestChannelError> {
        let id = self.begin()?;
        let result = tokio::time::timeout(CHANNEL_TIMEOUT, self.exchange_observe(id)).await;
        self.finish(result)
    }

    pub(super) async fn apply_input(
        &mut self,
        input: &GuestInput,
    ) -> Result<(), GuestChannelError> {
        if input.validate().is_err() {
            return Err(GuestChannelError::Failed);
        }
        let id = self.begin()?;
        let result = tokio::time::timeout(CHANNEL_TIMEOUT, self.exchange_input(id, input)).await;
        self.finish(result)
    }

    fn begin(&mut self) -> Result<u64, GuestChannelError> {
        if !self.usable {
            return Err(GuestChannelError::ConnectionUnusable);
        }
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or(GuestChannelError::ConnectionUnusable)?;
        // Poison BEFORE the first await. Drop after a possible write cannot
        // reuse this stream or replay the effect.
        self.usable = false;
        Ok(id)
    }

    fn finish<T>(
        &mut self,
        result: Result<Result<T, GuestChannelError>, tokio::time::error::Elapsed>,
    ) -> Result<T, GuestChannelError> {
        match result {
            Ok(Ok(value)) => {
                self.usable = true;
                Ok(value)
            }
            Ok(Err(GuestChannelError::Failed)) => {
                self.usable = true;
                Err(GuestChannelError::Failed)
            }
            _ => Err(GuestChannelError::UnknownOutcome),
        }
    }

    async fn exchange_observe(&mut self, id: u64) -> Result<GuestFrame, GuestChannelError> {
        write_json(
            &mut self.stream,
            &WireRequest::Observe { id },
            MAX_CONTROL_BYTES,
        )
        .await?;
        match read_json::<_, WireReply>(&mut self.stream, MAX_CONTROL_BYTES).await? {
            WireReply::Observation {
                id: reply_id,
                width,
                height,
            } if reply_id == id => {
                let png = read_frame(&mut self.stream, GUEST_FRAME_LIMIT).await?;
                let frame =
                    GuestFrame::from_png(png).map_err(|_| GuestChannelError::UnknownOutcome)?;
                if frame.width != width || frame.height != height {
                    return Err(GuestChannelError::UnknownOutcome);
                }
                Ok(frame)
            }
            WireReply::Failed { id: reply_id } if reply_id == id => Err(GuestChannelError::Failed),
            _ => Err(GuestChannelError::UnknownOutcome),
        }
    }

    async fn exchange_input(
        &mut self,
        id: u64,
        input: &GuestInput,
    ) -> Result<(), GuestChannelError> {
        write_json(
            &mut self.stream,
            &WireRequest::Input {
                id,
                input: input.clone(),
            },
            MAX_CONTROL_BYTES,
        )
        .await?;
        match read_json::<_, WireReply>(&mut self.stream, MAX_CONTROL_BYTES).await? {
            WireReply::Applied { id: reply_id } if reply_id == id => Ok(()),
            WireReply::Failed { id: reply_id } if reply_id == id => Err(GuestChannelError::Failed),
            _ => Err(GuestChannelError::UnknownOutcome),
        }
    }
}

/// GuestDriver over an already-owned channel. Identity is bound by the native
/// owner; it is never taken from a guest reply or tool payload.
pub(super) struct RemoteGuestDriver<S> {
    guest_id: String,
    channel: tokio::sync::Mutex<GuestChannel<S>>,
}

impl<S> RemoteGuestDriver<S> {
    pub(super) fn bind(guest_id: String, channel: GuestChannel<S>) -> Result<Self> {
        ensure!(identifier(&guest_id), "guest identity is invalid");
        Ok(Self {
            guest_id,
            channel: tokio::sync::Mutex::new(channel),
        })
    }

    pub(super) fn guest_id(&self) -> &str {
        &self.guest_id
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send> GuestDriver for RemoteGuestDriver<S> {
    fn guest_id(&self) -> &str {
        &self.guest_id
    }

    async fn capture(&self) -> Result<GuestFrame> {
        let mut channel = self
            .channel
            .try_lock()
            .map_err(|_| anyhow::anyhow!("guest channel is busy"))?;
        channel.observe().await.map_err(anyhow::Error::from)
    }

    async fn input(&self, input: &GuestInput) -> Result<()> {
        let mut channel = self
            .channel
            .try_lock()
            .map_err(|_| anyhow::anyhow!("guest channel is busy"))?;
        channel
            .apply_input(input)
            .await
            .map_err(anyhow::Error::from)
    }
}

/// Guest-side execution endpoint. Reads typed observe/input frames and calls
/// the local driver. No identity, shell, or extra arguments are accepted.
pub(super) async fn serve_guest_desktop<S, D>(
    mut stream: S,
    driver: &D,
) -> Result<(), GuestChannelError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    D: GuestDriver + Sync,
{
    loop {
        let bytes = match read_frame_or_eof(&mut stream, MAX_CONTROL_BYTES).await? {
            None => return Ok(()),
            Some(bytes) => bytes,
        };
        let request: WireRequest =
            serde_json::from_slice(&bytes).map_err(|_| GuestChannelError::UnknownOutcome)?;
        match request {
            WireRequest::Observe { id } => match driver.capture().await {
                Ok(frame) => {
                    write_json(
                        &mut stream,
                        &WireReply::Observation {
                            id,
                            width: frame.width,
                            height: frame.height,
                        },
                        MAX_CONTROL_BYTES,
                    )
                    .await?;
                    write_frame(&mut stream, &frame.png, GUEST_FRAME_LIMIT).await?;
                }
                Err(_) => {
                    write_json(&mut stream, &WireReply::Failed { id }, MAX_CONTROL_BYTES).await?;
                }
            },
            WireRequest::Input { id, input } => {
                let result = if input.validate().is_err() {
                    Err(())
                } else {
                    driver.input(&input).await.map_err(|_| ())
                };
                let reply = if result.is_ok() {
                    WireReply::Applied { id }
                } else {
                    WireReply::Failed { id }
                };
                write_json(&mut stream, &reply, MAX_CONTROL_BYTES).await?;
            }
        }
    }
}

/// Opens the fixed virtio-serial port inside a provisioned Linux guest.
#[cfg(target_os = "linux")]
pub(super) async fn serve_local_virtio_desktop<D: GuestDriver + Sync>(
    driver: &D,
) -> Result<(), GuestChannelError> {
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(GUEST_VIRTIO_PORT_PATH)
        .await
        .map_err(|_| GuestChannelError::Unavailable)?;
    serve_guest_desktop(file, driver).await
}

async fn write_json<S: AsyncWrite + Unpin, T: Serialize>(
    stream: &mut S,
    value: &T,
    max: usize,
) -> Result<(), GuestChannelError> {
    let bytes = serde_json::to_vec(value).map_err(|_| GuestChannelError::UnknownOutcome)?;
    write_frame(stream, &bytes, max).await
}

async fn read_json<S: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    stream: &mut S,
    max: usize,
) -> Result<T, GuestChannelError> {
    let bytes = read_frame(stream, max).await?;
    serde_json::from_slice(&bytes).map_err(|_| GuestChannelError::UnknownOutcome)
}

async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    bytes: &[u8],
    max: usize,
) -> Result<(), GuestChannelError> {
    if bytes.is_empty() || bytes.len() > max {
        return Err(GuestChannelError::UnknownOutcome);
    }
    let len = u32::try_from(bytes.len()).map_err(|_| GuestChannelError::UnknownOutcome)?;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|_| GuestChannelError::UnknownOutcome)?;
    stream
        .write_all(bytes)
        .await
        .map_err(|_| GuestChannelError::UnknownOutcome)?;
    stream
        .flush()
        .await
        .map_err(|_| GuestChannelError::UnknownOutcome)
}

async fn read_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    max: usize,
) -> Result<Vec<u8>, GuestChannelError> {
    match read_frame_or_eof(stream, max).await? {
        Some(bytes) => Ok(bytes),
        None => Err(GuestChannelError::UnknownOutcome),
    }
}

async fn read_frame_or_eof<S: AsyncRead + Unpin>(
    stream: &mut S,
    max: usize,
) -> Result<Option<Vec<u8>>, GuestChannelError> {
    let mut header = [0_u8; 4];
    match stream.read(&mut header[..1]).await {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(_) => return Err(GuestChannelError::UnknownOutcome),
    }
    if stream.read_exact(&mut header[1..]).await.is_err() {
        return Err(GuestChannelError::UnknownOutcome);
    }
    let size = u32::from_be_bytes(header) as usize;
    if size == 0 || size > max {
        return Err(GuestChannelError::UnknownOutcome);
    }
    let mut bytes = vec![0; size];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| GuestChannelError::UnknownOutcome)?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests;
