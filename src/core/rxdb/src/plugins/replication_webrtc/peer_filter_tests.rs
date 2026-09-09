// Included in local_session_tests: exercise pool dispatch, not just the predicate.
async fn filtered_pool(
    handler: &StdArc<MockHandler>,
    allowed: StdArc<std::sync::atomic::AtomicBool>,
) -> StdArc<RxWebRTCReplicationPool<MockHandler>> {
    let collection = crate::rx_collection::test_support::test_collection_named("peer_filter").await;
    replicate_web_rtc_multi_with_validators(
        collection.database.clone(),
        vec![collection],
        handler.clone(),
        Some(StdArc::new(move |_| allowed.load(Ordering::SeqCst))),
        Some(StdArc::new(|_, _| WebRTCPeerSessionValidation::Accept)),
        Some("peer-filter-room".into()),
        Some(StdArc::from("local-session")),
    )
    .await
    .unwrap()
}

async fn await_peer_closed(handler: &MockHandler) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while handler.closed_peers.lock().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("excluded peer must close");
}

#[tokio::test]
async fn peer_filter_denies_incoming_rpc_families_before_credentials_or_dispatch() {
    for method in [
        "ctoxProtocol",
        "token",
        "masterChangesSince",
        "masterWrite",
        "rxdb.query.fetch",
        "rxdb.query.cancel",
        "rxdb.file.read",
        "auxiliary.fixture",
    ] {
        let handler = MockHandler::new();
        *handler.local_provider.lock() =
            Some(StdArc::new(|_, _| panic!("excluded peer reached signer")));
        let pool = filtered_pool(
            &handler,
            StdArc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .await;
        let mut frames = handler.sent_subject.subscribe();
        // No connect event: the incoming stream must enforce admission itself.
        handler.inject_message(
            "remote",
            WebRTCMessage {
                id: format!("excluded-{method}"),
                method: method.into(),
                params: vec![json!({"peerSession":{"capabilityToken":"untrusted"}})],
                collection: Some("peer_filter".into()),
            },
        );
        let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
            panic!("response")
        };
        assert_eq!(response.id, format!("excluded-{method}"));
        assert_eq!(response.collection.as_deref(), Some("peer_filter"));
        assert_eq!(response.error.as_deref(), Some("peer_not_allowed"));
        assert!(response.result.is_null());
        await_peer_closed(&handler).await;
        assert!(pool.authenticated_peers.lock().is_empty());
        pool.cancel().await;
    }
}

#[tokio::test]
async fn peer_filter_closes_excluded_outgoing_peer_without_credentials_or_handshake() {
    let handler = MockHandler::new();
    *handler.local_provider.lock() =
        Some(StdArc::new(|_, _| panic!("excluded peer reached signer")));
    let pool = filtered_pool(
        &handler,
        StdArc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .await;
    handler.connect.next(MockPeer("remote".into(), 1));
    await_peer_closed(&handler).await;
    assert!(handler.sent.lock().is_empty());
    assert!(pool.authenticated_peers.lock().is_empty());
    pool.cancel().await;
}

#[tokio::test]
async fn peer_filter_revocation_during_key_store_wait_never_publishes_credentials() {
    for outgoing in [false, true] {
        let handler = MockHandler::new();
        let allowed = StdArc::new(std::sync::atomic::AtomicBool::new(true));
        let started = StdArc::new(tokio::sync::Notify::new());
        let release = StdArc::new(tokio::sync::Notify::new());
        let (start, proceed) = (started.clone(), release.clone());
        *handler.local_provider.lock() = Some(StdArc::new(move |_, nonce| {
            let (start, proceed) = (start.clone(), proceed.clone());
            Box::pin(async move {
                start.notify_one();
                proceed.notified().await;
                Ok(credentials("must-not-leak", nonce))
            })
        }));
        let pool = filtered_pool(&handler, allowed.clone()).await;
        if outgoing {
            handler.connect.next(MockPeer("remote".into(), 1));
        } else {
            inbound(&handler, json!("a".repeat(43)), "revoke-while-signing");
        }
        tokio::time::timeout(Duration::from_secs(3), started.notified())
            .await
            .unwrap();
        allowed.store(false, Ordering::SeqCst);
        release.notify_one();
        await_peer_closed(&handler).await;
        assert!(pool.authenticated_peers.lock().is_empty());
        let sent = handler.sent.lock().clone();
        if outgoing {
            assert!(sent.is_empty());
        } else {
            assert_eq!(sent.len(), 1);
            let WebRTCWireFrame::Response(response) = &sent[0] else {
                panic!("response")
            };
            assert_eq!(
                response.error.as_deref(),
                Some("local_session_credentials_unavailable")
            );
            assert!(response.result.is_null());
        }
        pool.cancel().await;
    }
}

#[tokio::test]
async fn peer_filter_applies_to_an_already_admitted_incoming_peer() {
    let handler = MockHandler::new();
    let allowed = StdArc::new(std::sync::atomic::AtomicBool::new(true));
    *handler.local_provider.lock() = Some(StdArc::new(|_, nonce| {
        Box::pin(async move { Ok(credentials("allowed-token", nonce)) })
    }));
    let pool = filtered_pool(&handler, allowed.clone()).await;
    let mut frames = handler.sent_subject.subscribe();
    inbound(&handler, json!("a".repeat(43)), "allowed");
    let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
        panic!("response")
    };
    assert!(response.error.is_none());
    assert_eq!(
        response.result["peerSession"]["capabilityToken"],
        "allowed-token"
    );
    assert!(pool
        .authenticated_peers
        .lock()
        .contains(&MockPeer("remote".into(), 1)));
    allowed.store(false, Ordering::SeqCst);
    handler.inject_message(
        "remote",
        WebRTCMessage {
            id: "revoked-read".into(),
            method: "masterChangesSince".into(),
            params: vec![Value::Null, json!(10)],
            collection: Some("peer_filter".into()),
        },
    );
    let WebRTCWireFrame::Response(response) = next_frame(&mut frames).await else {
        panic!("response")
    };
    assert_eq!(response.error.as_deref(), Some("peer_not_allowed"));
    assert!(response.result.is_null());
    await_peer_closed(&handler).await;
    assert!(pool.authenticated_peers.lock().is_empty());
    assert!(!pool.is_peer_ready_for_control(&MockPeer("remote".into(), 1)));
    pool.cancel().await;
}
