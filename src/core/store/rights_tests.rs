use super::*;
use rusqlite::Connection;
use serde_json::json;
use ctox_store::change_log;

fn rule() -> Right {
    Right {
        role: "user".into(),
        collection: "leads".into(),
        row_filter: scope::RowFilter::Eq {
            path: "/owner".into(),
            value: scope::FilterValue::ActorId,
        },
        field_mask: scope::FieldMask::Fields {
            paths: ["/name".into()].into(),
        },
        ops: [Operation::Read, Operation::Write].into(),
    }
}
#[test]
fn rights_share_p3_seq_and_pages_and_rollback_with_their_transaction() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    change_log::initialize(&tx).unwrap();
    initialize(&tx).unwrap();
    assert_eq!(replace(&tx, 1, &[rule()]).unwrap(), 2);
    let snapshot = read(&tx).unwrap();
    assert_eq!(snapshot.rights_ver, 2);
    assert_eq!(snapshot.rules.len(), 1);
    let page = change_log::read_page(&tx, "_rights", 1, 0, 100).unwrap();
    assert_eq!(page.docs.len(), 2);
    assert!(page.docs.iter().all(|d| d.body.is_some()));
    assert_eq!(page.head_seq, 2);
    let seq = page.head_seq;
    assert!(replace(&tx, 1, &[]).is_err());
    assert_eq!(change_log::head(&tx).unwrap().head_seq, seq);
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    // A pre-P3 prototype used a physical _rights table. Migration keeps the
    // same rows, global seq and policy version while adopting P3's source name.
    tx.execute_batch(
        "DROP VIEW _rights; ALTER TABLE c_rights__g1 RENAME TO _rights;
        UPDATE _change_sources SET table_name='_rights' WHERE collection='_rights'",
    )
    .unwrap();
    initialize(&tx).unwrap();
    assert_eq!(read(&tx).unwrap().rights_ver, 2);
    assert_eq!(read(&tx).unwrap().rules, vec![rule()]);
    assert_eq!(replace(&tx, 2, &[]).unwrap(), 3);
    let page = change_log::read_page(&tx, "_rights", 1, seq, 100).unwrap();
    assert!(page
        .docs
        .iter()
        .any(|d| d.op == change_log::Operation::Delete));
    assert!(read(&tx).unwrap().rules.is_empty());
    tx.rollback().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(read(&tx).unwrap().rights_ver, 2);
    assert_eq!(change_log::head(&tx).unwrap().head_seq, seq);
}
#[test]
fn rights_replacement_is_atomic_even_if_caller_catches_failure() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    change_log::initialize(&tx).unwrap();
    initialize(&tx).unwrap();
    replace(&tx, 1, &[rule()]).unwrap();
    let mut duplicate = rule();
    duplicate.row_filter = scope::RowFilter::All;
    assert!(replace(&tx, 2, &[rule(), duplicate]).is_err());
    assert_eq!(read(&tx).unwrap().rights_ver, 2);
    assert_eq!(read(&tx).unwrap().rules, vec![rule()]);
}
#[test]
fn current_rights_gate_outbox_owner_and_hidden_field_mutations() {
    let snapshot = RightsSnapshot {
        rights_ver: 2,
        manifests: vec![],
        rules: vec![rule()],
    };
    let before = json!({"owner":"alice","name":"A","secret":"S"});
    let after = json!({"owner":"alice","name":"B","secret":"S"});
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&after))
        .is_ok());
    for bad in [
        json!({"owner":"alice","name":"B","secret":"leak"}),
        json!({"owner":"bob","name":"B","secret":"S"}),
        json!({"owner":"alice","name":"B"}),
    ] {
        assert!(snapshot
            .authorize_write("user", "alice", "leads", Some(&before), Some(&bad))
            .is_err());
    }
    assert!(snapshot
        .authorize_write("admin", "alice", "leads", Some(&before), Some(&after))
        .is_err());
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), None)
        .is_err());
    assert!(snapshot
        .resolve("user", "alice")
        .unwrap()
        .projection("leads")
        .is_some());
    assert!(snapshot
        .resolve("admin", "alice")
        .unwrap()
        .projections
        .is_empty());
}

#[test]
fn write_cannot_replace_a_masked_parent_object_with_a_scalar() {
    let mut right = rule();
    right.field_mask = scope::FieldMask::Fields {
        paths: ["/profile/name".into()].into(),
    };
    let snapshot = RightsSnapshot {
        rights_ver: 2,
        manifests: vec![],
        rules: vec![right],
    };
    let before = json!({"owner":"alice","profile":{"name":"A"}});
    let bad = json!({"owner":"alice","profile":"forbidden"});
    let good = json!({"owner":"alice","profile":{"name":"B"}});
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&bad))
        .is_err());
    assert!(snapshot
        .authorize_write("user", "alice", "leads", Some(&before), Some(&good))
        .is_ok());
}

