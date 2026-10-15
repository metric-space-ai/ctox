// Origin: CTOX
// License: AGPL-3.0-only
//! §16.43 invariants with mutation proofs (E13).
use super::*;

/// Mutations: `was != connected => false` (no immediate change), or the
/// 30-second floor replaced by `>= 0` (write storm).
#[test]
fn presence_writes_change_immediately_and_last_seen_at_most_every_30s() {
    let conn = Connection::open_in_memory().unwrap();
    let t = 1_000_000;
    assert!(
        observe_presence(&conn, "dev", true, t).unwrap(),
        "first sighting"
    );
    assert!(!observe_presence(&conn, "dev", true, t + 1_000).unwrap());
    assert!(!observe_presence(&conn, "dev", true, t + 29_999).unwrap());
    assert!(observe_presence(&conn, "dev", true, t + 30_000).unwrap());
    assert!(
        observe_presence(&conn, "dev", false, t + 30_001).unwrap(),
        "disconnect now"
    );
    assert!(
        observe_presence(&conn, "dev", true, t + 30_002).unwrap(),
        "reconnect now"
    );
    assert!(!observe_presence(&conn, "dev", true, t + 30_003).unwrap());
    assert!(
        observe_presence(&conn, "dev", false, t + 30_004).unwrap(),
        "disconnect now"
    );
}

/// Mutation: `== Some(viewer_id)` → `.is_some()` (foreign devices readable).
#[test]
fn foreign_devices_are_readable_only_with_machines_manage() {
    let device = json!({"id": "d1", "user_id": "alice"});
    assert!(may_read("alice", false, &device));
    assert!(!may_read("bob", false, &device));
    assert!(!may_read("", false, &json!({"id": "d2"})));
    assert!(may_read("bob", true, &device));
}

/// Format + source: invite-bound device of a §16.41 machine is kind
/// `machine`, owned by the enrollment creator, label from the enrollment.
#[test]
fn machine_devices_project_from_existing_binding_and_enrollment() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE business_mobile_invites (invite_id_hash TEXT PRIMARY KEY,
            user_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL, redeemed_at_ms INTEGER,
            revoked_at_ms INTEGER, display_name TEXT NOT NULL, device_pairing_id TEXT,
            created_by_user_id TEXT, expires_at_ms INTEGER NOT NULL DEFAULT 0);
         INSERT INTO business_mobile_invites (invite_id_hash, user_id, created_at_ms,
            redeemed_at_ms, revoked_at_ms, display_name, device_pairing_id, created_by_user_id)
         VALUES
            ('h1','workjet-mobile-invite-m',10,20,NULL,'Workjet Gerät','pair-m',NULL),
            ('h2','workjet-mobile-invite-p',11,21,NULL,'Telefon','pair-p','alice'),
            ('h3','workjet-mobile-invite-r',12,22,99,'Alt','pair-r','alice');",
    )
    .unwrap();
    let begun = super::super::machine_enrollment::begin(
        &conn,
        "inst",
        "admin-1",
        "Werkbank",
        "ssh",
        Value::Null,
        Some("workjet-mobile-invite-m"),
        None,
        5,
    )
    .unwrap();
    let id = begun.document["id"].as_str().unwrap();
    conn.execute(
        "UPDATE business_machine_enrollments SET bound_user_id='workjet-mobile-invite-m' WHERE enrollment_id=?1",
        params![id],
    )
    .unwrap();
    observe_presence(&conn, "pair-m", true, 30).unwrap();
    let docs = list(&conn).unwrap();
    assert_eq!(docs.len(), 2, "revoked edges are not devices");
    let machine = docs.iter().find(|d| d["id"] == "pair-m").unwrap();
    assert_eq!(machine["kind"], "machine");
    assert_eq!(machine["kind_source"], "enrollment");
    assert_eq!(machine["user_id"], "admin-1");
    assert_eq!(machine["label"], "Werkbank");
    assert_eq!(machine["connected"], true);
    assert_eq!(machine["last_seen_at_ms"], 30);
    let phone = docs.iter().find(|d| d["id"] == "pair-p").unwrap();
    assert_eq!(phone["kind"], "mobile");
    assert_eq!(phone["kind_source"], "legacy");
    assert_eq!(phone["user_id"], "alice");
    let mut keys: Vec<_> = machine.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "connected",
            "created_at_ms",
            "id",
            "kind",
            "kind_source",
            "label",
            "last_seen_at_ms",
            "user_id"
        ]
    );
}

