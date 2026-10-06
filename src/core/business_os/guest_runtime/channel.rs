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
//! systemd udev creates that path as `SYMLINK+="virtio-ports/$attr{name}"` to
//! `/dev/vport<device>p<port>`. The guest hook follows only that named alias after
//! pinning the sysfs port name and the opened character-device `rdev` against the
//! sysfs `dev` major:minor. It does not open an arbitrary character device.
//! https://www.qemu.org/docs/master/interop/qemu-ga.html
//!
//! Framing reuses the local IPC convention (u32be length prefix) rather than
//! QMP newlines, because an observation carries a raw PNG. Guest replies are
//! untrusted and bounded. One in-flight request; no automatic replay.
//!
//! Guest-only hook: `run_guest_desktop_effects` opens the fixed character
//! device and serves typed observe/input. The host QEMU owner accepts the
//! chardev. Authorization and guest-process provisioning remain outside this
//! module.

use super::{identifier, GuestDriver, GuestFrame, GuestInput, GUEST_FRAME_LIMIT};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
#[cfg(unix)]
use std::path::{Component, Path, PathBuf};
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
    Probe {
        id: u64,
    },
    Observe {
        id: u64,
    },
    Input {
        id: u64,
        input: GuestInput,
    },
    ObserveSession {
        id: u64,
        session_id: String,
    },
    InputSession {
        id: u64,
        session_id: String,
        input: GuestInput,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireReply {
    Endpoint {
        id: u64,
        guest_id: String,
        session_id: String,
    },
    Observation {
        id: u64,
        width: u32,
        height: u32,
    },
    Applied {
        id: u64,
    },
    Failed {
        id: u64,
    },
}

pub(super) struct GuestChannel<S> {
    stream: S,
    next_id: u64,
    usable: bool,
    session: Option<GuestEndpointSession>,
}

/// Correlated to this owned channel; neither a controller nor an input permit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GuestEndpointSession {
    pub(super) guest_id: String,
    pub(super) session_id: String,
}

impl<S: AsyncRead + AsyncWrite + Unpin> GuestChannel<S> {
    pub(super) fn new(stream: S) -> Self {
        Self {
            stream,
            next_id: 1,
            usable: true,
            session: None,
        }
    }

