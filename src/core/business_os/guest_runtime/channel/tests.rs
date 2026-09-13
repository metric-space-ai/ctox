// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use anyhow::{ensure, Result};
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(unix)]
use std::task::Poll;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

const TINY_PNG: [u8; 69] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 96, 96, 96, 0, 0, 0, 4, 0,
    1, 246, 23, 56, 85, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn click() -> GuestInput {
    GuestInput::Click {
        x: 2,
        y: 3,
        button: super::super::MouseButton::Left,
    }
}

struct RecordingDriver {
    captures: AtomicUsize,
    inputs: AtomicUsize,
    fail_capture: AtomicBool,
    fail_after_input: AtomicBool,
}

impl RecordingDriver {
    fn new() -> Self {
        Self {
            captures: AtomicUsize::new(0),
            inputs: AtomicUsize::new(0),
            fail_capture: AtomicBool::new(false),
            fail_after_input: AtomicBool::new(false),
        }
    }
}

impl GuestDriver for RecordingDriver {
    fn guest_id(&self) -> &str {
        "guest-a"
    }

    async fn capture(&self) -> Result<GuestFrame> {
        self.captures.fetch_add(1, Ordering::SeqCst);
        if self.fail_capture.load(Ordering::SeqCst) {
            anyhow::bail!("capture denied");
        }
        GuestFrame::from_png(TINY_PNG.to_vec())
    }

    async fn input(&self, input: &GuestInput) -> Result<()> {
        self.inputs.fetch_add(1, Ordering::SeqCst);
        if self.fail_after_input.load(Ordering::SeqCst) {
            anyhow::bail!("partial input");
        }
        input.validate()
    }
}

async fn write_raw(stream: &mut DuplexStream, bytes: &[u8]) -> Result<()> {
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_raw(stream: &mut DuplexStream) -> Result<Vec<u8>> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header).await?;
    let size = u32::from_be_bytes(header) as usize;
    ensure!(size > 0 && size <= MAX_CONTROL_BYTES, "test frame size");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

#[tokio::test]
async fn observe_and_input_roundtrip_through_guest_endpoint() -> Result<()> {
    let (host_stream, guest_stream) = tokio::io::duplex(4096);
    let driver = Arc::new(RecordingDriver::new());
    let endpoint = driver.clone();
    let (host, guest) = tokio::join!(
        async move {
            let mut channel = GuestChannel::new(host_stream);
            let frame = channel.observe().await?;
            ensure!(frame.width == 1 && frame.height == 1 && frame.png == TINY_PNG);
            channel.apply_input(&click()).await?;
            Ok::<_, anyhow::Error>(())
        },
        async move { serve_guest_desktop(guest_stream, endpoint.as_ref()).await }
    );
    host?;
    ensure!(
        matches!(guest, Ok(()) | Err(GuestChannelError::UnknownOutcome)),
        "endpoint exit"
    );
    ensure!(driver.captures.load(Ordering::SeqCst) == 1, "captures");
    ensure!(driver.inputs.load(Ordering::SeqCst) == 1, "inputs");
    Ok(())
}

#[tokio::test]
async fn remote_driver_is_a_guest_driver_over_the_owned_channel() -> Result<()> {
    let (host_stream, guest_stream) = tokio::io::duplex(4096);
    let endpoint = RecordingDriver::new();
    let (host, guest) = tokio::join!(
        async move {
            let driver = RemoteGuestDriver::bind("guest-a".into(), GuestChannel::new(host_stream))?;
            ensure!(driver.guest_id() == "guest-a");
            let frame = GuestDriver::capture(&driver).await?;
            ensure!(frame.png == TINY_PNG);
            GuestDriver::input(&driver, &click()).await?;
            ensure!(
                RemoteGuestDriver::bind("".into(), GuestChannel::new(tokio::io::duplex(8).0))
                    .is_err()
            );
            Ok::<_, anyhow::Error>(())
        },
        async { serve_guest_desktop(guest_stream, &endpoint).await }
    );
    host?;
    let _ = guest;
    Ok(())
}

