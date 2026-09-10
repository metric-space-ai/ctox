#![cfg(feature = "webrtc")]
//! Real WebRTC plus the private BusinessData host frame client.
//! The isolated capability issuer is not the production policy gate.
#[allow(dead_code)]
#[path = "support/native.rs"]
mod native_fixture;
#[allow(dead_code)]
#[path = "support/signaling.rs"]
mod signaling_fixture;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ctox_sync::{
    business_data_contract::{
        NativeBusinessDataErrorCode, NativeBusinessDataHostFrame as Frame,
        NativeBusinessDataIdentityRequest, NativeBusinessDataOperation,
        NativeBusinessDataPrincipal, NativeBusinessDataRequest, NativeBusinessDataResponse,
        NativeBusinessDataResult, NativeBusinessDataScope, NativeBusinessDataSessionRef,
        NativeBusinessDataSessionState, CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
    },
    business_data_ipc::BusinessDataIpc,
    business_data_session::{
        BusinessDataService, BusinessDataSessionHost, SavedBusinessDataTarget,
    },
    native::{NativePeerRole, NativeSyncOptions, NativeSyncSession},
};
use ring::{
    rand::SystemRandom,
    signature::{self, EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_FIXED},
};
use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

const AUTHORIZATION_EPOCH: u64 = 12;
const ACCOUNT_EPOCH: u64 = 34;

struct TestHost {
    options: Mutex<Option<NativeSyncOptions>>,
    current_saved: Mutex<Option<SavedBusinessDataTarget>>,
    current_principal: Mutex<Option<NativeBusinessDataPrincipal>>,
}

#[async_trait::async_trait]
impl BusinessDataSessionHost for TestHost {
    async fn saved_target(
        &self,
        target_id: &str,
    ) -> std::io::Result<Option<SavedBusinessDataTarget>> {
        if target_id != "saved-fixture" {
            return Ok(None);
        }
        Ok(self.current_saved.lock().await.clone())
    }

    async fn current_principal(
        &self,
        target_id: &str,
    ) -> std::io::Result<Option<NativeBusinessDataPrincipal>> {
        if target_id != "saved-fixture" {
            return Ok(None);
        }
        Ok(self.current_principal.lock().await.clone())
    }

    async fn native_options(&self, target_id: &str) -> std::io::Result<NativeSyncOptions> {
        if target_id != "saved-fixture" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unknown target",
            ));
        }
        self.options
            .lock()
            .await
            .take()
            .ok_or_else(|| std::io::Error::other("BusinessData options already used"))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PrincipalFault {
    None,
    Wrong,
    Missing,
}

#[derive(Clone, Copy, PartialEq)]
enum AuthorizationInvalidation {
    None,
    SavedTarget,
    Principal,
}

