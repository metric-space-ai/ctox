use super::super::v4_native_commands::NativeCommandAuthority;
use super::*;
use crate::rx_error::RxResult;
use ctox_store::change_log::subscription::write_contract::routes::{self, NativeCommand};
use serde_json::json;
use std::sync::atomic::AtomicBool;

struct CommandAuthority(Arc<AtomicBool>);
impl NativeCommandAuthority for CommandAuthority {
    fn with_current(
        &self,
        command: &NativeCommand,
        peer: &str,
        token: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()> {
        if !self.0.load(Ordering::SeqCst)
            || command.actor_id != "alice"
            || peer != "browser"
            || token != "admitted-token"
        {
            return Err(new_rx_error("E25_AUTHORITY_REVOKED", None));
        }
        effect()
    }
    fn with_completion(
        &self,
        _tx: &rusqlite::Transaction<'_>,
        command: &NativeCommand,
        peer: &str,
        token: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()> {
        self.with_current(command, peer, token, effect)
    }
}
fn command(method: &str) -> NativeCommand {
    NativeCommand {
        command_id: "command-e25".into(),
        kind: method.into(),
        payload: json!({"action":"fixture"}),
        app_id: "sender-app".into(),
        actor_id: "alice".into(),
    }
}
#[tokio::test]
async fn e25_adapter_reuses_registered_handler_and_retains_original_actor() {
    use sha2::Digest;
    println!(
        "E25_ADAPTER_SOURCE_SHA256 {:x}",
        sha2::Sha256::digest(include_bytes!("v4_native_commands.rs"))
    );
    let collection = crate::rx_collection::test_support::test_collection_named("e25_routes").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    let inventory: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/sync_v4_e25/e25-methods.json"
    ))
    .unwrap();
    for item in inventory["methods"].as_array().unwrap() {
        let method = item["method"].as_str().unwrap();
        let expected_method = method.to_owned();
        pool.register_auxiliary_request_handler(
            method,
            Arc::new(move |identity, token, params| {
                let expected_method = expected_method.clone();
                Box::pin(async move {
                    assert_eq!(identity, "browser");
                    assert_eq!(token, "admitted-token");
                    assert_eq!(params, vec![json!({"action":"fixture"})]);
                    Ok(json!({"native_method":expected_method,"state":"finished"}))
                })
            }),
        )
        .unwrap();
        let pending = pool
            .execute_native_command(peer.clone(), command(method), authority.clone())
            .await
            .unwrap_or_else(|e| panic!("route {} must not return {}", method, e.reason));
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        let mut published = false;
        pending
            .publish(&tx, None, &mut |_, result| {
                assert_eq!(result.command_id, "command-e25");
                assert_eq!(result.status, "done");
                assert_eq!(result.result.unwrap()["native_method"], method);
                published = true;
                Ok(())
            })
            .unwrap();
        assert!(published);
        let mut forged = command(method);
        forged.actor_id = "mallory".into();
        let denied = pool
            .execute_native_command(peer.clone(), forged, authority.clone())
            .await
            .err()
            .unwrap();
        assert_eq!(denied.reason, "command_authority_changed");
    }
    pool.cancel().await;
}
#[tokio::test]
async fn e25_revocation_between_execution_and_completion_cannot_publish() {
    let collection = crate::rx_collection::test_support::test_collection_named("e25_revoked").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let active = Arc::new(AtomicBool::new(true));
    let authority = Arc::new(CommandAuthority(active.clone()));
    pool.register_auxiliary_request_handler(
        routes::ROUTES[0].method,
        Arc::new(|_, _, _| Box::pin(async { Ok(json!({"ok":true})) })),
    )
    .unwrap();
    for revoke_actor in [false, true] {
        active.store(true, Ordering::SeqCst);
        handler
            .capabilities
            .lock()
            .insert(peer.clone(), "admitted-token".into());
        let pending = pool
            .execute_native_command(
                peer.clone(),
                command(routes::ROUTES[0].method),
                authority.clone(),
            )
            .await
            .unwrap();
        if revoke_actor {
            active.store(false, Ordering::SeqCst)
        } else {
            handler
                .capabilities
                .lock()
                .insert(peer.clone(), "different-token".into());
        }
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        let mut called = false;
        assert!(pending
            .publish(&tx, None, &mut |_, _| {
                called = true;
                Ok(())
            })
            .is_err());
        assert!(!called);
    }
    pool.cancel().await;
}

