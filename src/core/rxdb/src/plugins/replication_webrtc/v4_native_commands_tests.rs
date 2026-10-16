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

#[tokio::test]
async fn e25_speech_session_controls_and_native_events_keep_one_opening_identity() {
    let collection =
        crate::rx_collection::test_support::test_collection_named("e25_sessions").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    let kind = "ctox.workjet.speech.dictation.v1";
    let state = Arc::new(PlMutex::new(
        json!({"streamId":"a064d23c-d6c5-4caa-a517-b37c93edbc4a","state":"open","events":[],"text":null}),
    ));
    let (updates, _) = tokio::sync::watch::channel(());
    let events = state.clone();
    let observe = updates.clone();
    pool.register_native_session_observer(
        kind,
        Arc::new(move |_, _, id| {
            assert_eq!(id, "a064d23c-d6c5-4caa-a517-b37c93edbc4a");
            let events = events.clone();
            Ok(super::super::v4_native_sessions::NativeSessionFeed::new(
                observe.subscribe(),
                Arc::new(move |after| {
                    let mut value = events.lock().clone();
                    value["events"] = json!(value["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| e["sequence"].as_u64().unwrap() > after)
                        .cloned()
                        .collect::<Vec<_>>());
                    Ok(value)
                }),
            ))
        }),
    )
    .unwrap();
    let native = state.clone();
    let changes = updates.clone();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let invoked = calls.clone();
    pool.register_auxiliary_request_handler(
        kind,
        Arc::new(move |_, _, params| {
            let native = native.clone();
            let changes = changes.clone();
            let invoked = invoked.clone();
            Box::pin(async move {
                let p = &params[0];
                assert!(p.get("session_command_id").is_none());
                assert_eq!(p["scope"], json!({"instanceId":"isolated"}));
                if p["op"] != "open" {
                    assert_eq!(p["streamId"], "a064d23c-d6c5-4caa-a517-b37c93edbc4a");
                }
                if p["op"] == "finish" {
                    native.lock()["state"] = json!("finished");
                    native.lock()["text"] = json!("actual final text");
                    changes.send_replace(());
                }
                invoked.fetch_add(1, Ordering::SeqCst);
                let value = native.lock().clone();
                Ok(value)
            })
        }),
    )
    .unwrap();
    let mut open = command(kind);
    open.command_id = "4a66a0c7-3834-48b6-89f8-6f67f77ec328".into();
    open.payload = json!({"op":"open","scope":{"instanceId":"isolated"}});
    let pending = pool
        .execute_native_command(peer.clone(), open.clone(), authority.clone())
        .await
        .unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE completions(command_id TEXT,status TEXT,body TEXT)")
        .unwrap();
    fn persist(tx: &rusqlite::Transaction<'_>, v: routes::Completion) -> RxResult<()> {
        tx.execute(
            "INSERT INTO completions VALUES(?1,?2,?3)",
            rusqlite::params![
                v.command_id,
                v.status,
                serde_json::to_string(&v.result).unwrap()
            ],
        )
        .map_err(|_| new_rx_error("fixture persistence", None))?;
        Ok(())
    }
    {
        let tx = db.transaction().unwrap();
        pending.publish(&tx, None, &mut persist).unwrap();
        tx.commit().unwrap();
    }
    let initial: String = db
        .query_row("SELECT body FROM completions LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&initial).unwrap()["partial"]["streamId"],
        "a064d23c-d6c5-4caa-a517-b37c93edbc4a"
    );
    let mut stream = pool.watch_native_session(&peer, &open).unwrap();
    state.lock()["events"] = json!([{"sequence":1,"text":"partial"}]);
    updates.send_replace(());
    let event = stream.next().await.unwrap().unwrap();
    {
        let tx = db.transaction().unwrap();
        event.publish(&tx, None, &mut persist).unwrap();
        tx.commit().unwrap();
    }
    let mut read = command(kind);
    read.command_id = "read-control".into();
    read.payload = json!({"op":"read","session_command_id":open.command_id});
    let before = calls.load(Ordering::SeqCst);
    let read_result = pool
        .execute_native_command(peer.clone(), read.clone(), authority.clone())
        .await
        .unwrap();
    {
        let tx = db.transaction().unwrap();
        read_result.publish(&tx, None, &mut persist).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "read must use the latest pushed native partial, not poll the backend"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM completions WHERE command_id=?1",
            [&open.command_id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2,
        "read controls must not rewrite the producer-owned running opening command"
    );
    let swapped = MockPeer("browser".into(), 2);
    pool.authenticated_peers.lock().insert(swapped.clone());
    handler
        .capabilities
        .lock()
        .insert(swapped.clone(), "admitted-token".into());
    assert_eq!(
        pool.execute_native_command(swapped, read.clone(), authority.clone())
            .await
            .err()
            .unwrap()
            .reason,
        "SPEECH_STREAM_CLOSED"
    );
    let mut finish = read.clone();
    finish.command_id = "finish-control".into();
    finish.payload["op"] = json!("finish");
    let final_result = pool
        .execute_native_command(peer, finish, authority)
        .await
        .unwrap();
    {
        let tx = db.transaction().unwrap();
        final_result.publish(&tx, None, &mut persist).unwrap();
        tx.commit().unwrap();
    }
    // A terminal producer notification after an already-delivered partial must
    // not revalidate old full-snapshot events against the incremental cursor.
    let terminal_event = stream.next().await.unwrap().unwrap();
    assert_eq!(
        terminal_event.result.as_ref().unwrap()["text"],
        "actual final text"
    );
    assert!(stream.next().await.unwrap().is_none());
    let final_body: String = db
        .query_row(
            "SELECT body FROM completions WHERE command_id='finish-control'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&final_body).unwrap()["text"],
        "actual final text"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM completions WHERE command_id='finish-control'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "done"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM completions WHERE command_id=?1 ORDER BY rowid DESC LIMIT 1",
            [open.command_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "done"
    );
    pool.cancel().await;
}

#[tokio::test]
async fn e25_command_bus_registry_is_core_only_and_never_trusts_a_payload_actor() {
    let collection =
        crate::rx_collection::test_support::test_collection_named("e25_core_only").await;
    let handler = MockHandler::new();
    let pool = RxWebRTCReplicationPool::new(collection, handler.clone());
    let peer = MockPeer("browser".into(), 1);
    pool.authenticated_peers.lock().insert(peer.clone());
    handler
        .capabilities
        .lock()
        .insert(peer.clone(), "admitted-token".into());
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    for route in routes::ROUTES
        .iter()
        .copied()
        .filter(|r| routes::is_command_bus(*r))
    {
        let method = route.method.to_owned();
        pool.register_native_command_executor(
            route.method,
            Arc::new(move |identity, token, document, _authority| {
                assert_eq!(identity, "browser");
                assert_eq!(token, "admitted-token");
                assert_eq!(document.kind, method);
                assert_eq!(document.actor_id, "alice");
                Box::pin(async move {
                    Ok(json!({"native_actor":document.actor_id,"type":document.kind}))
                })
            }),
        )
        .unwrap();
        assert!(
            !pool
                .auxiliary_request_handlers
                .lock()
                .contains_key(route.method),
            "Core executors must not expose a parallel unguarded requestNative wire route"
        );
        let pending = pool
            .execute_native_command(peer.clone(), command(route.method), authority.clone())
            .await
            .unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        pending
            .publish(&tx, None, &mut |_, value| {
                assert_eq!(value.status, "done");
                assert_eq!(value.result.unwrap()["native_actor"], "alice");
                Ok(())
            })
            .unwrap();
        let mut forged = command(route.method);
        forged.actor_id = "mallory".into();
        assert_eq!(
            pool.execute_native_command(peer.clone(), forged, authority.clone())
                .await
                .err()
                .unwrap()
                .reason,
            "command_authority_changed"
        );
    }
    pool.cancel().await;
}

#[tokio::test]
async fn e25_linked_results_stage_distinct_verified_cas_digests_before_one_commit() {
    use ctox_store::change_log::subscription::write_contract::routes::{
        ResultFiles, VerifiedResultFiles,
    };
    let collection =
        crate::rx_collection::test_support::test_collection_named("e25_batch_files").await;
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
        Arc::new(|_, _, p| {
            Box::pin(async move { Ok(json!({"frame":"x".repeat(32768),"marker":p[0]["marker"]})) })
        }),
    )
    .unwrap();
    let authority = Arc::new(CommandAuthority(Arc::new(AtomicBool::new(true))));
    let mut control = command("ctox.browser.live.v1");
    control.command_id = "batch-control".into();
    control.payload["marker"] = json!(1);
    let mut parent = control.clone();
    parent.command_id = "batch-opening".into();
    parent.payload["marker"] = json!(2);
    let mut pending = pool
        .execute_native_command(peer.clone(), control, authority.clone())
        .await
        .unwrap();
    pending.linked = Some(Box::new(
        pool.execute_native_command(peer, parent, authority)
            .await
            .unwrap(),
    ));
    let root = tempfile::tempdir().unwrap();
    let cas = ctox_store::cas::Cas::open(root.path().join("cas")).unwrap();
    let directory = root.path().to_owned();
    let staged = pending
        .stage_all_files(move |command, bytes| {
            let source = directory.join(format!("{}.json", command.command_id));
            let cas = cas.clone();
            async move {
                std::fs::write(&source, &bytes).unwrap();
                let blob = cas.import(source, "c".repeat(64)).await.unwrap();
                Ok(Arc::new(VerifiedResultFiles {
                    blob,
                    insert: Arc::new(|tx, record| {
                        tx.execute(
                            "INSERT INTO files VALUES(?1,?2)",
                            rusqlite::params![record.blob.r#ref, record.owner_app],
                        )
                        .unwrap();
                        Ok(())
                    }),
                }) as Arc<dyn ResultFiles>)
            }
        })
        .await
        .unwrap()
        .unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE files(ref TEXT,app TEXT); CREATE TABLE completions(id TEXT,ref TEXT)",
    )
    .unwrap();
    let tx = db.transaction().unwrap();
    pending
        .publish(&tx, Some(staged.as_ref()), &mut |tx, value| {
            let reference = value.result.unwrap()["ref"].as_str().unwrap().to_owned();
            tx.execute(
                "INSERT INTO completions VALUES(?1,?2)",
                rusqlite::params![value.command_id, reference],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(DISTINCT ref) FROM completions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM files WHERE app='sender-app'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    pool.cancel().await;
}