#[tokio::test]
async fn malformed_oversized_and_uncorrelated_replies_poison_connection() -> Result<()> {
    for mode in [
        "eof",
        "oversized",
        "oversized-png",
        "malformed",
        "wrong-id",
        "bad-png",
        "identity",
    ] {
        let (host_stream, guest_stream) = tokio::io::duplex(8192);
        let (host, guest) = tokio::join!(
            async move {
                let mut channel = GuestChannel::new(host_stream);
                ensure!(
                    matches!(
                        channel.observe().await,
                        Err(GuestChannelError::UnknownOutcome)
                    ),
                    "{mode}"
                );
                ensure!(
                    channel.apply_input(&click()).await
                        == Err(GuestChannelError::ConnectionUnusable),
                    "reused {mode}"
                );
                Ok::<_, anyhow::Error>(())
            },
            async move {
                let mut peer = guest_stream;
                let _request = read_raw(&mut peer).await?;
                match mode {
                    "eof" => {
                        peer.write_all(&0u32.to_be_bytes()).await?;
                        peer.flush().await?;
                    }
                    "oversized" => {
                        let _ = peer
                            .write_all(&((MAX_CONTROL_BYTES as u32) + 1).to_be_bytes())
                            .await;
                        let _ = peer.write_all(&[b'x'; 8]).await;
                        let _ = peer.flush().await;
                    }
                    "malformed" => {
                        write_raw(&mut peer, b"not-json").await?;
                    }
                    "wrong-id" => {
                        write_raw(
                            &mut peer,
                            br#"{"kind":"observation","id":999,"width":1,"height":1}"#,
                        )
                        .await?;
                    }
                    "bad-png" => {
                        write_raw(
                            &mut peer,
                            br#"{"kind":"observation","id":1,"width":1,"height":1}"#,
                        )
                        .await?;
                        write_raw(&mut peer, b"not-a-png-frame-at-all").await?;
                    }
                    "identity" => {
                        write_raw(
                            &mut peer,
                            br#"{"kind":"observation","id":1,"width":1,"height":1,"guest_id":"taken-over"}"#,
                        )
                        .await?;
                    }
                    "oversized-png" => {
                        write_raw(
                            &mut peer,
                            br#"{"kind":"observation","id":1,"width":1,"height":1}"#,
                        )
                        .await?;
                        let _ = peer
                            .write_all(&((GUEST_FRAME_LIMIT as u32) + 1).to_be_bytes())
                            .await;
                        let _ = peer.write_all(&[b'x'; 8]).await;
                        let _ = peer.flush().await;
                    }
                    _ => unreachable!(),
                }
                let mut tail = [0_u8; 1];
                ensure!(
                    peer.read(&mut tail).await? == 0,
                    "effect repeated after {mode}"
                );
                Ok::<_, anyhow::Error>(())
            }
        );
        host?;
        guest?;
    }
    Ok(())
}