#[tokio::test]
async fn e25_native_publication_guard_survives_the_command_adapter() {
    let collection =
        crate::rx_collection::test_support::test_collection_named("e25_native_guard").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let alive = Arc::new(PlMutex::new(true));
    let native_alive = alive.clone();
    pool.register_guarded_auxiliary_request_handler(
        "ctox.workjet.grok.v1",
        Arc::new(move |_, _, _| {
            let native_alive = native_alive.clone();
            Box::pin(async move {
                Ok(GuardedAuxiliaryResponse {
                    result: json!({"ok":true}),
                    publication: Arc::new(AuxiliaryAuthority(native_alive)),
                })
            })
        }),
    )
    .unwrap();
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    let pending = pool
        .execute_native_command(peer, command("ctox.workjet.grok.v1"), authority)
        .await
        .unwrap();
    *alive.lock() = false;
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    let mut called = false;
    assert!(pending
        .publish(&tx, None, &mut |_, _| {
            called = true;
            Ok(())
        })
        .is_err());
    assert!(!called);
    pool.cancel().await;
}

#[tokio::test]
async fn e25_verified_cas_stages_off_writer_and_failed_completion_rolls_back_files() {
    use ctox_store::change_log::subscription::write_contract::routes::{
        ResultFiles, VerifiedResultFiles,
    };
    let collection =
        crate::rx_collection::test_support::test_collection_named("e25_cas_result").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    pool.register_auxiliary_request_handler(
        "ctox.browser.live.v1",
        Arc::new(|_, _, _| Box::pin(async { Ok(json!({"frame":"x".repeat(32768)})) })),
    )
    .unwrap();
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    let dir = tempfile::tempdir().unwrap();
    let cas = ctox_store::cas::Cas::open(dir.path().join("cas")).unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE files(ref TEXT,owner_app TEXT); CREATE TABLE completions(id TEXT,ref TEXT)",
    )
    .unwrap();
    for fail in [true, false] {
        let pending = pool
            .execute_native_command(
                peer.clone(),
                command("ctox.browser.live.v1"),
                authority.clone(),
            )
            .await
            .unwrap();
        let source = dir.path().join("native-result.json");
        let cas = cas.clone();
        let staged = pending
            .stage_files(move |command, bytes| async move {
                assert_eq!(command.app_id, "sender-app");
                // Test-only file creation outside any DB transaction; import itself
                // is the real bounded CAS path and produces an unforgeable receipt.
                std::fs::write(&source, &bytes).unwrap();
                let blob = cas.import(source, "a".repeat(64)).await.unwrap();
                Ok(Arc::new(VerifiedResultFiles {
                    blob,
                    insert: Arc::new(|tx, record| {
                        assert_eq!(record.owner_app, "sender-app");
                        assert_eq!(record.mime, "application/json");
                        tx.execute(
                            "INSERT INTO files VALUES(?1,?2)",
                            rusqlite::params![record.blob.r#ref, record.owner_app],
                        )
                        .unwrap();
                        Ok(())
                    }),
                }) as Arc<dyn ResultFiles>)
            })
            .await
            .unwrap()
            .unwrap();
        let tx = db.transaction().unwrap();
        let result = pending.publish(&tx, Some(staged.as_ref()), &mut |tx, completed| {
            let reference = completed.result.unwrap()["ref"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(reference.len(), 64);
            tx.execute(
                "INSERT INTO completions VALUES(?1,?2)",
                rusqlite::params![completed.command_id, reference],
            )
            .unwrap();
            if fail {
                Err(new_rx_error("E25_COMPLETION_CONFLICT", None))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), fail);
        // Even a caller that catches the error and commits cannot leave one
        // side published: the adapter savepoint must roll both back itself.
        tx.commit().unwrap();
        for table in ["files", "completions"] {
            let n: i64 = db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, if fail { 0 } else { 1 });
        }
    }
    pool.cancel().await;
}

#[tokio::test]
async fn e25_native_typed_error_reaches_completion_without_coarse_rethrow() {
    let collection = crate::rx_collection::test_support::test_collection_named("e25_error").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    pool.register_guarded_auxiliary_request_handler("ctox.workjet.grok.v1",Arc::new(|_,_,_|Box::pin(async {
        Err(serde_json::json!({"status":"rejected","code":"conflict","reason":"stale_fence"}).to_string())
    }))).unwrap();
    let pending = pool
        .execute_native_command(peer, command("ctox.workjet.grok.v1"), authority)
        .await
        .unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    pending
        .publish(&tx, None, &mut |_, completed| {
            assert_eq!(completed.command_id, "command-e25");
            assert_eq!(completed.status, "failed");
            let error = completed.error.unwrap();
            assert_eq!(error.code, "conflict");
            assert_eq!(error.reason, "stale_fence");
            assert!(completed.result.is_none());
            Ok(())
        })
        .unwrap();
    pool.cancel().await;
}