struct RouterKeys(Vec<u8>);
impl scope::InstanceSigningKeyStore for RouterKeys {
    fn with_instance_signing_key<T>(
        &self,
        f: impl FnOnce(&[u8]) -> scope::Result<T>,
    ) -> scope::Result<T> {
        f(&self.0)
    }
}
fn router_keys() -> (RouterKeys, Vec<u8>) {
    use ring::signature::KeyPair;
    let doc =
        ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let key = ring::signature::Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
    (
        RouterKeys(doc.as_ref().to_vec()),
        key.public_key().as_ref().to_vec(),
    )
}
#[test]
fn four_participants_two_scopes_route_each_change_and_revoke_old_frames_peers_and_outbox() {
    let (keys, public) = router_keys();
    let mut right = rule();
    right.field_mask = scope::FieldMask::All;
    let snapshot = RightsSnapshot {
        rights_ver: 2,
        manifests: vec![],
        rules: vec![right.clone()],
    };
    let alice = snapshot.resolve("user", "alice").unwrap();
    let bob = snapshot.resolve("user", "bob").unwrap();
    let router = RightsRouter::new("instance".into(), "epoch".into(), snapshot).unwrap();
    let mut tokens = Vec::new();
    for (id, actor) in [
        ("a1", "alice"),
        ("a2", "alice"),
        ("b1", "bob"),
        ("b2", "bob"),
    ] {
        let s = if actor == "alice" { &alice } else { &bob };
        let token = scope::SignedScope::issue(&keys, "instance", id, s, 100).unwrap();
        router
            .subscribe(
                id,
                Subscription {
                    device_id: id.into(),
                    actor_id: actor.into(),
                    role: "user".into(),
                    collection: "leads".into(),
                    generation: 1,
                },
                &token,
                &public,
                100,
            )
            .unwrap();
        tokens.push((id, token));
    }
    assert_eq!(router.active_subscriptions("leads").unwrap().len(), 4);
    let doc = json!({"owner":"alice","name":"A"});
    for (id, token) in &tokens {
        router
            .with_change_frame(
                id,
                token,
                &public,
                101,
                "leads",
                1,
                "lead",
                1,
                Some(&doc),
                |change| {
                    assert_eq!(
                        matches!(change, scope::scope_filter::ProjectedChange::Put { .. }),
                        id.starts_with('a')
                    );
                    Ok(())
                },
            )
            .unwrap();
    }
    let payload = b"masked payload";
    let a1 = scope::PeerLease::issue(&keys, "instance", "a1", &alice, 100).unwrap();
    let a2 = scope::PeerLease::issue(&keys, "instance", "a2", &alice, 100).unwrap();
    let packet = scope::PeerPacket {
        instance_id: "instance".into(),
        scope_hash: alice.hash().unwrap(),
        rights_ver: 2,
        epoch: "epoch".into(),
        collection: "leads".into(),
        gen: 1,
        package_id: "packet".into(),
        kind: "page".into(),
        seq_from: 1,
        seq_to: 1,
        payload_hash: scope::digest(payload),
    };
    let signature = packet.sign(&keys).unwrap();
    router
        .with_peer_frame(
            "a2",
            &public,
            101,
            ("a1", &a1),
            &a2,
            &packet,
            &signature,
            payload,
            || Ok(()),
        )
        .unwrap();
    let next = json!({"owner":"alice","name":"B"});
    assert!(router
        .with_outbox_commit("a1", "op", Some(&doc), Some(&next), || Ok(()))
        .is_ok());
    right.ops = [Operation::Read].into();
    right.field_mask = scope::FieldMask::Fields {
        paths: ["/name".into()].into(),
    };
    let updates = router
        .update_committed(|_| {
            Ok(RightsSnapshot {
                rights_ver: 3,
                manifests: vec![],
                rules: vec![right],
            })
        })
        .unwrap();
    assert_eq!(updates.len(), 4);
    for (id, token) in &tokens {
        assert!(router
            .with_change_frame(
                id,
                token,
                &public,
                102,
                "leads",
                1,
                "lead",
                2,
                Some(&doc),
                |_| -> scope::Result<()> { panic!("stale token must not reach handoff") }
            )
            .is_err());
    }
    assert!(router
        .with_peer_frame(
            "a2",
            &public,
            102,
            ("a1", &a1),
            &a2,
            &packet,
            &signature,
            payload,
            || -> scope::Result<()> { panic!("old rights peer packet must not reach handoff") }
        )
        .is_err());
    assert_eq!(
        router
            .with_outbox_commit("a1", "op", Some(&doc), Some(&next), || Ok(()))
            .unwrap_err(),
        OutboxError::Revoked(scope::scope_filter::RevokedWrite::new("op"))
    );
    let new_alice = updates.iter().find(|s| s.subscription_id == "a1").unwrap();
    let token = scope::SignedScope::issue(&keys, "instance", "a1", &new_alice.scope, 102).unwrap();
    router.with_change_frame("a1",&token,&public,102,"leads",1,"lead",2,Some(&doc),|change|{
        assert!(matches!(change,scope::scope_filter::ProjectedChange::Put{doc,..} if doc==json!({"name":"A"})));
        Ok(())
    }).unwrap();
    // A subsequent owner change must unproject from the same subscription.
    let foreign = json!({"owner":"bob","name":"A"});
    router
        .with_change_frame(
            "a1",
            &token,
            &public,
            103,
            "leads",
            1,
            "lead",
            3,
            Some(&foreign),
            |change| {
                assert!(matches!(
                    change,
                    scope::scope_filter::ProjectedChange::Unproject { .. }
                ));
                Ok(())
            },
        )
        .unwrap();
    router.unsubscribe("a1").unwrap();
    assert!(router
        .with_change_frame(
            "a1",
            &token,
            &public,
            104,
            "leads",
            1,
            "lead",
            4,
            Some(&doc),
            |_| Ok(())
        )
        .is_err());
}

