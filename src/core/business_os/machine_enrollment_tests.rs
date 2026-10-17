// Origin: CTOX
// License: AGPL-3.0-only
//! §16.41 contract invariants. Each test names the mutation that turns it red
//! (E13 Mutationsbeleg); see befund-machine-join.md for the recorded runs.
use super::*;

const INSTANCE: &str = "inst-a";
const ADMIN: &str = "admin-1";
const MACHINE: &str = "workjet-mobile-invite-aaaa";
const T0: i64 = 1_000_000;

fn conn() -> Connection {
    Connection::open_in_memory().unwrap()
}

fn start(conn: &Connection) -> (String, String) {
    let begun = begin(
        conn,
        INSTANCE,
        ADMIN,
        "Werkbank",
        "ssh",
        json!({"type":"ctox-business-os-invite"}),
        Some(MACHINE),
        None,
        T0,
    )
    .unwrap();
    let secret = JoinToken::decode(&begun.join_token).unwrap().secret;
    (
        begun.document["id"].as_str().unwrap().to_owned(),
        join_proof(&secret),
    )
}

fn step(key: &str, state: &str) -> StepUpdate {
    StepUpdate::from_payload(&json!({"key": key, "state": state})).unwrap()
}

fn code(result: anyhow::Result<Value>) -> &'static str {
    error_code(&result.expect_err("must be rejected")).expect("deterministic error code")
}

fn state<'a>(doc: &'a Value, key: &str) -> &'a str {
    doc["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == key)
        .unwrap()["state"]
        .as_str()
        .unwrap()
}