    pub(super) async fn probe_endpoint(
        &mut self,
        expected_guest: &str,
    ) -> Result<GuestEndpointSession, GuestChannelError> {
        if !identifier(expected_guest) {
            return Err(GuestChannelError::Failed);
        }
        let id = self.begin()?;
        let result = tokio::time::timeout(CHANNEL_TIMEOUT, async {
            write_json(
                &mut self.stream,
                &WireRequest::Probe { id },
                MAX_CONTROL_BYTES,
            )
            .await?;
            match read_json::<_, WireReply>(&mut self.stream, MAX_CONTROL_BYTES).await? {
                WireReply::Endpoint {
                    id: reply_id,
                    guest_id,
                    session_id,
                } if reply_id == id
                    && guest_id == expected_guest
                    && uuid::Uuid::parse_str(&session_id)
                        .is_ok_and(|uuid| uuid.to_string() == session_id) =>
                {
                    Ok(GuestEndpointSession {
                        guest_id,
                        session_id,
                    })
                }
                _ => Err(GuestChannelError::UnknownOutcome),
            }
        })
        .await;
        let session = self.finish(result)?;
        if self.session.as_ref().is_some_and(|old| old != &session) {
            self.usable = false;
            return Err(GuestChannelError::UnknownOutcome);
        }
        self.session = Some(session.clone());
        Ok(session)
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
        let request = match &self.session {
            Some(session) => WireRequest::ObserveSession {
                id,
                session_id: session.session_id.clone(),
            },
            None => WireRequest::Observe { id },
        };
        write_json(&mut self.stream, &request, MAX_CONTROL_BYTES).await?;
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
        let request = match &self.session {
            Some(session) => WireRequest::InputSession {
                id,
                session_id: session.session_id.clone(),
                input: input.clone(),
            },
            None => WireRequest::Input {
                id,
                input: input.clone(),
            },
        };
        write_json(&mut self.stream, &request, MAX_CONTROL_BYTES).await?;
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
#[cfg(unix)]
impl RemoteGuestDriver<tokio::net::UnixStream> {
    /// Only a retained host channel can validate this pinned endpoint. No IO
    /// future or guest-channel lock escapes the current physical-poll fence.
    pub(super) fn ensure_current_endpoint(&self, expected_session: &str) -> Result<()> {
        use std::os::fd::AsRawFd;
        let channel = self
            .channel
            .try_lock()
            .map_err(|_| anyhow::anyhow!("guest endpoint is busy"))?;
        ensure!(
            channel.usable
                && channel
                    .session
                    .as_ref()
                    .is_some_and(|session| session.guest_id == self.guest_id
                        && session.session_id == expected_session),
            "guest endpoint session retired or replaced"
        );
        let mut byte = 0u8;
        // No bytes are consumed. EOF, unexpected pending replies, and syscall
        // errors retire the send; EAGAIN means the same socket is still open.
        let read = unsafe {
            libc::recv(
                channel.stream.as_raw_fd(),
                (&mut byte as *mut u8).cast(),
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        ensure!(
            read < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock,
            "guest endpoint closed or has an unresolved reply"
        );
        Ok(())
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

impl<S: AsyncRead + AsyncWrite + Unpin + Send> RemoteGuestDriver<S> {
    pub(super) async fn probe_endpoint(&self) -> Result<GuestEndpointSession, GuestChannelError> {
        self.channel
            .try_lock()
            .map_err(|_| GuestChannelError::Unavailable)?
            .probe_endpoint(&self.guest_id)
            .await
    }
}

/// Guest-side execution endpoint. Reads typed observe/input frames and calls
/// the local driver. No identity, shell, or extra arguments are accepted.
pub(super) async fn serve_guest_desktop<S, D>(
    stream: S,
    driver: &D,
) -> Result<(), GuestChannelError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    D: GuestDriver + Sync,
{
    serve_guest_desktop_timed(stream, driver, CHANNEL_TIMEOUT).await
}

async fn serve_guest_desktop_timed<S, D>(
    mut stream: S,
    driver: &D,
    frame_timeout: Duration,
) -> Result<(), GuestChannelError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    D: GuestDriver + Sync,
{
    if !identifier(driver.guest_id()) {
        return Err(GuestChannelError::Unavailable);
    }
    // A restarted endpoint receives a new identity even in the same QEMU.
    let session_id = uuid::Uuid::new_v4().to_string();
    loop {
        let bytes = match read_idle_frame(&mut stream, MAX_CONTROL_BYTES, frame_timeout).await? {
            None => return Ok(()),
            Some(bytes) => bytes,
        };
        let request: WireRequest =
            serde_json::from_slice(&bytes).map_err(|_| GuestChannelError::UnknownOutcome)?;
        let request = match request {
            WireRequest::Probe { id } => {
                with_deadline(
                    frame_timeout,
                    write_json(
                        &mut stream,
                        &WireReply::Endpoint {
                            id,
                            guest_id: driver.guest_id().into(),
                            session_id: session_id.clone(),
                        },
                        MAX_CONTROL_BYTES,
                    ),
                )
                .await?;
                continue;
            }
            WireRequest::ObserveSession {
                id,
                session_id: expected,
            } if expected == session_id => WireRequest::Observe { id },
            WireRequest::InputSession {
                id,
                session_id: expected,
                input,
            } if expected == session_id => WireRequest::Input { id, input },
            WireRequest::ObserveSession { id, .. } | WireRequest::InputSession { id, .. } => {
                with_deadline(
                    frame_timeout,
                    write_json(&mut stream, &WireReply::Failed { id }, MAX_CONTROL_BYTES),
                )
                .await?;
                continue;
            }
            legacy => legacy,
        };
        match request {
            WireRequest::Observe { id } => match driver.capture().await {
                Ok(frame) => {
                    with_deadline(frame_timeout, async {
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
                        write_frame(&mut stream, &frame.png, GUEST_FRAME_LIMIT).await
                    })
                    .await?;
                }
                Err(_) => {
                    with_deadline(
                        frame_timeout,
                        write_json(&mut stream, &WireReply::Failed { id }, MAX_CONTROL_BYTES),
                    )
                    .await?;
                }
            },
            WireRequest::Input { id, input } => {
                if input.validate().is_err() {
                    with_deadline(
                        frame_timeout,
                        write_json(&mut stream, &WireReply::Failed { id }, MAX_CONTROL_BYTES),
                    )
                    .await?;
                    continue;
                }
                match driver.input(&input).await {
                    Ok(()) => {
                        with_deadline(
                            frame_timeout,
                            write_json(&mut stream, &WireReply::Applied { id }, MAX_CONTROL_BYTES),
                        )
                        .await?;
                    }
                    // The helper may have applied a partial effect. Do not send
                    // Failed (that would re-enable the host) and do not replay.
                    Err(_) => return Err(GuestChannelError::UnknownOutcome),
                }
            }
            _ => return Err(GuestChannelError::UnknownOutcome),
        }
    }
}

/// Guest-only typed startup hook. Opens the fixed virtio-serial character
/// device and serves observe/input. This does not start QEMU, provision an
/// image, or authorize a caller; the guest process owner invokes it after the
/// port exists.
#[cfg(target_os = "linux")]
pub(in crate::business_os) async fn run_guest_desktop_effects<D: GuestDriver + Sync>(
    driver: &D,
) -> Result<(), GuestChannelError> {
    let port = open_guest_virtio_port()?;
    serve_guest_desktop(port, driver).await
}

#[cfg(unix)]
struct NonblockingIo<T: std::os::fd::AsRawFd> {
    inner: tokio::io::unix::AsyncFd<T>,
}

#[cfg(unix)]
impl<T: std::os::fd::AsRawFd> NonblockingIo<T> {
    fn new(io: T) -> std::io::Result<Self> {
        Ok(Self {
            inner: tokio::io::unix::AsyncFd::new(io)?,
        })
    }
}

#[cfg(unix)]
impl<T> tokio::io::AsyncRead for NonblockingIo<T>
where
    T: std::os::fd::AsRawFd + Unpin,
    for<'a> &'a T: std::io::Read,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let inner = &self.get_mut().inner;
        loop {
            let mut guard = match inner.poll_read_ready(cx) {
                std::task::Poll::Ready(Ok(guard)) => guard,
                std::task::Poll::Ready(Err(error)) => return std::task::Poll::Ready(Err(error)),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            };
            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|io| {
                let mut reader = io.get_ref();
                std::io::Read::read(&mut reader, unfilled)
            }) {
                Ok(Ok(0)) => return std::task::Poll::Ready(Ok(())),
                Ok(Ok(count)) => {
                    buf.advance(count);
                    return std::task::Poll::Ready(Ok(()));
                }
                Ok(Err(error)) => return std::task::Poll::Ready(Err(error)),
                Err(_would_block) => continue,
            }
        }
    }
}

#[cfg(unix)]
impl<T> tokio::io::AsyncWrite for NonblockingIo<T>
where
    T: std::os::fd::AsRawFd + Unpin,
    for<'a> &'a T: std::io::Write,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let inner = &self.get_mut().inner;
        loop {
            let mut guard = match inner.poll_write_ready(cx) {
                std::task::Poll::Ready(Ok(guard)) => guard,
                std::task::Poll::Ready(Err(error)) => return std::task::Poll::Ready(Err(error)),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            };
            match guard.try_io(|io| {
                let mut writer = io.get_ref();
                std::io::Write::write(&mut writer, buf)
            }) {
                Ok(result) => return std::task::Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let inner = &self.get_mut().inner;
        loop {
            let mut guard = match inner.poll_write_ready(cx) {
                std::task::Poll::Ready(Ok(guard)) => guard,
                std::task::Poll::Ready(Err(error)) => return std::task::Poll::Ready(Err(error)),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            };
            match guard.try_io(|io| {
                let mut writer = io.get_ref();
                std::io::Write::flush(&mut writer)
            }) {
                Ok(result) => return std::task::Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(unix)]
fn named_virtio_symlink_target(port_path: &Path) -> Result<PathBuf, GuestChannelError> {
    let file_name = port_path
        .file_name()
        .ok_or(GuestChannelError::Unavailable)?;
    if file_name != std::ffi::OsStr::new(GUEST_DESKTOP_PORT) {
        return Err(GuestChannelError::Unavailable);
    }
    let ports_dir = port_path.parent().ok_or(GuestChannelError::Unavailable)?;
    if ports_dir.file_name() != Some(std::ffi::OsStr::new("virtio-ports")) {
        return Err(GuestChannelError::Unavailable);
    }
    let device_dir = ports_dir.parent().ok_or(GuestChannelError::Unavailable)?;
    let metadata =
        std::fs::symlink_metadata(port_path).map_err(|_| GuestChannelError::Unavailable)?;
    if !metadata.file_type().is_symlink() {
        return Err(GuestChannelError::Unavailable);
    }
    let raw = std::fs::read_link(port_path).map_err(|_| GuestChannelError::Unavailable)?;
    let target = lexical_join(ports_dir, &raw).ok_or(GuestChannelError::Unavailable)?;
    if target.parent() != Some(device_dir) {
        return Err(GuestChannelError::Unavailable);
    }
    let basename = target.file_name().ok_or(GuestChannelError::Unavailable)?;
    if vport_device_name(basename).is_none() {
        return Err(GuestChannelError::Unavailable);
    }
    let target_metadata =
        std::fs::symlink_metadata(&target).map_err(|_| GuestChannelError::Unavailable)?;
    if target_metadata.file_type().is_symlink() {
        return Err(GuestChannelError::Unavailable);
    }
    Ok(target)
}

#[cfg(unix)]
fn pin_virtio_sysfs_name(
    sysfs_class: &Path,
    vport: &std::ffi::OsStr,
) -> Result<(), GuestChannelError> {
    let name_path = sysfs_class.join(vport).join("name");
    let name_metadata =
        std::fs::symlink_metadata(&name_path).map_err(|_| GuestChannelError::Unavailable)?;
    if name_metadata.file_type().is_symlink() || !name_metadata.is_file() {
        return Err(GuestChannelError::Unavailable);
    }
    let name = std::fs::read_to_string(&name_path).map_err(|_| GuestChannelError::Unavailable)?;
    if name.trim() != GUEST_DESKTOP_PORT {
        return Err(GuestChannelError::Unavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn parse_sysfs_device_id(contents: &str) -> Result<(u32, u32), GuestChannelError> {
    let line = contents.trim();
    let (major, minor) = line.split_once(':').ok_or(GuestChannelError::Unavailable)?;
    if major.is_empty()
        || minor.is_empty()
        || line.bytes().filter(|byte| *byte == b':').count() != 1
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(GuestChannelError::Unavailable);
    }
    Ok((
        major.parse().map_err(|_| GuestChannelError::Unavailable)?,
        minor.parse().map_err(|_| GuestChannelError::Unavailable)?,
    ))
}

#[cfg(unix)]
fn device_id_from_rdev(rdev: u64) -> (u32, u32) {
    (
        libc::major(rdev as libc::dev_t) as u32,
        libc::minor(rdev as libc::dev_t) as u32,
    )
}

#[cfg(unix)]
fn pin_opened_device_id(rdev: u64, expected: (u32, u32)) -> Result<(), GuestChannelError> {
    if device_id_from_rdev(rdev) != expected {
        return Err(GuestChannelError::Unavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn read_sysfs_device_id(
    sysfs_class: &Path,
    vport: &std::ffi::OsStr,
) -> Result<(u32, u32), GuestChannelError> {
    let dev_path = sysfs_class.join(vport).join("dev");
    let metadata =
        std::fs::symlink_metadata(&dev_path).map_err(|_| GuestChannelError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(GuestChannelError::Unavailable);
    }
    let contents =
        std::fs::read_to_string(&dev_path).map_err(|_| GuestChannelError::Unavailable)?;
    parse_sysfs_device_id(&contents)
}

#[cfg(unix)]
fn open_nonblocking_char_device(
    path: &Path,
    expected: (u32, u32),
) -> Result<NonblockingIo<std::fs::File>, GuestChannelError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};

    let metadata = std::fs::symlink_metadata(path).map_err(|_| GuestChannelError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_char_device() {
        return Err(GuestChannelError::Unavailable);
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| GuestChannelError::Unavailable)?;
    let opened = file
        .metadata()
        .map_err(|_| GuestChannelError::Unavailable)?;
    if !opened.file_type().is_char_device() {
        return Err(GuestChannelError::Unavailable);
    }
    pin_opened_device_id(opened.rdev(), expected)?;
    NonblockingIo::new(file).map_err(|_| GuestChannelError::Unavailable)
}

#[cfg(unix)]
fn open_named_virtio_port(
    port_path: &Path,
    sysfs_class: &Path,
) -> Result<NonblockingIo<std::fs::File>, GuestChannelError> {
    let target = named_virtio_symlink_target(port_path)?;
    let basename = target.file_name().ok_or(GuestChannelError::Unavailable)?;
    pin_virtio_sysfs_name(sysfs_class, basename)?;
    let expected = read_sysfs_device_id(sysfs_class, basename)?;
    open_nonblocking_char_device(&target, expected)
}

#[cfg(unix)]
fn lexical_join(base: &Path, rel: &Path) -> Option<PathBuf> {
    if rel.as_os_str().is_empty() {
        return None;
    }
    let path = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        base.join(rel)
    };
    lexical_normalize(&path)
}

#[cfg(unix)]
fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => return None,
            Component::RootDir => out.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(_) => out.push(component),
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(unix)]
fn vport_device_name(name: &std::ffi::OsStr) -> Option<&str> {
    let name = name.to_str()?;
    let rest = name.strip_prefix("vport")?;
    let (device, port) = rest.split_once('p')?;
    if !device.is_empty()
        && device.bytes().all(|b| b.is_ascii_digit())
        && !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
    {
        Some(name)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn open_guest_virtio_port() -> Result<NonblockingIo<std::fs::File>, GuestChannelError> {
    open_named_virtio_port(
        Path::new(GUEST_VIRTIO_PORT_PATH),
        Path::new("/sys/class/virtio-ports"),
    )
}

async fn with_deadline<T>(
    timeout: Duration,
    future: impl Future<Output = Result<T, GuestChannelError>>,
) -> Result<T, GuestChannelError> {
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| GuestChannelError::UnknownOutcome)?
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
    match read_idle_frame(stream, max, CHANNEL_TIMEOUT).await? {
        Some(bytes) => Ok(bytes),
        None => Err(GuestChannelError::UnknownOutcome),
    }
}

/// Idle before the first byte may wait. After a frame starts, the remainder
/// and any later write are deadline-bound.
async fn read_idle_frame<S: AsyncRead + Unpin>(
    stream: &mut S,
    max: usize,
    started_timeout: Duration,
) -> Result<Option<Vec<u8>>, GuestChannelError> {
    let mut header = [0_u8; 4];
    match stream.read(&mut header[..1]).await {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(_) => return Err(GuestChannelError::UnknownOutcome),
    }
    with_deadline(started_timeout, async {
        stream
            .read_exact(&mut header[1..])
            .await
            .map_err(|_| GuestChannelError::UnknownOutcome)?;
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
    })
    .await
}

#[cfg(test)]
#[path = "channel/endpoint_tests.rs"]
mod endpoint_tests;
#[cfg(test)]
mod tests;
