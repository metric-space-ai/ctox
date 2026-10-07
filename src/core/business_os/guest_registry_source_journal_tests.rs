// Origin: CTOX
// License: AGPL-3.0-only
//! Journal storage regressions on the actual native policy store.
//! Source/quorum/producer inputs remain fixtures; this is not model or handoff acceptance.
use super::*;

fn journal_fixture(session: &str, message: &str) -> Vec<u8> {
    let timestamp = "2026-10-07T02:00:00.123Z";
    let mut bytes = Vec::new();
    for line in [
        json!({"timestamp":timestamp,"type":"session_meta","payload":{
            "id":session,"timestamp":timestamp,"cwd":"/fixture/workspace",
            "originator":"codex_cli_rs","cli_version":ctox_core::native_harness_version(),
            "source":"exec","model_provider":"openai","base_instructions":{"text":"fixture"},
            "capability_profile":"workspace_worker"
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"user_message","message":message
        }}),
    ] {
        serde_json::to_writer(&mut bytes, &line).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

fn source_spec() -> (ExecutionSpec, Ownership) {
    (
        ExecutionSpec {
            job_id: "fixture-source-job".into(),
            session_id: "11111111-1111-1111-1111-111111111111".into(),
            scope_id: "native-test-scope".into(),
            harness: ctox_core::native_harness_name().into(),
            harness_version: ctox_core::native_harness_version().into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::from(["fixture-requirement".into()]),
        },
        Ownership {
            node_id: 1,
            generation: 1,
        },
    )
}

#[test]
fn native_source_journal_reopens_exact_bytes_without_handoff_permission_or_payload_receipt() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let bytes = journal_fixture(&spec.session_id, "private-fixture-history");
    let first = registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    let second = registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    assert_eq!(first, second);
    let wire = serde_json::to_string(&first).unwrap();
    assert!(!wire.contains("private-fixture-history"));
    assert!(!wire.contains("journal_bytes"));
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let stored: Vec<u8> = conn
        .query_row(
            "SELECT journal_bytes FROM business_native_source_journals WHERE capture_id=?1",
            [&first.capture_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, bytes);
    for table in [
        "business_session_handoff_bindings",
        "business_permission_grants",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "journal publication cannot grant {table}");
    }
}

#[test]
fn native_source_journal_rejects_truncated_foreign_and_malformed_input_before_storage() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let mut truncated = journal_fixture(&spec.session_id, "fixture");
    truncated.pop();
    for bytes in [
        truncated,
        journal_fixture("22222222-2222-2222-2222-222222222222", "fixture"),
        b"not a journal\n".to_vec(),
    ] {
        assert!(registry
            .with_policy(|tx| {
                super::super::source_journal::persist(
                    tx,
                    &assignment.destination,
                    &spec,
                    &ownership,
                    "fixture-policy",
                    &bytes,
                )
            })
            .is_err());
    }
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_native_source_journals",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn native_source_journal_never_replaces_an_existing_capture_with_changed_policy_controller_or_bytes(
) {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let bytes = journal_fixture(&spec.session_id, "original-private-fixture");
    registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    for mutation in ["policy", "controller", "payload"] {
        let mut destination = assignment.destination.clone();
        if mutation == "controller" {
            destination.controller_generation += 1;
        }
        let changed = if mutation == "payload" {
            journal_fixture(&spec.session_id, "different-private-fixture")
        } else {
            bytes.clone()
        };
        assert!(
            registry
                .with_policy(|tx| {
                    super::super::source_journal::persist(
                        tx,
                        &destination,
                        &spec,
                        &ownership,
                        if mutation == "policy" {
                            "changed-policy"
                        } else {
                            "fixture-policy"
                        },
                        &changed,
                    )
                })
                .is_err(),
            "{mutation}"
        );
    }
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let stored: Vec<u8> = conn
        .query_row(
            "SELECT journal_bytes FROM business_native_source_journals",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, bytes);
}
