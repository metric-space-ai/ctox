use super::*;
use rusqlite::Connection;
use serde_json::json;
#[path = "change_log.rs"]
mod change_log;

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
                rules: vec![]
            })
        )
        .is_err());
    router
        .finish_rights_update(
            update,
            Ok(RightsSnapshot {
                rights_ver: 3,
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
