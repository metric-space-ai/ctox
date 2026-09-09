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
    LocalSessionCredentials, WebRTCConnectionHandler, WebRTCMessage,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_device_credentials_unlock_real_webrtc_reads_and_obey_current_revocation() {
    exercise(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_device_credentials_from_another_key_cannot_unlock_replication() {
    exercise(true).await;
}

async fn exercise(wrong_key: bool) {
    tokio::time::timeout(Duration::from_secs(35), async {
        let signaling =
            signaling_fixture::SignalingFixture::with_roles(["ctox_instance", "workjet_executor"])
                .await;
        let expected_key = key();
        let signer = if wrong_key {
            key()
        } else {
            expected_key.clone()
        };
        let public_key = expected_key.public_key().as_ref().to_vec();
        let revoked = Arc::new(AtomicBool::new(false));
        let verified_proofs = Arc::new(AtomicUsize::new(0));
        let (server_root, server_db, mut server_options) =
            native_fixture::options(signaling.url.clone(), "credential-room", "server-session")
                .await;
        server_options.collections[0]
            .insert(json!({"id":"private-record"}))
            .await
            .unwrap();
        let (policy_revoked, proof_counter) = (revoked.clone(), verified_proofs.clone());
        server_options.admission.session = Arc::new(move |payload, challenge| {
            use WebRTCPeerSessionValidation::{Accept, Defer, Reject};
            if policy_revoked.load(Ordering::SeqCst)
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
        let server = NativeSyncSession::start(server_options).await.unwrap();
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
        client_options.local_session_provider = Some(Arc::new(move |connection, nonce| {
            // This test fixture pins its server route. Production hosts must
            // resolve authenticated instance identity; a route is not a proof.
            assert_eq!(connection.peer_id(), "native000001");
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
        }));
        let client = NativeSyncSession::start(client_options).await.unwrap();
        // Wait for role-bearing signaling membership; only the lower ID offers.
        loop {
            if server
                .pool()
                .connection_handler
                .connect_native_execution_peer("native000002".into())
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if wrong_key {
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
            revoked.store(true, Ordering::SeqCst);
            let response = send_message_and_await_answer(
                client.pool().connection_handler.clone(),
                connection,
                WebRTCMessage {
                    id: "after-revocation".into(),
                    method: "masterChangesSince".into(),
                    params: vec![Value::Null, json!(20)],
                    collection: Some("records".into()),
                },
            )
            .await
            .unwrap();
            assert_eq!(response.result["code"], "RC_WEBRTC_PEER");
            assert!(response.result.get("documents").is_none());
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