fn device_key() -> Arc<EcdsaKeyPair> {
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
async fn private_ipc_opens_statuses_and_closes_a_real_native_data_session() {
    exercise_session(PrincipalFault::None, AuthorizationInvalidation::None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_or_missing_principal_never_exposes_a_ready_session() {
    exercise_session(PrincipalFault::Wrong, AuthorizationInvalidation::None).await;
    exercise_session(PrincipalFault::Missing, AuthorizationInvalidation::None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_target_invalidation_revokes_a_ready_session_and_drains_it() {
    exercise_session(PrincipalFault::None, AuthorizationInvalidation::SavedTarget).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn principal_invalidation_revokes_a_ready_session_and_drains_it() {
    exercise_session(PrincipalFault::None, AuthorizationInvalidation::Principal).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_stream_awaits_owned_open_startup_cleanup() {
    tokio::time::timeout(Duration::from_secs(20), async {
        // Accept the TCP stream but never perform the WebSocket handshake, so
        // NativeSyncSession::start_data_client remains inside its bounded
        // signaling-connect phase.
        let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", stalled.local_addr().unwrap());
        let accepted = tokio::spawn(async move {
            let (stream, _address) = stalled.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(8)).await;
            drop(stream);
        });
        let (_, _client_db, mut client_options) =
            native_fixture::control_options(url, "business-data-room", "workjet-session").await;
        client_options.peer_role = NativePeerRole::WorkjetExecutor;
        let saved = SavedBusinessDataTarget {
            public_identity: "startup-fixture".into(),
            instance_id: "fixture-instance".into(),
            account_epoch: ACCOUNT_EPOCH,
        };
        let host = Arc::new(TestHost {
            options: Mutex::new(Some(client_options)),
            current_saved: Mutex::new(Some(saved)),
            current_principal: Mutex::new(Some(NativeBusinessDataPrincipal {
                user_id: "fixture-user".into(),
                authorization_epoch: AUTHORIZATION_EPOCH,
                device: None,
            })),
        });
        let service = Arc::new(BusinessDataService::new(host.clone()));
        let service_factory = service.clone();
        let ipc = BusinessDataIpc::new(Arc::new(move |credentials, _events| {
            Ok(Arc::new(service_factory.dispatcher(credentials)))
        }));
        let (mut client, native) = tokio::io::duplex(64 * 1024);
        let serve = tokio::spawn(async move { ipc.serve(Box::new(native)).await });
        send_request(&mut client, "open", open_request()).await;
        while service.owned_cleanup_count() != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Prove startup is still owned before cancelling the private stream.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(service.owned_cleanup_count(), 1);
        let started = std::time::Instant::now();
        drop(client);
        // Existing private-stream EOF semantics intentionally surface an error
        // after connection-owned startup and transport cleanup have completed.
        let result = serve.await.unwrap();
        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_secs(6),
            "startup drain exceeded its deadline: {:?}",
            started.elapsed()
        );
        service.shutdown().await.unwrap();
        assert_eq!(service.owned_cleanup_count(), 0);
        accepted.await.unwrap();
    })
    .await
    .expect("owned startup cleanup deadline");
}

async fn exercise_session(
    principal_fault: PrincipalFault,
    invalidation: AuthorizationInvalidation,
) {
    tokio::time::timeout(Duration::from_secs(40), async {
        let signaling =
            signaling_fixture::SignalingFixture::with_roles(["ctox_instance", "browser"]).await;
        let device = device_key();
        let device_public = device.public_key().as_ref().to_vec();
        let revoked = Arc::new(AtomicBool::new(false));
        let credential_requests = Arc::new(AtomicU8::new(0));
        let (server_root, server_db, mut server_options) = native_fixture::options(
            signaling.url.clone(),
            "business-data-room",
            "server-session",
        )
        .await;
        server_options.collections[0]
            .insert(json!({"id":"private-record"}))
            .await
            .unwrap();
        let policy_revoked = revoked.clone();
        server_options.admission.collection_read = Some(Arc::new(move |token, collection| {
            token == "fixture-private-read"
                && collection == "records"
                && !policy_revoked.load(Ordering::SeqCst)
        }));
        server_options.admission.eager_pull = Some(Arc::new(|_, _| true));
        let proof_revoked = revoked.clone();
        let proof_key = device_public.clone();
        server_options.admission.session = Arc::new(move |payload, challenge| {
            use WebRTCPeerSessionValidation::{Accept, Defer, Reject};
            if proof_revoked.load(Ordering::SeqCst)
                || payload.pointer("/peerSession/role").and_then(Value::as_str) != Some("browser")
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
                || proof["publicJwk"]["x"] != URL_SAFE_NO_PAD.encode(&proof_key[1..33])
                || proof["publicJwk"]["y"] != URL_SAFE_NO_PAD.encode(&proof_key[33..65])
            {
                return Reject;
            }
            let signature = match proof["signature"]
                .as_str()
                .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
            {
                Some(value) => value,
                None => return Reject,
            };
            if UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &proof_key)
                .verify(nonce.as_bytes(), &signature)
                .is_err()
            {
                Reject
            } else {
                Accept
            }
        });
        let source_key = Arc::new(
            ctox_sync::authority::auth::SigningIdentity::from_pkcs8(
                &ctox_sync::authority::auth::SigningIdentity::generate_pkcs8().unwrap(),
            )
            .unwrap(),
        );
        let source_pin = source_key.public_identity();
        let identity_revoked = revoked.clone();
        let signed_principal = match principal_fault {
            PrincipalFault::None | PrincipalFault::Wrong => Some(NativeBusinessDataPrincipal {
                user_id: if principal_fault == PrincipalFault::Wrong {
                    "wrong-user".into()
                } else {
                    "fixture-user".into()
                },
                authorization_epoch: AUTHORIZATION_EPOCH,
                device: None,
            }),
            PrincipalFault::Missing => None,
        };
        let responder_principal = signed_principal.clone();
        let server = NativeSyncSession::start_with_pool_setup(server_options, |pool| {
            let key = source_key.clone();
            let revoked = identity_revoked.clone();
            let transport = pool.connection_handler.clone();
            let signed_principal = responder_principal.clone();
            pool.register_identity_request_handler(
                ctox_sync::business_data_contract::CTOX_BUSINESS_DATA_IDENTITY_METHOD,
                Arc::new(move |peer_id, token, params| {
                    let key = key.clone();
                    let revoked = revoked.clone();
                    let transport = transport.clone();
                    let signed_principal = signed_principal.clone();
                    Box::pin(async move {
                        let connection = transport
                            .connection_for_peer(&peer_id)
                            .ok_or_else(|| "retired peer".to_string())?;
                        let channel_binding = transport
                            .channel_binding(&connection)
                            .await
                            .map_err(|_| "invalid channel".to_string())?;
                        if revoked.load(Ordering::SeqCst)
                            || (!token.is_empty() && token != "fixture-private-read")
                        {
                            return Err("identity capability rejected".into());
                        }
                        let principal = (!token.is_empty())
                            .then(|| signed_principal.clone())
                            .flatten();
                        let request: NativeBusinessDataIdentityRequest =
                            serde_json::from_value(params[0].clone())
                                .map_err(|_| "invalid request".to_string())?;
                        let proof = key
                            .attest_business_data_identity(
                                "fixture-instance",
                                &request.challenge,
                                &channel_binding,
                                principal,
                            )
                            .map_err(|_| "invalid challenge".to_string())?;
                        serde_json::to_value(proof).map_err(|_| "invalid identity".into())
                    })
                }),
            )
            .expect("install BusinessData identity responder");
            Ok(())
        })
        .await
        .unwrap();

        let (_, client_db, client_options) = native_fixture::control_options(
            signaling.url.clone(),
            "business-data-room",
            "workjet-session",
        )
        .await;
        let mut client_options = client_options;
        client_options.peer_role = NativePeerRole::WorkjetExecutor;
        let saved = SavedBusinessDataTarget {
            public_identity: source_pin,
            instance_id: "fixture-instance".into(),
            account_epoch: ACCOUNT_EPOCH,
        };
        // The trusted host principal is independent from the identity responder
        // fixture. Wrong/missing responder principals must therefore fail Open.
        let current_principal =
            (principal_fault != PrincipalFault::Missing).then(|| NativeBusinessDataPrincipal {
                user_id: "fixture-user".into(),
                authorization_epoch: AUTHORIZATION_EPOCH,
                device: None,
            });
        let host = Arc::new(TestHost {
            options: Mutex::new(Some(client_options)),
            current_saved: Mutex::new(Some(saved.clone())),
            current_principal: Mutex::new(current_principal),
        });
        let service = Arc::new(BusinessDataService::new(host.clone()));
        let service_factory = service.clone();
        let ipc = BusinessDataIpc::new(Arc::new(move |credentials, _events| {
            Ok(Arc::new(service_factory.dispatcher(credentials)))
        }));
        let (mut client, native) = tokio::io::duplex(64 * 1024);
        let serve = tokio::spawn(async move { ipc.serve(Box::new(native)).await });

        send_request(&mut client, "unsupported-query", query_request()).await;
        assert_rejected(
            read_frame(&mut client).await,
            NativeBusinessDataErrorCode::Unsupported,
        );

        send_request(&mut client, "open", open_request()).await;
        // The private challenge is only sent after source/channel proof. A
        // signed reply must still bind the exact saved target and epoch.
        let response = open_with_credentials(&mut client, &device, &credential_requests).await;
        let mut ready = None;
        if principal_fault == PrincipalFault::None {
            let NativeBusinessDataResult::Session { state } = response.result else {
                panic!("expected session state: {response:?}");
            };
            let NativeBusinessDataSessionState::Ready { session, binding } = state else {
                panic!("expected ready session");
            };
            assert_eq!(binding.user_id, "fixture-user");
            assert_eq!(binding.instance_id, "fixture-instance");
            ready = Some(session.clone());
            send_request(
                &mut client,
                "status",
                NativeBusinessDataOperation::Status {
                    session: ready.clone().unwrap(),
                },
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "status" => {
                    let NativeBusinessDataResult::Session {
                        state: NativeBusinessDataSessionState::Ready { .. },
                    } = response.result
                    else {
                        panic!("expected ready status: {response:?}");
                    };
                }
                _ => panic!("expected status response"),
            }
            if invalidation != AuthorizationInvalidation::None {
                if invalidation == AuthorizationInvalidation::SavedTarget {
                    let mut current = host.current_saved.lock().await;
                    let mut stale = current.clone().unwrap();
                    stale.account_epoch += 1;
                    *current = Some(stale);
                } else {
                    *host.current_principal.lock().await = None;
                }
                send_request(
                    &mut client,
                    "invalidated-status",
                    NativeBusinessDataOperation::Status {
                        session: ready.clone().unwrap(),
                    },
                )
                .await;
                match read_frame(&mut client).await {
                    Frame::Response { response } if response.request_id == "invalidated-status" => {
                        let NativeBusinessDataResult::Session {
                            state:
                                NativeBusinessDataSessionState::Revoked {
                                    session: revoked_session,
                                    ..
                                },
                        } = response.result
                        else {
                            panic!("expected revoked status: {response:?}");
                        };
                        assert_eq!(revoked_session, ready.clone().unwrap());
                    }
                    _ => panic!("expected invalidated status response"),
                }
            }
            if invalidation == AuthorizationInvalidation::None {
                send_request(
                    &mut client,
                    "stale-generation",
                    NativeBusinessDataOperation::Status {
                        session: NativeBusinessDataSessionRef {
                            handle: ready.as_ref().unwrap().handle.clone(),
                            generation: ready.as_ref().unwrap().generation + 1,
                        },
                    },
                )
                .await;
                assert_rejected(
                    read_frame(&mut client).await,
                    NativeBusinessDataErrorCode::StaleGeneration,
                );
            }
        } else {
            assert_rejected(
                Frame::Response { response },
                NativeBusinessDataErrorCode::Unauthorized,
            );
        }

        // Close drops the response path after bounded native transport drain.
        if principal_fault == PrincipalFault::None
            && invalidation == AuthorizationInvalidation::None
        {
            let close = Frame::Request {
                request: NativeBusinessDataRequest {
                    version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
                    request_id: "close".into(),
                    operation: NativeBusinessDataOperation::Close {
                        session: ready.clone().unwrap(),
                    },
                },
            };
            write_frame(&mut client, &close).await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "close" => {
                    let NativeBusinessDataResult::Session {
                        state: NativeBusinessDataSessionState::Disconnected { .. },
                    } = response.result
                    else {
                        panic!("expected disconnected state: {response:?}");
                    };
                }
                _ => panic!("expected close response"),
            }
        }
        drop(client);
        // EOF is the normal private-stream disconnect. Dispatcher cleanup is
        // awaited and bounded; revoked startup cannot leave a transport behind.
        let cleanup_started = std::time::Instant::now();
        assert!(serve.await.unwrap().is_err());
        service.shutdown().await.unwrap();
        assert!(cleanup_started.elapsed() < Duration::from_secs(6));
        assert_eq!(service.owned_cleanup_count(), 0);
        revoked.store(true, Ordering::SeqCst);
        assert!(credential_requests.load(Ordering::SeqCst) > 0);
        server.shutdown().await;
        client_db.close().await.unwrap();
        server_db.close().await.unwrap();
        drop((server_root, signaling));
    })
    .await
    .expect("private native BusinessData IPC deadline");
}

async fn open_with_credentials(
    stream: &mut (impl tokio::io::AsyncWrite + tokio::io::AsyncRead + Unpin + Send),
    device: &EcdsaKeyPair,
    credential_requests: &AtomicU8,
) -> NativeBusinessDataResponse {
    for _ in 0..8 {
        match read_frame(stream).await {
            Frame::CredentialChallenge { challenge } => {
                assert_eq!(challenge.target_id, "saved-fixture");
                assert_eq!(challenge.session_epoch, ACCOUNT_EPOCH);
                credential_requests.fetch_add(1, Ordering::SeqCst);
                let proof = challenge.nonce.map(|nonce| {
                    let signature = device.sign(&SystemRandom::new(), nonce.as_bytes()).unwrap();
                    let public = device.public_key().as_ref();
                    ctox_sync::business_data_contract::NativeBusinessDataDeviceProof {
                        public_x: URL_SAFE_NO_PAD.encode(&public[1..33]),
                        public_y: URL_SAFE_NO_PAD.encode(&public[33..65]),
                        signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
                    }
                });
                write_frame(
                    stream,
                    &Frame::CredentialReply {
                        reply:
                            ctox_sync::business_data_contract::NativeBusinessDataCredentialReply {
                                version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
                                request_id: challenge.request_id,
                                connection_id: challenge.connection_id,
                                session_epoch: challenge.session_epoch,
                                capability_token: Some("fixture-private-read".into()),
                                device_proof: proof,
                            },
                    },
                )
                .await;
            }
            Frame::Response { response } if response.request_id == "open" => return response,
            _ => panic!("expected credential challenge or open response"),
        }
    }
    panic!("BusinessData open did not settle after credential replies");
}

async fn send_request(
    stream: &mut (impl tokio::io::AsyncWrite + Unpin + Send),
    request_id: &str,
    operation: NativeBusinessDataOperation,
) {
    write_frame(
        stream,
        &Frame::Request {
            request: NativeBusinessDataRequest {
                version: CTOX_BUSINESS_DATA_PROTOCOL_VERSION,
                request_id: request_id.into(),
                operation,
            },
        },
    )
    .await;
}

fn open_request() -> NativeBusinessDataOperation {
    NativeBusinessDataOperation::Open {
        target_id: "saved-fixture".into(),
    }
}

fn query_request() -> NativeBusinessDataOperation {
    NativeBusinessDataOperation::Query {
        session: ctox_sync::business_data_contract::NativeBusinessDataSessionRef {
            handle: "unknown".into(),
            generation: 1,
        },
        query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
            collection: "records".into(),
            scope: NativeBusinessDataScope::Instance {},
            query: json!({}),
            page_size: 10,
        },
        page_cursor: None,
    }
}

async fn write_frame(stream: &mut (impl tokio::io::AsyncWrite + Unpin + Send), frame: &Frame) {
    let bytes = serde_json::to_vec(frame).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&bytes).await.unwrap();
    stream.flush().await.unwrap();
}

async fn read_frame(stream: &mut (impl tokio::io::AsyncRead + Unpin + Send)) -> Frame {
    let mut header = [0; 4];
    stream.read_exact(&mut header).await.unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(header) as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn assert_rejected(frame: Frame, code: NativeBusinessDataErrorCode) {
    let Frame::Response { response } = frame else {
        panic!("expected response frame");
    };
    let NativeBusinessDataResult::Rejected {
        code: actual,
        retryable,
        ..
    } = response.result
    else {
        panic!("expected rejection: {response:?}");
    };
    assert_eq!(actual, code);
    if code == NativeBusinessDataErrorCode::Unsupported {
        assert!(!retryable);
    }
}
