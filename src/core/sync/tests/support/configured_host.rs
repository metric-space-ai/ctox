//! Restart acceptance of persisted host pins through the real native attachments.
use super::{native_fixture, SignalingFixture};
use ctox_sync::{
    authority::{
        auth::SigningIdentity, client::ExecutionAuthority, Command, ExecutionSpec, Peer, Receipt,
        Request, WorkerMembership,
    },
    host_config::{self, HostConfiguration, HostMember},
    native::{NativePeerRole, NativeSyncSession},
};
use rusqlite::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restarted_hosts_recover_confirmed_worker_and_job_from_persisted_configuration() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let signal = SignalingFixture::with_roles([
            "ctox_instance", "ctox_instance", "ctox_instance", "workjet_executor",
            "ctox_instance", "ctox_instance", "ctox_instance", "workjet_executor",
        ]).await;
        let root = tempfile::tempdir().unwrap();
        let key_bytes: Vec<_> = (0..4).map(|_| SigningIdentity::generate_pkcs8().unwrap()).collect();
        let identities: Vec<_> = key_bytes.iter().map(|bytes| SigningIdentity::from_pkcs8(bytes).unwrap().public_identity()).collect();
        let voters: BTreeMap<_,_> = (1..=3).map(|id| (id, Peer { identity: identities[id as usize - 1].clone(), executor: id != 3, data_replica: id != 3 })).collect();
        let member = WorkerMembership { node_id: 4, identity: identities[3].clone(), data_replica: true, revoked: false };
        for id in 1..=4 {
            let host_root = root.path().join(id.to_string());
            std::fs::create_dir_all(&host_root).unwrap();
            let mut connection = Connection::open(host_root.join("ctox-runtime.sqlite3")).unwrap();
            host_config::save(&mut connection, &HostConfiguration {
                version: 1, scope_id: "restart-network".into(),
                local: if id == 4 { HostMember::Worker { member: member.clone() } } else { HostMember::Voter { node_id: id } },
                voters: voters.clone(), timing: Default::default(),
            }).unwrap();
        }
        let request = Request {
            request_id: "create-before-restart".into(), actor: 4,
            command: Command::Create { owner: 4, spec: ExecutionSpec {
                job_id: "persisted-job".into(), session_id: "persisted-session".into(), scope_id: "restart-network".into(),
                harness: "codex".into(), harness_version: "fixture".into(), model_route_id: "route".into(),
                gateway_account_id: "account".into(), model_id: "model".into(), required_capabilities: BTreeSet::new(),
            } },
        };
        let mut ownership = None;
        for round in 0..2 {
            let mut sessions = Vec::new();
            let mut databases = Vec::new();
            let mut nodes = Vec::new();
            let mut worker = None;
            for id in 1..=4 {
                let host_root = root.path().join(id.to_string());
                // Reopen the runtime DB and reconstruct the key object on every
                // start. Ephemeral signaling IDs and route hints are not loaded.
                let connection = Connection::open(host_root.join("ctox-runtime.sqlite3")).unwrap();
                let config = host_config::load(&connection).unwrap().unwrap();
                drop(connection);
                let key = Arc::new(SigningIdentity::from_existing_pkcs8(&key_bytes[id as usize - 1], config.identity().unwrap()).unwrap());
                let (directory, database, mut options) = native_fixture::control_options(signal.url.clone(), &config.room(), &format!("session-{id}")).await;
                options.peer_role = if id == 4 { NativePeerRole::WorkjetExecutor } else { NativePeerRole::CtoxInstance };
                options.admission.session = Arc::new(|payload, _| {
                    use rxdb::plugins::replication_webrtc::webrtc_types::WebRTCPeerSessionValidation;
                    let expected = match payload.pointer("/peerSession/sessionId").and_then(serde_json::Value::as_str) {
                        Some("session-1" | "session-2" | "session-3") => Some("ctox_instance"),
                        Some("session-4") => Some("workjet_executor"), _ => None,
                    };
                    if expected.is_some() && payload.pointer("/peerSession/role").and_then(serde_json::Value::as_str) == expected {
                        WebRTCPeerSessionValidation::Accept
                    } else { WebRTCPeerSessionValidation::Reject }
                });
                let mut session = NativeSyncSession::start(options).await.unwrap();
                if id == 4 {
                    worker = Some(session.attach_worker(config.worker_options(&root.path().join(format!("ipc{id}")), &key).unwrap(), key).await.unwrap().clone());
                    assert!(!config.directory(&host_root).unwrap().join("authority.sqlite3").exists());
                } else {
                    let attachment = config.voter_options(&host_root, &root.path().join(format!("ipc{id}")), &key).unwrap();
                    std::fs::create_dir_all(attachment.store_path.parent().unwrap()).unwrap();
                    nodes.push(session.attach_execution(attachment, key).await.unwrap().node().clone());
                }
                sessions.push(session);
                databases.push((directory, database));
            }
            for node in &nodes { node.wait_for_leader(Duration::from_secs(15)).await.unwrap(); }
            let worker = worker.unwrap();
            if round == 0 {
                assert!(worker.node().submit(request.clone()).await.is_err(), "a local configuration must not admit its worker");
                let admission = nodes[0].submit(Request { request_id: "admit".into(), actor: 1, command: Command::AdmitWorker { worker: member.clone() } }).await.unwrap();
                assert!(matches!(admission, Receipt::WorkerApplied(_)), "admission: {admission:?}");
                let Receipt::Applied(job) = worker.node().submit(request.clone()).await.unwrap() else { panic!("worker job not committed") };
                ownership = Some(job.ownership);
            } else {
                let job = worker.node().validate_ownership("persisted-job", ownership.as_ref().unwrap()).await.unwrap();
                assert_eq!(&job.ownership, ownership.as_ref().unwrap());
                assert!(matches!(worker.node().submit(request.clone()).await.unwrap(), Receipt::Replayed(_)));
            }
            for node in &nodes { assert_eq!(node.worker_membership(4).await.unwrap(), Some(member.clone())); }
            if round == 1 {
                assert!(matches!(nodes[0].submit(Request { request_id: "revoke".into(), actor: 1, command: Command::RevokeWorker { node_id: 4 } }).await.unwrap(), Receipt::WorkerApplied(_)));
                assert!(worker.node().validate_ownership("persisted-job", ownership.as_ref().unwrap()).await.is_err());
            }
            for session in &sessions { session.shutdown().await; }
            assert!(worker.node().validate_ownership("persisted-job", ownership.as_ref().unwrap()).await.is_err());
            for (_, database) in &databases { database.close().await.unwrap(); }
        }
    }).await.expect("persisted native host restart deadline");
}
