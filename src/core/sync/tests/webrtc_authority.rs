#![cfg(feature = "webrtc")]
//! Actual localhost WebRTC/UDP channels; only signaling is an isolated fixture.
#[path = "support/checkpoint.rs"]
mod checkpoint_fixture;
#[cfg(unix)]
#[path = "support/configured_host.rs"]
mod configured_host;
#[cfg(unix)]
#[path = "support/host_lifecycle.rs"]
mod host_lifecycle;
#[path = "support/native.rs"]
mod native_fixture;
use ctox_sync::authority::{
    auth::{SignedTransport, SigningIdentity},
    node::AuthorityNode,
    webrtc::{register_receiver, WebRtcControlChannel},
    Command, ExecutionSpec, Peer, Receipt, Request,
};
use futures::StreamExt;
use rxdb::plugins::replication_webrtc::{
    replicate_web_rtc_multi_with_validators, SignalingClient, WebRTCConnectionHandler,
    WebRTCRsConfig, WebRTCRsConnectionHandler,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

#[path = "support/signaling.rs"]
mod signaling_fixture;
use signaling_fixture::{route_ready, SignalingFixture};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn additional_worker_connects_to_three_voters_and_owns_a_supervised_ipc() {
    exercise_worker_session(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn worker_reconnect_preserves_identity_and_rebuilds_channels() {
    exercise_worker_session(Some(4)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn voter_reconnect_uses_pinned_key_to_restore_quorum_and_worker_routes() {
    exercise_worker_session(Some(3)).await;
}

#[cfg(unix)]
async fn exercise_worker_session(reconnect: Option<u64>) {
    use ctox_sync::{
        authority::{client::ExecutionAuthority, WorkerMembership},
        contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
        ipc::IPC_PROTOCOL_VERSION,
        native::{NativePeerRole, NativeSyncSession},
        native_execution::{ExecutionGroupOptions, WorkerExecutionOptions},
    };
    use rxdb::plugins::replication_webrtc::{send_message_and_await_answer, WebRTCMessage};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };
    async fn call(
        endpoint: &std::path::Path,
        id: &str,
        operation: SyncIpcOperation,
    ) -> SyncIpcResult {
        let mut stream = UnixStream::connect(endpoint).await.unwrap();
        let bytes = serde_json::to_vec(&SyncIpcRequest {
            version: IPC_PROTOCOL_VERSION,
            request_id: id.into(),
            operation,
        })
        .unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(&bytes).await.unwrap();
        let length = stream.read_u32().await.unwrap() as usize;
        assert!(length > 0 && length <= ctox_sync::ipc::IPC_MAX_FRAME_BYTES);
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await.unwrap();
        let reply: SyncIpcResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reply.version, IPC_PROTOCOL_VERSION);
        assert_eq!(reply.request_id, id);
        reply.result
    }
    tokio::time::timeout(Duration::from_secs(60), async {
        let signal = SignalingFixture::with_roles([
            "ctox_instance",
            "ctox_instance",
            "ctox_instance",
            "workjet_executor",
            if reconnect == Some(3) { "ctox_instance" } else { "workjet_executor" },
        ])
        .await;
        let root = tempfile::tempdir().unwrap();
        let keys: BTreeMap<_, _> = (1..=4)
            .map(|id| {
                (
                    id,
                    Arc::new(
                        SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap())
                            .unwrap(),
                    ),
                )
            })
            .collect();
        let voters: BTreeMap<_, _> = (1..=3)
            .map(|id| {
                (
                    id,
                    Peer {
                        identity: keys[&id].public_identity(),
                        executor: id != 3,
                        data_replica: id != 3,
                    },
                )
            })
            .collect();
        let routes: BTreeMap<_, _> = (1..=3).map(|id| (id, format!("native{id:06}"))).collect();
        let member = WorkerMembership {
            node_id: 4,
            identity: keys[&4].public_identity(),
            data_replica: true,
            revoked: false,
        };
        let mut sessions = BTreeMap::new();
        let mut nodes = BTreeMap::new();
        let mut databases = Vec::new();
        let mut peer_errors = BTreeMap::new();
        for id in 1..=4 {
            // The coordination vote and Workjet worker never create Business OS
            // collections. Voters 1/2 retain real data for the denial assertion.
            let (directory, database, mut options) = if id >= 3 {
                native_fixture::control_options(signal.url.clone(), "authority-fixture", &format!("session-{id}")).await
            } else {
                native_fixture::options(signal.url.clone(), "authority-fixture", &format!("session-{id}")).await
            };
            if id == 1 {
                let mut extra = database.add_collections(std::collections::HashMap::from([
                    ("extra_records".into(), rxdb::rx_database::RxCollectionCreator {
                        schema: options.collections[0].schema.as_ref().unwrap().json_schema.clone(),
                        conflict_handler: None,
                        options: Default::default(),
                    })
                ])).await.unwrap();
                // Different representatives on the one-/two-collection pair
                // must not prevent room admission or compare unrelated schemas.
                options.collections.insert(0, extra.remove("extra_records").unwrap());
            }
            options.peer_role = if id == 4 {
                NativePeerRole::WorkjetExecutor
            } else {
                NativePeerRole::CtoxInstance
            };
            options.admission.session = Arc::new(|payload, _| {
                use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation;
                let expected = match payload
                    .pointer("/peerSession/sessionId")
                    .and_then(Value::as_str)
                {
                    Some("session-1" | "session-2" | "session-3") => Some("ctox_instance"),
                    Some("session-4") => Some("workjet_executor"),
                    _ => None,
                };
                if expected.is_some()
                    && payload.pointer("/peerSession/role").and_then(Value::as_str) == expected
                {
                    WebRTCPeerSessionValidation::Accept
                } else {
                    WebRTCPeerSessionValidation::Reject
                }
            });
            let mut session = NativeSyncSession::start(options).await.unwrap();
            peer_errors.insert(id, session.pool().error_subject.subscribe());
            if id >= 3 {
                assert!(database.collections.lock().is_empty());
                assert!(session.pool().collections().is_empty());
            }
            if id != 4 {
                let group = session
                    .attach_execution(
                        ExecutionGroupOptions {
                            timing: Default::default(),
                            node_id: id,
                            scope_id: "scope".into(),
                            room: "authority-fixture".into(),
                            peers: voters.clone(),
                            routes: if reconnect == Some(3) { BTreeMap::new() } else { routes.clone() },
                            store_path: root.path().join(format!("{id}.sqlite")),
                            ipc_directory: root.path().join(format!("ipc{id}")),
                        },
                        keys[&id].clone(),
                    )
                    .await
                    .unwrap();
                nodes.insert(id, group.node().clone());
            }
            sessions.insert(id, session);
            databases.push((directory, database));
        }
        for node in nodes.values() {
            node.wait_for_leader(Duration::from_secs(15)).await.unwrap_or_else(|error| {
                let states: Vec<_> = peer_errors.iter_mut().map(|(id, stream)| {
                    let mut errors = Vec::new();
                    for _ in 0..20 {
                        match futures::FutureExt::now_or_never(stream.next()) {
                            Some(Some(error)) => errors.push(error.to_string()),
                            _ => break,
                        }
                    }
                    json!({"id": id, "errors": errors,
                        "frames": sessions[id].pool().connection_handler.frame_transport_status_json(),
                        "ready": routes.values().map(|route| (route, route_ready(sessions[id].pool(), route))).collect::<BTreeMap<_,_>>()})
                }).collect();
                panic!("{error}; native room admission: {states:?}");
            });
        }
        let worker_options = || WorkerExecutionOptions {
            member: member.clone(),
            scope_id: "scope".into(),
            room: "authority-fixture".into(),
            voters: voters.clone(),
            routes: if reconnect == Some(3) { BTreeMap::new() } else { routes.clone() },
            ipc_directory: root.path().join("worker-ipc"),
        };
        let diagnostic_pools: BTreeMap<_, _> = sessions
            .iter()
            .map(|(&id, session)| (id, Arc::downgrade(session.pool())))
            .collect();
        let diagnostics = || diagnostic_pools.iter().map(|(id, pool)| {
            json!({
                "id": id,
                "leader": nodes.get(id).and_then(|node| node.leader()),
                "authority": nodes.get(id).map(|node| node.diagnostics()),
                "frames": pool.upgrade().map(|pool| pool.connection_handler.frame_transport_status_json()),
                "ready": pool.upgrade().map(|pool| routes.values().map(|route| (route, route_ready(&pool, route))).collect::<BTreeMap<_, _>>()),
            })
        }).collect::<Vec<_>>();
        let mut worker_session = sessions.remove(&4).unwrap();
        let session = &mut worker_session;
        let worker = session
            .attach_worker(worker_options(), keys[&4].clone())
            .await
            .unwrap()
            .clone();
        assert_eq!(
            session
                .attach_worker(worker_options(), keys[&4].clone())
                .await
                .err()
                .unwrap()
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        // The worker's native000004 route is greater than every voter route.
        // Discovery must connect it using the same single-initiator rule as voters.
        tokio::time::timeout(Duration::from_secs(15), async {
            while !routes
                .values()
                .all(|route| route_ready(session.pool(), route))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| {
            panic!(
                "worker did not establish admitted channels: {error}; {}",
                session
                    .pool()
                    .connection_handler
                    .frame_transport_status_json()
            )
        });
        let spec = ExecutionSpec {
            job_id: "worker-job".into(),
            session_id: "worker-session".into(),
            scope_id: "scope".into(),
            harness: "codex".into(),
            harness_version: "fixture".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
        };
        assert!(matches!(
            call(
                worker.ipc_endpoint(),
                "before-admission",
                SyncIpcOperation::Create { spec: spec.clone() }
            )
            .await,
            SyncIpcResult::Unavailable { .. }
        ));
        let admission = nodes[&1].submit(Request {
            request_id: "admit-worker".into(), actor: 1,
            command: Command::AdmitWorker { worker: member.clone() }
        }).await.unwrap_or_else(|error| panic!("worker admission was not confirmed: {error}; {:?}", diagnostics()));
        assert!(matches!(admission, Receipt::WorkerApplied(_)), "{admission:?}; {:?}", diagnostics());
        let membership = call(
            worker.ipc_endpoint(),
            "current-membership",
            SyncIpcOperation::WorkerMembership { node_id: 4 },
        ).await;
        assert!(matches!(membership,
            SyncIpcResult::WorkerMembership { node_id: 4, worker: Some(ref current) }
                if current == &member), "{membership:?}; {:?}", diagnostics());
        let created = call(
            worker.ipc_endpoint(),
            "worker-create",
            SyncIpcOperation::Create { spec: spec.clone() },
        )
        .await;
        let ownership = match created {
            SyncIpcResult::Applied { ownership, .. } => ownership,
            other => panic!("worker create was not confirmed: {other:?}; {:?}", diagnostics()),
        };
        assert_eq!(ownership.node_id, 4);
        assert_eq!(ownership.generation, 1);
        if reconnect == Some(4) {
            // Capture old generations BEFORE rejoining. Looking them up after
            // signaling announces the new address can select a replacement.
            let retired: Vec<_> = routes.values().map(|route| {
                session.pool().connection_handler.connection_for_peer(route)
                    .expect("old worker channel is open")
            }).collect();
            let reopened = signal.disconnect_and_wait_for_rejoin("native000004").await;
            assert_eq!(reopened, "native000005");
            // Leave the old DataChannels open. A fresh offer to the new local
            // signaling identity must replace them without a manual reset.
            tokio::time::timeout(Duration::from_secs(15), async {
                while !retired.iter().all(|old| {
                    let pool = session.pool();
                    pool.connection_handler.connection_for_peer(old.peer_id())
                        .is_some_and(|current| current != *old && pool.is_peer_ready_for_control(&current))
                }) || !(1..=3).all(|id| {
                    route_ready(&diagnostic_pools[&id].upgrade().unwrap(), &reopened)
                }) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.unwrap_or_else(|error| panic!("worker did not establish three admitted channels from its new signaling route: {error}; {:?}", diagnostics()));
            // Delayed teardown must target the retired generation, never the
            // replacement sharing the same remote signaling address.
            for old in retired {
                let handler = &session.pool().connection_handler;
                let replacement = handler.connection_for_peer(old.peer_id()).unwrap();
                handler.close_peer(&old).await;
                assert_eq!(handler.connection_for_peer(old.peer_id()), Some(replacement));
                assert!(route_ready(session.pool(), old.peer_id()));
            }
            let restored = call(worker.ipc_endpoint(), "after-reconnect", SyncIpcOperation::Validate {
                job_id: "worker-job".into(), ownership: ownership.clone(),
            }).await;
            assert!(matches!(restored, SyncIpcResult::Authorized { ownership: ref current, .. } if current == &ownership), "{restored:?}; {:?}", diagnostics());
            assert_eq!(worker.node().node_id(), 4);
            assert_eq!(worker.node().scope_id(), "scope");
            let membership = call(worker.ipc_endpoint(), "current-membership",
                SyncIpcOperation::WorkerMembership { node_id: 4 }).await;
            assert!(matches!(membership,
                SyncIpcResult::WorkerMembership { node_id: 4, worker: Some(ref current) }
                    if current == &member), "{membership:?}; {:?}", diagnostics());
        }
        if reconnect == Some(3) {
            let pool = sessions[&3].pool();
            let retired: Vec<_> = ["native000001", "native000002", "native000004"]
                .into_iter().map(|route| {
                    pool.connection_handler.connection_for_peer(route)
                        .expect("old voter channel is open")
                }).collect();
            let reopened = signal.disconnect_and_wait_for_rejoin("native000003").await;
            assert_eq!(reopened, "native000005");
            // Tear down precisely the pre-reconnect generations. A fresh offer
            // may already have installed a replacement at the same remote route.
            for connection in &retired {
                pool.connection_handler.close_peer(connection).await;
            }
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let connected = [1, 2, 4].into_iter().all(|id| {
                        let pool = diagnostic_pools[&id].upgrade().unwrap();
                        route_ready(&pool, &reopened)
                    });
                    if connected { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.unwrap_or_else(|error| panic!("new voter route was not admitted: {error}; {:?}", diagnostics()));
            // Voters 1 and 3 must now provide the majority without voter 2.
            // Stop the actual native session, including its IPC and Raft node.
            sessions[&2].shutdown().await;
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    if let Ok(Some(current)) = nodes[&3].worker_membership(4).await {
                        if current == member { break; }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.unwrap_or_else(|error| panic!("reconnected voter did not restore quorum: {error}; {:?}", diagnostics()));
            let restored = call(worker.ipc_endpoint(), "after-voter-reconnect", SyncIpcOperation::Validate {
                job_id: "worker-job".into(), ownership: ownership.clone(),
            }).await;
            assert!(matches!(restored, SyncIpcResult::Authorized { ownership: ref current, .. } if current == &ownership), "{restored:?}; {:?}", diagnostics());
        }
        let replayed = call(worker.ipc_endpoint(), "worker-create", SyncIpcOperation::Create { spec }).await;
        assert!(matches!(replayed, SyncIpcResult::Replayed { .. }), "worker replay was not confirmed: {replayed:?}; {:?}", diagnostics());
        let response = send_message_and_await_answer(
            session.pool().connection_handler.clone(),
            session.pool().connection_handler.connection_for_peer(&routes[&1]).unwrap(),
            WebRTCMessage {
                id: "worker-denied-business-read".into(),
                method: "masterChangesSince".into(),
                params: vec![Value::Null, json!(20)],
                collection: Some("records".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.result["code"], "RC_WEBRTC_PEER");
        assert_eq!(
            response.result["message"],
            "peer is not authorized for collection"
        );
        assert!(response.result.get("documents").is_none());
        assert!(matches!(
            nodes[&1]
                .submit(Request {
                    request_id: "revoke-worker".into(),
                    actor: 1,
                    command: Command::RevokeWorker { node_id: 4 }
                })
                .await
                .unwrap_or_else(|error| panic!("worker revocation was not confirmed: {error}; {:?}", diagnostics())),
            Receipt::WorkerApplied(_)
        ));
        assert!(matches!(
            call(
                worker.ipc_endpoint(),
                "revoked-worker",
                SyncIpcOperation::Validate {
                    job_id: "worker-job".into(),
                    ownership: ownership.clone()
                }
            )
            .await,
            SyncIpcResult::Rejected { .. }
        ));
        // A replayed admission is historical. Reusing the same read request ID
        // must still return the current tombstone over the worker's DataChannel.
        assert!(matches!(nodes[&1].submit(Request {
            request_id: "admit-worker".into(), actor: 1,
            command: Command::AdmitWorker { worker: member.clone() },
        }).await.unwrap(), Receipt::WorkerReplayed(ref historical)
            if historical == &member));
        let membership = call(worker.ipc_endpoint(), "current-membership",
            SyncIpcOperation::WorkerMembership { node_id: 4 }).await;
        let revoked = WorkerMembership { revoked: true, ..member };
        assert!(matches!(membership,
            SyncIpcResult::WorkerMembership { node_id: 4, worker: Some(ref current) }
                if current == &revoked), "{membership:?}; {:?}", diagnostics());
        assert!(matches!(call(worker.ipc_endpoint(), "foreign-membership",
            SyncIpcOperation::WorkerMembership { node_id: 1 }).await,
            SyncIpcResult::Rejected { .. }));
        let retained = worker.node().clone();
        let endpoint = worker.ipc_endpoint().to_path_buf();
        session.shutdown().await;
        assert!(!endpoint.exists());
        assert!(retained
            .validate_ownership("worker-job", &ownership)
            .await
            .is_err());
        assert!(retained.worker_membership(4).await.is_err());
        let offers = signal.offers.lock().unwrap().clone();
        assert!(!offers.is_empty(), "fixture must observe the actual SDP offers");
        assert!(signal.signals.lock().unwrap().keys().any(|(_, _, kind)| kind == "answer"),
            "fixture must observe SDP answers as well as offers");
        assert!(offers.iter().all(|(sender, receiver)| sender < receiver),
            "every native edge must have only its lower-ID initiator: {offers:?}");
        assert!(
            !root.path().join("4.sqlite").exists(),
            "a nonvoting worker must not open a Raft store"
        );
        for session in sessions.values() {
            session.shutdown().await;
        }
        for (_, database) in &databases {
            database.close().await.unwrap();
        }
    })
    .await
    .expect("four-peer native worker lifecycle timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_native_peers_commit_over_real_webrtc_without_http_data() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let signal = SignalingFixture::start().await;
        let root = tempfile::tempdir().unwrap();
        let keys: BTreeMap<_, _> = (1..=3)
            .map(|id| {
                (
                    id,
                    Arc::new(
                        SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap())
                            .unwrap(),
                    ),
                )
            })
            .collect();
        let peers: BTreeMap<_, _> = keys
            .iter()
            .map(|(&id, key)| {
                (
                    id,
                    Peer {
                        identity: key.public_identity(),
                        executor: id != 3,
                        data_replica: id != 3,
                    },
                )
            })
            .collect();
        let allowed = peers
            .values()
            .map(|peer| peer.identity.clone())
            .collect::<BTreeSet<_>>();
        let mut clients = BTreeMap::new();
        let mut handlers = BTreeMap::new();
        let mut connections = BTreeMap::new();
        for id in 1..=3 {
            let client = SignalingClient::connect(&signal.url).await.unwrap();
            let mut config = WebRTCRsConfig::new(client.clone(), "authority-fixture");
            config.udp_bind_addr = "127.0.0.1:0".into();
            let handler = WebRTCRsConnectionHandler::new_with_signaling(config)
                .await
                .unwrap();
            connections.insert(id, handler.connect_stream());
            clients.insert(id, client);
            handlers.insert(id, handler);
        }
        // Wait for every peer's descriptor, not an arbitrary network sleep.
        let browser_client = SignalingClient::connect(&signal.url).await.unwrap();
        let mut browser_config = WebRTCRsConfig::new(browser_client.clone(), "authority-fixture");
        browser_config.udp_bind_addr = "127.0.0.1:0".into();
        let browser_handler = WebRTCRsConnectionHandler::new_with_signaling(browser_config)
            .await
            .unwrap();
        for client in clients.values() {
            let mut stream = client.peer_list_stream();

            while stream.next().await.unwrap().len() < 4 {}
        }
        let browser_peer = browser_client.own_peer_id().unwrap();
        assert_eq!(
            clients[&1].peer_role(&browser_peer).as_deref(),
            Some("browser")
        );
        assert!(handlers[&1]
            .connect_native_execution_peer(browser_peer)
            .await
            .is_err());
        assert!(handlers[&1]
            .connect_native_execution_peer("unknown-peer".into())
            .await
            .is_err());
        let mut nodes = BTreeMap::new();

        let mut pools = BTreeMap::new();
        let mut databases = Vec::new();
        for id in 1..=3 {
            let (directory, database, options) = native_fixture::options(
                signal.url.clone(),
                "authority-fixture",
                &format!("session-{id}"),
            )
            .await;
            let handler = handlers[&id].clone();
            handler.set_collection_authz(options.admission.collection_read);
            handler.set_collection_write_authz(options.admission.collection_write);
            let pool = replicate_web_rtc_multi_with_validators(
                options.database,
                options.collections,
                handler,
                Some(Arc::new(move |connection| {
                    (options.admission.peer)(&connection.peer_id().to_owned())
                })),
                Some(options.admission.session),
                Some(options.room),
                Some(Arc::from(options.peer_session_id)),
            )
            .await
            .unwrap();
            databases.push((directory, database));
            let channel = Arc::new(
                WebRtcControlChannel::new(&pool, allowed.clone(), Duration::from_secs(3)).unwrap(),
            );
            for (&target, peer) in &peers {
                if target != id {
                    channel
                        .set_route(&peer.identity, clients[&target].own_peer_id().unwrap())
                        .unwrap();
                }
            }
            let transport = Arc::new(SignedTransport::new(
                keys[&id].clone(),
                "scope".into(),
                channel,
            ));
            let config = openraft::Config {
                heartbeat_interval: 150,
                election_timeout_min: 500,
                election_timeout_max: 900,
                ..Default::default()
            };
            let node = Arc::new(
                AuthorityNode::open(
                    id,
                    "scope".into(),
                    peers.clone(),
                    &root.path().join(format!("{id}.sqlite")),
                    transport,
                    config,
                )
                .await
                .unwrap(),
            );
            register_receiver(&pool, keys[&id].clone(), "scope".into(), &node).unwrap();
            pools.insert(id, pool);
            nodes.insert(id, node);
        }
        for (&id, handler) in &handlers {
            for (&target, client) in &clients {
                if target != id {
                    handler
                        .connect_native_execution_peer(client.own_peer_id().unwrap())
                        .await
                        .unwrap();
                }
            }
        }
        for stream in connections.values_mut() {
            let mut connected = BTreeSet::new();
            while connected.len() < 2 {
                connected.insert(stream.next().await.unwrap());
            }
        }
        nodes[&1].bootstrap().await.unwrap();
        for node in nodes.values() {
            node.wait_for_leader(Duration::from_secs(10)).await.unwrap();
        }
        let spec = ExecutionSpec {
            job_id: "job".into(),
            session_id: "session".into(),
            scope_id: "scope".into(),
            harness: "codex".into(),
            harness_version: "fixture".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
        };
        let job = match nodes[&1]
            .submit(Request {
                request_id: "create".into(),
                actor: 1,
                command: Command::Create { spec, owner: 1 },
            })
            .await
            .unwrap()
        {
            Receipt::Applied(job) => job,
            other => panic!("{other:?}"),
        };
        nodes[&1]
            .validate_ownership("job", &job.ownership)
            .await
            .unwrap();
        let receipts: Vec<_> = [1, 2]
            .into_iter()
            .map(|id| {
                checkpoint_fixture::copy_receipt(
                    root.path(),
                    id,
                    &keys[&id],
                    &job.spec,
                    &job.ownership,
                    1,
                )
            })
            .collect();
        let checkpoint_digest = receipts[0].checkpoint_digest.clone();
        assert!(matches!(
            nodes[&1]
                .submit(Request {
                    request_id: "protect".into(),
                    actor: 1,
                    command: Command::ProtectCheckpoint {
                        job_id: "job".into(),
                        ownership: job.ownership.clone(),
                        receipts
                    },
                })
                .await
                .unwrap(),
            Receipt::Applied(_)
        ));
        // Sever actual DataChannels, not a mock transport flag.
        let previous_leader = nodes[&2].leader().unwrap();
        let source_peer = clients[&1].own_peer_id().unwrap();
        for id in [2, 3] {
            let source_connection = handlers[&1]
                .connection_for_peer(&clients[&id].own_peer_id().unwrap())
                .unwrap();
            let target_connection = handlers[&id].connection_for_peer(&source_peer).unwrap();
            handlers[&1].close_peer(&source_connection).await;
            handlers[&id].close_peer(&target_connection).await;
        }
        if previous_leader == 1 {
            nodes[&2]
                .wait_for_other_leader(1, Duration::from_secs(10))
                .await
                .unwrap();
        }
        assert!(nodes[&1]
            .validate_ownership("job", &job.ownership)
            .await
            .is_err());
        let taken = nodes[&2]
            .submit(Request {
                request_id: "takeover".into(),
                actor: 2,
                command: Command::TakeOver {
                    job_id: "job".into(),
                    expected: job.ownership.clone(),
                    checkpoint_digest,
                    owner: 2,
                },
            })
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "takeover was not confirmed: {error}; {:?}",
                    nodes
                        .iter()
                        .map(|(id, node)| (id, node.diagnostics()))
                        .collect::<BTreeMap<_, _>>()
                )
            });
        let new_owner = match taken {
            Receipt::Applied(job) => job.ownership,
            other => panic!("{other:?}"),
        };
        assert_eq!(new_owner.generation, 2);
        nodes[&2]
            .validate_ownership("job", &new_owner)
            .await
            .unwrap();
        // Rejoin using the explicit native setup. The old owner remains fenced.
        for id in [2, 3] {
            handlers[&1]
                .connect_native_execution_peer(clients[&id].own_peer_id().unwrap())
                .await
                .unwrap();
        }
        let mut rejoined = BTreeSet::new();
        while rejoined.len() < 2 {
            rejoined.insert(connections.get_mut(&1).unwrap().next().await.unwrap());
        }
        assert!(nodes[&1]
            .validate_ownership("job", &job.ownership)
            .await
            .is_err());
        for node in nodes.values() {
            node.shutdown().await.unwrap();
        }

        for pool in pools.values() {
            pool.cancel().await;
        }
        for (_directory, database) in databases {
            database.close().await.unwrap();
        }
        for handler in handlers.values() {
            handler.close().await.unwrap();
        }
        for client in clients.values() {
            client.close().await;
        }
        browser_handler.close().await.unwrap();
        browser_client.close().await;
    })
    .await
    .expect("real WebRTC authority fixture exceeded 60 seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn native_sessions_own_authority_without_granting_business_data_access() {
    exercise_native_session_group(NativeGroupScenario::Shutdown).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn workjet_ipc_uses_the_owned_native_execution_group() {
    exercise_native_session_group(NativeGroupScenario::WorkjetIpc).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn dropping_native_session_stops_retained_authority_and_ipc_handles() {
    exercise_native_session_group(NativeGroupScenario::Drop).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn configured_workjet_signaling_peers_reach_native_authority() {
    exercise_native_session_group(NativeGroupScenario::WorkjetRoles).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(unix)]
async fn configured_workjet_signaling_peers_connect_to_ctox_coordinator() {
    exercise_native_session_group(NativeGroupScenario::MixedRoles).await;
}

#[cfg(unix)]
enum NativeGroupScenario {
    Shutdown,
    WorkjetIpc,
    Drop,
    WorkjetRoles,
    MixedRoles,
}

#[cfg(unix)]
async fn exercise_native_session_group(scenario: NativeGroupScenario) {
    use ctox_sync::{native::NativeSyncSession, native_execution::ExecutionGroupOptions};
    use rxdb::plugins::replication_webrtc::{send_message_and_await_answer, WebRTCMessage};
    tokio::time::timeout(Duration::from_secs(60), async {
        let roles = match scenario {
            NativeGroupScenario::WorkjetRoles => ["workjet_executor"; 3],
            NativeGroupScenario::MixedRoles => {
                ["workjet_executor", "workjet_executor", "ctox_instance"]
            }
            _ => ["ctox_instance"; 3],
        };
        let signal = SignalingFixture::with_roles(roles).await;
        let root = tempfile::tempdir().unwrap();
        let keys: BTreeMap<_, _> = (1..=3)
            .map(|id| {
                (
                    id,
                    Arc::new(
                        SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap())
                            .unwrap(),
                    ),
                )
            })
            .collect();
        let peers: BTreeMap<_, _> = keys
            .iter()
            .map(|(&id, key)| {
                (
                    id,
                    Peer {
                        identity: key.public_identity(),
                        executor: id != 3,
                        data_replica: id != 3,
                    },
                )
            })
            .collect();
        let routes: BTreeMap<_, _> = (1..=3).map(|id| (id, format!("native{id:06}"))).collect();
        let group_options = |id| ExecutionGroupOptions {
            timing: Default::default(),
            node_id: id,
            scope_id: "scope".into(),
            room: "authority-fixture".into(),
            peers: peers.clone(),
            routes: routes.clone(),
            store_path: root.path().join(format!("{id}.sqlite")),
            ipc_directory: root.path().join(format!("ipc{id}")),
        };
        let mut sessions = BTreeMap::new();
        let mut nodes = BTreeMap::new();
        let mut endpoints = BTreeMap::new();
        let mut groups = BTreeMap::new();
        let mut databases = Vec::new();
        for id in 1..=3 {
            let (directory, database, mut options) = native_fixture::options(
                signal.url.clone(),
                "authority-fixture",
                &format!("session-{id}"),
            )
            .await;
            options.peer_role =
                ctox_sync::native::NativePeerRole::from_wire(roles[id as usize - 1]).unwrap();
            // Validate the runtime handshake independently of signaling. A worker
            // silently identifying itself as a production instance must fail.
            options.admission.session = Arc::new(move |payload, _| {
                use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation;
                let session_id = payload
                    .pointer("/peerSession/sessionId")
                    .and_then(Value::as_str);
                let expected = roles.iter().enumerate().find_map(|(index, role)| {
                    (session_id == Some(format!("session-{}", index + 1).as_str())).then_some(*role)
                });
                if expected.is_some()
                    && payload.pointer("/peerSession/role").and_then(Value::as_str) == expected
                {
                    WebRTCPeerSessionValidation::Accept
                } else {
                    WebRTCPeerSessionValidation::Reject
                }
            });
            let mut session = NativeSyncSession::start(options).await.unwrap();
            let group = session
                .attach_execution(group_options(id), keys[&id].clone())
                .await
                .unwrap();
            nodes.insert(id, group.node().clone());
            endpoints.insert(id, group.ipc_endpoint().to_path_buf());
            groups.insert(id, group.clone());
            assert_eq!(
                session
                    .attach_execution(group_options(id), keys[&id].clone())
                    .await
                    .err()
                    .unwrap()
                    .kind(),
                std::io::ErrorKind::AlreadyExists
            );
            sessions.insert(id, session);
            databases.push((directory, database));
        }
        // The lifecycle itself discovers and connects the configured peers.
        for node in nodes.values() {
            if let Err(error) = node.wait_for_leader(Duration::from_secs(15)).await {
                let status: Vec<_> = sessions
                    .iter()
                    .map(|(id, session)| {
                        (
                            id,
                            session
                                .pool()
                                .connection_handler
                                .frame_transport_status_json(),
                            routes
                                .values()
                                .map(|route| (route, route_ready(session.pool(), route)))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                panic!("{error}; native transport/admission state: {status:?}");
            }
        }
        let spec = ExecutionSpec {
            job_id: "job".into(),
            session_id: "session".into(),
            scope_id: "scope".into(),
            harness: "codex".into(),
            harness_version: "fixture".into(),
            model_route_id: "route".into(),
            gateway_account_id: "account".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::new(),
        };
        let job = match nodes[&1]
            .submit(Request {
                request_id: "create".into(),
                actor: 1,
                command: Command::Create { spec, owner: 1 },
            })
            .await
            .unwrap()
        {
            Receipt::Applied(job) => job,
            other => panic!("{other:?}"),
        };
        nodes[&1]
            .validate_ownership("job", &job.ownership)
            .await
            .unwrap_or_else(|error| {
                let state: Vec<_> = nodes
                    .iter()
                    .map(|(id, node)| {
                        let pool = sessions[id].pool();
                        (
                            id,
                            node.leader(),
                            pool.connection_handler.frame_transport_status_json(),
                            routes
                                .values()
                                .map(|route| (route, route_ready(pool, route)))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                panic!("{error}; native control transport at failed quorum read: {state:?}");
            });
        sessions[&1]
            .pool()
            .collections()
            .into_iter()
            .find(|collection| collection.name == "records")
            .unwrap()
            .insert(json!({"id":"private-business-record"}))
            .await
            .unwrap();
        let response = send_message_and_await_answer(
            sessions[&2].pool().connection_handler.clone(),
            sessions[&2]
                .pool()
                .connection_handler
                .connection_for_peer(&routes[&1])
                .unwrap(),
            WebRTCMessage {
                id: "denied-business-read".into(),
                method: "masterChangesSince".into(),
                params: vec![Value::Null, json!(20)],
                collection: Some("records".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.result["type"], "ctoxError", "{response:?}");
        assert_eq!(response.result["code"], "RC_WEBRTC_PEER");
        assert_eq!(
            response.result["message"],
            "peer is not authorized for collection"
        );
        assert!(response.result.get("documents").is_none());
        if matches!(scenario, NativeGroupScenario::WorkjetIpc) {
            // A second native lifecycle must not replace the first group's listener.
            // Its own node/transport are torn down after the bind failure.
            let (rejected_root, rejected_db, rejected_options) = native_fixture::options(
                signal.url.clone(),
                "authority-fixture",
                "rejected-session",
            )
            .await;
            let mut rejected = NativeSyncSession::start(rejected_options).await.unwrap();
            let mut conflicting = group_options(3);
            conflicting.routes.insert(3, "native000004".into());
            conflicting.store_path = root.path().join("rejected.sqlite");
            conflicting.ipc_directory = root.path().join("ipc1");
            let error = rejected
                .attach_execution(conflicting, keys[&3].clone())
                .await
                .err()
                .unwrap();
            assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
            assert!(rejected
                .pool()
                .canceled
                .load(std::sync::atomic::Ordering::SeqCst));
            assert!(endpoints[&1].exists());
            rejected_db.close().await.unwrap();
            drop(rejected_root);

            // The real Workjet consumer reaches the same group through its owned IPC
            // endpoint, not a separately constructed listener or simulated bus.
            use std::process::Stdio;
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let workjet = source.join("../../../../workjet").canonicalize().unwrap();
            let mut client_spec = job.spec.clone();
            client_spec.job_id = "workjet-job".into();
            client_spec.session_id = "workjet-session".into();
            let mut client = tokio::process::Command::new("node")
                .arg(source.join("tests/support/workjet_ipc_client.mjs"))
                .arg(workjet.join("apps/server/src/workjet/sync/WorkjetSyncIpc.ts"))
                .arg(&endpoints[&1])
                .arg(serde_json::to_string(&client_spec).unwrap())
                .current_dir(&workjet)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut input = client.stdin.take().unwrap();
            let mut output = BufReader::new(client.stdout.take().unwrap()).lines();
            assert_eq!(
                output.next_line().await.unwrap().as_deref(),
                Some("authorized")
            );
            sessions[&2].shutdown().await;
            sessions[&3].shutdown().await;
            input.write_all(b"partition\n").await.unwrap();
            assert_eq!(
                output.next_line().await.unwrap().as_deref(),
                Some("revoked")
            );
            sessions[&1].shutdown().await;
            assert!(!endpoints[&1].exists());
            input.write_all(b"host-stopped\n").await.unwrap();
            assert_eq!(
                output.next_line().await.unwrap().as_deref(),
                Some("disconnected")
            );
            drop(input);
            assert!(client.wait().await.unwrap().success());
        } else {
            // Preserve the stronger shutdown check: the other two voters are
            // still available, so loss of authorization is not merely quorum loss.
            if matches!(scenario, NativeGroupScenario::Drop) {
                let pool = sessions[&1].pool().clone();
                drop(sessions.remove(&1).unwrap());
                // Retaining a group/node handle must not keep its authority or
                // endpoint alive after the owning session is dropped.
                groups[&1].wait_stopped().await.unwrap();
                assert!(!endpoints[&1].exists());
                // The group acknowledgement precedes transport cleanup. Await
                // its idempotent completion before closing the fixture database.
                pool.cancel().await;
            } else {
                sessions[&1].shutdown().await;
            }
            assert!(!endpoints[&1].exists());
        }
        assert!(nodes[&1]
            .validate_ownership("job", &job.ownership)
            .await
            .is_err());
        for session in sessions.values() {
            session.shutdown().await;
        }
        for (_directory, database) in databases {
            database.close().await.unwrap();
        }
    })
    .await
    .expect("native authority lifecycle exceeded 60 seconds");
}
