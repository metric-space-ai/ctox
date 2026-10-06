// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use crate::business_os::guest_runtime::channel::tests::TINY_PNG;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::DuplexStream;

struct EndpointDriver {
    captures: AtomicUsize,
    inputs: AtomicUsize,
}
impl EndpointDriver {
    fn new() -> Self {
        Self {
            captures: AtomicUsize::new(0),
            inputs: AtomicUsize::new(0),
        }
    }
}
impl GuestDriver for EndpointDriver {
    fn guest_id(&self) -> &str {
        "guest-a"
    }
    async fn capture(&self) -> Result<GuestFrame> {
        self.captures.fetch_add(1, Ordering::SeqCst);
        GuestFrame::from_png(TINY_PNG.to_vec())
    }
    async fn input(&self, input: &GuestInput) -> Result<()> {
        self.inputs.fetch_add(1, Ordering::SeqCst);
        input.validate()
    }
}
fn key() -> GuestInput {
    GuestInput::Key {
        key: super::super::GuestKey::Enter,
    }
}
fn endpoint_eof(result: Result<(), GuestChannelError>) {
    assert!(matches!(
        result,
        Ok(()) | Err(GuestChannelError::UnknownOutcome)
    ));
}
async fn probe_id(stream: &mut DuplexStream) -> u64 {
    match read_json::<_, WireRequest>(stream, MAX_CONTROL_BYTES)
        .await
        .unwrap()
    {
        WireRequest::Probe { id } => id,
        _ => panic!("expected the actual probe frame"),
    }
}