#[test]
fn pending_publication_releases_mutex_and_keeps_outbox_pending_instead_of_falsely_revoked() {
    let (keys, public) = router_keys();
    let mut right = rule();
    right.field_mask = scope::FieldMask::All;
    let make = || RightsSnapshot {
        rights_ver: 2,
        manifests: vec![],
        rules: vec![right.clone()],
    };
    let scope = make().resolve("user", "alice").unwrap();
    let router = RightsRouter::new("i".into(), "e".into(), make()).unwrap();
    let token = scope::SignedScope::issue(&keys, "i", "device", &scope, 100).unwrap();
    router
        .subscribe(
            "s",
            Subscription {
                device_id: "device".into(),
                actor_id: "alice".into(),
                role: "user".into(),
                collection: "leads".into(),
                generation: 1,
            },
            &token,
            &public,
            100,
        )
        .unwrap();
    let doc = json!({"owner":"alice","name":"A"});
    let expected: std::result::Result<(), OutboxError> =
        router.with_outbox_commit("s", "op", Some(&doc), Some(&doc), || {
            Err("writer transaction failed".into())
        });
    assert_eq!(
        expected,
        Err(OutboxError::CommitFailed(
            "writer transaction failed".into()
        ))
    );
    let update = router.begin_rights_update().unwrap();
    assert_eq!(update.expected_version(), 2);
    // Reentrant queue-side check returns immediately: no authority lock is held
    // while the Store handles queued work or the Core awaits the commit result.
    assert_eq!(
        router.with_outbox_commit("s", "op", Some(&doc), Some(&doc), || panic!(
            "unknown authority must not write"
        )),
        Err::<(), _>(OutboxError::AuthorityUnavailable)
    );
    assert!(router
        .with_change_frame(
            "s",
            &token,
            &public,
            101,
            "leads",
            1,
            "a",
            1,
            Some(&doc),
            |_| Ok(())
        )
        .is_err());
    let foreign = RightsRouter::new("i".into(), "e".into(), make()).unwrap();
    let foreign_update = foreign.begin_rights_update().unwrap();
    assert!(router
        .finish_rights_update(
            foreign_update,
            Ok(RightsSnapshot {
                rights_ver: 3,
                manifests: vec![],
                rules: vec![]
            })
        )
        .is_err());
    router
        .finish_rights_update(
            update,
            Ok(RightsSnapshot {
                rights_ver: 3,
                manifests: vec![],
                rules: vec![right],
            }),
        )
        .unwrap();
    assert!(router
        .with_change_frame(
            "s",
            &token,
            &public,
            102,
            "leads",
            1,
            "a",
            1,
            Some(&doc),
            |_| Ok(())
        )
        .is_err());
}

