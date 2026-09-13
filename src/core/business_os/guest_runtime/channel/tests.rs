// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
    fail_capture: bool,
}

impl RecordingDriver {
    fn new() -> Self {
        Self {
            captures: AtomicUsize::new(0),
            inputs: AtomicUsize::new(0),
            fail_capture: false,
        }
    }
}

impl GuestDriver for RecordingDriver {
    fn guest_id(&self) -> &str {
        "guest-a"
    }

    async fn capture(&self) -> Result<GuestFrame> {
        self.captures.fetch_add(1, Ordering::SeqCst);
        if self.fail_capture {
            anyhow::bail!("capture denied");
        }
        GuestFrame::from_png(TINY_PNG.to_vec())
    }

    async fn input(&self, input: &GuestInput) -> Result<()> {
        self.inputs.fetch_add(1, Ordering::SeqCst);
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
        async move { serve_guest_desktop(guest_stream, &endpoint).await }
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
                    channel.observe().await == Err(GuestChannelError::UnknownOutcome),
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
        async move {
            write_raw(
                &mut host_stream,
                br#"{"kind":"shell","id":1,"command":"id"}"#,
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        },
        async move { serve_guest_desktop(guest_stream, &driver).await }
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
    let mut driver = RecordingDriver::new();
    driver.fail_capture = true;
    let (host, guest) = tokio::join!(
        async move {
            let mut channel = GuestChannel::new(host_stream);
            let error = channel.observe().await.unwrap_err();
            ensure!(error == GuestChannelError::Failed, "explicit failure");
            ensure!(
                !format!("{error:?} {error}").contains("capture denied"),
                "guest detail leaked"
            );
            ensure!(channel.next_id == 2, "failed observe consumed the id");
            Ok::<_, anyhow::Error>(())
        },
        async move { serve_guest_desktop(guest_stream, &driver).await }
    );
    host?;
    let _ = guest;
    Ok(())
}

#[test]
fn virtio_port_name_is_fixed_and_safe_for_qemu_keyval() {
    assert_eq!(GUEST_DESKTOP_PORT, "org.ctox.guest.desktop");
    assert!(!GUEST_DESKTOP_PORT.contains(','));
    assert!(!GUEST_VIRTIO_PORT_PATH.contains(','));
    assert!(GUEST_VIRTIO_PORT_PATH.ends_with(GUEST_DESKTOP_PORT));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn missing_local_virtio_port_is_unavailable_and_creates_nothing() -> Result<()> {
    let driver = RecordingDriver::new();
    ensure!(
        serve_local_virtio_desktop(&driver).await == Err(GuestChannelError::Unavailable),
        "missing virtio port started an endpoint"
    );
    ensure!(driver.captures.load(Ordering::SeqCst) == 0);
    ensure!(driver.inputs.load(Ordering::SeqCst) == 0);
    Ok(())
}
