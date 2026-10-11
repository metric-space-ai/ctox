// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

const PARENT: &str = "21a77758-8723-431b-bbeb-3aa9f0507c4d";

fn enrollment(root: &Path, id: &str, kind: &str, revision: u64) -> anyhow::Result<Value> {
    let result = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({
            "id":id, "module":"ctox", "command_type":kind,
            "payload":{"project_id":"project-1","supervisor_thread_id":NATIVE_SUPERVISOR,
                "thread_id":PARENT,"expected_revision":revision},
            "client_context":{"actor":{"id":"owner","role":"chef","is_admin":true}}
        }),
    )?;
    anyhow::ensure!(result["status"] == "completed", "{result}");
    Ok(result["result"]["binding"].clone())
}

fn parent_fixture(with_policy: bool) -> anyhow::Result<(tempfile::TempDir, Value)> {
    let root = autonomous_fixture()?;
    crate::business_os::store_workjet_projects::tests::create_workjet_rxdb_projection_tables(
        root.path(),
    )?;
    let reference = enrollment(
        root.path(),
        "parent-enroll",
        "ctox.workjet.project.persistent_worker.bind",
        0,
    )?;
    let mut binding = autonomous_binding();
    binding["sourceSupervisorThreadId"] = json!(PARENT);
    binding["parentTeamBinding"] = reference;
    if !with_policy {
        binding.as_object_mut().unwrap().remove("executionPolicy");
    }
    Ok((root, binding))
}

fn issue_parent(root: &Path, binding: &Value) -> anyhow::Result<Value> {
    call(
        root,
        "owner",
        json!({"action":"issue","binding":binding,"ttl_seconds":300}),
    )
}

fn parent_operation(action: &str, receipt: &Value) -> Value {
    let mut arguments = operation(action, receipt, Some("execution-1"));
    if action == "renew" {
        arguments["renewal_sequence"] = json!(1);
        arguments["ttl_seconds"] = json!(300);
    }
    arguments
}