#[test]
fn router_rights_change_under_load_stops_old_handoffs_before_commit() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    };
    let (keys, public) = router_keys();
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    change_log::initialize(&tx).unwrap();
    initialize(&tx).unwrap();
    replace(&tx, 1, &[rule()]).unwrap();
    let snapshot = read(&tx).unwrap();
    tx.commit().unwrap();
    let resolved = snapshot.resolve("user", "alice").unwrap();
    let router = Arc::new(RightsRouter::new("instance".into(), "epoch".into(), snapshot).unwrap());
    let warm = Arc::new(Barrier::new(5));
    let committed = Arc::new(Barrier::new(5));
    let count = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for n in 0..4 {
        let id = format!("device-{n}");
        let token = scope::SignedScope::issue(&keys, "instance", &id, &resolved, 100).unwrap();
        router
            .subscribe(
                &id,
                Subscription {
                    device_id: id.clone(),
                    actor_id: "alice".into(),
                    role: "user".into(),
                    collection: "leads".into(),
                    generation: 1,
                },
                &token,
                &public,
                100,
            )
            .unwrap();
        let (router, warm, committed, count, public) = (
            router.clone(),
            warm.clone(),
            committed.clone(),
            count.clone(),
            public.clone(),
        );
        workers.push(std::thread::spawn(move || {
            let doc = json!({"owner":"alice","name":"A"});
            let send = || {
                router.with_change_frame(
                    &id,
                    &token,
                    &public,
                    100,
                    "leads",
                    1,
                    "lead",
                    1,
                    Some(&doc),
                    |_| {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                )
            };
            send().unwrap();
            warm.wait();
            for _ in 0..1000 {
                let _ = send();
            }
            committed.wait();
            for _ in 0..100 {
                assert!(send().is_err());
            }
        }));
    }
    warm.wait();
    let pending = router.begin_rights_update().unwrap();
    let count_at_suspension = count.load(Ordering::SeqCst);
    assert!(count_at_suspension >= 4);
    let tx = db.transaction().unwrap();
    replace(&tx, pending.expected_version(), &[]).unwrap();
    let committed_snapshot = read(&tx).unwrap();
    assert_eq!(change_log::head(&tx).unwrap().head_seq, 4);
    tx.commit().unwrap();
    router
        .finish_rights_update(pending, Ok(committed_snapshot))
        .unwrap();
    committed.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(count.load(Ordering::SeqCst), count_at_suspension);
}

