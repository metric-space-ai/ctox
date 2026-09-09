#[tokio::test]
async fn native_query_concurrency_is_bounded_and_pool_cancel_wakes_all_waiters() {
    let (handler, pool, peer, first, _) = start().await;
    let mut tasks = vec![first];
    let mut frames = handler.sent_subject.subscribe();
    let limit = crate::plugins::replication_webrtc::protocol_contract_generated::CTOX_QUERY_MAX_IN_FLIGHT_STREAMS;
    for _ in 1..limit {
        tasks.push(tokio::spawn(fetch_query_page(
            pool.clone(),
            peer.clone(),
            request(),
        )));
        tokio::time::timeout(Duration::from_secs(3), frames.next())
            .await
            .unwrap()
            .unwrap();
    }
    let error = fetch_query_page(pool.clone(), peer, request())
        .await
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "local_query_limit");
    pool.cancel().await;
    for task in tasks {
        let error = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(matches!(
            error.parameters()["reason"].as_str(),
            Some("query_pool_closed" | "peer_not_ready")
        ));
    }
}

use super::*;
use crate::plugins::replication_webrtc::query_fetch_client::{fetch_query_page, QueryPage};
use crate::plugins::replication_webrtc::query_fetch_handler::QueryFetchRequest;
use serde_json::json;
use std::io::Write;
include!("query_fetch_client_compression_tests.rs");

fn request() -> QueryFetchRequest {
    QueryFetchRequest {
        request_id: "reused-ui-id".into(),
        database_name: None,
        collection_name: "query_client".into(),
        schema_version: 0,
        query_fingerprint: "query-client-fixture".into(),
        query: json!({"selector":{}}),
        window: json!({"offset":0,"limit":20}),
        projection: None,
    }
}

type QueryFixture = (
    StdArc<MockHandler>,
    StdArc<RxWebRTCReplicationPool<MockHandler>>,
    MockPeer,
    tokio::task::JoinHandle<Result<QueryPage, RxError>>,
    String,
);

async fn start() -> QueryFixture {
    let handler = MockHandler::new();
    let collection =
        crate::rx_collection::test_support::test_collection_named("query_client").await;
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("remote".into(), 1);
    pool.mark_peer_admitted(&peer, true);
    let mut frames = handler.sent_subject.subscribe();
    let task = tokio::spawn(fetch_query_page(pool.clone(), peer.clone(), request()));
    let frame = tokio::time::timeout(Duration::from_secs(3), frames.next())
        .await
        .unwrap()
        .unwrap();
    let WebRTCWireFrame::Message(message) = frame else {
        panic!("query request")
    };
    assert_eq!(message.method, "rxdb.query.fetch");
    assert_ne!(message.id, "reused-ui-id");
    assert_eq!(message.params[0]["requestId"], message.id);
    (handler, pool, peer, task, message.id)
}

fn ack(handler: &MockHandler, peer: &MockPeer, id: &str) {
    handler.response.next(PeerWithResponse {
        peer: peer.clone(),
        response: WebRTCResponse {
            id: id.into(),
            result: json!({"accepted":true,"requestId":id}),
            error: None,
            collection: None,
        },
    });
}

fn chunk(
    handler: &MockHandler,
    peer: MockPeer,
    id: &str,
    sequence: u32,
    complete: bool,
    documents: Value,
) {
    handler.message.next(PeerWithMessage { peer, message: WebRTCMessage {
        id: format!("{id}-{sequence}"), method: "rxdb.query.chunk".into(), collection: None,
        params: vec![json!({"requestId":id,"sequence":sequence,"complete":complete,"documents":documents})],
    }});
}

#[tokio::test]
async fn query_page_waits_for_ack_and_complete_and_ignores_other_generations() {
    let (handler, pool, peer, task, id) = start().await;
    chunk(
        &handler,
        MockPeer("remote".into(), 2),
        &id,
        0,
        true,
        json!([{"id":"wrong-generation"}]),
    );
    chunk(
        &handler,
        peer.clone(),
        "other-request",
        0,
        true,
        json!([{"id":"wrong-query"}]),
    );
    chunk(
        &handler,
        peer.clone(),
        &id,
        0,
        false,
        json!([{"id":"first"}]),
    );
    chunk(
        &handler,
        peer.clone(),
        &id,
        1,
        true,
        json!([{"id":"second"}]),
    );
    // Ack and data arrive through different subscriptions; either may be polled first.
    ack(&handler, &peer, &id);
    let page = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        page.documents,
        vec![json!({"id":"first"}), json!({"id":"second"})]
    );
    pool.cancel().await;
}

#[tokio::test]
async fn query_page_sequence_gap_fails_and_sends_scoped_cancel() {
    let (handler, pool, peer, task, id) = start().await;
    let mut frames = handler.sent_subject.subscribe();
    ack(&handler, &peer, &id);
    chunk(&handler, peer, &id, 2, true, json!([{"id":"partial"}]));
    let error = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "chunk_sequence_gap");
    let frame = tokio::time::timeout(Duration::from_secs(3), frames.next())
        .await
        .unwrap()
        .unwrap();
    let WebRTCWireFrame::Message(message) = frame else {
        panic!("cancel")
    };
    assert_eq!(message.method, "rxdb.query.cancel");
    assert_eq!(message.params[0]["requestId"], id);
    pool.cancel().await;
}

#[tokio::test]
async fn dropping_native_query_cancels_the_remote_request() {
    let (handler, pool, _peer, task, id) = start().await;
    let mut frames = handler.sent_subject.subscribe();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let frame = tokio::time::timeout(Duration::from_secs(3), frames.next())
        .await
        .unwrap()
        .unwrap();
    let WebRTCWireFrame::Message(message) = frame else {
        panic!("cancel")
    };
    assert_eq!(message.method, "rxdb.query.cancel");
    assert_eq!(message.params[0]["requestId"], id);
    pool.cancel().await;
}

#[tokio::test]
async fn query_page_disconnect_discards_partial_data_without_waiting_for_timeout() {
    let (handler, pool, peer, task, id) = start().await;
    ack(&handler, &peer, &id);
    chunk(
        &handler,
        peer.clone(),
        &id,
        0,
        false,
        json!([{"id":"partial"}]),
    );
    handler.disconnect.next(peer);
    let error = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "peer_disconnected");
    pool.cancel().await;
}

#[tokio::test]
async fn query_page_never_reads_before_admission_or_with_unbounded_window() {
    let handler = MockHandler::new();
    let collection =
        crate::rx_collection::test_support::test_collection_named("query_client").await;
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("remote".into(), 1);
    let error = fetch_query_page(pool.clone(), peer.clone(), request())
        .await
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "peer_not_ready");
    pool.mark_peer_admitted(&peer, true);
    let mut unbounded = request();
    unbounded.window = json!({"limit":10000000});
    let error = fetch_query_page(pool.clone(), peer, unbounded)
        .await
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "page_limit_required");
    assert!(handler.sent.lock().is_empty());
    pool.cancel().await;
}
