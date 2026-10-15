use super::*;
use ring::signature::KeyPair;
use scope_filter::{project, ArtifactExposure, FrameAuthority, ProjectedChange, ScopeRevoke};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
};

fn owner_filter() -> RowFilter {
    RowFilter::Eq {
        path: "/owner".into(),
        value: FilterValue::ActorId,
    }
}
fn projection(filter: RowFilter, mask: FieldMask) -> Projection {
    Projection {
        collection: "leads".into(),
        row_filter: filter,
        field_mask: mask,
    }
}
fn scope(ver: u64) -> ResolvedScope {
    ResolvedScope::new(
        ver,
        vec![projection(
            owner_filter(),
            FieldMask::Fields {
                paths: ["/name".into(), "/owner".into()].into(),
            },
        )],
        "alice",
    )
    .unwrap()
}

#[test]
fn canonical_scope_resolves_actor_sorts_fields_and_normalizes_filter() {
    let a = RowFilter::And {
        filters: vec![RowFilter::All, owner_filter(), owner_filter()],
    };
    let b = owner_filter();
    let a = ResolvedScope::new(7, vec![projection(a, FieldMask::All)], "alice").unwrap();
    let b = ResolvedScope::new(7, vec![projection(b.clone(), FieldMask::All)], "alice").unwrap();
    assert_eq!(a.hash().unwrap(), b.hash().unwrap());
    assert_ne!(
        a.hash().unwrap(),
        ResolvedScope::new(7, vec![projection(owner_filter(), FieldMask::All)], "bob")
            .unwrap()
            .hash()
            .unwrap()
    );
    assert_ne!(
        a.hash().unwrap(),
        ResolvedScope::new(8, b.projections.clone(), "alice")
            .unwrap()
            .hash()
            .unwrap()
    );
    let x: Value = serde_json::from_str(r#"{"b":{"z":0,"a":1},"a":2}"#).unwrap();
    let y: Value = serde_json::from_str(r#"{"a":2,"b":{"a":1,"z":0}}"#).unwrap();
    assert_eq!(canonical_json(&x).unwrap(), canonical_json(&y).unwrap());
    assert!(ResolvedScope::new(1, vec![projection(owner_filter(), FieldMask::All)], "").is_err());
    let mask = FieldMask::Fields {
        paths: ["/profile".into(), "/profile/name".into()].into(),
    };
    assert_eq!(
        mask.normalize().unwrap(),
        FieldMask::Fields {
            paths: ["/profile".into()].into()
        }
    );
}

#[test]
fn filter_departure_and_delete_emit_unproject_without_tracking_prior_visibility() {
    let s = scope(2);
    let row = json!({"id":"a","owner":"alice","name":"A","secret":"hidden"});
    assert_eq!(
        project(s.projection("leads"), "leads", "a", 1, Some(&row)).unwrap(),
        ProjectedChange::Put {
            collection: "leads".into(),
            id: "a".into(),
            seq: 1,
            doc: json!({"owner":"alice","name":"A"})
        }
    );
    for doc in [Some(json!({"owner":"bob","name":"A"})), None] {
        assert!(matches!(
            project(s.projection("leads"), "leads", "a", 2, doc.as_ref()).unwrap(),
            ProjectedChange::Unproject { seq: 2, .. }
        ));
    }
    assert!(matches!(
        project(None, "unknown", "a", 3, Some(&row)).unwrap(),
        ProjectedChange::Unproject { .. }
    ));
}

#[test]
fn nested_masks_fail_closed_and_missing_is_not_null() {
    let mask = FieldMask::Fields {
        paths: ["/profile/name".into(), "/a~1b".into()].into(),
    };
    assert_eq!(
        mask.apply(&json!({"profile":{"name":"A","secret":"S"},"a/b":true,"token":"X"}))
            .unwrap(),
        json!({"profile":{"name":"A"},"a/b":true})
    );
    assert!(mask
        .apply(&json!({"profile":[{"name":"A","secret":"S"}]}))
        .is_err());
    assert!(!RowFilter::Eq {
        path: "/absent".into(),
        value: FilterValue::Literal { value: Value::Null }
    }
    .matches(&json!({})));
    assert!(FieldMask::Fields {
        paths: ["/bad~2path".into()].into()
    }
    .normalize()
    .is_err());
}

struct TestStore(Vec<u8>);
impl InstanceSigningKeyStore for TestStore {
    fn with_instance_signing_key<T>(&self, f: impl FnOnce(&[u8]) -> Result<T>) -> Result<T> {
        f(&self.0)
    }
}
fn signer() -> (TestStore, Vec<u8>) {
    let doc = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let key = Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap();
    (
        TestStore(doc.as_ref().to_vec()),
        key.public_key().as_ref().to_vec(),
    )
}

#[test]
fn tokens_and_both_peer_leases_bind_identity_version_signature_and_expiry() {
    let (store, public) = signer();
    let s = scope(2);
    let token = SignedScope::issue(&store, "instance", "a", &s, 1000).unwrap();
    token.verify(&public, "instance", "a", &s, 1001).unwrap();
    assert!(token.verify(&public, "instance", "b", &s, 1001).is_err());
    assert!(token.verify(&public, "other", "a", &s, 1001).is_err());
    assert!(token.verify(&public, "instance", "a", &s, 1300).is_err());
    assert!(token
        .verify(&public, "instance", "a", &scope(3), 1001)
        .is_err());
    let mut forged = token.clone();
    forged.claims.device_id = "b".into();
    assert!(forged.verify(&public, "instance", "b", &s, 1001).is_err());
    let a = PeerLease::issue(&store, "instance", "a", &s, 1000).unwrap();
    let b = PeerLease::issue(&store, "instance", "b", &s, 1000).unwrap();
    assert!(!a.renewal_due(1119));
    assert!(a.renewal_due(1120));
    // Domain separation: ordinary scope signature cannot become peer authority.
    let spoof = PeerLease {
        claims: token.claims,
        signature: token.signature,
    };
    assert!(spoof.verify(&public, "instance", "a", &s, 1001).is_err());
    let payload = b"filtered page";
    let mut packet = PeerPacket {
        instance_id: "instance".into(),
        scope_hash: s.hash().unwrap(),
        rights_ver: 2,
        epoch: "epoch".into(),
        collection: "leads".into(),
        gen: 1,
        package_id: "p".into(),
        kind: "page".into(),
        seq_from: 1,
        seq_to: 8,
        payload_hash: digest(payload),
    };
    let sig = packet.sign(&store).unwrap();
    let authorize = |packet: &PeerPacket, scope: &ResolvedScope, now| {
        packet.authorize(
            &sig,
            payload,
            &public,
            ("a", &a),
            ("b", &b),
            "instance",
            "epoch",
            1,
            scope,
            now,
        )
    };
    authorize(&packet, &s, 1001).unwrap();
    assert!(authorize(&packet, &scope(3), 1001).is_err());
    assert!(authorize(&packet, &s, 1300).is_err());
    packet.seq_to += 1;
    assert!(authorize(&packet, &s, 1001).is_err());
}

#[test]
fn checkpoint_carry_requires_equal_collection_projection_not_equal_roles_or_versions() {
    let old = scope(1);
    let equal = scope(2);
    assert_eq!(old.carry_checkpoint(&equal, "leads", 80), 80);
    let wide =
        ResolvedScope::new(2, vec![projection(RowFilter::All, FieldMask::All)], "alice").unwrap();
    assert_eq!(old.carry_checkpoint(&wide, "leads", 80), 0);
    assert_eq!(old.carry_checkpoint(&equal, "absent", 80), 0);
}

#[test]
fn revocation_cleans_rows_fields_artifacts_and_preserves_rejected_outbox_receipt() {
    let current = scope(2);
    let rows = BTreeMap::from([
        (
            ("leads".into(), "a".into()),
            json!({"owner":"alice","name":"A","secret":"S"}),
        ),
        (
            ("leads".into(), "b".into()),
            json!({"owner":"bob","name":"B"}),
        ),
    ]);
    let artifacts = vec![
        ArtifactExposure {
            hash: "public".into(),
            collection: "leads".into(),
            row_id: "a".into(),
            field_path: "/name".into(),
        },
        ArtifactExposure {
            hash: "secret".into(),
            collection: "leads".into(),
            row_id: "a".into(),
            field_path: "/secret".into(),
        },
        ArtifactExposure {
            hash: "foreign".into(),
            collection: "leads".into(),
            row_id: "b".into(),
            field_path: "/name".into(),
        },
    ];
    let revoke = ScopeRevoke::new(&current, &rows, &artifacts);
    assert_eq!(revoke.artifacts, vec!["foreign", "secret"]);
    assert_eq!(
        revoke
            .clean_document("leads", &rows[&("leads".into(), "a".into())])
            .unwrap(),
        Some(json!({"owner":"alice","name":"A"}))
    );
    assert!(revoke
        .clean_document("leads", &rows[&("leads".into(), "b".into())])
        .unwrap()
        .is_none());
    let rejected = scope_filter::RevokedWrite::new("original-op");
    assert_eq!(
        serde_json::to_value(rejected).unwrap(),
        json!({"op_id":"original-op","status":"rejected","reason":"revoked"})
    );
}

#[test]
fn rights_change_under_load_rejects_old_authority_before_next_frame() {
    let old = scope(1);
    let hash = old.hash().unwrap();
    let authority = Arc::new(FrameAuthority::new(old));
    let start = Arc::new(Barrier::new(5));
    let warmed = Arc::new(Barrier::new(5));
    let delivered = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for _ in 0..4 {
        let authority = authority.clone();
        let start = start.clone();
        let warmed = warmed.clone();
        let delivered = delivered.clone();
        let hash = hash.clone();
        workers.push(std::thread::spawn(move || {
            start.wait();
            authority
                .with_frame(1, &hash, |_| {
                    delivered.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .unwrap();
            warmed.wait();
            for _ in 0..1000 {
                let _ = authority.with_frame(1, &hash, |_| {
                    delivered.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                });
            }
        }));
    }
    start.wait();
    warmed.wait();
    authority.update_committed(|_| Ok(scope(2))).unwrap();
    let at_commit = delivered.load(Ordering::SeqCst);
    assert!(at_commit >= 4);
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(delivered.load(Ordering::SeqCst), at_commit);
    assert!(authority.with_frame(1, &hash, |_| Ok(())).is_err());
    authority
        .with_frame(2, &scope(2).hash().unwrap(), |_| Ok(()))
        .unwrap();
}

#[test]
fn failed_rights_publication_cannot_leave_stale_frame_authority_live() {
    let original = scope(1);
    let hash = original.hash().unwrap();
    let authority = FrameAuthority::new(original);
    assert!(authority
        .update_committed(|_| Err("commit uncertain".into()))
        .is_err());
    assert!(authority.with_frame(1, &hash, |_| Ok(())).is_err());
    let authority = FrameAuthority::new(scope(1));
    assert!(authority.update_committed(|_| Ok(scope(1))).is_err());
    assert!(authority.with_frame(1, &hash, |_| Ok(())).is_err());
}

#[test]
fn equivalent_projection_checkpoint_is_still_bound_to_epoch_instance_and_generation() {
    let old = scope(1);
    let next = scope(2);
    let cp = ScopedCheckpoint {
        instance_id: "i".into(),
        epoch: "e".into(),
        collection: "leads".into(),
        generation: 1,
        scope_hash: old.hash().unwrap(),
        seq: 9007199254740993,
    };
    assert_eq!(
        cp.reuse_for(&old, &next, "i", "e", "leads", 1).unwrap(),
        cp.seq
    );
    for (instance, epoch, collection, generation) in [
        ("foreign", "e", "leads", 1),
        ("i", "restored", "leads", 1),
        ("i", "e", "foreign", 1),
        ("i", "e", "leads", 2),
    ] {
        assert_eq!(
            cp.reuse_for(&old, &next, instance, epoch, collection, generation)
                .unwrap(),
            0
        );
    }
    let wire = serde_json::to_value(&cp).unwrap();
    assert_eq!(wire["seq"], "9007199254740993");
    let mut forged = cp.clone();
    forged.scope_hash = "other".into();
    assert_eq!(
        forged.reuse_for(&old, &next, "i", "e", "leads", 1).unwrap(),
        0
    );
}
