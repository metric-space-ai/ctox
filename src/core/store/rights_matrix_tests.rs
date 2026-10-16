//! One joint Store-backed rights contract. Cases cross read/write/publication;
//! assertions name the authorization dimension so mutation evidence is useful.
use super::*;
use serde_json::json;
use std::sync::Arc;
use sync_rights::{
    manifest_rules::ManifestRules,
    scope::{
        scope_filter::{project, ProjectedChange},
        FieldMask, RowFilter,
    },
    Operation, Right, StoreRightsAuthority,
};

#[derive(Clone)]
struct Case {
    name: &'static str,
    role: &'static str,
    grant_role: &'static str,
    grant_collection: &'static str,
    actor: &'static str,
    app: &'static str,
    read_grant: bool,
    write_grant: bool,
    foreign_ops: Vec<&'static str>,
    export_ops: Vec<&'static str>,
    row_filter: bool,
    mask: bool,
    changed: &'static str,
    from: Value,
    to: Value,
    command_roles: Vec<&'static str>,
    command: &'static str,
    assigned: bool,
    expected: (bool, bool, bool, bool), // read, direct write, handler dispatch, handler write
    publish: bool,
}
impl Default for Case {
    fn default() -> Self {
        Self {
            name: "own",
            role: "user",
            grant_role: "user",
            grant_collection: "leads",
            actor: "alice",
            app: "owner",
            read_grant: true,
            write_grant: true,
            foreign_ops: vec!["read", "write"],
            export_ops: vec!["read", "write", "watch"],
            row_filter: true,
            mask: true,
            changed: "name",
            from: json!("A"),
            to: json!("B"),
            command_roles: vec!["user"],
            command: "lead.edit",
            assigned: false,
            expected: (true, true, true, true),
            publish: true,
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rights_matrix_read_write_replication_share_committed_authority() {
    use ctox_store::{registry::Registry, writer::WriterOptions, DbId};
    let cases = vec![
        Case::default(),
        Case {
            name: "role/admin grant missing",
            role: "admin",
            expected: (false, false, false, false),
            ..Case::default()
        },
        Case {
            name: "role/chef grant present",
            role: "chef",
            grant_role: "chef",
            command_roles: vec!["chef"],
            ..Case::default()
        },
        Case {
            name: "role/founder grant present",
            role: "founder",
            grant_role: "founder",
            command_roles: vec!["founder"],
            ..Case::default()
        },
        Case {
            name: "collection mismatch",
            grant_collection: "other",
            expected: (false, false, true, false),
            ..Case::default()
        },
        Case {
            name: "row filter actor",
            actor: "bob",
            expected: (false, false, true, false),
            ..Case::default()
        },
        Case {
            name: "field mask changed secret",
            changed: "secret",
            expected: (true, false, true, false),
            ..Case::default()
        },
        Case {
            name: "field mask all",
            mask: false,
            changed: "secret",
            ..Case::default()
        },
        Case {
            name: "foreign read only",
            app: "consumer",
            foreign_ops: vec!["read"],
            expected: (true, false, true, false),
            ..Case::default()
        },
        Case {
            name: "foreign write only",
            app: "consumer",
            foreign_ops: vec!["write"],
            expected: (false, true, true, true),
            ..Case::default()
        },
        Case {
            name: "foreign undeclared",
            app: "consumer",
            foreign_ops: vec![],
            expected: (false, false, true, false),
            ..Case::default()
        },
        Case {
            name: "foreign watch without read",
            app: "consumer",
            foreign_ops: vec!["watch"],
            expected: (false, false, true, false),
            ..Case::default()
        },
        Case {
            name: "user read op denied",
            read_grant: false,
            expected: (false, true, true, true),
            ..Case::default()
        },
        Case {
            name: "user write op denied",
            write_grant: false,
            expected: (true, false, true, false),
            ..Case::default()
        },
        Case {
            name: "command right role",
            command_roles: vec!["admin"],
            expected: (true, true, false, false),
            ..Case::default()
        },
        Case {
            name: "command assigned by Core",
            command_roles: vec![],
            assigned: true,
            ..Case::default()
        },
        Case {
            name: "command exact denial wins",
            command: "lead.deny",
            expected: (true, true, false, false),
            ..Case::default()
        },
        Case {
            name: "command-only field",
            changed: "review",
            expected: (true, false, true, true),
            ..Case::default()
        },
        Case {
            name: "command-only transition",
            changed: "status",
            from: json!("researched"),
            to: json!("failed"),
            expected: (true, false, true, true),
            ..Case::default()
        },
        Case {
            name: "transition other target",
            changed: "status",
            from: json!("researched"),
            to: json!("active"),
            ..Case::default()
        },
        Case {
            name: "transition other source",
            changed: "status",
            from: json!("new"),
            to: json!("failed"),
            ..Case::default()
        },
        Case {
            name: "transition unchanged",
            changed: "status",
            from: json!("failed"),
            to: json!("failed"),
            ..Case::default()
        },
        Case {
            name: "transition typed null",
            changed: "status",
            from: Value::Null,
            to: json!("failed"),
            ..Case::default()
        },
        Case {
            name: "row filter disabled",
            actor: "bob",
            row_filter: false,
            ..Case::default()
        },
        Case {
            name: "unrestricted projection",
            row_filter: false,
            mask: false,
            ..Case::default()
        },
        Case {
            name: "foreign unexported",
            app: "consumer",
            export_ops: vec!["read"],
            publish: false,
            ..Case::default()
        },
    ];
    let dir = tempfile::tempdir().unwrap();
    let writer = Registry
        .open(
            DbId::new(dir.path().join("matrix.sqlite")).unwrap(),
            WriterOptions {
                configure: Some(Arc::new(|db| {
                    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                    Ok(())
                })),
                head_seq: Some(Arc::new(|db| {
                    db.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                        .map_err(Into::into)
                })),
            },
        )
        .unwrap();
    let authority = StoreRightsAuthority::attach(writer, "matrix-instance".into())
        .await
        .unwrap();
    let admin = BusinessOsActor::new(Some("admin".into()), "admin");
    assert!(
        !evaluate(
            &admin,
            BusinessOsPermission::UsersManage,
            &BusinessOsScope::workspace()
        )
        .allowed,
        "no static fallback"
    );
    assert!(
        !allow_decision(
            BusinessOsPermission::DataRead,
            &BusinessOsScope::collection("leads")
        )
        .allowed,
        "actor-less legacy grants cannot override current Store denial"
    );
    let registration = bind_policy_authority(authority.clone()).unwrap();
    let mut previous_seq = 0;
    authority
        .writer()
        .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
            ctox_store::schema::create_collection(
                tx,
                &ctox_store::schema::CollectionDefinition {
                    name: "leads".into(),
                    app_id: "owner".into(),
                    schema_ver: 1,
                    schema_json: json!({"type":"object","properties":{"id":{"type":"string"}}}),
                    class: "full".into(),
                    client_limit: 1000,
                    byte_budget: 1_000_000,
                    rights_ver: 1,
                    app_ver: "1".into(),
                },
            )?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let readers = ctox_store::read_pool::ReadPool::for_database(
        authority.writer().db_id(),
        authority.writer().generation(),
        Default::default(),
        // This contract deliberately holds a WAL snapshot over a writer commit.
        // Production readers keep the normal 20-ms execution budget unchanged.
        ctox_store::config::AdmissionBudget {
            execution: std::time::Duration::from_secs(2),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    struct Keys(Vec<u8>);
    impl sync_rights::scope::InstanceSigningKeyStore for Keys {
        fn with_instance_signing_key<T>(
            &self,
            f: impl FnOnce(&[u8]) -> sync_rights::scope::Result<T>,
        ) -> sync_rights::scope::Result<T> {
            f(&self.0)
        }
    }
    let keys = Keys(
        ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .unwrap()
            .as_ref()
            .to_vec(),
    );
    let mut preceding_frame: Option<(NativeStorePolicy, u64, String)> = None;

    // Retarget the existing critical default-policy invariants to the imported
    // Store data, including actor-owned record scope and projection protection.
    let seed_collections = [
        "business_users",
        "customer_accounts",
        "ctox_runs",
        "ctox_crew_members",
        "ctox_harness_status",
    ]
    .map(String::from);
    authority
        .replace(base_release_rights(&seed_collections), vec![])
        .await
        .unwrap();
    let (_, seed_rights) = authority.hello_snapshot().await.unwrap();
    let owned_record = BusinessOsScope {
        scope_type: BusinessOsScopeType::Record,
        scope_id: Some("accounts/a".into()),
        assigned_to_actor: false,
        owned_by_actor: true,
    };
    let foreign_record = BusinessOsScope {
        owned_by_actor: false,
        ..owned_record.clone()
    };
    let owned_workspace = BusinessOsScope {
        owned_by_actor: true,
        ..BusinessOsScope::workspace()
    };
    let assigned_module = BusinessOsScope::module("app", true);
    let other_module = BusinessOsScope::module("app", false);
    let seed_cases = [
        (
            "chef",
            BusinessOsPermission::WorkspaceManage,
            BusinessOsScope::workspace(),
            true,
        ),
        (
            "admin",
            BusinessOsPermission::WorkspaceManage,
            BusinessOsScope::workspace(),
            false,
        ),
        (
            "admin",
            BusinessOsPermission::UsersManage,
            BusinessOsScope::workspace(),
            true,
        ),
        (
            "user",
            BusinessOsPermission::UsersManage,
            BusinessOsScope::workspace(),
            false,
        ),
        (
            "founder",
            BusinessOsPermission::AppsModify,
            assigned_module.clone(),
            true,
        ),
        (
            "founder",
            BusinessOsPermission::AppsModify,
            other_module,
            false,
        ),
        (
            "user",
            BusinessOsPermission::AppsModify,
            assigned_module.clone(),
            false,
        ),
        (
            "user",
            BusinessOsPermission::DataWrite,
            owned_record.clone(),
            true,
        ),
        (
            "user",
            BusinessOsPermission::DataWrite,
            foreign_record.clone(),
            false,
        ),
        (
            "admin",
            BusinessOsPermission::DataWrite,
            foreign_record,
            true,
        ),
        (
            "user",
            BusinessOsPermission::DataWrite,
            owned_workspace,
            false,
        ),
        (
            "user",
            BusinessOsPermission::DataWrite,
            assigned_module,
            true,
        ),
        (
            "user",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("business_users"),
            false,
        ),
        (
            "admin",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("business_users"),
            true,
        ),
        (
            "founder",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("business_users"),
            false,
        ),
        (
            "user",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("customer_accounts"),
            true,
        ),
        (
            "user",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("ctox_runs"),
            false,
        ),
        (
            "user",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("ctox_crew_members"),
            false,
        ),
        (
            "founder",
            BusinessOsPermission::DataRead,
            BusinessOsScope::collection("ctox_runs"),
            true,
        ),
        (
            "admin",
            BusinessOsPermission::DataWrite,
            BusinessOsScope::collection("ctox_runs"),
            false,
        ),
        (
            "chef",
            BusinessOsPermission::SessionHandoffExecute,
            BusinessOsScope::workspace(),
            false,
        ),
    ];
    for (role, permission, scope, expected) in seed_cases {
        let actor = BusinessOsActor::new(Some("seed-actor".into()), role);
        assert_eq!(
            evaluate_with_rights(&seed_rights, &actor, permission, &scope).allowed,
            expected,
            "base import role={role} permission={} scope={}",
            permission.as_str(),
            scope.scope_type.as_str()
        );
        assert_eq!(
            evaluate(&actor, permission, &scope).allowed,
            expected,
            "runtime follows imported seed"
        );
    }
    for (alias, canonical) in [
        ("owner", "chef"),
        ("business_os_admin", "admin"),
        ("business_os_user", "user"),
        ("team", "user"),
        ("unknown", "user"),
    ] {
        assert_eq!(normalize_role(alias), canonical);
    }
    assert!(role_can_manage("admin"));
    assert!(!role_can_manage("founder"));
    assert!(role_may_read_collection(
        BusinessOsRole::User,
        "customer_accounts"
    ));
    assert!(!role_may_read_collection(
        BusinessOsRole::User,
        "business_users"
    ));
    let public = seed_rights
        .resolve_collection("user", "alice", "ctox_crew_members")
        .unwrap();
    let public = project(
        public.projection("ctox_crew_members"),
        "ctox_crew_members",
        "a",
        1,
        Some(&json!({"id":"a","name":"Alice","private_cost":42})),
    )
    .unwrap();
    assert!(
        matches!(public,ProjectedChange::Put{doc,..} if doc.get("private_cost").is_none()),
        "base import must keep private crew fields off replication"
    );
    for c in cases {
        let owner = ManifestRules::from_manifest("owner","1",["leads".into(),"other".into()].into(), &json!({
            "row_filters":{"leads":if c.row_filter {json!({"op":"eq","path":"/owner","value":{"kind":"actor_id"}})} else {json!({"op":"all"})}},
            "write_guard":{"leads":{"command_only_fields":["review"],"command_only_transitions":{"status":{"from":["researched"],"to":["failed"]}}}},
            "exports":{"leads":c.export_ops},
            "command_rights":{"lead.*":{"roles":c.command_roles,"assigned_users":true},"lead.deny":{"roles":[]}}
        })).unwrap();
        let foreign = if c.foreign_ops.is_empty() {
            json!({})
        } else {
            json!({"leads":{"owner_app":"owner","ops":c.foreign_ops}})
        };
        let consumer = ManifestRules::from_manifest("consumer","1",Default::default(), &json!({
            "foreign_collections":foreign,
            "command_rights":{"lead.*":{"roles":c.command_roles,"assigned_users":true},"lead.deny":{"roles":[]}}
        })).unwrap();
        let mut rows = base_release_rights(&[]);
        rows.push(Right {
            role: c.grant_role.into(),
            collection: c.grant_collection.into(),
            row_filter: RowFilter::All,
            field_mask: if c.mask {
                FieldMask::Fields {
                    paths: ["/id", "/owner", "/name", "/review", "/status"]
                        .map(String::from)
                        .into(),
                }
            } else {
                FieldMask::All
            },
            ops: [
                (c.read_grant, Operation::Read),
                (c.write_grant, Operation::Write),
            ]
            .into_iter()
            .filter_map(|(yes, op)| yes.then_some(op))
            .collect(),
        });
        let result = authority.replace(rows, vec![owner, consumer]).await;
        if !c.publish {
            assert!(
                result.is_err(),
                "{}: invalid foreign export must fail publication",
                c.name
            );
            let (head, still_current) = authority.hello_snapshot().await.unwrap();
            assert_eq!(
                head.head_seq, previous_seq,
                "invalid publication must not write policy data"
            );
            assert!(still_current.rights_ver > 0);
            assert!(
                evaluate(
                    &admin,
                    BusinessOsPermission::UsersManage,
                    &BusinessOsScope::workspace()
                )
                .allowed,
                "invalid manifest must not suspend committed rights"
            );
            continue;
        }
        result.unwrap();
        let (head, rights) = authority.hello_snapshot().await.unwrap();
        assert!(
            head.head_seq > previous_seq,
            "{}: rights must share Store ordering",
            c.name
        );

        let actor = BusinessOsActor::new(Some(c.actor.into()), c.role);
        let scope = BusinessOsScope::collection("leads");
        let grant_matches = c.role == c.grant_role && c.grant_collection == "leads";
        assert_eq!(
            evaluate(&actor, BusinessOsPermission::DataRead, &scope).allowed,
            grant_matches && c.read_grant && !c.row_filter && !c.mask,
            "{}: runtime read op",
            c.name
        );
        assert_eq!(
            evaluate(&actor, BusinessOsPermission::DataWrite, &scope).allowed,
            false,
            "{}: runtime write op",
            c.name
        );
        let mut before = json!({"id":"lead-1","owner":"alice","name":"A","secret":"private","review":"pending","status":"researched"});
        before[c.changed] = c.from.clone();
        let mut after = before.clone();
        after[c.changed] = c.to.clone();
        let adapter = NativeStorePolicy::from_verified_peer(
            authority.clone(),
            VerifiedSyncPeer {
                actor: actor.clone(),
                instance_id: "matrix-instance".into(),
                device_id: "device-a".into(),
            },
            c.app.into(),
        )
        .unwrap();
        if let Some((old, version, epoch)) = preceding_frame.take() {
            assert!(
                old.with_page_frame(version, &epoch, || Ok(())).is_err(),
                "stale prepared page must not reach IO after rights publication"
            );
        }
        preceding_frame = Some((adapter.clone(), rights.rights_ver, head.epoch.clone()));
        let input = before.clone();
        authority.writer().submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
            tx.execute("INSERT INTO c_leads(id,rev,schema_ver,data,deleted) VALUES('lead-1','fixture',1,?1,0)
                ON CONFLICT(id) DO UPDATE SET data=excluded.data,deleted=0", [input.to_string()])?;
            Ok(())
        })).unwrap().await.unwrap();
        let resolved = rights.resolve_app_collection(c.app, c.role, c.actor, "leads");
        let hash = resolved
            .as_ref()
            .ok()
            .and_then(|s| s.hash().ok())
            .unwrap_or_default();
        let scoped = adapter.clone();
        let read_hash = hash.clone();
        let page = readers
            .read(move |tx| {
                scoped.authorize_snapshot(tx, "device-a", "leads", 1, &read_hash)?;
                let page = ctox_store::change_log::read_page(tx, "leads", 1, 0, 100)?;
                page.docs
                    .iter()
                    .map(|doc| scoped.project_change(tx, "device-a", "leads", &read_hash, doc))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .await;
        let projected = page
            .as_ref()
            .ok()
            .and_then(|docs| docs.first())
            .and_then(Option::as_ref);
        assert_eq!(
            projected.is_some(),
            c.expected.0,
            "{}: actual Store cursor visibility",
            c.name
        );
        if let Some(doc) = projected {
            assert_eq!(
                doc.get("secret").is_some(),
                !c.mask,
                "{}: actual Store cursor field mask",
                c.name
            );
        }
        if c.name == "own" {
            // A real change of an already-visible row must become unproject,
            // using exactly the same canonical query as initial catch-up.
            let hidden = json!({"id":"lead-1","owner":"bob","name":"A"});
            authority
                .writer()
                .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
                    tx.execute(
                        "UPDATE c_leads SET data=?1,rev='left-filter' WHERE id='lead-1'",
                        [hidden.to_string()],
                    )?;
                    Ok(())
                }))
                .unwrap()
                .await
                .unwrap();
            let scoped = adapter.clone();
            let expected_hash = hash.clone();
            let left = readers
                .read(move |tx| {
                    let page = ctox_store::change_log::read_page(tx, "leads", 1, 0, 100)?;
                    scoped.project_change(tx, "device-a", "leads", &expected_hash, &page.docs[0])
                })
                .await
                .unwrap();
            assert!(
                left.is_none(),
                "leaving the actual Store row filter must unproject"
            );
            let original = before.clone();
            authority
                .writer()
                .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
                    tx.execute(
                        "UPDATE c_leads SET data=?1,rev='fixture' WHERE id='lead-1'",
                        [original.to_string()],
                    )?;
                    Ok(())
                }))
                .unwrap()
                .await
                .unwrap();
        }
        let scoped = adapter.clone();
        let proposed = after.clone();
        let written = authority
            .writer()
            .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
                scoped
                    .admit_write(
                        tx,
                        "device-a",
                        "leads",
                        "lead-1",
                        Some(&proposed),
                        None,
                        |tx| {
                            tx.execute(
                                "UPDATE c_leads SET data=?1,rev='admitted' WHERE id='lead-1'",
                                [proposed.to_string()],
                            )?;
                            Ok(())
                        },
                    )
                    .map_err(Into::into)
            }))
            .unwrap()
            .await;
        assert!(
            written.is_ok(),
            "{}: committed rule denial must be typed, not a transient Store failure",
            c.name
        );
        assert_eq!(
            matches!(written, Ok(NativeWriteAdmission::Accepted(()))),
            c.expected.1,
            "{}: actual Store write admission",
            c.name
        );
        let lease = adapter.issue_peer_lease(&keys, "leads", 1000).await;
        let authorized_scope = resolved
            .as_ref()
            .ok()
            .is_some_and(|s| s.projection("leads").is_some());
        assert_eq!(
            lease.is_ok(),
            authorized_scope,
            "{}: real policy peer lease",
            c.name
        );
        if let Ok(lease) = lease {
            use ring::signature::KeyPair;
            let key = ring::signature::Ed25519KeyPair::from_pkcs8(&keys.0).unwrap();
            let scope = resolved.as_ref().unwrap();
            assert!(lease
                .verify(
                    key.public_key().as_ref(),
                    "matrix-instance",
                    "device-a",
                    scope,
                    1001
                )
                .is_ok());
            assert!(lease.renewal_due(1120));
            assert!(lease
                .verify(
                    key.public_key().as_ref(),
                    "matrix-instance",
                    "device-a",
                    scope,
                    1300
                )
                .is_err());
        }
        // Exact device binding and a substituted scope are rejected inside the snapshot.
        let scoped = adapter.clone();
        let participant_hash = hash.clone();
        assert!(
            readers
                .read(move |tx| scoped.authorize_snapshot(
                    tx,
                    "other-device",
                    "leads",
                    1,
                    &participant_hash
                ))
                .await
                .is_err(),
            "participant substitution must not borrow an authenticated actor"
        );
        for (generation, bad_hash) in [(2, hash.clone()), (1, "forged-scope".into())] {
            let scoped = adapter.clone();
            assert!(
                readers
                    .read(move |tx| scoped
                        .authorize_snapshot(tx, "device-a", "leads", generation, &bad_hash))
                    .await
                    .is_err(),
                "generation or scope substitution must be rejected"
            );
        }
        let read = read_sync_document(&actor, &rights, c.app, "leads", &before);
        assert_eq!(read.is_ok(), c.expected.0, "{}: read boundary", c.name);
        let projection = rights
            .resolve_app_collection(c.app, c.role, c.actor, "leads")
            .ok();
        let change = project(
            projection.as_ref().and_then(|s| s.projection("leads")),
            "leads",
            "lead-1",
            head.head_seq,
            Some(&before),
        )
        .unwrap();
        match (read, change) {
            (Ok(read), ProjectedChange::Put { doc, .. }) => {
                assert_eq!(read, doc, "{}: read and replication field mask", c.name);
                assert_eq!(
                    doc.get("secret").is_some(),
                    !c.mask,
                    "{}: hidden fields",
                    c.name
                );
            }
            (Err(_), ProjectedChange::Unproject { .. }) => {}
            _ => panic!("{}: read and replication visibility differ", c.name),
        }
        assert_eq!(
            write_sync_document(
                &actor,
                &rights,
                c.app,
                "leads",
                Some(&before),
                Some(&after),
                None
            )
            .is_ok(),
            c.expected.1,
            "{}: direct write",
            c.name
        );
        let mut entered = false;
        let permit = dispatch_sync_command(
            &authority.router,
            &actor,
            c.app,
            c.command,
            c.assigned,
            |p| {
                entered = true;
                Ok(p)
            },
        );
        assert_eq!(
            entered, c.expected.2,
            "{}: command_rights before handler",
            c.name
        );
        let written = permit.and_then(|p| {
            write_sync_document(
                &actor,
                &rights,
                c.app,
                "leads",
                Some(&before),
                Some(&after),
                Some(&p),
            )
        });
        assert_eq!(written.is_ok(), c.expected.3, "{}: command write", c.name);
        previous_seq = authority.hello_snapshot().await.unwrap().0.head_seq;
    }
    // Hold an actual old WAL snapshot across a policy commit. An old reader
    // cannot authorize from it even though its SQLite transaction is still valid.
    let (_, committed) = authority.hello_snapshot().await.unwrap();
    let owner_scope = committed
        .resolve_app_collection("owner", "user", "alice", "leads")
        .unwrap();
    let hash = owner_scope.hash().unwrap();
    let scoped = NativeStorePolicy::from_verified_peer(
        authority.clone(),
        VerifiedSyncPeer {
            actor: BusinessOsActor::new(Some("alice".into()), "user"),
            instance_id: "matrix-instance".into(),
            device_id: "device-a".into(),
        },
        "owner".into(),
    )
    .unwrap();
    let old_lease = scoped.issue_peer_lease(&keys, "leads", 1000).await.unwrap();
    let held_readers = readers.clone();
    let (established, wait_snapshot) = tokio::sync::oneshot::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    let held = tokio::spawn(async move {
        held_readers
            .read(move |tx| {
                sync_rights::read(tx)?;
                let _ = established.send(());
                wait_release
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .map_err(|_| {
                        rusqlite::Error::InvalidParameterName("snapshot barrier timeout".into())
                    })?;
                let read_denied = scoped
                    .authorize_snapshot(tx, "device-a", "leads", 1, &hash)
                    .is_err();
                let write_transient = scoped
                    .admit_write(
                        tx,
                        "device-a",
                        "leads",
                        "lead-1",
                        Some(&json!({"id":"lead-1","owner":"alice","name":"B"})),
                        None,
                        |_| Ok(()),
                    )
                    .is_err();
                Ok((read_denied, write_transient))
            })
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), wait_snapshot)
        .await
        .unwrap()
        .unwrap();
    authority
        .replace(committed.rules, committed.manifests)
        .await
        .unwrap();
    release.send(()).unwrap();
    let (read_denied, write_transient) = held.await.unwrap().unwrap();
    assert!(
        read_denied,
        "a WAL snapshot preceding a rights commit must be rejected"
    );
    assert!(
        write_transient,
        "uncertain write authority must retain the outbox, never emit a terminal rejection"
    );
    let (_, renewed) = authority.hello_snapshot().await.unwrap();
    let next_scope = renewed
        .resolve_app_collection("owner", "user", "alice", "leads")
        .unwrap();
    use ring::signature::KeyPair;
    let public = ring::signature::Ed25519KeyPair::from_pkcs8(&keys.0).unwrap();
    assert!(
        old_lease
            .verify(
                public.public_key().as_ref(),
                "matrix-instance",
                "device-a",
                &next_scope,
                1001
            )
            .is_err(),
        "a peer lease from an old rights version must be rejected"
    );
    authority.replace(vec![], vec![]).await.unwrap();
    assert!(
        !evaluate(
            &admin,
            BusinessOsPermission::UsersManage,
            &BusinessOsScope::workspace()
        )
        .allowed,
        "committed revoke must remove runtime authority"
    );
    readers.shutdown().await.unwrap();
    authority.writer().shutdown().await.unwrap();
    drop(registration);
    assert!(
        !evaluate(
            &admin,
            BusinessOsPermission::UsersManage,
            &BusinessOsScope::workspace()
        )
        .allowed,
        "retired Store cannot authorize"
    );
}