/// Mutation: drop `steps[0] = token_issued done` or add a field to `document()`.
#[test]
fn begin_emits_exact_document_and_stores_only_the_token_hash() {
    let conn = conn();
    let (id, proof) = start(&conn);
    let doc = load(&conn, &id).unwrap().unwrap().document();
    let mut keys: Vec<_> = doc.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "created_at_ms",
            "created_by",
            "expires_at_ms",
            "id",
            "label",
            "status",
            "steps",
            "transport"
        ]
    );
    assert_eq!(doc["status"], "pending");
    assert_eq!(doc["expires_at_ms"], T0 + 15 * 60 * 1000);
    let keys: Vec<_> = doc["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, STEP_KEYS);
    assert_eq!(state(&doc, "token_issued"), "done");
    let stored: String = conn
        .query_row(
            "SELECT steps_json || token_hash FROM business_machine_enrollments",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Only sha256(proof) is persisted; neither the proof nor the token is.
    assert!(!stored.contains(&proof) && !doc.to_string().contains(&proof));
    assert!(stored.contains(&sha256_hex(&proof)));
}

/// Mutation: remove `ensure!(row.redeemed_at_ms.is_none(), TokenUsed)`.
#[test]
fn token_is_single_use() {
    let conn = conn();
    let (id, token) = start(&conn);
    let first = machine_step(
        &conn,
        INSTANCE,
        MACHINE,
        true,
        &id,
        MachineProof::Token(&token),
        &step("service_started", "done"),
        T0 + 1,
    )
    .unwrap();
    assert_eq!(state(&first, "service_started"), "done");
    let second = machine_step(
        &conn,
        INSTANCE,
        MACHINE,
        true,
        &id,
        MachineProof::Token(&token),
        &step("device_bound", "done"),
        T0 + 2,
    );
    assert_eq!(code(second), "token_used");
}

/// Mutation: remove `ensure!(now < row.expires_at_ms, TokenExpired)`.
#[test]
fn token_expires_after_fifteen_minutes() {
    let conn = conn();
    let (id, token) = start(&conn);
    let late = machine_step(
        &conn,
        INSTANCE,
        MACHINE,
        true,
        &id,
        MachineProof::Token(&token),
        &step("service_started", "done"),
        T0 + TOKEN_TTL_MS,
    );
    assert_eq!(code(late), "token_expired");
    // Bound to the instance: the same token is invalid at another instance.
    let foreign = machine_step(
        &conn,
        "inst-b",
        MACHINE,
        true,
        &id,
        MachineProof::Token(&token),
        &step("service_started", "done"),
        T0 + 1,
    );
    assert_eq!(code(foreign), "token_invalid");
}

/// Mutations: remove the creator check in `client_step`, or the
/// `bound_user_id == actor` check in `MachineProof::BoundDevice`.
#[test]
fn foreign_writers_cannot_write_steps() {
    let conn = conn();
    let (id, token) = start(&conn);
    let foreign = client_step(
        &conn,
        "other-admin",
        &id,
        &step("transport_connected", "done"),
        T0 + 1,
    );
    assert_eq!(code(foreign), "step_forbidden");
    // The desktop may not forge machine steps.
    let forged = client_step(&conn, ADMIN, &id, &step("service_started", "done"), T0 + 1);
    assert_eq!(code(forged), "step_forbidden");
    // Another paired device cannot redeem this enrollment's token.
    let stranger = machine_step(
        &conn,
        INSTANCE,
        "workjet-mobile-invite-other",
        true,
        &id,
        MachineProof::Token(&token),
        &step("service_started", "done"),
        T0 + 1,
    );
    assert_eq!(code(stranger), "bind_rejected");
    machine_step(
        &conn,
        INSTANCE,
        MACHINE,
        true,
        &id,
        MachineProof::Token(&token),
        &step("service_started", "done"),
        T0 + 2,
    )
    .unwrap();
    let unbound = machine_step(
        &conn,
        INSTANCE,
        "workjet-mobile-invite-other",
        true,
        &id,
        MachineProof::BoundDevice,
        &step("device_bound", "done"),
        T0 + 3,
    );
    assert_eq!(code(unbound), "bind_rejected");
    // A revoked device binding loses write access, too.
    let revoked = machine_step(
        &conn,
        INSTANCE,
        MACHINE,
        false,
        &id,
        MachineProof::BoundDevice,
        &step("device_bound", "done"),
        T0 + 3,
    );
    assert_eq!(code(revoked), "bind_rejected");
    let own = client_step(&conn, ADMIN, &id, &step("installed", "done"), T0 + 4).unwrap();
    assert_eq!(state(&own, "installed"), "done");
}

/// Mutations: let `client_step`/`machine_step` accept key `green`, or drop the
/// heartbeat condition in `settle_green`.
#[test]
fn only_the_core_sets_green_after_peer_and_first_heartbeat() {
    let conn = conn();
    let (id, token) = start(&conn);
    assert_eq!(
        code(client_step(
            &conn,
            ADMIN,
            &id,
            &step("green", "done"),
            T0 + 1
        )),
        "step_forbidden"
    );
    assert_eq!(
        code(machine_step(
            &conn,
            INSTANCE,
            MACHINE,
            true,
            &id,
            MachineProof::Token(&token),
            &step("green", "done"),
            T0 + 1,
        )),
        "step_forbidden"
    );
    for (n, key) in ["service_started", "device_bound", "peer_connected"]
        .into_iter()
        .enumerate()
    {
        let proof = if n == 0 {
            MachineProof::Token(&token)
        } else {
            MachineProof::BoundDevice
        };
        let doc = machine_step(
            &conn,
            INSTANCE,
            MACHINE,
            true,
            &id,
            proof,
            &step(key, "done"),
            T0 + 10 + n as i64,
        )
        .unwrap();
        assert_eq!(doc["status"], "running", "no green before a life sign");
        assert_eq!(state(&doc, "green"), "pending");
    }
    assert_eq!(
        code(heartbeat(
            &conn,
            "workjet-mobile-invite-other",
            true,
            &id,
            T0 + 20
        )),
        "bind_rejected"
    );
    let green = heartbeat(&conn, MACHINE, true, &id, T0 + 20).unwrap();
    assert_eq!(green["status"], "green");
    assert_eq!(state(&green, "green"), "done");
}

/// Mutation: remove `ensure!(row.open(), EnrollmentClosed)` in `open_row`.
#[test]
fn cancel_stops_further_steps() {
    let conn = conn();
    let (id, token) = start(&conn);
    assert_eq!(
        code(cancel(&conn, "other-admin", false, &id)),
        "step_forbidden",
        "only the creator cancels"
    );
    let cancelled = cancel(&conn, ADMIN, false, &id).unwrap();
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(
        code(client_step(
            &conn,
            ADMIN,
            &id,
            &step("transport_connected", "done"),
            T0 + 1
        )),
        "enrollment_closed"
    );
    assert_eq!(
        code(machine_step(
            &conn,
            INSTANCE,
            MACHINE,
            true,
            &id,
            MachineProof::Token(&token),
            &step("service_started", "done"),
            T0 + 1,
        )),
        "enrollment_closed"
    );
    let after = heartbeat(&conn, MACHINE, true, &id, T0 + 2);
    assert!(after.is_err() || after.unwrap()["status"] == "cancelled");
}

/// Mutation: move `MachinesManage` from the Chef|Admin arm to the open arm.
#[test]
fn machines_manage_is_admin_only() {
    use super::super::policy::{evaluate, BusinessOsActor, BusinessOsPermission, BusinessOsScope};
    let decide = |role: &str| {
        evaluate(
            &BusinessOsActor::new(Some("a".into()), role),
            BusinessOsPermission::MachinesManage,
            &BusinessOsScope::workspace(),
        )
        .allowed
    };
    assert!(decide("admin") && decide("owner"));
    assert!(!decide("user") && !decide("founder"));
    assert_eq!(
        BusinessOsPermission::MachinesManage.as_str(),
        "machines.manage"
    );
}

/// Wiring: the platform command reaches the handler and is gated by
/// `machines.manage` before any token is issued.
/// Mutation: require `CtoxTaskCreate` instead of `MachinesManage` in `handle_command`.
#[test]
fn begin_command_is_denied_without_machines_manage() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    super::super::store::tests::seed_business_user(root, "viewer", "user")?;
    let outcome = super::super::store::accept_rxdb_business_command(
        root,
        json!({
            "id": "machine-begin-denied",
            "command_id": "machine-begin-denied",
            "module": "ctox",
            "command_type": BEGIN,
            "payload": {"label": "Werkbank", "transport": "ssh"},
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
    assert!(!outcome.to_string().contains(TOKEN_PREFIX));
    let issued: i64 = super::super::store::open_store(root)?.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name='business_machine_enrollments'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(issued, 0, "no enrollment state before the policy gate");
    Ok(())
}

/// Part C: cancel = creator OR machines.manage.
/// Mutations: drop `manages ||`, or drop the creator clause in `cancel`.
#[test]
fn cancel_allows_creator_and_machines_manage_holders_only() {
    let conn = conn();
    let (first, _) = start(&conn);
    assert_eq!(
        code(cancel(&conn, "stranger", false, &first)),
        "step_forbidden"
    );
    let by_manager = cancel(&conn, "other-admin", true, &first).unwrap();
    assert_eq!(by_manager["status"], "cancelled");
    let (second, _) = start(&conn);
    let by_creator = cancel(&conn, ADMIN, false, &second).unwrap();
    assert_eq!(by_creator["status"], "cancelled");
}