#[tokio::test]
async fn endpoint_probe_pins_one_session_without_capture_or_input() {
    let (host, guest) = tokio::io::duplex(4096);
    let local = EndpointDriver::new();
    let (_, server) = tokio::join!(
        async {
            let remote =
                RemoteGuestDriver::bind("guest-a".into(), GuestChannel::new(host)).unwrap();
            let first = remote.probe_endpoint().await.unwrap();
            assert_eq!(first.guest_id, "guest-a");
            assert!(uuid::Uuid::parse_str(&first.session_id).is_ok());
            assert_eq!(local.captures.load(Ordering::SeqCst), 0);
            assert_eq!(local.inputs.load(Ordering::SeqCst), 0);
            assert_eq!(remote.probe_endpoint().await.unwrap(), first);
            assert_eq!(remote.capture().await.unwrap().png, TINY_PNG);
            remote.input(&key()).await.unwrap();
        },
        serve_guest_desktop(guest, &local)
    );
    endpoint_eof(server);
    assert_eq!(local.captures.load(Ordering::SeqCst), 1);
    assert_eq!(local.inputs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn endpoint_probe_rejects_foreign_guest_without_effect_or_reuse() {
    let (host, guest) = tokio::io::duplex(4096);
    let local = EndpointDriver::new();
    let (_, server) = tokio::join!(
        async {
            let mut channel = GuestChannel::new(host);
            assert_eq!(
                channel.probe_endpoint("guest-b").await,
                Err(GuestChannelError::UnknownOutcome)
            );
            assert_eq!(
                channel.observe().await.err(),
                Some(GuestChannelError::ConnectionUnusable)
            );
        },
        serve_guest_desktop(guest, &local)
    );
    endpoint_eof(server);
    assert_eq!(local.captures.load(Ordering::SeqCst), 0);
    assert_eq!(local.inputs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn endpoint_restart_retires_the_pinned_channel_without_rebinding() {
    let (host, mut guest) = tokio::io::duplex(4096);
    let first = uuid::Uuid::new_v4().to_string();
    let next = uuid::Uuid::new_v4().to_string();
    tokio::join!(
        async {
            let mut channel = GuestChannel::new(host);
            assert_eq!(
                channel.probe_endpoint("guest-a").await.unwrap().session_id,
                first
            );
            assert_eq!(
                channel.probe_endpoint("guest-a").await,
                Err(GuestChannelError::UnknownOutcome)
            );
            assert_eq!(
                channel.apply_input(&key()).await,
                Err(GuestChannelError::ConnectionUnusable)
            );
        },
        async {
            for session in [&first, &next] {
                let id = probe_id(&mut guest).await;
                write_json(
                    &mut guest,
                    &WireReply::Endpoint {
                        id,
                        guest_id: "guest-a".into(),
                        session_id: session.clone(),
                    },
                    MAX_CONTROL_BYTES,
                )
                .await
                .unwrap();
            }
        }
    );
}

#[tokio::test]
async fn endpoint_stale_session_never_captures_or_applies_input() {
    let (mut host, guest) = tokio::io::duplex(4096);
    let local = EndpointDriver::new();
    let (_, server) = tokio::join!(
        async {
            write_json(&mut host, &WireRequest::Probe { id: 1 }, MAX_CONTROL_BYTES)
                .await
                .unwrap();
            let current: WireReply = read_json(&mut host, MAX_CONTROL_BYTES).await.unwrap();
            let session = match current {
                WireReply::Endpoint { session_id, .. } => session_id,
                _ => panic!("endpoint reply"),
            };
            let stale = uuid::Uuid::new_v4().to_string();
            assert_ne!(session, stale);
            for (expected_id, request) in [
                (
                    2,
                    WireRequest::ObserveSession {
                        id: 2,
                        session_id: stale.clone(),
                    },
                ),
                (
                    3,
                    WireRequest::InputSession {
                        id: 3,
                        session_id: stale,
                        input: key(),
                    },
                ),
            ] {
                write_json(&mut host, &request, MAX_CONTROL_BYTES)
                    .await
                    .unwrap();
                match read_json::<_, WireReply>(&mut host, MAX_CONTROL_BYTES)
                    .await
                    .unwrap()
                {
                    WireReply::Failed { id } => assert_eq!(id, expected_id),
                    _ => panic!("stale session must receive its exact correlated denial"),
                }
            }
            drop(host);
        },
        serve_guest_desktop(guest, &local)
    );
    endpoint_eof(server);
    assert_eq!(local.captures.load(Ordering::SeqCst), 0);
    assert_eq!(local.inputs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelled_endpoint_probe_retires_possible_write_without_replay() {
    let (host, mut guest) = tokio::io::duplex(4096);
    let mut channel = GuestChannel::new(host);
    let mut pending = Box::pin(channel.probe_endpoint("guest-a"));
    tokio::select! {
        _ = &mut pending => panic!("probe cannot complete without a reply"),
        id = probe_id(&mut guest) => assert_eq!(id, 1),
    }
    drop(pending);
    assert_eq!(
        channel.probe_endpoint("guest-a").await,
        Err(GuestChannelError::ConnectionUnusable)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), probe_id(&mut guest))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_or_uncorrelated_endpoint_session_is_unknown_and_retired() {
    for (reply_id, session) in [
        (2, uuid::Uuid::new_v4().to_string()),
        (1, "not-a-native-session".into()),
    ] {
        let (host, mut guest) = tokio::io::duplex(4096);
        tokio::join!(
            async {
                let mut channel = GuestChannel::new(host);
                assert_eq!(
                    channel.probe_endpoint("guest-a").await,
                    Err(GuestChannelError::UnknownOutcome)
                );
                assert_eq!(
                    channel.observe().await.err(),
                    Some(GuestChannelError::ConnectionUnusable)
                );
            },
            async {
                assert_eq!(probe_id(&mut guest).await, 1);
                write_json(
                    &mut guest,
                    &WireReply::Endpoint {
                        id: reply_id,
                        guest_id: "guest-a".into(),
                        session_id: session,
                    },
                    MAX_CONTROL_BYTES,
                )
                .await
                .unwrap();
            }
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn physical_frame_poll_checks_pinned_socket_without_await_or_consuming_reply() {
    use tokio::io::AsyncWriteExt;
    let (host, mut guest) = tokio::net::UnixStream::pair().unwrap();
    let mut channel = GuestChannel::new(host);
    channel.session = Some(GuestEndpointSession {
        guest_id: "guest-a".into(),
        session_id: "session-a".into(),
    });
    let remote = RemoteGuestDriver::bind("guest-a".into(), channel).unwrap();
    assert!(remote.ensure_current_endpoint("session-a").is_ok());
    assert!(remote.ensure_current_endpoint("foreign-session").is_err());
    guest.write_all(&[7]).await.unwrap();
    assert!(remote.ensure_current_endpoint("session-a").is_err());
    let mut byte = [0u8];
    {
        let channel = remote.channel.try_lock().unwrap();
        // Test-only readiness wait, outside every native publication fence.
        channel.stream.readable().await.unwrap();
        channel.stream.try_read(&mut byte).unwrap();
    }
    assert_eq!(
        byte,
        [7],
        "the liveness check must not consume protocol bytes"
    );
    assert!(remote.ensure_current_endpoint("session-a").is_ok());
    drop(guest);
    assert!(remote.ensure_current_endpoint("session-a").is_err());
}
