use super::*;
use crate::plugins::replication_webrtc::{LocalDeviceProof, LocalSessionCredentials};
use std::sync::atomic::{AtomicUsize, Ordering};

fn credentials(token: &str, nonce: Option<String>) -> LocalSessionCredentials {
    LocalSessionCredentials {
        capability_token: token.into(),
        // Envelope assembly fixture, not a cryptographic signer. The production
        // Business OS validator remains responsible for verifying this proof.
        device_proof: nonce.map(|_| LocalDeviceProof {
            public_x: "x".repeat(43),
            public_y: "y".repeat(43),
            signature: "s".repeat(86),
        }),
    }
}

async fn pool(handler: &StdArc<MockHandler>) -> StdArc<RxWebRTCReplicationPool<MockHandler>> {
    let collection =
        crate::rx_collection::test_support::test_collection_named("local_credentials").await;
    replicate_web_rtc_multi_with_validators(
        collection.database.clone(),
        vec![collection],
        handler.clone(),
        None,
        Some(StdArc::new(|_, _| WebRTCPeerSessionValidation::Accept)),
        Some("credential-test-room".into()),
        Some(StdArc::from("local-session")),
    )
    .await
    .unwrap()
}

async fn next_frame(frames: &mut RxStream<WebRTCWireFrame>) -> WebRTCWireFrame {
    tokio::time::timeout(Duration::from_secs(3), frames.next())
        .await
        .expect("handshake frame deadline")
        .expect("frame stream open")
}

fn inbound(handler: &MockHandler, nonce: Value, id: &str) {
    handler.inject_message(
        "remote",
        WebRTCMessage {
            id: id.into(),
            method: "ctoxProtocol".into(),
            params: vec![json!({"peerSession":{"sessionId":"remote-session",
            "capabilityToken":"remote-token", "deviceProofNonce":nonce}})],
            collection: None,
        },
    );
}

#[tokio::test]
async fn local_credentials_are_fresh_on_each_outgoing_connection() {
    let handler = MockHandler::new();
    let calls = StdArc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    *handler.local_provider.lock() = Some(StdArc::new(move |peer, nonce| {
        assert!(nonce.is_none(), "outgoing challenge does not sign itself");
        let n = counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(credentials(&format!("token-{}-{n}", peer.1), None)) })
    }));
    let pool = pool(&handler).await;
    let mut frames = handler.sent_subject.subscribe();
    for generation in [1, 2] {
        handler.connect.next(MockPeer("remote".into(), generation));
        let WebRTCWireFrame::Message(message) = next_frame(&mut frames).await else {
            panic!("request")
        };
        assert_eq!(message.method, "ctoxProtocol");
        assert_eq!(
            message.params[0]["peerSession"]["capabilityToken"],
            format!("token-{generation}-{}", generation - 1)
        );
        assert_eq!(
            message.params[0]["peerSession"]["deviceProofNonce"]
                .as_str()
                .unwrap()
                .len(),
            43
        );
        assert!(message.params[0]["peerSession"]
            .get("deviceProof")
            .is_none());
        let peer = MockPeer("remote".into(), generation);
        handler.retired.lock().insert(peer.clone());
        handler.disconnect.next(peer);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    pool.cancel().await;
}

#[tokio::test]
async fn local_credentials_answer_the_remote_nonce_without_caching_proofs() {
    let handler = MockHandler::new();
    let calls = StdArc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    *handler.local_provider.lock() = Some(StdArc::new(move |peer, nonce| {
        assert_eq!(peer.0, "remote");
        let n = counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(credentials(&format!("token-{n}"), nonce)) })
    }));
    let pool = pool(&handler).await;
    let mut frames = handler.sent_subject.subscribe();
    for (n, nonce) in ["a".repeat(43), "b".repeat(43)].into_iter().enumerate() {
        inbound(&handler, json!(nonce), &format!("request-{n}"));
        let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
            panic!("response")
        };
        assert!(response.error.is_none());
        assert_eq!(
            response.result["peerSession"]["capabilityToken"],
            format!("token-{n}")
        );
        let proof = &response.result["peerSession"]["deviceProof"];
        assert_eq!(proof["nonce"], nonce);
        assert_eq!(proof["version"], "ctox-device-proof-v1");
        assert_eq!(proof["publicJwk"].as_object().unwrap().len(), 4);
        assert_eq!(proof["publicJwk"]["kty"], "EC");
        assert_eq!(proof["publicJwk"]["crv"], "P-256");
        assert!(pool
            .authenticated_peers
            .lock()
            .contains(&MockPeer("remote".into(), 1)));
    }
    pool.cancel().await;
}

