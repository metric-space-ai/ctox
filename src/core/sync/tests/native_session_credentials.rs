#![cfg(feature = "webrtc")]
//! Real SQLite and WebRTC/UDP, actual P-256 proofs. The capability issuer and
//! policy are an isolated fixture, not the production Business OS policy gate.
#[allow(dead_code)]
#[path = "support/native.rs"]
mod native_fixture;
#[allow(dead_code)]
#[path = "support/signaling.rs"]
mod signaling_fixture;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ctox_sync::native::{NativePeerRole, NativeSyncSession};
use futures::StreamExt;
use ring::{
    rand::SystemRandom,
    signature::{self, EcdsaKeyPair, KeyPair},
};
use rxdb::plugins::replication_webrtc::{
    send_message_and_await_answer, webrtc_types::WebRTCPeerSessionValidation, LocalDeviceProof,
    LocalSessionCredentials, WebRTCMessage,
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn key() -> Arc<EcdsaKeyPair> {
    let rng = SystemRandom::new();
    let pkcs8 =
        EcdsaKeyPair::generate_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    Arc::new(
        EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8.as_ref(),
            &rng,
        )
        .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_device_credentials_unlock_real_webrtc_reads_and_obey_current_revocation() {
    exercise(false, false, TargetFault::None, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_device_credentials_from_another_key_cannot_unlock_replication() {
    exercise(true, false, TargetFault::None, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_peer_filter_revocation_denies_real_webrtc_reads_with_valid_credentials() {
    exercise(false, true, TargetFault::None, false).await;
}

#[derive(Clone, Copy, PartialEq)]
enum TargetFault {
    None,
    Key,
    Instance,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_source_pin_or_instance_never_requests_credentials_over_real_webrtc() {
    for fault in [TargetFault::Key, TargetFault::Instance] {
        exercise(false, false, fault, false).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn data_client_offers_to_passive_source_without_execution_membership() {
    exercise(false, false, TargetFault::None, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn data_client_wrong_target_never_obtains_credentials() {
    for fault in [TargetFault::Key, TargetFault::Instance] {
        exercise(false, false, fault, true).await;
    }
}

async fn exercise(
    wrong_key: bool,
    revoke_peer_only: bool,
    target_fault: TargetFault,
    data_client: bool,
) {
    tokio::time::timeout(Duration::from_secs(35), async {
        let signaling =
            signaling_fixture::SignalingFixture::with_roles(["ctox_instance", if data_client { "browser" } else { "workjet_executor" }])
                .await;
        let expected_key = key();
        let signer = if wrong_key {
            key()
        } else {
            expected_key.clone()
        };
        let public_key = expected_key.public_key().as_ref().to_vec();
        let revoked = Arc::new(AtomicBool::new(false));
        let peer_allowed = Arc::new(AtomicBool::new(true));
        let verified_proofs = Arc::new(AtomicUsize::new(0));
        let credential_requests = Arc::new(AtomicUsize::new(0));
        let public_identity_seen = Arc::new(AtomicBool::new(false));
        let (server_root, server_db, mut server_options) =
            native_fixture::options(signaling.url.clone(), "credential-room", "server-session")
                .await;
        server_options.collections[0]
            .insert(json!({"id":"private-record"}))
            .await
            .unwrap();
        let (policy_revoked, proof_counter) = (revoked.clone(), verified_proofs.clone());
        // Enough data for the real query dispatcher to emit compressed chunks.
        for n in 0..120 {
            server_options.collections[0]
                .insert(json!({"id":format!("z-query-{n:03}-{}", "x".repeat(40))}))
                .await
                .unwrap();
        }
        let peer_gate = peer_allowed.clone();
        server_options.admission.peer = Arc::new(move |_| peer_gate.load(Ordering::SeqCst));
        server_options.admission.session = Arc::new(move |payload, challenge| {
            use WebRTCPeerSessionValidation::{Accept, Defer, Reject};
            if policy_revoked.load(Ordering::SeqCst)
                || payload["peerSession"]["role"] != if data_client { "browser" } else { "workjet_executor" }
                || payload
                    .pointer("/peerSession/sessionId")
                    .and_then(Value::as_str)
                    != Some("workjet-session")
                || payload
                    .pointer("/peerSession/capabilityToken")
                    .and_then(Value::as_str)
                    != Some("fixture-private-read")
            {
                return Reject;
            }
            let Some(nonce) = challenge else {
                return Defer;
            };
            let proof = &payload["peerSession"]["deviceProof"];
            if proof["version"] != "ctox-device-proof-v1"
                || proof["nonce"] != nonce
                || proof["publicJwk"]["x"] != URL_SAFE_NO_PAD.encode(&public_key[1..33])
                || proof["publicJwk"]["y"] != URL_SAFE_NO_PAD.encode(&public_key[33..65])
            {
                return Reject;
            }
            let Some(signature_bytes) = proof["signature"]
                .as_str()
                .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
            else {
                return Reject;
            };
            if signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, &public_key)
                .verify(nonce.as_bytes(), &signature_bytes)
                .is_err()
            {
                return Reject;
            }
            proof_counter.fetch_add(1, Ordering::SeqCst);
            Accept
        });
        let policy_revoked = revoked.clone();
        server_options.admission.collection_read = Some(Arc::new(move |token, collection| {
            token == "fixture-private-read"
                && collection == "records"
                && !policy_revoked.load(Ordering::SeqCst)
        }));
        server_options.admission.eager_pull = Some(Arc::new(|_, _| true));
        let source_key = Arc::new(ctox_sync::authority::auth::SigningIdentity::from_pkcs8(
            &ctox_sync::authority::auth::SigningIdentity::generate_pkcs8().unwrap()
        ).unwrap());
        let source_pin = source_key.public_identity();
        let identity_revoked = revoked.clone();
        let public_seen = public_identity_seen.clone();
        let server = NativeSyncSession::start_with_pool_setup(server_options, |pool| {
        let identity_transport = pool.connection_handler.clone();
        pool.register_identity_request_handler(
            ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_IDENTITY_METHOD,
            Arc::new(move |peer_id, token, params| {
                let key = source_key.clone();
                let revoked = identity_revoked.clone();
                let transport = identity_transport.clone();
                let public_seen = public_seen.clone();
                Box::pin(async move {
                    let connection = transport.connection_for_peer(&peer_id).ok_or_else(|| "retired peer".to_string())?;
                    let channel_binding = transport.channel_binding(&connection).await.map_err(|_| "invalid channel".to_string())?;
                    if (!token.is_empty() && token != "fixture-private-read") || revoked.load(Ordering::SeqCst) {
                        return Err("identity capability rejected".into());
                    }
                    let request: ctox_sync::business_data_contract::NativeBusinessDataIdentityRequest =
                        serde_json::from_value(params[0].clone()).map_err(|_| "invalid request".to_string())?;
                    let proof = key.attest_business_data_identity("fixture-instance", &request.challenge, &channel_binding,
                        (!token.is_empty()).then(|| ctox_sync::business_data_contract::NativeBusinessDataPrincipal {
                            user_id: "fixture-user".into(), authorization_epoch: 1, device: None,
                        })).map_err(|_| "invalid challenge".to_string())?;
                    if token.is_empty() { public_seen.store(true, Ordering::SeqCst); }
                    serde_json::to_value(proof).map_err(|_| "invalid identity".to_string())
                })
            }),
        )
        }).await.unwrap();
        let mut server_errors = server.pool().error_subject.subscribe();
        let (client_root, client_db, mut client_options) = native_fixture::control_options(
            signaling.url.clone(),
            "credential-room",
            "workjet-session",
        )
        .await;
        client_options.peer_role = NativePeerRole::WorkjetExecutor;
        let signed_challenges = Arc::new(AtomicUsize::new(0));
        let counter = signed_challenges.clone();
        let credential_counter = credential_requests.clone();
        let public_seen = public_identity_seen.clone();
        let credentials: rxdb::plugins::replication_webrtc::LocalSessionProvider<
            rxdb::plugins::replication_webrtc::WebRTCRsConnection,
        > = Arc::new(move |_connection, nonce| {
            assert!(public_seen.load(Ordering::SeqCst), "credentials accessed before source proof");
            credential_counter.fetch_add(1, Ordering::SeqCst);
            let signer = signer.clone();
            let counter = counter.clone();
            Box::pin(async move {
                let device_proof = nonce.map(|nonce| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let signature = signer.sign(&SystemRandom::new(), nonce.as_bytes()).unwrap();
                    let public = signer.public_key().as_ref();
                    LocalDeviceProof {
                        public_x: URL_SAFE_NO_PAD.encode(&public[1..33]),
                        public_y: URL_SAFE_NO_PAD.encode(&public[33..65]),
                        signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
                    }
                });
                Ok(LocalSessionCredentials {
                    capability_token: "fixture-private-read".into(),
                    device_proof,
                })
            })
        });
        let pinned_key = if target_fault == TargetFault::Key {
            ctox_sync::authority::auth::SigningIdentity::from_pkcs8(
                &ctox_sync::authority::auth::SigningIdentity::generate_pkcs8().unwrap()
            ).unwrap().public_identity()
        } else { source_pin.clone() };
        client_options.local_session_provider = Some(Arc::new(move |_connection| {
            let public_identity = pinned_key.clone();
            let credentials = credentials.clone();
            Box::pin(async move {
                Ok(ctox_sync::native::NativeSessionTarget {
                    public_identity,
                    instance_id: if target_fault == TargetFault::Instance { "wrong-instance" } else { "fixture-instance" }.into(),
                    credentials,
                })
            })
        }));
        let mut client_errors = None;
        let setup = |pool: &ctox_sync::native::NativePool| {
            client_errors = Some(pool.error_subject.subscribe());
            Ok(())
        };
        let client = if data_client {
            NativeSyncSession::start_data_client_with_pool_setup(client_options, setup).await.unwrap()
        } else {
            NativeSyncSession::start_with_pool_setup(client_options, setup).await.unwrap()
        };
        let mut client_errors = client_errors.expect("observer installed before join");
        if data_client {
            assert!(server.connect_data_peer("native000002".into()).await.is_err());
            assert!(client.connect_data_peer("native000002".into()).await.is_err());
            assert!(client.pool().connection_handler
                .connect_native_execution_peer("native000001".into()).await.is_err());
        }
        // No manual offer for data clients: the owned discovery must establish
        // the actual channel, including with the larger signaling ID.
        while !data_client {
            let connected = server
                .pool()
                .connection_handler
                .connect_native_execution_peer("native000002".into())
                .await
                .is_ok();
            if connected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if target_fault != TargetFault::None {
            // Signaling/transport diagnostics share this stream. Only the
            // credential-admission rejection proves that the target check ran;
            // an unrelated first event cannot stand in for that boundary.
            // The enclosing 35-second deadline still bounds this wait.
            loop {
                let error = client_errors.next().await
                    .expect("peer error stream closed before credential-admission rejection");
                if error.parameters()["code"] == "local_session_credentials_unavailable" {
                    break;
                }
            }
            assert!(public_identity_seen.load(Ordering::SeqCst), "public proof traversed WebRTC");
            assert_eq!(credential_requests.load(Ordering::SeqCst), 0);
            assert_eq!(signed_challenges.load(Ordering::SeqCst), 0);
            assert_eq!(verified_proofs.load(Ordering::SeqCst), 0);
            assert!(!signaling_fixture::route_ready(server.pool(), "native000002"));
        } else if wrong_key {
            let error = server_errors.next().await.unwrap();
            assert_eq!(error.parameters()["code"], "peer_authentication_failed");
            assert_eq!(verified_proofs.load(Ordering::SeqCst), 0);
            assert!(!signaling_fixture::route_ready(
                server.pool(),
                "native000002"
            ));
        } else {
            while !signaling_fixture::route_ready(server.pool(), "native000002")
                || !signaling_fixture::route_ready(client.pool(), "native000001")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(verified_proofs.load(Ordering::SeqCst) > 0);
            assert!(signed_challenges.load(Ordering::SeqCst) > 0);
            let connection = client
                .pool()
                .connection_handler
                .connection_for_peer("native000001")
                .unwrap();
            // Actual nonce/signature exchanges over the established DataChannel.
            // The actual core now verifies the enrolled fixture key before
            // credentials. Workjet's production enrollment UI is still separate.
            let mut identity_timings = Vec::new();
            for _ in 0..30 {
                let started = Instant::now();
                let proof = client.peer_identity_proof(connection.clone(), &source_pin, "fixture-instance").await.unwrap();
                assert_eq!(proof.principal.unwrap().user_id, "fixture-user");
                identity_timings.push(started.elapsed().as_micros());
            }
            identity_timings.sort_unstable();
            eprintln!("native_peer_identity n=30 p50_us={} p95_us={}", identity_timings[14], identity_timings[28]);
            assert!(client.peer_identity_proof(connection.clone(), &source_pin, "wrong-instance").await.is_err());
            let other_key = ctox_sync::authority::auth::SigningIdentity::from_pkcs8(
                &ctox_sync::authority::auth::SigningIdentity::generate_pkcs8().unwrap()).unwrap();
            assert!(client.peer_identity_proof(connection.clone(), &other_key.public_identity(), "fixture-instance").await.is_err());
            let mut timings = Vec::new();
            for n in 0..30 {
                let started = Instant::now();
                let response = send_message_and_await_answer(
                    client.pool().connection_handler.clone(),
                    connection.clone(),
                    WebRTCMessage {
                        id: format!("authorized-read-{n}"),
                        method: "masterChangesSince".into(),
                        params: vec![Value::Null, json!(20)],
                        collection: Some("records".into()),
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    response.result["documents"][0]["id"], "private-record",
                    "{response:?}"
                );
                timings.push(started.elapsed().as_micros());
            }
            timings.sort_unstable();
            eprintln!(
                "native_authenticated_read n=30 p50_us={} p95_us={}",
                timings[14], timings[28]
            );
            let mut query_timings = Vec::new();
            for _ in 0..30 {
                let started = Instant::now();
                let page = client
                    .query_page(
                        connection.clone(),
                        rxdb::plugins::replication_webrtc::query_fetch_handler::QueryFetchRequest {
                            request_id: "caller-id-is-replaced".into(),
                            database_name: None,
                            collection_name: "records".into(),
                            schema_version: 0,
                            query_fingerprint: "native-query-fixture".into(),
                            query: json!({"selector":{"id":{"$gte":"z-query-"}}}),
                            window: json!({"offset":10,"limit":100}),
                            projection: Some(vec!["id".into()]),
                        },
                    )
                    .await
                    .unwrap();
                assert_eq!(page.documents.len(), 100);
                assert_eq!(
                    page.documents[0]["id"],
                    format!("z-query-010-{}", "x".repeat(40))
                );
                assert_eq!(
                    page.documents[99]["id"],
                    format!("z-query-109-{}", "x".repeat(40))
                );
                assert!(page
                    .documents
                    .iter()
                    .all(|doc| doc.as_object().unwrap().len() == 1));
                query_timings.push(started.elapsed().as_micros());
            }
            query_timings.sort_unstable();
            eprintln!(
                "native_query_page n=30 documents=100 p50_us={} p95_us={}",
                query_timings[14], query_timings[28]
            );
            if revoke_peer_only {
                // Keep the capability, proof and document policy valid: only
                // the incoming peer filter can deny this read.
                peer_allowed.store(false, Ordering::SeqCst);
            } else {
                revoked.store(true, Ordering::SeqCst);
            }
            let result = send_message_and_await_answer(
                client.pool().connection_handler.clone(),
                connection,
                WebRTCMessage {
                    id: "after-revocation".into(),
                    method: "masterChangesSince".into(),
                    params: vec![Value::Null, json!(20)],
                    collection: Some("records".into()),
                },
            )
            .await;
            if revoke_peer_only {
                let error = server_errors.next().await.unwrap();
                assert_eq!(error.parameters()["code"], "peer_not_allowed");
                assert!(!signaling_fixture::route_ready(
                    server.pool(),
                    "native000002"
                ));
                match result {
                    Ok(response) => {
                        assert_eq!(response.error.as_deref(), Some("peer_not_allowed"));
                        assert!(response.result.is_null());
                    }
                    // Closing the transport may overtake its final denial.
                    // The server error above must still prove the exact gate.
                    Err(error) => assert_eq!(
                        error.parameters()["message"],
                        "peer disconnected before an answer was received"
                    ),
                }
                assert!(!revoked.load(Ordering::SeqCst));
            } else {
                let response = result.unwrap();
                assert_eq!(response.result["code"], "RC_WEBRTC_PEER");
                assert!(response.result.get("documents").is_none());
            }
        }
        client.shutdown().await;
        server.shutdown().await;
        client_db.close().await.unwrap();
        server_db.close().await.unwrap();
        drop((client_root, server_root));
    })
    .await
    .expect("native authenticated data exchange deadline");
}
