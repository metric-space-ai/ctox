use super::super::hello::tests::{hello, policy};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn authority(count: Arc<AtomicUsize>) -> AuthenticatePeer {
    Arc::new(move |_, peer| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            // Isolated verifier stub. No claim of signature/production pairing proof.
            Ok(AuthorizedSession {
                instance_id: peer.instance_id,
                device_id: peer.device_proof.device_id,
                epoch: "test-epoch".into(),
                head_seq: WireSeq(u64::MAX),
                scope_tokens: vec!["test-issued-scope".into()],
                rights_ver: WireSeq(1),
            })
        })
    })
}
fn start(registry: &PeerLifecycleRegistry, peer: &str) -> PeerLifecycle {
    registry
        .start(
            peer.into(),
            "A".repeat(43),
            policy(),
            authority(Arc::new(AtomicUsize::new(0))),
            Duration::from_secs(5),
        )
        .unwrap()
}
#[tokio::test]
async fn v4_connection_requires_ordered_handshake_and_sctp_confirmation() {
    let registry = PeerLifecycleRegistry::default();
    let peer = start(&registry, "peer");
    assert_eq!(peer.snapshot().state, ConnectionState::Connecting);
    assert_eq!(
        peer.hello_ok_sent().await,
        Err(LifecycleError::InvalidTransition)
    );
    assert!(matches!(
        peer.receive_hello(hello()).await,
        Err(LifecycleError::InvalidTransition)
    ));
    peer.transport_connected().await.unwrap();
    assert_eq!(peer.snapshot().state, ConnectionState::Authenticating);
    assert!(matches!(
        peer.receive_hello(hello()).await.unwrap(),
        HelloFrame::HelloOk(_)
    ));
    assert_eq!(peer.snapshot().state, ConnectionState::Negotiated);
    peer.hello_ok_sent().await.unwrap();
    assert_eq!(peer.snapshot().state, ConnectionState::Live);
    assert!(matches!(
        peer.receive_hello(hello()).await,
        Err(LifecycleError::InvalidTransition)
    ));
    peer.drain().await;
    assert_eq!(peer.snapshot().state, ConnectionState::Closed);
    assert_eq!(registry.peer_count(), 0);
    assert_eq!(
        peer.transport_connected().await,
        Err(LifecycleError::Closed)
    );
}
#[tokio::test]
async fn v4_connection_has_one_task_per_peer_until_retired() {
    let registry = PeerLifecycleRegistry::default();
    let old = start(&registry, "peer");
    assert!(matches!(
        registry.start(
            "peer".into(),
            "A".repeat(43),
            policy(),
            authority(Arc::new(AtomicUsize::new(0))),
            Duration::from_secs(5)
        ),
        Err(LifecycleError::PeerAlreadyOwned)
    ));
    let second = start(&registry, "other-peer");
    assert_eq!(registry.peer_count(), 2);
    old.drain().await;
    let replacement = start(&registry, "peer");
    assert_eq!(registry.peer_count(), 2);
    registry.drain_all().await;
    assert_eq!(second.snapshot().state, ConnectionState::Closed);
    assert_eq!(replacement.snapshot().state, ConnectionState::Closed);
    assert_eq!(registry.peer_count(), 0);
}
#[tokio::test]
async fn v4_connection_rejection_is_wire_visible_and_retained_without_authorizing() {
    let registry = PeerLifecycleRegistry::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let peer = registry
        .start(
            "peer".into(),
            "A".repeat(43),
            policy(),
            authority(calls.clone()),
            Duration::from_secs(5),
        )
        .unwrap();
    peer.transport_connected().await.unwrap();
    let mut incompatible = hello();
    incompatible.app_api_ver.min = "5.0".into();
    incompatible.app_api_ver.max = "5.0".into();
    let response = peer.receive_hello(incompatible).await.unwrap();
    assert!(matches!(
        response,
        HelloFrame::HelloReject(HelloReject {
            reason: RejectReason::AppApiIncompatible,
            ..
        })
    ));
    peer.drain().await;
    assert_eq!(
        peer.snapshot().rejection.unwrap().reason,
        RejectReason::AppApiIncompatible
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn v4_connection_replayed_proof_and_wrong_authority_never_enter_live() {
    for wrong_authority in [false, true] {
        let registry = PeerLifecycleRegistry::default();
        let verify: AuthenticatePeer = Arc::new(|_, _| {
            Box::pin(async {
                Ok(AuthorizedSession {
                    instance_id: "foreign".into(),
                    device_id: "test-device".into(),
                    epoch: "e".into(),
                    head_seq: WireSeq(0),
                    scope_tokens: vec![],
                    rights_ver: WireSeq(1),
                })
            })
        });
        let peer = registry
            .start(
                "peer".into(),
                "A".repeat(43),
                policy(),
                verify,
                Duration::from_secs(5),
            )
            .unwrap();
        peer.transport_connected().await.unwrap();
        let mut input = hello();
        if !wrong_authority {
            input.device_proof.nonce = "D".repeat(43);
        }
        let response = peer.receive_hello(input).await.unwrap();
        let HelloFrame::HelloReject(error) = response else {
            panic!("must reject")
        };
        assert_eq!(
            error.reason,
            if wrong_authority {
                RejectReason::AuthenticationFailed
            } else {
                RejectReason::DeviceProofInvalid
            }
        );
        peer.drain().await;
        assert_eq!(peer.snapshot().state, ConnectionState::Closed);
    }
}
#[tokio::test(start_paused = true)]
async fn v4_connection_times_out_at_connect_authenticate_and_negotiated_boundaries() {
    for boundary in 0..3 {
        let registry = PeerLifecycleRegistry::default();
        let peer = start(&registry, "peer");
        if boundary >= 1 {
            peer.transport_connected().await.unwrap();
        }
        if boundary == 2 {
            peer.receive_hello(hello()).await.unwrap();
        }
        tokio::time::advance(Duration::from_secs(6)).await;
        let mut states = peer.subscribe();
        states
            .wait_for(|s| s.state == ConnectionState::Closed)
            .await
            .unwrap();
        assert_eq!(
            peer.snapshot().rejection.unwrap().reason,
            RejectReason::HandshakeTimeout
        );
        peer.drain().await;
    }
}
#[tokio::test]
async fn v4_connection_drain_cancels_stalled_authority_and_registry_drop_closes_peers() {
    let registry = PeerLifecycleRegistry::default();
    let verify: AuthenticatePeer = Arc::new(|_, _| Box::pin(std::future::pending()));
    let peer = registry
        .start(
            "peer".into(),
            "A".repeat(43),
            policy(),
            verify,
            Duration::from_secs(60),
        )
        .unwrap();
    peer.transport_connected().await.unwrap();
    let request_peer = peer.clone();
    let request = tokio::spawn(async move { request_peer.receive_hello(hello()).await });
    peer.drain().await;
    assert!(matches!(
        request.await.unwrap(),
        Err(LifecycleError::Closed)
    ));
    let peer = start(&registry, "new-peer");
    drop(registry);
    let mut states = peer.subscribe();
    tokio::time::timeout(
        Duration::from_secs(1),
        states.wait_for(|s| s.state == ConnectionState::Closed),
    )
    .await
    .unwrap()
    .unwrap();
}
#[tokio::test]
async fn v4_connection_malformed_wire_is_a_visible_rejection_and_panic_retires_owner() {
    let registry = PeerLifecycleRegistry::default();
    let peer = start(&registry, "peer");
    peer.transport_connected().await.unwrap();
    assert!(matches!(
        peer.receive_frame(b"{not-json}").await.unwrap(),
        HelloFrame::HelloReject(HelloReject {
            reason: RejectReason::InvalidHello,
            ..
        })
    ));
    peer.drain().await;
    assert_eq!(registry.peer_count(), 0);
    let panic_verifier: AuthenticatePeer = Arc::new(|_, _| {
        Box::pin(async {
            panic!("isolated authority failure");
        })
    });
    let peer = registry
        .start(
            "peer".into(),
            "A".repeat(43),
            policy(),
            panic_verifier,
            Duration::from_secs(5),
        )
        .unwrap();
    peer.transport_connected().await.unwrap();
    assert!(matches!(
        peer.receive_hello(hello()).await,
        Err(LifecycleError::Closed)
    ));
    peer.drain().await;
    assert_eq!(registry.peer_count(), 0);
    assert_eq!(
        peer.snapshot().rejection.unwrap().reason,
        RejectReason::AuthenticationFailed
    );
}
