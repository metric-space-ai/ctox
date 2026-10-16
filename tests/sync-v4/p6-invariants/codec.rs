//! Real SQLite/schema/policy/codec contract checks, no host or policy stand-ins.
use super::*;

#[test]
fn p6_revocation_in_writer_transaction_prevents_effect() {
    let mut f = Fixture::new();
    let binding = f.binding();
    let writes = [StoreWrite {
        collection: "rows".into(),
        op: json!({"type":"put","doc":{"id":"probe","n":1}}),
    }];
    let tx = f.db.transaction().unwrap();
    f.codec
        .write(&tx, &binding, None, &writes)
        .expect("positive control must write");
    tx.rollback().unwrap();
    let tx = f.db.transaction().unwrap();
    tx.execute("UPDATE _rights_state SET rights_ver=rights_ver+1", [])
        .unwrap();
    assert!(
        f.codec.write(&tx, &binding, None, &writes).is_err(),
        "writer used policy from before the transaction"
    );
    assert!(
        document(&tx, "rows", "probe").unwrap().is_none(),
        "revoked write left an effect"
    );
    tx.rollback().unwrap();
}

#[test]
fn p6_manifest_isolates_foreign_collection_read_and_write() {
    let mut f = Fixture::new();
    let tx = f.db.transaction().unwrap();
    schema::create_collection(
        &tx,
        &CollectionDefinition {
            name: "foreign_rows".into(),
            app_id: "other-app".into(),
            schema_ver: 1,
            schema_json: schema_json(),
            class: "replicated".into(),
            client_limit: 100,
            byte_budget: 1048576,
            rights_ver: 1,
            app_ver: "v1".into(),
        },
    )
    .unwrap();
    super::super::super::commands::put(
        &tx,
        "foreign_rows",
        "secret",
        &json!({"id":"secret","n":17}),
    )
    .unwrap();
    let mut r = rights(&tx).unwrap();
    let mut right = r.rules[0].clone();
    right.collection = "foreign_rows".into();
    r.rules.push(right);
    r.manifests.push(sync_rights::manifest_rules::ManifestRules::from_manifest(
        "other-app","v1",["foreign_rows".into()].into(),
        &json!({"app_id":"other-app","app_ver":"v1","collections":{"foreign_rows":schema_json()}})).unwrap());
    sync_rights::replace_with_manifests(&tx, r.rights_ver, &r.rules, &r.manifests).unwrap();
    tx.commit().unwrap();
    let binding = f.binding();
    assert!(
        f.codec.read(&f.db, &binding, "rows").is_ok(),
        "own collection control denied"
    );
    assert!(
        f.codec.read(&f.db, &binding, "foreign_rows").is_err(),
        "app A read app B's declared collection"
    );
    let tx = f.db.transaction().unwrap();
    assert!(
        f.codec
            .write(
                &tx,
                &binding,
                None,
                &[StoreWrite {
                    collection: "foreign_rows".into(),
                    op: json!({"type":"put","doc":{"id":"probe","n":9}})
                }]
            )
            .is_err(),
        "app A wrote app B's collection"
    );
    assert!(document(&tx, "foreign_rows", "probe").unwrap().is_none());
    assert_eq!(
        document(&tx, "foreign_rows", "secret").unwrap().unwrap()["n"],
        17
    );
    tx.rollback().unwrap();
}

#[test]
fn p6_invalid_release_signature_never_admits_document_effect() {
    let mut f = Fixture::new();
    let binding = f.binding();
    let tx = f.db.transaction().unwrap();
    let mut record = document(&tx, "business_app_versions", "app@v1")
        .unwrap()
        .unwrap();
    record["signature"] = json!("00".repeat(64));
    super::super::super::commands::put(&tx, "business_app_versions", "app@v1", &record).unwrap();
    tx.commit().unwrap();
    let tx = f.db.transaction().unwrap();
    let writes = [StoreWrite {
        collection: "rows".into(),
        op: json!({"type":"put","doc":{"id":"bad-signature","n":1}}),
    }];
    assert!(
        f.codec.write(&tx, &binding, None, &writes).is_err(),
        "invalidly signed manifest authorized an applied document"
    );
    assert!(document(&tx, "rows", "bad-signature").unwrap().is_none());
    tx.rollback().unwrap();
}

#[test]
fn p6_blob_bytes_require_digest_size_and_private_cas() {
    let f = Fixture::new();
    let bytes = b"p6 content addressed positive control";
    let hash = digest(bytes);
    let mut receiver = f
        .codec
        .cas
        .receive(&"a".repeat(64), &hash, bytes.len() as u64)
        .unwrap();
    receiver.append(0, bytes).unwrap();
    receiver.finish().unwrap();
    assert_eq!(f.codec.bytes(&hash, bytes.len() as u64).unwrap(), bytes);
    for reference in [
        "../../outside",
        "/etc/passwd",
        "aaaaaaaa/../../outside",
        &"G".repeat(64),
    ] {
        assert!(
            f.codec.bytes(reference, 1).is_err(),
            "non-content-addressed reference accepted"
        );
    }
    assert!(
        f.codec.bytes(&hash, bytes.len() as u64 + 1).is_err(),
        "CAS accepted inconsistent object size"
    );
    assert!(f.codec.bytes(&"b".repeat(64), bytes.len() as u64).is_err());
    assert!(f.codec.bytes(&hash, 64 * 1024 * 1024 + 1).is_err());
    let object = f._root.path().join("objects").join(&hash);
    std::fs::write(&object, vec![0; bytes.len()]).unwrap();
    assert!(
        f.codec.bytes(&hash, bytes.len() as u64).is_err(),
        "modified CAS content passed digest verification"
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("external");
        std::fs::write(&external, bytes).unwrap();
        std::fs::remove_file(&object).unwrap();
        std::os::unix::fs::symlink(external, &object).unwrap();
        assert!(
            f.codec.bytes(&hash, bytes.len() as u64).is_err(),
            "CAS symlink escaped the artifact root"
        );
    }
}
