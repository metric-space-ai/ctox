use super::*;
use crate::plugins::replication_webrtc::collection_authority::COLLECTION_AUTHORITY_ATTEMPTS;

async fn authority_pool(
    name: &str,
) -> (StdArc<RxWebRTCReplicationPool<MockHandler>>, StdArc<MockHandler>) {
    let collection = crate::rx_collection::test_support::test_collection_named(name).await;
    collection
        .insert(serde_json::json!({ "id": "protected-row", "age": 1 }))
        .await
        .expect("seed native collection");
    let handler = MockHandler::new();
    let pool = replicate_web_rtc_multi(
        vec![collection],
        handler.clone(),
        None,
        Some(format!("{name}-room")),
        Some(StdArc::<str>::from("authority-test-native")),
    )
    .await
    .expect("bring up replication responder");
    (pool, handler)
}

async fn authority_response(
    handler: &MockHandler,
    collection: &str,
    method: &str,
) -> WebRTCResponse {
    let id = format!("{collection}-{method}");
    let mut sent = handler.sent_subject.subscribe();
    handler.inject_message(
        "authority-browser",
        WebRTCMessage {
            id: id.clone(),
            method: method.into(),
            params: if method == "masterChangesSince" {
                vec![Value::Null, Value::from(10u64)]
            } else {
                vec![serde_json::json!([])]
            },
            collection: Some(collection.into()),
        },
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(frame) = sent.next().await {
            if let WebRTCWireFrame::Response(response) = frame {
                if response.id == id {
                    return response;
                }
            }
        }
        panic!("response stream closed");
    })
    .await
    .expect("bounded admission must answer")
}

fn assert_admission_error(response: &WebRTCResponse, method: &str, code: &str, retryable: bool) {
    let result = &response.result;
    assert_eq!(response.error, None);
    assert_eq!(result["type"], "ctoxError");
    assert_eq!(result["scope"], "replication");
    assert_eq!(result["code"], code);
    assert_eq!(result["retryable"], retryable);
    assert_eq!(result["phase"], "replication-io");
    assert_eq!(result["collection"], response.collection.as_deref().unwrap());
    assert_eq!(result["direction"], if method == "masterWrite" { "push" } else { "pull" });
    assert!(result.get("documents").is_none(), "denied admission cannot expose rows");
    assert!(result.get("checkpoint").is_none(), "denied admission cannot confirm a checkpoint");
    assert!(!result.is_array(), "denied write cannot acknowledge an empty conflict list");
    assert!(!result.to_string().contains("private-authority-detail"));
}

#[tokio::test]
async fn master_pull_recovers_after_transient_collection_authority() {
    let name = "authority_pull_recovers";
    let (pool, handler) = authority_pool(name).await;
    let checks = StdArc::new(AtomicU64::new(0));
    let hook_checks = checks.clone();
    *handler.collection_authority.lock() = Some(StdArc::new(move |_, _| {
        if hook_checks.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(new_rx_error("COLLECTION_AUTHORITY_UNAVAILABLE", None))
        } else {
            Ok(true)
        }
    }));
    let response = authority_response(handler.as_ref(), name, "masterChangesSince").await;
    assert_eq!(checks.load(Ordering::SeqCst), 3);
    assert_eq!(handler.collection_authority_checks.load(Ordering::SeqCst), 3);
    assert_eq!(response.result["documents"][0]["id"], "protected-row");
    assert!(response.result.get("checkpoint").is_some());
    assert!(response.result.get("code").is_none());
    pool.cancel().await;
}

#[tokio::test]
async fn master_rpcs_exhaust_unavailable_authority_without_rows_or_acknowledgement() {
    let name = "authority_rpc_unavailable";
    let (pool, handler) = authority_pool(name).await;
    *handler.collection_authority.lock() = Some(StdArc::new(|_, _| {
        Err(new_rx_error(
            "COLLECTION_AUTHORITY_UNAVAILABLE",
            Some(serde_json::json!({"secret": "private-authority-detail"})),
        ))
    }));
    for method in ["masterChangesSince", "masterWrite"] {
        handler.collection_authority_checks.store(0, Ordering::SeqCst);
        let response = authority_response(handler.as_ref(), name, method).await;
        assert_admission_error(&response, method, "COLLECTION_AUTHORITY_UNAVAILABLE", true);
        assert_eq!(
            handler.collection_authority_checks.load(Ordering::SeqCst),
            COLLECTION_AUTHORITY_ATTEMPTS as u64,
        );
    }
    pool.cancel().await;
}

#[tokio::test]
async fn master_rpcs_reject_policy_denial_without_retry() {
    let name = "authority_rpc_denied";
    let (pool, handler) = authority_pool(name).await;
    *handler.collection_authority.lock() = Some(StdArc::new(|_, _| Ok(false)));
    for method in ["masterChangesSince", "masterWrite"] {
        handler.collection_authority_checks.store(0, Ordering::SeqCst);
        let response = authority_response(handler.as_ref(), name, method).await;
        assert_admission_error(&response, method, "RC_WEBRTC_PEER", false);
        assert_eq!(handler.collection_authority_checks.load(Ordering::SeqCst), 1);
    }
    pool.cancel().await;
}

#[tokio::test]
async fn master_rpcs_do_not_retry_other_authority_errors() {
    let name = "authority_rpc_corrupt";
    let (pool, handler) = authority_pool(name).await;
    *handler.collection_authority.lock() = Some(StdArc::new(|_, _| {
        Err(new_rx_error(
            "AUTHORITY_CORRUPT",
            Some(serde_json::json!({"secret": "private-authority-detail"})),
        ))
    }));
    for method in ["masterChangesSince", "masterWrite"] {
        handler.collection_authority_checks.store(0, Ordering::SeqCst);
        let response = authority_response(handler.as_ref(), name, method).await;
        assert_admission_error(&response, method, "AUTHORITY_CORRUPT", false);
        assert_eq!(handler.collection_authority_checks.load(Ordering::SeqCst), 1);
    }
    pool.cancel().await;
}

#[tokio::test]
async fn master_pull_stops_when_policy_denies_during_availability_retry() {
    let name = "authority_pull_revoked";
    let (pool, handler) = authority_pool(name).await;
    let checks = StdArc::new(AtomicU64::new(0));
    let hook_checks = checks.clone();
    *handler.collection_authority.lock() = Some(StdArc::new(move |_, _| {
        if hook_checks.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(new_rx_error("COLLECTION_AUTHORITY_UNAVAILABLE", None))
        } else {
            Ok(false)
        }
    }));
    let response = authority_response(handler.as_ref(), name, "masterChangesSince").await;
    assert_admission_error(&response, "masterChangesSince", "RC_WEBRTC_PEER", false);
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(handler.collection_authority_checks.load(Ordering::SeqCst), 2);
    pool.cancel().await;
}