#[derive(Default)]
struct NativeFixture {
    frames: std::sync::Mutex<Vec<(String, rxdb::plugins::replication_webrtc::WebRTCWireFrame)>>,
    pause: std::sync::atomic::AtomicBool,
    queued: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl rxdb::plugins::replication_webrtc::WebRTCConnectionHandler for NativeFixture {
    type Peer = String;
    fn connect_stream(&self) -> rxdb::rxjs_compat::RxStream<String> {
        panic!("unused fixture stream")
    }
    fn disconnect_stream(&self) -> rxdb::rxjs_compat::RxStream<String> {
        panic!("unused fixture stream")
    }
    fn message_stream(
        &self,
    ) -> rxdb::rxjs_compat::RxStream<rxdb::plugins::replication_webrtc::PeerWithMessage<String>>
    {
        panic!("unused fixture stream")
    }
    fn response_stream(
        &self,
    ) -> rxdb::rxjs_compat::RxStream<rxdb::plugins::replication_webrtc::PeerWithResponse<String>>
    {
        panic!("unused fixture stream")
    }
    fn error_stream(&self) -> rxdb::rxjs_compat::RxStream<rxdb::rx_error::RxError> {
        panic!("unused fixture stream")
    }
    async fn send(
        &self,
        _: &String,
        _: rxdb::plugins::replication_webrtc::WebRTCWireFrame,
    ) -> rxdb::rx_error::RxResult<()> {
        panic!("unguarded publication is forbidden")
    }
    async fn send_guarded(
        &self,
        peer: &String,
        frame: rxdb::plugins::replication_webrtc::WebRTCWireFrame,
        guard: std::sync::Arc<dyn rxdb::plugins::replication_webrtc::WebRTCPublicationGuard>,
    ) -> rxdb::rx_error::RxResult<()> {
        if self.pause.load(std::sync::atomic::Ordering::SeqCst) {
            self.queued.notify_one();
            self.resume.notified().await;
        }
        let mut frame = Some(frame);
        guard.with_current(&mut || {
            self.frames
                .lock()
                .unwrap()
                .push((peer.clone(), frame.take().unwrap()));
            Ok(())
        })
    }
    async fn close(&self) -> rxdb::rx_error::RxResult<()> {
        Ok(())
    }
    fn document_fields_for_peer(&self, _: &String, _: &str) -> Option<Vec<String>> {
        None
    }
}
fn manifest_fixture() -> ManifestRules {
    ManifestRules::from_manifest(
        "app",
        "1",
        ["leads".into()].into(),
        &json!({
            "row_filters":{"leads":{"op":"eq","path":"/owner","value":{"kind":"actor_id"}}},
            "write_guard":{"leads":{"command_only_fields":["review"]}},
            "command_rights":{"review.set":{"roles":["user"]}}
        }),
    )
    .unwrap()
}
async fn authority_fixture() -> (tempfile::TempDir, std::sync::Arc<StoreRightsAuthority>) {
    use ctox_store::{registry::Registry, writer::WriterOptions, DbId};
    let dir = tempfile::tempdir().unwrap();
    let writer = Registry
        .open(
            DbId::new(dir.path().join("store.sqlite")).unwrap(),
            WriterOptions {
                configure: Some(std::sync::Arc::new(|db| {
                    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                    Ok(())
                })),
                head_seq: Some(std::sync::Arc::new(|db| {
                    db.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                        .map_err(Into::into)
                })),
            },
        )
        .unwrap();
    let authority = StoreRightsAuthority::attach(writer, "instance".into())
        .await
        .unwrap();
    let mut right = rule();
    right.row_filter = scope::RowFilter::All;
    right.field_mask = scope::FieldMask::Fields {
        paths: ["/owner".into(), "/name".into(), "/review".into()].into(),
    };
    authority
        .replace(vec![right], vec![manifest_fixture()])
        .await
        .unwrap();
    (dir, authority)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manifest_policies_commit_with_rights_seq_and_gate_outbox_before_handlers() {
    let (_dir, authority) = authority_fixture().await;
    let second = StoreRightsAuthority::attach(authority.writer().clone(), "instance".into())
        .await
        .unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&authority, &second),
        "one authority per Store incarnation"
    );
    assert!(
        StoreRightsAuthority::attach(authority.writer().clone(), "other-instance".into())
            .await
            .is_err()
    );
    let (head, rights) = authority.hello_snapshot().await.unwrap();
    assert_eq!(rights.rights_ver, 2);
    assert!(head.head_seq >= 2);
    assert_eq!(rights.manifests.len(), 1);
    assert!(rights
        .resolve_collection("user", "alice", "leads")
        .unwrap()
        .projection("leads")
        .unwrap()
        .row_filter
        .matches(&json!({"owner":"alice"})));
    assert!(!rights
        .resolve_collection("user", "bob", "leads")
        .unwrap()
        .projection("leads")
        .unwrap()
        .row_filter
        .matches(&json!({"owner":"alice"})));
    let before = json!({"owner":"alice","name":"A","review":"pending"});
    let after = json!({"owner":"alice","name":"A","review":"done"});
    assert_eq!(
        rights.authorize_write("user", "alice", "leads", Some(&before), Some(&after)),
        Err("command_only_field".into())
    );
    let mut started = false;
    let permit = authority
        .router
        .with_command_dispatch("app", "alice", "user", "review.set", false, |permit| {
            started = true;
            Ok(permit)
        })
        .unwrap();
    assert!(started);
    assert!(rights
        .authorize_command_write(
            "user",
            "alice",
            "leads",
            Some(&before),
            Some(&after),
            &permit
        )
        .is_ok());
    let mut denied = false;
    assert!(authority
        .router
        .with_command_dispatch("app", "alice", "other", "review.set", false, |_| {
            denied = true;
            Ok(())
        })
        .is_err());
    assert!(!denied);
    let mut commits = authority.subscribe_commits();
    let old_head = commits.latest().unwrap().head_seq;
    authority
        .replace(vec![], vec![manifest_fixture()])
        .await
        .unwrap();
    assert!(commits.latest().unwrap().head_seq > old_head);
    let (_, now) = authority.hello_snapshot().await.unwrap();
    assert!(now
        .authorize_command_write(
            "user",
            "alice",
            "leads",
            Some(&before),
            Some(&after),
            &permit
        )
        .is_err());
    // Restart reconstructs exactly the committed manifest rules; no ambient cache.
    authority.writer().shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_four_participants_real_writer_hello_native_queue_and_stale_peer_fence() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rxdb::plugins::replication_webrtc::{
        index_mod::{
            connection_lifecycle::{ConnectionState, PeerLifecycleRegistry},
            hello::{ApiRange, Codec, DeviceProof, Hello, HelloFrame, HelloPolicy, TransportOffer},
        },
        WebRTCPublicationGuard, WebRTCWireFrame,
    };
    use scope::scope_filter::{
        send_native_change, NativeChange, NativePeerPublication, NativePublication,
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    let (_dir, authority) = authority_fixture().await;
    let (core_keys, core_public) = router_keys();
    let core_keys = Arc::new(core_keys);
    let mut devices = Vec::new();
    let mut directory = std::collections::BTreeMap::new();
    for (device, actor) in [
        ("a1", "alice"),
        ("a2", "alice"),
        ("b1", "bob"),
        ("b2", "bob"),
    ] {
        let (key, public) = router_keys();
        directory.insert(device.to_string(), (actor.to_string(), public.clone()));
        devices.push((device.to_string(), key, public));
    }
    let directory = Arc::new(directory);
    let verify: crate::business_os::policy::VerifySyncPeer = Arc::new(move |peer, hello| {
        let directory = directory.clone();
        Box::pin(async move {
            let (actor, public) = directory.get(&peer).ok_or("device not paired")?;
            let supplied = URL_SAFE_NO_PAD
                .decode(&hello.device_proof.public_key)
                .map_err(|_| "bad proof")?;
            let signature = URL_SAFE_NO_PAD
                .decode(&hello.device_proof.signature)
                .map_err(|_| "bad proof")?;
            if supplied != *public || hello.device_proof.device_id != peer {
                return Err("wrong device".into());
            }
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
                .verify(hello.device_proof.nonce.as_bytes(), &signature)
                .map_err(|_| "bad device signature")?;
            Ok(crate::business_os::policy::VerifiedSyncPeer {
                actor: crate::business_os::policy::BusinessOsActor::new(
                    Some(actor.clone()),
                    "user",
                ),
                instance_id: "instance".into(),
                device_id: peer,
            })
        })
    });
    let clock: scope::scope_filter::CoreClock = Arc::new(|| 100);
    let authenticate = crate::business_os::policy::sync_peer_authenticator(
        authority.clone(),
        core_keys.clone(),
        verify,
        clock.clone(),
    );
    let transport_offer = TransportOffer {
        codecs: vec![Codec::Zstd],
        max_frame_bytes: 16 * 1024,
        fragmentation: true,
        reassembly: true,
    };
    let api = ApiRange {
        min: "4.0".into(),
        max: "4.0".into(),
    };
    let policy = HelloPolicy {
        instance_id: "instance".into(),
        bootstrap_min: 1,
        bootstrap_max: 1,
        app_api_ver: api.clone(),
        capabilities: vec![],
        required_capabilities: vec![],
        transport: transport_offer.clone(),
    };
    let registry = PeerLifecycleRegistry::default();
    let mut peers = Vec::new();
    let mut publishers = Vec::new();
    let mut scopes = Vec::new();
    for (n, (device, key, public)) in devices.into_iter().enumerate() {
        let nonce = URL_SAFE_NO_PAD.encode([n as u8 + 1; 32]);
        let signature = ring::signature::Ed25519KeyPair::from_pkcs8(&key.0)
            .unwrap()
            .sign(nonce.as_bytes());
        let hello = Hello {
            protocol_ver: 4,
            bootstrap_ver: 1,
            app_api_ver: api.clone(),
            instance_id: "instance".into(),
            capabilities: vec![],
            device_proof: DeviceProof {
                device_id: device.clone(),
                public_key: URL_SAFE_NO_PAD.encode(public),
                nonce: nonce.clone(),
                signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
            },
            scopes_wanted: vec!["leads".into()],
            transport: transport_offer.clone(),
        };
        let lifecycle = registry
            .start(
                device.clone(),
                nonce,
                policy.clone(),
                authenticate.clone(),
                std::time::Duration::from_secs(10),
            )
            .unwrap();
        lifecycle.transport_connected().await.unwrap();
        let frame = lifecycle.receive_hello(hello).await.unwrap();
        let HelloFrame::HelloOk(ok) = frame else {
            panic!("paired device rejected");
        };
        assert_eq!(ok.rights_ver.0, 2);
        let token = scope::SignedScope::from_wire(&ok.scope_tokens[0]).unwrap();
        let (_, snapshot) = authority.hello_snapshot().await.unwrap();
        let actor = if device.starts_with('a') {
            "alice"
        } else {
            "bob"
        };
        let scope = snapshot.resolve_collection("user", actor, "leads").unwrap();
        authority
            .router
            .subscribe(
                &device,
                Subscription {
                    device_id: device.clone(),
                    actor_id: actor.into(),
                    role: "user".into(),
                    collection: "leads".into(),
                    generation: 1,
                },
                &token,
                &core_public,
                100,
            )
            .unwrap();
        lifecycle.hello_ok_sent().await.unwrap();
        assert_eq!(lifecycle.snapshot().state, ConnectionState::Live);
        scopes.push(scope);
        publishers.push(Arc::new(NativePublication {
            authority: authority.clone(),
            subscription_id: device,
            token,
            epoch: ok.epoch.clone(),
            public_key: core_public.clone(),
            collection: "leads".into(),
            generation: 1,
            clock: clock.clone(),
        }));
        peers.push(lifecycle);
    }
    assert_eq!(scopes[0].hash().unwrap(), scopes[1].hash().unwrap());
    assert_ne!(scopes[0].hash().unwrap(), scopes[2].hash().unwrap());
    // Real writer + trigger + compacted change-log source for outgoing changes.
    let (epoch,seq,document)=authority.writer().submit(ctox_store::WriteJob::new(Default::default(),|tx|{
        tx.execute_batch("CREATE TABLE c_leads__g1(id TEXT PRIMARY KEY,rev TEXT NOT NULL,schema_ver INTEGER NOT NULL,data TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0)")?;
        ctox_store::change_log::install_triggers(tx,&ctox_store::change_log::TableBinding::generic("leads",1,"c_leads__g1",1,"1"))?;
        tx.execute("INSERT INTO c_leads__g1 VALUES('lead','r1',1,?1,0)",[json!({"owner":"alice","name":"A","review":"pending","secret":"hidden"}).to_string()])?;
        let page=ctox_store::change_log::read_page(tx,"leads",1,0,100)?;
        let doc=&page.docs[0];
        Ok((page.epoch,doc.seq,doc.body.clone().unwrap()))
    })).unwrap().await.unwrap();
    let transport = Arc::new(NativeFixture::default());
    for p in &publishers {
        send_native_change(
            &*transport,
            &p.subscription_id,
            p.clone(),
            NativeChange {
                collection: "leads",
                generation: 1,
                epoch: &epoch,
                id: "lead",
                seq,
                document: Some(&document),
                core_sent_at_ms: 100000,
            },
        )
        .await
        .unwrap();
    }
    let frames = transport.frames.lock().unwrap().clone();
    assert_eq!(frames.len(), 4);
    for (device, frame) in frames {
        let WebRTCWireFrame::Response(frame) = frame else {
            panic!("expected response")
        };
        let c = &frame.result["changes"][0];
        assert_eq!(
            c["op"],
            if device.starts_with('a') {
                "put"
            } else {
                "unproject"
            }
        );
        assert!(c["doc"].get("secret").is_none());
    }
    // A committed owner change comes from the same change-log read path.
    let (move_seq, moved) = authority
        .writer()
        .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
            tx.execute(
                "UPDATE c_leads__g1 SET rev='r2',data=?1 WHERE id='lead'",
                [json!({"owner":"bob","name":"B","review":"pending"}).to_string()],
            )?;
            let page = ctox_store::change_log::read_page(tx, "leads", 1, seq, 100)?;
            Ok((page.docs[0].seq, page.docs[0].body.clone().unwrap()))
        }))
        .unwrap()
        .await
        .unwrap();
    send_native_change(
        &*transport,
        &"a1".into(),
        publishers[0].clone(),
        NativeChange {
            collection: "leads",
            generation: 1,
            epoch: &epoch,
            id: "lead",
            seq: move_seq,
            document: Some(&moved),
            core_sent_at_ms: 100001,
        },
    )
    .await
    .unwrap();
    let frames = transport.frames.lock().unwrap();
    let WebRTCWireFrame::Response(last) = &frames.last().unwrap().1 else {
        panic!("expected response")
    };
    assert_eq!(last.result["changes"][0]["op"], "unproject");
    drop(frames);
    let payload = b"scoped page".to_vec();
    let packet = scope::PeerPacket {
        instance_id: "instance".into(),
        scope_hash: scopes[0].hash().unwrap(),
        rights_ver: 2,
        epoch: epoch.clone(),
        collection: "leads".into(),
        gen: 1,
        package_id: "p".into(),
        kind: "page".into(),
        seq_from: seq,
        seq_to: seq,
        payload_hash: scope::digest(&payload),
    };
    let peer_guard = NativePeerPublication {
        authority: authority.clone(),
        subscription_id: "a2".into(),
        public_key: core_public.clone(),
        sender_device: "a1".into(),
        sender_lease: scope::PeerLease::issue(&*core_keys, "instance", "a1", &scopes[0], 100)
            .unwrap(),
        receiver_lease: scope::PeerLease::issue(&*core_keys, "instance", "a2", &scopes[0], 100)
            .unwrap(),
        signature: packet.sign(&*core_keys).unwrap(),
        packet,
        payload,
        clock: clock.clone(),
    };
    peer_guard.with_current(&mut || Ok(())).unwrap();
    // Retain one already-prepared frame behind backpressure across revocation.
    let blocked = Arc::new(NativeFixture::default());
    blocked.pause.store(true, Ordering::SeqCst);
    let (q, p, e, d) = (
        blocked.clone(),
        publishers[0].clone(),
        epoch.clone(),
        document.clone(),
    );
    let queued = tokio::spawn(async move {
        send_native_change(
            &*q,
            &"a1".into(),
            p,
            NativeChange {
                collection: "leads",
                generation: 1,
                epoch: &e,
                id: "lead",
                seq,
                document: Some(&d),
                core_sent_at_ms: 100002,
            },
        )
        .await
    });
    blocked.queued.notified().await;
    let running = Arc::new(AtomicBool::new(true));
    let delivered = Arc::new(AtomicUsize::new(0));
    let warm = Arc::new(std::sync::Barrier::new(5));
    let mut producers = Vec::new();
    for p in &publishers {
        let (p, running, delivered, warm) =
            (p.clone(), running.clone(), delivered.clone(), warm.clone());
        producers.push(std::thread::spawn(move || {
            p.with_current(&mut || {
                delivered.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
            warm.wait();
            for _ in 0..1000 {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let _ = p.with_current(&mut || {
                    delivered.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                });
            }
        }));
    }
    warm.wait();
    assert!(delivered.load(Ordering::SeqCst) >= 4);
    let updates = authority
        .replace(vec![], vec![manifest_fixture()])
        .await
        .unwrap();
    let at_commit = delivered.load(Ordering::SeqCst);
    running.store(false, Ordering::SeqCst);
    for task in producers {
        task.join().unwrap();
    }
    assert_eq!(delivered.load(Ordering::SeqCst), at_commit);
    blocked.resume.notify_one();
    assert!(queued.await.unwrap().is_err());
    assert!(blocked.frames.lock().unwrap().is_empty());
    assert!(peer_guard
        .with_current(&mut || panic!("stale peer packet delivered"))
        .is_err());
    let mut held = std::collections::BTreeMap::new();
    held.insert(("leads".into(), "lead".into()), document.clone());
    let unrelated = json!({"label":"keep other app"});
    held.insert(("other".into(), "other-row".into()), unrelated.clone());
    let revoke = scope::scope_filter::ScopeRevoke::for_collection(
        &updates[0].scope,
        "leads",
        &held,
        &[
            scope::scope_filter::ArtifactExposure {
                hash: "artifact".into(),
                collection: "leads".into(),
                row_id: "lead".into(),
                field_path: "/name".into(),
            },
            scope::scope_filter::ArtifactExposure {
                hash: "other-artifact".into(),
                collection: "other".into(),
                row_id: "other-row".into(),
                field_path: "/label".into(),
            },
        ],
    );
    assert_eq!(revoke.clean_document("leads", &document).unwrap(), None);
    assert_eq!(
        revoke.clean_document("other", &unrelated).unwrap(),
        Some(unrelated)
    );
    assert_eq!(revoke.artifacts, vec!["artifact"]);
    let renewed = Arc::new(NativePublication {
        authority: authority.clone(),
        subscription_id: "a1".into(),
        token: scope::SignedScope::issue(&*core_keys, "instance", "a1", &updates[0].scope, 100)
            .unwrap(),
        epoch: epoch.clone(),
        public_key: core_public.clone(),
        collection: "leads".into(),
        generation: 1,
        clock: clock.clone(),
    });
    let mut wrong_domain = revoke.clone();
    wrong_domain.collections = ["other".into()].into();
    assert!(scope::scope_filter::send_native_revoke(
        &*transport,
        &"a1".into(),
        renewed.clone(),
        &wrong_domain
    )
    .await
    .is_err());
    let mut forged = revoke.clone();
    forged.remaining = scopes[0].projections.clone();
    assert!(scope::scope_filter::send_native_revoke(
        &*transport,
        &"a1".into(),
        renewed.clone(),
        &forged
    )
    .await
    .is_err());
    scope::scope_filter::send_native_revoke(&*transport, &"a1".into(), renewed, &revoke)
        .await
        .unwrap();
    assert_eq!(
        authority.router.with_outbox_commit(
            "a1",
            "offline-op",
            Some(&document),
            Some(&document),
            || Ok(())
        ),
        Err(OutboxError::Revoked(
            scope::scope_filter::RevokedWrite::new("offline-op")
        ))
    );
    for p in peers {
        p.drain().await;
    }
    assert_eq!(registry.peer_count(), 0);
    authority.writer().shutdown().await.unwrap();
}
