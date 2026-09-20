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
const DATA_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct QueryGate {
    armed: AtomicBool,
    reject_queries: AtomicBool,
    reject_documents: AtomicBool,
    command_owner_checks: std::sync::atomic::AtomicUsize,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

struct FixtureSourcePolicy {
    query_gate: Arc<QueryGate>,
}

const FIXTURE_CAPABILITY: &str = "fixture-private-read";

#[async_trait::async_trait]
impl ctox_sync::business_data_remote::BusinessDataAccessPolicy for FixtureSourcePolicy {
    async fn identity(
        &self,
        capability_token: &str,
    ) -> Option<ctox_sync::business_data_remote::RemoteIdentity> {
        (capability_token == FIXTURE_CAPABILITY).then_some(
            ctox_sync::business_data_remote::RemoteIdentity {
                user_id: "fixture-user".into(),
                authorization_epoch: AUTHORIZATION_EPOCH,
                instance_id: "fixture-instance".into(),
            },
        )
    }

    async fn authorize(
        &self,
        identity: &ctox_sync::business_data_remote::RemoteIdentity,
        capability_token: &str,
        collection: &str,
        access: ctox_sync::business_data_remote::Access,
        scope: &NativeBusinessDataScope,
    ) -> std::io::Result<()> {
        let allowed = matches!(scope, NativeBusinessDataScope::Instance {})
            && ((collection == "records"
                && access == ctox_sync::business_data_remote::Access::Read)
                || collection == "business_commands");
        if capability_token != FIXTURE_CAPABILITY || identity.user_id != "fixture-user" || !allowed
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture authorization denied",
            ));
        }
        Ok(())
    }

    async fn authorize_query(
        &self,
        identity: &ctox_sync::business_data_remote::RemoteIdentity,
        capability_token: &str,
        collection: &str,
        scope: &NativeBusinessDataScope,
        query: &Value,
    ) -> std::io::Result<()> {
        self.authorize(
            identity,
            capability_token,
            collection,
            ctox_sync::business_data_remote::Access::Read,
            scope,
        )
        .await?;
        if self.query_gate.reject_queries.load(Ordering::SeqCst)
            || !rxdb::plugins::replication_webrtc::webrtc_types::readable_query_fields(
                query,
                &["id".into()],
            )
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture query field policy denied",
            ));
        }
        Ok(())
    }

    async fn submit_command(
        &self,
        _identity: &ctox_sync::business_data_remote::RemoteIdentity,
        _capability_token: &str,
        command: &ctox_sync::business_data_contract::NativeBusinessDataCommand,
    ) -> std::io::Result<ctox_sync::business_data_contract::NativeBusinessDataCommandState> {
        Ok(
            ctox_sync::business_data_contract::NativeBusinessDataCommandState {
                command_id: command.command_id.clone(),
                status: ctox_sync::business_data_contract::NativeBusinessDataCommandStatus::Pending,
                result: Some(json!({"durable_fixture": true})),
                error: None,
            },
        )
    }

    async fn command_state(
        &self,
        identity: &ctox_sync::business_data_remote::RemoteIdentity,
        document: &Value,
    ) -> std::io::Result<ctox_sync::business_data_contract::NativeBusinessDataCommandState> {
        self.query_gate
            .command_owner_checks
            .fetch_add(1, Ordering::SeqCst);
        let owner = document
            .get("owner_user_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "fixture command has no owner",
                )
            })?;
        if owner != identity.user_id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture command owner mismatch",
            ));
        }
        Ok(
            ctox_sync::business_data_contract::NativeBusinessDataCommandState {
                command_id: document["id"].as_str().unwrap_or_default().into(),
                status: ctox_sync::business_data_contract::NativeBusinessDataCommandStatus::Pending,
                result: None,
                error: None,
            },
        )
    }

    async fn document_view(
        &self,
        identity: &ctox_sync::business_data_remote::RemoteIdentity,
        capability_token: &str,
        collection: &str,
        document: &Value,
    ) -> std::io::Result<Option<Value>> {
        self.authorize(
            identity,
            capability_token,
            collection,
            ctox_sync::business_data_remote::Access::Read,
            &NativeBusinessDataScope::Instance {},
        )
        .await?;
        if self.query_gate.armed.swap(false, Ordering::SeqCst) {
            self.query_gate.entered.notify_one();
            self.query_gate.release.notified().await;
        }
        if self.query_gate.reject_documents.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture document policy unavailable",
            ));
        }
        Ok(Some(document.clone()))
    }

    async fn command_event(
        &self,
        identity: &ctox_sync::business_data_remote::RemoteIdentity,
        capability_token: &str,
        expected_command_id: &str,
        document: &Value,
    ) -> std::io::Result<Option<ctox_sync::business_data_contract::NativeBusinessDataCommandState>>
    {
        if document.get("id").and_then(Value::as_str) != Some(expected_command_id) {
            return Ok(None);
        }
        self.command_state(identity, document).await.map(Some)
    }
}

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
        let ipc = BusinessDataIpc::new(Arc::new(move |credentials, events| {
            Ok(Arc::new(service_factory.dispatcher(credentials, events)))
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
        for index in 0..3 {
            server_db
                .collection("records")
                .unwrap()
                .insert(json!({"id": format!("native-{index}")}))
                .await
                .unwrap();
        }
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
        let query_gate = Arc::new(QueryGate::default());
        let server = NativeSyncSession::start_with_pool_setup(server_options, |pool| {
            let source = ctox_sync::business_data_remote::BusinessDataSource::new(
                server_db.clone(),
                Arc::new(FixtureSourcePolicy {
                    query_gate: query_gate.clone(),
                }),
                pool.connection_handler.clone(),
            );
            source.register(pool).unwrap();
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
        let ipc = BusinessDataIpc::new(Arc::new(move |credentials, events| {
            Ok(Arc::new(service_factory.dispatcher(credentials, events)))
        }));
        let (mut client, native) = tokio::io::duplex(64 * 1024);
        let serve = tokio::spawn(async move { ipc.serve(Box::new(native)).await });

        send_request(&mut client, "unsupported-query", query_request()).await;
        assert_rejected(
            read_frame(&mut client).await,
            NativeBusinessDataErrorCode::UnknownSession,
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
                // A data request must also observe the same authority fence,
                // not merely the lifecycle-only Status path.
                send_request(
                    &mut client,
                    "invalidated-query",
                    NativeBusinessDataOperation::Query {
                        session: ready.clone().unwrap(),
                        query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
                            collection: "records".into(),
                            scope: NativeBusinessDataScope::Instance {},
                            query: fixture_query(),
                            page_size: 2,
                        },
                        page_cursor: None,
                    },
                )
                .await;
                assert_rejected(
                    read_frame(&mut client).await,
                    NativeBusinessDataErrorCode::Unauthorized,
                );
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

        if principal_fault == PrincipalFault::None
            && invalidation == AuthorizationInvalidation::None
        {
            for forbidden_query in [
                json!({"selector":{"secret":"probe"},"sort":[{"id":"asc"}]}),
                json!({"selector":{},"sort":[{"secret":"asc"}]}),
                json!({"selector":{},"sort":[{"id":"asc"}],"index":["secret"]}),
            ] {
                let query = ctox_sync::business_data_contract::NativeBusinessDataQuery {
                    collection: "records".into(),
                    scope: NativeBusinessDataScope::Instance {},
                    query: forbidden_query,
                    page_size: 2,
                };
                for operation in [
                    NativeBusinessDataOperation::Query {
                        session: ready.clone().unwrap(),
                        query: query.clone(),
                        page_cursor: None,
                    },
                    NativeBusinessDataOperation::Watch {
                        session: ready.clone().unwrap(),
                        query,
                        resume_cursor: None,
                    },
                ] {
                    send_request(&mut client, "unreadable-query-field", operation).await;
                    assert_rejected(
                        read_frame(&mut client).await,
                        NativeBusinessDataErrorCode::Unauthorized,
                    );
                }
            }

            send_request(
                &mut client,
                "query-page-1",
                query_request_with_cursor(ready.as_ref().unwrap(), None),
            )
            .await;
            let page_cursor = match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "query-page-1" => {
                    let NativeBusinessDataResult::Page {
                        session,
                        snapshot_id,
                        records,
                        next_page_cursor,
                        snapshot_complete,
                    } = response.result
                    else {
                        panic!("expected query page: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    assert!(!snapshot_id.is_empty());
                    assert_eq!(
                        records
                            .iter()
                            .map(|record| record.document_id.as_str())
                            .collect::<Vec<_>>(),
                        ["native-0", "native-1"]
                    );
                    assert!(!snapshot_complete);
                    next_page_cursor.expect("paged query returns a cursor")
                }
                _ => panic!("expected query response"),
            };

            query_gate.reject_queries.store(true, Ordering::SeqCst);
            send_request(
                &mut client,
                "query-page-policy-revoked",
                query_request_with_cursor(ready.as_ref().unwrap(), Some(page_cursor.clone())),
            )
            .await;
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::Unauthorized,
            );
            query_gate.reject_queries.store(false, Ordering::SeqCst);

            // An unknown page cursor must fail closed with ResetRequired; the
            // following valid cursor proves recovery uses the same snapshot only
            // when it is still valid.
            send_request(
                &mut client,
                "invalid-query-cursor",
                NativeBusinessDataOperation::Query {
                    session: ready.clone().unwrap(),
                    query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
                        collection: "records".into(),
                        scope: NativeBusinessDataScope::Instance {},
                        query: fixture_query(),
                        page_size: 2,
                    },
                    page_cursor: Some("business-data-expired-cursor".into()),
                },
            )
            .await;
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::ResetRequired,
            );

            send_request(
                &mut client,
                "query-page-2",
                query_request_with_cursor(ready.as_ref().unwrap(), Some(page_cursor)),
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "query-page-2" => {
                    let NativeBusinessDataResult::Page {
                        session,
                        records,
                        next_page_cursor,
                        snapshot_complete,
                        ..
                    } = response.result
                    else {
                        panic!("expected final query page: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    assert_eq!(
                        records
                            .iter()
                            .map(|record| record.document_id.as_str())
                            .collect::<Vec<_>>(),
                        ["native-2"]
                    );
                    assert!(snapshot_complete);
                    assert!(next_page_cursor.is_none());
                }
                _ => panic!("expected query response"),
            }

            // Replay a consumed cursor while the snapshot is still live. A
            // stale cursor must not consume the following page.
            let mut next_cursor = None;
            let mut consumed_cursor = None;
            for index in 0..3 {
                let mut operation =
                    query_request_with_cursor(ready.as_ref().unwrap(), next_cursor.take());
                if let NativeBusinessDataOperation::Query { query, .. } = &mut operation {
                    query.page_size = 1;
                }
                send_request(&mut client, "single-record-page", operation).await;
                let Frame::Response { response } = read_frame(&mut client).await else {
                    panic!("expected single-record query response");
                };
                let NativeBusinessDataResult::Page {
                    records,
                    next_page_cursor,
                    snapshot_complete,
                    ..
                } = response.result
                else {
                    panic!("expected single-record page");
                };
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].document_id, format!("native-{index}"));
                assert_eq!(snapshot_complete, index == 2);
                if index == 0 {
                    consumed_cursor = next_page_cursor.clone();
                    assert!(consumed_cursor.is_some());
                }
                next_cursor = next_page_cursor;
                if index == 1 {
                    let mut replay =
                        query_request_with_cursor(ready.as_ref().unwrap(), consumed_cursor.take());
                    if let NativeBusinessDataOperation::Query { query, .. } = &mut replay {
                        query.page_size = 1;
                    }
                    send_request(&mut client, "consumed-page-cursor", replay).await;
                    assert_rejected(
                        read_frame(&mut client).await,
                        NativeBusinessDataErrorCode::ResetRequired,
                    );
                }
            }
            assert!(next_cursor.is_none());

            send_request(
                &mut client,
                "watch-invalid-resume",
                watch_request(
                    ready.as_ref().unwrap(),
                    Some("business-data-expired-cursor"),
                ),
            )
            .await;
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::ResetRequired,
            );

            query_gate.reject_documents.store(true, Ordering::SeqCst);
            send_request(
                &mut client,
                "watch-failed-snapshot",
                watch_request(ready.as_ref().unwrap(), None),
            )
            .await;
            let Frame::Response { response } = read_frame(&mut client).await else {
                panic!("failed snapshot watch must first respond");
            };
            let NativeBusinessDataResult::Subscribed {
                subscription_id: failed_id,
                ..
            } = response.result
            else {
                panic!("expected accepted watch before snapshot failure");
            };
            loop {
                let event = read_event(&mut client, "failed snapshot").await;
                assert_eq!(event.subscription_id, failed_id);
                match event.payload {
                    NativeBusinessDataEventPayload::SnapshotStart { .. } => {}
                    NativeBusinessDataEventPayload::Error { .. }
                    | NativeBusinessDataEventPayload::Reset { .. } => break,
                    other => panic!("failed snapshot must not publish success or data: {other:?}"),
                }
            }
            send_request(
                &mut client,
                "unwatch-failed-snapshot",
                NativeBusinessDataOperation::Unwatch {
                    session: ready.clone().unwrap(),
                    subscription_id: failed_id.clone(),
                },
            )
            .await;
            loop {
                match read_frame(&mut client).await {
                    Frame::Event { event } => {
                        assert_eq!(event.subscription_id, failed_id);
                        assert!(matches!(
                            event.payload,
                            NativeBusinessDataEventPayload::Error { .. }
                                | NativeBusinessDataEventPayload::Reset { .. }
                        ));
                    }
                    Frame::Response { response } => {
                        assert_eq!(response.request_id, "unwatch-failed-snapshot");
                        let NativeBusinessDataResult::Unwatched {
                            subscription_id, ..
                        } = response.result
                        else {
                            panic!("failed snapshot subscription must remain cleanable");
                        };
                        assert_eq!(subscription_id, failed_id);
                        break;
                    }
                    _ => panic!("unexpected frame during failed snapshot cleanup"),
                }
            }
            query_gate.reject_documents.store(false, Ordering::SeqCst);

            // Reset cannot stand in for a snapshot: start without a cursor and
            // require the full snapshot sequence before declaring caught up.
            query_gate.armed.store(true, Ordering::SeqCst);
            send_request(
                &mut client,
                "watch",
                watch_request(ready.as_ref().unwrap(), None),
            )
            .await;
            let subscription_id = match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "watch" => {
                    let NativeBusinessDataResult::Subscribed {
                        session,
                        subscription_id,
                    } = response.result
                    else {
                        panic!("expected subscribed response: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    subscription_id
                }
                _ => panic!("expected watch response"),
            };

            timeout(Duration::from_secs(5), query_gate.entered.notified())
                .await
                .expect("watch must pause while projecting its snapshot");
            server_db
                .collection("records")
                .unwrap()
                .insert(json!({"id": "native-during-snapshot"}))
                .await
                .unwrap();
            query_gate.release.notify_one();

            let mut saw_start = false;
            let mut saw_page = false;
            let mut saw_buffered_change = false;
            let mut saw_end = false;
            let mut last_sequence = 0;
            loop {
                let event = read_event(&mut client, "watch snapshot").await;
                assert_eq!(event.session, ready.clone().unwrap());
                assert_eq!(event.subscription_id, subscription_id);
                assert!(event.sequence > last_sequence);
                last_sequence = event.sequence;
                match event.payload {
                    NativeBusinessDataEventPayload::SnapshotStart { snapshot_id } => {
                        assert!(!saw_page);
                        assert_eq!(snapshot_id, subscription_id);
                        saw_start = true;
                    }
                    NativeBusinessDataEventPayload::SnapshotPage {
                        snapshot_id,
                        records,
                    } => {
                        assert!(saw_start);
                        assert_eq!(snapshot_id, subscription_id);
                        assert_eq!(
                            records
                                .iter()
                                .map(|record| record.document_id.as_str())
                                .collect::<Vec<_>>(),
                            ["native-0", "native-1", "native-2"]
                        );
                        saw_page = true;
                    }
                    NativeBusinessDataEventPayload::SnapshotEnd {
                        snapshot_id,
                        cursor,
                    } => {
                        assert!(saw_page);
                        assert!(!saw_end);
                        assert_eq!(snapshot_id, subscription_id);
                        assert!(!cursor.is_empty());
                        saw_end = true;
                    }
                    NativeBusinessDataEventPayload::Upsert {
                        record, recovery, ..
                    } => {
                        assert!(saw_page);
                        assert!(!saw_end);
                        assert!(!recovery);
                        assert_eq!(record.document_id, "native-during-snapshot");
                        saw_buffered_change = true;
                    }
                    NativeBusinessDataEventPayload::CaughtUp { cursor } => {
                        assert!(saw_start && saw_page && saw_end && saw_buffered_change);
                        assert!(!cursor.is_empty());
                        break;
                    }
                    other => panic!("unexpected watch event: {other:?}"),
                }
            }

            // A collection-wide event outside the watch selector must produce
            // neither an Upsert nor a Remove leaking its document ID.
            server_db
                .collection("records")
                .unwrap()
                .insert(json!({"id": "outside-watch-selector"}))
                .await
                .unwrap();
            server_db
                .collection("records")
                .unwrap()
                .insert(json!({"id": "native-live"}))
                .await
                .unwrap();
            let live = read_event(&mut client, "watch live upsert").await;
            assert_eq!(live.session, ready.clone().unwrap());
            assert_eq!(live.subscription_id, subscription_id);
            assert!(live.sequence > last_sequence);
            let NativeBusinessDataEventPayload::Upsert {
                cursor,
                record,
                recovery,
            } = live.payload
            else {
                panic!("expected live upsert: {:?}", live.payload);
            };
            assert!(!cursor.is_empty());
            assert_eq!(record.document_id, "native-live");
            assert!(!recovery);

            send_request(
                &mut client,
                "watch-resume",
                watch_request(ready.as_ref().unwrap(), Some(&cursor)),
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "watch-resume" => {
                    let NativeBusinessDataResult::Subscribed {
                        session,
                        subscription_id: resumed,
                    } = response.result
                    else {
                        panic!("expected resumed subscription: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    assert_eq!(resumed, subscription_id);
                }
                _ => panic!("expected resume response"),
            }
            let caught_up = read_event(&mut client, "resume caught-up").await;
            assert_eq!(caught_up.session, ready.clone().unwrap());
            assert_eq!(caught_up.subscription_id, subscription_id);
            assert!(caught_up.sequence > live.sequence);
            let NativeBusinessDataEventPayload::CaughtUp {
                cursor: resume_cursor,
            } = caught_up.payload
            else {
                panic!("expected resume caught-up");
            };
            // Repeat without any write: Notify must wake every resume, not only
            // the first one or a later collection change.
            send_request(
                &mut client,
                "watch-resume-again",
                watch_request(ready.as_ref().unwrap(), Some(&resume_cursor)),
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "watch-resume-again" => {
                    let NativeBusinessDataResult::Subscribed {
                        subscription_id: resumed,
                        ..
                    } = response.result
                    else {
                        panic!("expected second resumed subscription");
                    };
                    assert_eq!(resumed, subscription_id);
                }
                _ => panic!("expected second resume response before events"),
            }
            let resumed_again = read_event(&mut client, "second resume caught-up").await;
            assert_eq!(resumed_again.session, ready.clone().unwrap());
            assert_eq!(resumed_again.subscription_id, subscription_id);
            assert!(resumed_again.sequence > caught_up.sequence);
            assert!(matches!(
                resumed_again.payload,
                NativeBusinessDataEventPayload::CaughtUp { .. }
            ));

            let command = ctox_sync::business_data_contract::NativeBusinessDataCommand {
                command_id: "fixture-command".into(),
                command_type: "fixture.noop".into(),
                payload: json!({}),
            };
            send_request(
                &mut client,
                "submit-command",
                NativeBusinessDataOperation::SubmitCommand {
                    session: ready.clone().unwrap(),
                    command,
                },
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "submit-command" => {
                    let NativeBusinessDataResult::Command { session, state } = response.result
                    else {
                        panic!("expected command response: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    assert_eq!(state.command_id, "fixture-command");
                    assert_eq!(
                        state.status,
                        ctox_sync::business_data_contract::NativeBusinessDataCommandStatus::Pending
                    );
                    assert_eq!(state.result, Some(json!({"durable_fixture": true})));
                }
                _ => panic!("expected command response"),
            }

            // The fixture has no durable owned command projection, so an
            // observation must reject before creating an event pump.
            let owner_checks_before = query_gate.command_owner_checks.load(Ordering::SeqCst);
            send_request(
                &mut client,
                "observe-foreign-command",
                NativeBusinessDataOperation::ObserveCommand {
                    session: ready.clone().unwrap(),
                    command_id: "foreign-command".into(),
                },
            )
            .await;
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::Unauthorized,
            );

            assert_eq!(
                query_gate.command_owner_checks.load(Ordering::SeqCst),
                owner_checks_before + 1,
                "observation must reach the owner gate before rejection",
            );
            query_gate.reject_documents.store(true, Ordering::SeqCst);
            server_db
                .collection("records")
                .unwrap()
                .insert(json!({"id":"native-policy-failure"}))
                .await
                .unwrap();
            let failed_watch = read_event(&mut client, "live policy failure reset").await;
            assert_eq!(failed_watch.session, ready.clone().unwrap());
            assert_eq!(failed_watch.subscription_id, subscription_id);
            assert!(matches!(
                failed_watch.payload,
                NativeBusinessDataEventPayload::Reset {
                    code: NativeBusinessDataErrorCode::ResetRequired,
                }
            ));
            query_gate.reject_documents.store(false, Ordering::SeqCst);

            send_request(
                &mut client,
                "unwatch",
                NativeBusinessDataOperation::Unwatch {
                    session: ready.clone().unwrap(),
                    subscription_id: subscription_id.clone(),
                },
            )
            .await;
            match read_frame(&mut client).await {
                Frame::Response { response } if response.request_id == "unwatch" => {
                    let NativeBusinessDataResult::Unwatched {
                        session,
                        subscription_id: stopped,
                    } = response.result
                    else {
                        panic!("expected unwatched response: {response:?}");
                    };
                    assert_eq!(session, ready.clone().unwrap());
                    assert_eq!(stopped, subscription_id);
                }
                _ => panic!("expected unwatch response"),
            }

            query_gate.armed.store(true, Ordering::SeqCst);
            send_request(
                &mut client,
                "query-source-policy-revoked-during-snapshot",
                query_request_with_cursor(ready.as_ref().unwrap(), None),
            )
            .await;
            timeout(Duration::from_secs(5), query_gate.entered.notified())
                .await
                .expect("query must enter source document policy");
            query_gate.reject_queries.store(true, Ordering::SeqCst);
            query_gate.release.notify_one();
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::Unauthorized,
            );
            query_gate.reject_queries.store(false, Ordering::SeqCst);

            query_gate.armed.store(true, Ordering::SeqCst);
            send_request(
                &mut client,
                "query-revoked-during-snapshot",
                NativeBusinessDataOperation::Query {
                    session: ready.clone().unwrap(),
                    query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
                        collection: "records".into(),
                        scope: NativeBusinessDataScope::Instance {},
                        query: fixture_query(),
                        page_size: 2,
                    },
                    page_cursor: None,
                },
            )
            .await;
            timeout(Duration::from_secs(5), query_gate.entered.notified())
                .await
                .expect("query must enter the blocked document policy");
            let previous_principal = host.current_principal.lock().await.take();
            query_gate.release.notify_one();
            assert_rejected(
                read_frame(&mut client).await,
                NativeBusinessDataErrorCode::Unauthorized,
            );
            *host.current_principal.lock().await = previous_principal;
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
        // Reusing a stopped service must reject Open before asking the host
        // for another transport or requesting credentials.
        let requests_before = credential_requests.load(Ordering::SeqCst);
        let stopped_service = service.clone();
        let stopped_ipc = BusinessDataIpc::new(Arc::new(move |credentials, events| {
            Ok(Arc::new(stopped_service.dispatcher(credentials, events)))
        }));
        let (mut stopped_client, stopped_native) = tokio::io::duplex(64 * 1024);
        let stopped_serve =
            tokio::spawn(async move { stopped_ipc.serve(Box::new(stopped_native)).await });
        send_request(&mut stopped_client, "open-after-shutdown", open_request()).await;
        assert_rejected(
            read_frame(&mut stopped_client).await,
            NativeBusinessDataErrorCode::UnknownSession,
        );
        assert_eq!(credential_requests.load(Ordering::SeqCst), requests_before);
        assert_eq!(service.owned_cleanup_count(), 0);
        drop(stopped_client);
        assert!(stopped_serve.await.unwrap().is_err());
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
    query_request_with_cursor(
        &NativeBusinessDataSessionRef {
            handle: "unknown".into(),
            generation: 1,
        },
        None,
    )
}

fn fixture_query() -> Value {
    json!({
        "selector": {"id": {"$gte": "native-", "$lt": "native."}},
        "sort": [{"id": "asc"}]
    })
}

fn query_request_with_cursor(
    session: &NativeBusinessDataSessionRef,
    page_cursor: Option<String>,
) -> NativeBusinessDataOperation {
    NativeBusinessDataOperation::Query {
        session: session.clone(),
        query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
            collection: "records".into(),
            scope: NativeBusinessDataScope::Instance {},
            query: fixture_query(),
            page_size: 2,
        },
        page_cursor,
    }
}

fn watch_request(
    session: &ctox_sync::business_data_contract::NativeBusinessDataSessionRef,
    resume_cursor: Option<&str>,
) -> NativeBusinessDataOperation {
    NativeBusinessDataOperation::Watch {
        session: session.clone(),
        query: ctox_sync::business_data_contract::NativeBusinessDataQuery {
            collection: "records".into(),
            scope: NativeBusinessDataScope::Instance {},
            query: fixture_query(),
            page_size: 10,
        },
        resume_cursor: resume_cursor.map(str::to_owned),
    }
}

async fn read_event(
    stream: &mut (impl tokio::io::AsyncRead + Unpin),
    context: &str,
) -> ctox_sync::business_data_contract::NativeBusinessDataEvent {
    let frame = tokio::time::timeout(DATA_TIMEOUT, read_frame(stream))
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {context}"));
    match frame {
        Frame::Event { event } => event,
        other => panic!("expected event for {context}: {other:?}"),
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