/// Mutation: require `CtoxTaskCreate` instead of `MachinesManage` for unbind.
#[test]
fn unbind_requires_machines_manage() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    super::super::store::tests::seed_business_user(root, "viewer", "user")?;
    let outcome = super::super::store::accept_rxdb_business_command(
        root,
        json!({
            "id": "device-unbind-denied",
            "command_id": "device-unbind-denied",
            "module": "ctox",
            "command_type": UNBIND,
            "payload": {"id": "pair-x"},
            "client_context": {"actor": {"id": "viewer", "display_name": "Viewer"}}
        }),
    )?;
    assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(false));
    assert_eq!(
        outcome
            .pointer("/result/policy_decision/permission")
            .and_then(Value::as_str),
        Some("machines.manage")
    );
    Ok(())
}

/// Part C: the client's declared type is validated against the fixed list.
/// Mutation: `if !DEVICE_KINDS.contains(&kind)` → `if false`.
#[test]
fn declared_device_kind_is_validated_against_the_fixed_list() {
    use super::super::mobile_invites::device_descriptor;
    assert_eq!(device_descriptor(&json!({"version": "v1"})), Ok(None));
    let desktop = device_descriptor(&json!({"kind": "desktop", "platform": "macos-arm64"}))
        .unwrap()
        .unwrap();
    assert_eq!(desktop.kind, "desktop");
    assert_eq!(desktop.platform.as_deref(), Some("macos-arm64"));
    for bad in [
        json!({"kind": "machine"}),
        json!({"kind": "toaster"}),
        json!({"kind": 7}),
        json!({"kind": "browser", "platform": "Mac OS"}),
        json!({"kind": "browser", "platform": ""}),
    ] {
        assert!(device_descriptor(&bad).is_err(), "{bad}");
    }
}

/// Part C: the declaration is stored with the binding at first use and the
/// projection uses it (`kind_source: client`).
/// Mutations: bind stores `NULL` instead of the kind, or the projection
/// ignores `device_kind`.
#[test]
fn declared_kind_is_stored_with_the_binding_and_projected() -> anyhow::Result<()> {
    use super::super::mobile_invites::{
        authorize_or_bind_device_proof_with, create, DeviceDescriptor,
    };
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let created = create(root, 300, Some("Laptop"), None)?;
    let user_id = created["invite"]["session"]["user"]["id"].as_str().unwrap();
    let descriptor = DeviceDescriptor {
        kind: "desktop".into(),
        platform: Some("macos-arm64".into()),
    };
    assert!(authorize_or_bind_device_proof_with(
        root,
        user_id,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        chrono::Utc::now().timestamp_millis(),
        Some(&descriptor),
    ));
    let docs = list(&super::super::store::open_store(root)?)?;
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["kind"], "desktop");
    assert_eq!(docs[0]["kind_source"], "client");
    assert_eq!(docs[0]["platform"], "macos-arm64");
    Ok(())
}

/// Part C: presence for every admitted device without client heartbeats.
/// Mutations: drop the disconnect pass in `tick`, or the 30 s floor.
#[test]
fn tracker_reports_connect_and_disconnect_immediately_and_throttles_last_seen() {
    let set = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect();
    let mut tracker = PresenceTracker::default();
    let t = 1_000_000;
    assert_eq!(tracker.tick(&set(&["a"]), t), vec![("a".into(), true)]);
    assert!(tracker.tick(&set(&["a"]), t + 1_000).is_empty());
    assert!(tracker.tick(&set(&["a"]), t + 29_999).is_empty());
    assert_eq!(
        tracker.tick(&set(&["a"]), t + 30_000),
        vec![("a".into(), true)]
    );
    assert_eq!(
        tracker.tick(&set(&[]), t + 30_001),
        vec![("a".into(), false)]
    );
    assert_eq!(
        tracker.tick(&set(&["a"]), t + 30_002),
        vec![("a".into(), true)]
    );
    // A device left "connected" by a previous run flips on the first tick.
    let conn = Connection::open_in_memory().unwrap();
    observe_presence(&conn, "stale", true, t).unwrap();
    let mut resumed = PresenceTracker::resume(&conn).unwrap();
    assert_eq!(
        resumed.tick(&set(&[]), t + 5),
        vec![("stale".into(), false)]
    );
}