#[tokio::test]
async fn local_credential_failure_never_admits_pending_peer_or_exposes_provider_error() {
    let handler = MockHandler::new();
    let started = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let (start, proceed) = (started.clone(), release.clone());
    *handler.local_provider.lock() = Some(StdArc::new(move |_, _| {
        let (start, proceed) = (start.clone(), proceed.clone());
        Box::pin(async move {
            start.notify_one();
            proceed.notified().await;
            Err(new_rx_error(
                "secret-token-in-provider-error",
                Some(json!({"key":"private-material"})),
            ))
        })
    }));
    let pool = pool(&handler).await;
    let mut frames = handler.sent_subject.subscribe();
    let mut errors = pool.error_subject.subscribe();
    inbound(&handler, json!("a".repeat(43)), "pending-credentials");
    tokio::time::timeout(Duration::from_secs(3), started.notified())
        .await
        .unwrap();
    assert!(
        pool.authenticated_peers.lock().is_empty(),
        "no data admission while key store waits"
    );
    release.notify_one();
    let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
        panic!("response")
    };
    assert_eq!(
        response.error.as_deref(),
        Some("local_session_credentials_unavailable")
    );
    assert!(response.result.is_null());
    let error = tokio::time::timeout(Duration::from_secs(3), errors.next())
        .await
        .unwrap()
        .unwrap();
    assert!(!error.to_string().contains("private-material"));
    assert!(!error.to_string().contains("secret-token"));
    assert!(pool.authenticated_peers.lock().is_empty());
    pool.cancel().await;
}

#[tokio::test]
async fn retired_connection_cannot_publish_credentials_after_async_key_store_returns() {
    let handler = MockHandler::new();
    let started = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let (start, proceed) = (started.clone(), release.clone());
    *handler.local_provider.lock() = Some(StdArc::new(move |_, _| {
        let (start, proceed) = (start.clone(), proceed.clone());
        Box::pin(async move {
            start.notify_one();
            proceed.notified().await;
            Ok(credentials("must-not-leak", None))
        })
    }));
    let pool = pool(&handler).await;
    let mut errors = pool.error_subject.subscribe();
    let peer = MockPeer("remote".into(), 1);
    handler.connect.next(peer.clone());
    tokio::time::timeout(Duration::from_secs(3), started.notified())
        .await
        .unwrap();
    handler.retired.lock().insert(peer);
    release.notify_one();
    let error = tokio::time::timeout(Duration::from_secs(3), errors.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        error.parameters()["code"],
        "local_session_credentials_unavailable"
    );
    assert!(handler.sent.lock().is_empty());
    assert!(pool.authenticated_peers.lock().is_empty());
    pool.cancel().await;
}

#[tokio::test]
async fn malformed_challenge_is_rejected_before_signing() {
    let handler = MockHandler::new();
    *handler.local_provider.lock() =
        Some(StdArc::new(|_, _| panic!("must not sign malformed nonce")));
    let pool = pool(&handler).await;
    let mut frames = handler.sent_subject.subscribe();
    inbound(&handler, json!("not-a-protocol-nonce"), "invalid-nonce");
    let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
        panic!("response")
    };
    assert_eq!(
        response.error.as_deref(),
        Some("local_session_credentials_unavailable")
    );
    assert!(pool.authenticated_peers.lock().is_empty());
    pool.cancel().await;
}

#[tokio::test]
async fn missing_proof_cannot_silently_fall_back_to_token_only_response() {
    let handler = MockHandler::new();
    *handler.local_provider.lock() = Some(StdArc::new(|_, _| {
        Box::pin(async { Ok(credentials("bound-token-without-key", None)) })
    }));
    let pool = pool(&handler).await;
    let mut frames = handler.sent_subject.subscribe();
    inbound(&handler, json!("a".repeat(43)), "missing-key");
    let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
        panic!("response")
    };
    assert_eq!(
        response.error.as_deref(),
        Some("local_session_credentials_unavailable")
    );
    assert!(response.result.is_null());
    assert!(pool.authenticated_peers.lock().is_empty());
    pool.cancel().await;
}
