use super::*;
use crate::plugins::replication_webrtc::{
    file_fetch_client::{fetch_file_range, FileRangeBytes, MAX_NATIVE_FILE_RANGE_BYTES},
    file_fetch_handler::{FileFetchRequest, FileRange},
};
use base64::Engine;
use sha2::{Digest, Sha256};

fn request(length: u64) -> FileFetchRequest {
    FileFetchRequest {
        request_id: "caller-id".into(),
        collection_name: "files".into(),
        file_id: "blob".into(),
        range: Some(FileRange {
            offset: 1024,
            length,
        }),
        known_sequences: vec![],
    }
}

type Fixture = (
    StdArc<MockHandler>,
    StdArc<RxWebRTCReplicationPool<MockHandler>>,
    MockPeer,
    tokio::task::JoinHandle<Result<FileRangeBytes, RxError>>,
    String,
);

async fn start(length: u64) -> Fixture {
    let handler = MockHandler::new();
    let collection = crate::rx_collection::test_support::test_collection_named("files").await;
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("remote".into(), 1);
    pool.mark_peer_admitted(&peer, false);
    pool.mark_peer_admitted(&peer, true);
    let mut sent = handler.sent_subject.subscribe();
    let task = tokio::spawn(fetch_file_range(
        pool.clone(),
        peer.clone(),
        request(length),
    ));
    let frame = tokio::time::timeout(Duration::from_secs(3), sent.next())
        .await
        .unwrap()
        .unwrap();
    let WebRTCWireFrame::Message(message) = frame else {
        panic!("file request")
    };
    assert_eq!(message.method, "rxdb.file.fetch");
    assert_ne!(message.id, "caller-id");
    assert_eq!(message.params[0]["range"]["offset"], 1024);
    (handler, pool, peer, task, message.id)
}

fn ack(handler: &MockHandler, peer: &MockPeer, id: &str) {
    handler.response.next(PeerWithResponse {
        peer: peer.clone(),
        response: WebRTCResponse {
            id: id.into(),
            result: serde_json::json!({"accepted":true,"requestId":id}),
            error: None,
            collection: None,
        },
    });
}
fn payload(id: &str, sequence: u32, bytes: &[u8], complete: bool) -> Value {
    serde_json::json!({"requestId":id,"sequence":sequence,"bytesBase64":base64::engine::general_purpose::STANDARD.encode(bytes),
        "hash":format!("{:x}", Sha256::digest(bytes)),"complete":complete})
}
fn chunk(handler: &MockHandler, peer: MockPeer, value: Value) {
    handler.message.next(PeerWithMessage {
        peer,
        message: WebRTCMessage {
            id: "frame".into(),
            method: "rxdb.file.chunk".into(),
            collection: Some("files".into()),
            params: vec![value],
        },
    });
}

#[tokio::test]
async fn native_file_range_decodes_each_padded_chunk_and_requires_terminal_and_ack() {
    let (handler, pool, peer, task, id) = start(3).await;
    chunk(
        &handler,
        MockPeer("remote".into(), 2),
        payload(&id, 0, b"bad", true),
    );
    chunk(&handler, peer.clone(), payload("other", 0, b"bad", true));
    chunk(&handler, peer.clone(), payload(&id, 0, b"a", false));
    chunk(&handler, peer.clone(), payload(&id, 1, b"bc", false));
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    chunk(&handler, peer.clone(), payload(&id, 2, b"", true));
    ack(&handler, &peer, &id);
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.offset, 1024);
    assert_eq!(result.bytes, b"abc");
    pool.cancel().await;
}

#[tokio::test]
async fn native_file_range_rejects_corrupt_malformed_gapped_short_and_oversized_data() {
    for (kind, reason) in [
        ("hash", "chunk_hash_mismatch"),
        ("base64", "invalid_base64"),
        ("gap", "chunk_sequence_gap"),
        ("short", "range_incomplete"),
        ("large", "range_too_large"),
        ("cancel", "file_cancelled"),
    ] {
        let (handler, pool, peer, task, id) = start(3).await;
        let mut frames = handler.sent_subject.subscribe();
        ack(&handler, &peer, &id);
        let mut value = payload(&id, 0, b"abc", false);
        match kind {
            "hash" => value["hash"] = serde_json::json!("00"),
            "base64" => value["bytesBase64"] = serde_json::json!("???"),
            "gap" => value["sequence"] = serde_json::json!(1),
            "short" => value = payload(&id, 0, b"", true),
            "large" => value = payload(&id, 0, b"abcd", false),
            "cancel" => value["cancelled"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        chunk(&handler, peer, value);
        let error = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.parameters()["reason"], reason);
        let frame = tokio::time::timeout(Duration::from_secs(3), frames.next())
            .await
            .unwrap()
            .unwrap();
        let WebRTCWireFrame::Message(message) = frame else {
            panic!("file cancel")
        };
        assert_eq!(message.method, "rxdb.file.cancel");
        assert_eq!(message.params[0]["requestId"], id);
        pool.cancel().await;
    }
}

#[tokio::test]
async fn native_file_drop_and_disconnect_discard_partial_and_cancel() {
    for drop_request in [false, true] {
        let (handler, pool, peer, task, id) = start(3).await;
        let mut frames = handler.sent_subject.subscribe();
        ack(&handler, &peer, &id);
        chunk(&handler, peer.clone(), payload(&id, 0, b"a", false));
        if drop_request {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            handler.disconnect.next(peer);
            let error = tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(error.parameters()["reason"], "peer_disconnected");
        }
        let frame = tokio::time::timeout(Duration::from_secs(3), frames.next())
            .await
            .unwrap()
            .unwrap();
        let WebRTCWireFrame::Message(message) = frame else {
            panic!("file cancel")
        };
        assert_eq!(message.method, "rxdb.file.cancel");
        pool.cancel().await;
    }
}

#[tokio::test]
async fn native_file_requires_admission_bounded_ranges_and_shared_capacity() {
    let handler = MockHandler::new();
    let collection = crate::rx_collection::test_support::test_collection_named("files").await;
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("remote".into(), 1);
    let error = fetch_file_range(pool.clone(), peer.clone(), request(1))
        .await
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "peer_not_ready");
    for invalid in [
        None,
        Some(FileRange {
            offset: 0,
            length: MAX_NATIVE_FILE_RANGE_BYTES + 1,
        }),
        Some(FileRange {
            offset: u64::MAX,
            length: 1,
        }),
    ] {
        let mut req = request(1);
        req.range = invalid;
        assert!(fetch_file_range(pool.clone(), peer.clone(), req)
            .await
            .is_err());
    }
    pool.mark_peer_admitted(&peer, false);
    pool.mark_peer_admitted(&peer, true);
    let permits = pool.native_query_semaphore.available_permits() as u32;
    let _held = pool
        .native_query_semaphore
        .clone()
        .try_acquire_many_owned(permits)
        .unwrap();
    let error = fetch_file_range(pool.clone(), peer, request(1))
        .await
        .unwrap_err();
    assert_eq!(error.parameters()["reason"], "local_fetch_limit");
    assert!(handler.sent.lock().is_empty());
    pool.cancel().await;
}

#[tokio::test]
async fn native_file_empty_range_still_requires_remote_completion() {
    let (handler, pool, peer, task, id) = start(0).await;
    ack(&handler, &peer, &id);
    chunk(&handler, peer, payload(&id, 0, b"", true));
    assert!(tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .bytes
        .is_empty());
    pool.cancel().await;
}