#[tokio::test]
async fn cancellation_after_input_write_does_not_replay() -> Result<()> {
    let (host_stream, guest_stream) = tokio::io::duplex(4096);
    let (written, received) = tokio::sync::oneshot::channel();
    let (host, guest) = tokio::join!(
        async move {
            let mut channel = GuestChannel::new(host_stream);
            {
                let send = channel.apply_input(&click());
                tokio::pin!(send);
                tokio::select! {
                    _ = &mut send => anyhow::bail!("peer returned an input result"),
                    result = received => { result?; }
                }
            }
            ensure!(
                channel.apply_input(&click()).await == Err(GuestChannelError::ConnectionUnusable),
                "cancel reused connection"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = guest_stream;
            let request = read_raw(&mut peer).await?;
            let parsed: serde_json::Value = serde_json::from_slice(&request)?;
            ensure!(parsed["kind"] == "input", "missing input effect");
            ensure!(parsed.get("guest_id").is_none(), "identity on wire");
            written
                .send(())
                .map_err(|_| anyhow::anyhow!("client disappeared"))?;
            let mut tail = [0_u8; 1];
            ensure!(
                peer.read(&mut tail).await? == 0,
                "effect repeated after cancellation"
            );
            Ok::<_, anyhow::Error>(())
        }
    );
    host?;
    guest?;
    Ok(())
}

#[tokio::test]
async fn guest_endpoint_rejects_shell_identity_and_does_not_execute() -> Result<()> {
    let (mut host_stream, guest_stream) = tokio::io::duplex(4096);
    let driver = RecordingDriver::new();
    let (host, guest) = tokio::join!(
        async {
            write_raw(
                &mut host_stream,
                br#"{"kind":"shell","id":1,"command":"id"}"#,
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        },
        async { serve_guest_desktop(guest_stream, &driver).await }
    );
    host?;
    ensure!(
        guest == Err(GuestChannelError::UnknownOutcome),
        "malformed request was served"
    );
    ensure!(driver.captures.load(Ordering::SeqCst) == 0);
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}

#[tokio::test]
async fn explicit_failed_capture_does_not_leak_guest_detail_and_is_not_replayed() -> Result<()> {
    let (host_stream, guest_stream) = tokio::io::duplex(4096);
    let driver = RecordingDriver::new();
    driver.fail_capture.store(true, Ordering::SeqCst);
    let (host, guest) = tokio::join!(
        async move {
            let mut channel = GuestChannel::new(host_stream);
            let error = match channel.observe().await {
                Err(error) => error,
                Ok(_) => anyhow::bail!("explicit failure expected"),
            };
            ensure!(error == GuestChannelError::Failed, "explicit failure");
            ensure!(
                !format!("{error:?} {error}").contains("capture denied"),
                "guest detail leaked"
            );
            ensure!(channel.next_id == 2, "failed observe consumed the id");
            Ok::<_, anyhow::Error>(())
        },
        async { serve_guest_desktop(guest_stream, &driver).await }
    );
    host?;
    let _ = guest;
    Ok(())
}

#[tokio::test]
async fn input_error_after_dispatch_is_unknown_and_cannot_be_reused_or_replayed() -> Result<()> {
    let (host_stream, guest_stream) = tokio::io::duplex(4096);
    let driver = Arc::new(RecordingDriver::new());
    driver.fail_after_input.store(true, Ordering::SeqCst);
    let endpoint = driver.clone();
    let (host, guest) = tokio::join!(
        async move {
            let mut channel = GuestChannel::new(host_stream);
            ensure!(
                channel.apply_input(&click()).await == Err(GuestChannelError::UnknownOutcome),
                "partial input reported as reusable failure"
            );
            ensure!(
                channel.apply_input(&click()).await == Err(GuestChannelError::ConnectionUnusable),
                "partial input reused the channel"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move { serve_guest_desktop(guest_stream, endpoint.as_ref()).await }
    );
    host?;
    ensure!(
        guest == Err(GuestChannelError::UnknownOutcome),
        "partial input kept the endpoint"
    );
    ensure!(driver.inputs.load(Ordering::SeqCst) == 1, "effect replayed");
    Ok(())
}

#[tokio::test]
async fn pre_dispatch_invalid_input_is_failed_without_effect() -> Result<()> {
    let (mut host_stream, guest_stream) = tokio::io::duplex(4096);
    let driver = RecordingDriver::new();
    let (host, guest) = tokio::join!(
        async move {
            write_raw(
                &mut host_stream,
                br#"{"kind":"input","id":1,"input":{"kind":"scroll","x":0,"y":0,"direction":"down","steps":0}}"#,
            )
            .await?;
            let reply: serde_json::Value =
                serde_json::from_slice(&read_raw(&mut host_stream).await?)?;
            ensure!(
                reply["kind"] == "failed" && reply["id"] == 1,
                "invalid input was not failed before dispatch"
            );
            drop(host_stream);
            Ok::<_, anyhow::Error>(())
        },
        async { serve_guest_desktop(guest_stream, &driver).await }
    );
    host?;
    ensure!(
        matches!(guest, Ok(()) | Err(GuestChannelError::UnknownOutcome)),
        "invalid input left the endpoint running"
    );
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}

#[test]
fn virtio_port_name_is_fixed_and_safe_for_qemu_keyval() {
    assert_eq!(GUEST_DESKTOP_PORT, "org.ctox.guest.desktop");
    assert!(!GUEST_DESKTOP_PORT.contains(','));
    assert!(!GUEST_VIRTIO_PORT_PATH.contains(','));
    assert!(GUEST_VIRTIO_PORT_PATH.ends_with(GUEST_DESKTOP_PORT));
}

#[cfg(unix)]
struct VirtioPortFixture {
    _root: tempfile::TempDir,
    port_path: PathBuf,
    vport_path: PathBuf,
    sysfs_class: PathBuf,
}

#[cfg(unix)]
fn virtio_port_fixture(target_name: &str, sysfs_name: &str) -> Result<VirtioPortFixture> {
    let root = tempfile::tempdir()?;
    let dev = root.path().join("dev");
    let ports = dev.join("virtio-ports");
    std::fs::create_dir_all(&ports)?;
    let vport_path = dev.join(target_name);
    std::fs::write(&vport_path, b"")?;
    let port_path = ports.join(GUEST_DESKTOP_PORT);
    std::os::unix::fs::symlink(Path::new("..").join(target_name), &port_path)?;
    let sysfs_class = root.path().join("sys/class/virtio-ports");
    std::fs::create_dir_all(sysfs_class.join(target_name))?;
    std::fs::write(
        sysfs_class.join(target_name).join("name"),
        format!("{sysfs_name}\n"),
    )?;
    Ok(VirtioPortFixture {
        _root: root,
        port_path,
        vport_path,
        sysfs_class,
    })
}

#[cfg(unix)]
#[test]
fn udev_named_symlink_resolves_to_vport_identity() -> Result<()> {
    let fixture = virtio_port_fixture("vport0p1", GUEST_DESKTOP_PORT)?;
    let target = named_virtio_symlink_target(&fixture.port_path)?;
    ensure!(
        target == fixture.vport_path,
        "named virtio alias did not resolve to the vport sibling"
    );
    pin_virtio_sysfs_name(
        &fixture.sysfs_class,
        target
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("vport name"))?,
    )?;
    ensure!(
        open_named_virtio_port(&fixture.port_path, &fixture.sysfs_class)
            == Err(GuestChannelError::Unavailable),
        "regular-file fixture was treated as a virtio character device"
    );
    ensure!(
        named_virtio_symlink_target(&fixture.vport_path) == Err(GuestChannelError::Unavailable),
        "raw vport path was accepted as the named udev alias"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn udev_named_symlink_rejects_invalid_target() -> Result<()> {
    let substituted = virtio_port_fixture("null", GUEST_DESKTOP_PORT)?;
    ensure!(
        named_virtio_symlink_target(&substituted.port_path) == Err(GuestChannelError::Unavailable),
        "non-vport symlink target was accepted"
    );

    let wrong_name = virtio_port_fixture("vport0p1", "org.qemu.guest_agent.0")?;
    let target = named_virtio_symlink_target(&wrong_name.port_path)?;
    ensure!(
        pin_virtio_sysfs_name(
            &wrong_name.sysfs_class,
            target
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("vport name"))?,
        ) == Err(GuestChannelError::Unavailable),
        "sysfs name mismatch was accepted"
    );

    let nested = virtio_port_fixture("vport0p1", GUEST_DESKTOP_PORT)?;
    std::fs::remove_file(&nested.vport_path)?;
    std::os::unix::fs::symlink("null", &nested.vport_path)?;
    ensure!(
        named_virtio_symlink_target(&nested.port_path) == Err(GuestChannelError::Unavailable),
        "nested symlink target was accepted"
    );

    let escaped = tempfile::tempdir()?;
    let dev = escaped.path().join("dev");
    let ports = dev.join("virtio-ports");
    std::fs::create_dir_all(&ports)?;
    std::fs::write(escaped.path().join("secret"), b"")?;
    std::os::unix::fs::symlink(Path::new("../../secret"), ports.join(GUEST_DESKTOP_PORT))?;
    ensure!(
        named_virtio_symlink_target(&ports.join(GUEST_DESKTOP_PORT))
            == Err(GuestChannelError::Unavailable),
        "escaped symlink target was accepted"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn missing_local_virtio_port_is_unavailable_and_creates_nothing() -> Result<()> {
    let driver = RecordingDriver::new();
    ensure!(
        run_guest_desktop_effects(&driver).await == Err(GuestChannelError::Unavailable),
        "missing virtio port started an endpoint"
    );
    ensure!(driver.captures.load(Ordering::SeqCst) == 0);
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}

/// Private socketpair is a test surrogate for nonblocking virtio character IO.
/// It does not replace the host QEMU unix-listener/peer-cred handshake.
#[cfg(unix)]
fn nonblocking_pair() -> Result<(
    NonblockingIo<std::os::unix::net::UnixStream>,
    NonblockingIo<std::os::unix::net::UnixStream>,
)> {
    let (left, right) = std::os::unix::net::UnixStream::pair()?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    Ok((NonblockingIo::new(left)?, NonblockingIo::new(right)?))
}

#[cfg(unix)]
#[tokio::test]
async fn truncated_request_on_nonblocking_pair_hits_deadline_without_effect() -> Result<()> {
    let (mut host, guest) = nonblocking_pair()?;
    let driver = RecordingDriver::new();
    host.write_all(&[0x00]).await?;
    host.flush().await?;
    let started = std::time::Instant::now();
    let result = serve_guest_desktop_timed(guest, &driver, Duration::from_millis(80)).await;
    ensure!(
        result == Err(GuestChannelError::UnknownOutcome),
        "truncated request waited without a deadline"
    );
    ensure!(started.elapsed() < Duration::from_secs(2), "deadline hung");
    ensure!(driver.captures.load(Ordering::SeqCst) == 0);
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn started_frame_wait_on_nonblocking_pair_is_cancellable() -> Result<()> {
    let (mut host, guest) = nonblocking_pair()?;
    let driver = RecordingDriver::new();
    // Full length prefix for a body that never arrives: idle first-byte wait
    // is over, and the remainder is the started-frame deadline.
    host.write_all(&32u32.to_be_bytes()).await?;
    host.flush().await?;
    let mut serving = Box::pin(serve_guest_desktop_timed(
        guest,
        &driver,
        Duration::from_secs(5),
    ));
    let deadline = std::time::Instant::now() + Duration::from_millis(200);
    let mut saw_pending = false;
    loop {
        match futures_util::poll!(serving.as_mut()) {
            Poll::Ready(result) => {
                anyhow::bail!("started frame completed before cancellation: {result:?}")
            }
            Poll::Pending => saw_pending = true,
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::task::yield_now().await;
    }
    ensure!(saw_pending, "started frame wait was never pending");
    drop(serving);
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn blocked_peer_write_on_nonblocking_pair_hits_deadline() -> Result<()> {
    use std::os::fd::AsRawFd;

    let (left, right) = std::os::unix::net::UnixStream::pair()?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    let tiny: libc::c_int = 1024;
    for fd in [left.as_raw_fd(), right.as_raw_fd()] {
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                (&tiny as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                (&tiny as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
    let mut writer = NonblockingIo::new(left)?;
    let _unread_peer = NonblockingIo::new(right)?;
    loop {
        let mut stream = writer.inner.get_ref();
        match std::io::Write::write(&mut stream, &[0_u8; 4096]) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error.into()),
        }
    }
    let payload = vec![b'x'; MAX_CONTROL_BYTES];
    let started = std::time::Instant::now();
    let result = with_deadline(
        Duration::from_millis(80),
        write_frame(&mut writer, &payload, MAX_CONTROL_BYTES),
    )
    .await;
    ensure!(
        result == Err(GuestChannelError::UnknownOutcome),
        "blocked write completed without a deadline"
    );
    ensure!(
        started.elapsed() < Duration::from_secs(2),
        "write deadline hung"
    );
    Ok(())
}