fn assert_all_admissions_denied(root: &Path, receipt: &Value) -> anyhow::Result<()> {
    assert!(
        issue_parent(root, &receipt["binding"]).is_err(),
        "stale issue"
    );
    for action in ["claim", "revalidate", "renew"] {
        assert!(
            call(root, "owner", parent_operation(action, receipt)).is_err(),
            "{action}"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_parent_binding_preserves_actual_uuid_and_reference_across_all_phases(
) -> anyhow::Result<()> {
    for with_policy in [true, false] {
        let (root, binding) = parent_fixture(with_policy)?;
        let issued = issue_parent(root.path(), &binding)?;
        assert_eq!(issued["binding"], binding);
        assert_eq!(issued["binding"]["sourceSupervisorThreadId"], PARENT);
        assert_eq!(
            issued["binding"]["parentTeamBinding"]["supervisor_thread_id"],
            NATIVE_SUPERVISOR
        );
        assert_eq!(issue_parent(root.path(), &binding)?, issued);
        let claimed = call(root.path(), "owner", parent_operation("claim", &issued))?;
        assert_eq!(
            call(root.path(), "owner", parent_operation("claim", &issued))?,
            claimed
        );
        assert_eq!(
            call(
                root.path(),
                "owner",
                parent_operation("revalidate", &claimed)
            )?,
            claimed
        );
        let renewed = call(root.path(), "owner", parent_operation("renew", &claimed))?;
        assert_eq!(renewed["renewalSequence"], 1);
        assert_eq!(renewed["binding"], binding);
        assert_eq!(
            call(root.path(), "owner", parent_operation("renew", &claimed))?,
            renewed
        );
        assert_eq!(
            redact_receipt(renewed.clone())?["binding"]["parentTeamBinding"],
            binding["parentTeamBinding"]
        );
        assert_eq!(
            call(root.path(), "owner", operation("revoke", &renewed, None))?["state"],
            "revoked"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_parent_binding_revoke_rebind_invalidates_all_old_admissions() -> anyhow::Result<()>
{
    for with_policy in [true, false] {
        let (root, binding) = parent_fixture(with_policy)?;
        let issued = issue_parent(root.path(), &binding)?;
        let claimed = call(root.path(), "owner", parent_operation("claim", &issued))?;
        enrollment(
            root.path(),
            "parent-revoke",
            "ctox.workjet.project.persistent_worker.revoke",
            1,
        )?;
        assert_all_admissions_denied(root.path(), &claimed)?;
        let fresh = enrollment(
            root.path(),
            "parent-rebind",
            "ctox.workjet.project.persistent_worker.bind",
            2,
        )?;
        assert_eq!(fresh["revision"], 3);
        assert_all_admissions_denied(root.path(), &claimed)?;
        let mut replacement = binding.clone();
        replacement["requestId"] = json!("request-2");
        replacement["workspaceKey"] = json!("request-2");
        replacement["requestDigest"] = json!("c".repeat(64));
        replacement["parentTeamBinding"] = fresh;
        let new = issue_parent(root.path(), &replacement)?;
        assert_ne!(new["authorityFingerprint"], claimed["authorityFingerprint"]);
        assert_ne!(new["permitId"], claimed["permitId"]);
        assert_eq!(new["binding"], replacement);
        assert_eq!(
            call(root.path(), "owner", operation("revoke", &claimed, None))?["state"],
            "revoked"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_parent_binding_cannot_drop_reference_or_substitute_root_or_other_affiliation(
) -> anyhow::Result<()> {
    for with_policy in [true, false] {
        let (root, binding) = parent_fixture(with_policy)?;
        let issued = issue_parent(root.path(), &binding)?;
        let claimed = call(root.path(), "owner", parent_operation("claim", &issued))?;
        let mut omitted = binding.clone();
        omitted.as_object_mut().unwrap().remove("parentTeamBinding");
        let mut root_substitution = binding.clone();
        root_substitution["sourceSupervisorThreadId"] = json!(NATIVE_SUPERVISOR);
        let mut invalid = vec![omitted, root_substitution];
        for (field, value) in [
            ("owner_user_id", json!("foreign")),
            ("project_id", json!("other-project")),
            (
                "supervisor_thread_id",
                json!("ac45f8f3-97c8-42f8-a230-04767f2d410d"),
            ),
            ("thread_id", json!(NATIVE_SUPERVISOR)),
            ("revision", json!(2)),
        ] {
            let mut substituted = binding.clone();
            substituted["parentTeamBinding"][field] = value;
            invalid.push(substituted);
        }
        for candidate in invalid {
            assert!(
                issue_parent(root.path(), &candidate).is_err(),
                "{candidate}"
            );
            for action in ["claim", "revalidate", "renew"] {
                let mut request = parent_operation(action, &claimed);
                request["binding"] = candidate.clone();
                assert!(
                    call(root.path(), "owner", request).is_err(),
                    "{action}: {candidate}"
                );
            }
        }
        assert!(call(
            root.path(),
            "foreign",
            json!({"action":"issue","binding":binding,"ttl_seconds":300})
        )
        .is_err());
        assert_eq!(
            call(
                root.path(),
                "owner",
                parent_operation("revalidate", &claimed)
            )?,
            claimed
        );
    }
    Ok(())
}

#[test]
fn remote_worker_parent_binding_rechecks_role_history_and_root_provenance() -> anyhow::Result<()> {
    for (thread_id, pointer, value) in [
        (PARENT, "/metadata/workjet_team_role", json!("supervisor")),
        (PARENT, "/status", json!("closed")),
        (PARENT, "/archived_at_ms", json!(1)),
        (PARENT, "/is_deleted", json!(true)),
        (PARENT, "/owner_user_id", json!("foreign")),
        (PARENT, "/source_record_id", json!("other-project")),
        (NATIVE_SUPERVISOR, "/is_deleted", json!(true)),
    ] {
        let (root, binding) = parent_fixture(true)?;
        let issued = issue_parent(root.path(), &binding)?;
        let claimed = call(root.path(), "owner", parent_operation("claim", &issued))?;
        let mut history = store::outbound_load_record(
            &store::open_store(root.path())?,
            "user_threads",
            thread_id,
        )?
        .unwrap();
        *history.pointer_mut(pointer).unwrap() = value;
        record(root.path(), "user_threads", thread_id, history)?;
        assert_all_admissions_denied(root.path(), &claimed)?;
        // Retirement still uses the saved exact binding; history is never rewritten.
        assert_eq!(
            call(root.path(), "owner", operation("revoke", &claimed, None))?["state"],
            "revoked"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_parent_binding_rejects_malformed_wire_before_permit_mutation() -> anyhow::Result<()>
{
    let (root, binding) = parent_fixture(true)?;
    let corpus: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json"
    ))?;
    for case in corpus["invalid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["type"] == "WorkerParentBinding")
    {
        let mut invalid = binding.clone();
        invalid["parentTeamBinding"] = case["value"].clone();
        assert!(issue_parent(root.path(), &invalid).is_err(), "{case}");
    }
    for (field, value) in [
        ("thread_id", "00000000-0000-0000-0000-000000000000"),
        (
            "supervisor_thread_id",
            "00000000-0000-0000-0000-000000000000",
        ),
        (
            "supervisor_thread_id",
            "B2CD2661-181E-41FD-A54E-EA5642CCF0DC",
        ),
    ] {
        let mut invalid = binding.clone();
        invalid["parentTeamBinding"][field] = json!(value);
        assert!(issue_parent(root.path(), &invalid).is_err());
    }
    let table_created: bool = store::open_store(root.path())?.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_remote_worker_admissions')", [], |row| row.get(0),
    )?;
    assert!(
        !table_created,
        "invalid Parent reached native permit mutation"
    );
    let descriptor = serde_json::to_value(descriptor())?;
    let schema =
        &descriptor["inputSchema"]["properties"]["binding"]["properties"]["parentTeamBinding"];
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"].as_array().unwrap().len(), 8);
    assert_eq!(schema["properties"]["revision"]["minimum"], 1);
    assert_eq!(
        schema["properties"]["role"]["enum"],
        json!(["persistent_worker"])
    );
    Ok(())
}
