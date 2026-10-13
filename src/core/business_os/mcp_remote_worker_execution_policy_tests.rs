// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

const NATIVE_SUPERVISOR: &str = "b2cd2661-181e-41fd-a54e-ea5642ccf0dc";

fn policy(root: &Path, mode: &str, revision: u64) -> anyhow::Result<()> {
    let mut project =
        store::outbound_load_record(&store::open_store(root)?, "workjet_projects", "project-1")?
            .unwrap();
    project["execution_policy"] = json!({
        "schema":"ctox.workjet.project_execution_policy.v1", "mode":mode,"revision":revision
    });
    record(root, "workjet_projects", "project-1", project)
}

fn native_supervisor(root: &Path) -> anyhow::Result<()> {
    // Isolated fixture of the existing native bind command's durable output.
    // Client metadata alone cannot create this table or provenance in product.
    let conn = store::open_store(root)?;
    conn.execute_batch(
        "CREATE TABLE workjet_supervisor_bindings (
            project_id TEXT PRIMARY KEY NOT NULL, owner_user_id TEXT NOT NULL,
            thread_id TEXT UNIQUE NOT NULL, created_at_ms INTEGER NOT NULL);",
    )?;
    conn.execute(
        "INSERT INTO workjet_supervisor_bindings VALUES (?1,?2,?3,?4)",
        params!["project-1", "owner", NATIVE_SUPERVISOR, now_ms()],
    )?;
    record(
        root,
        "user_threads",
        NATIVE_SUPERVISOR,
        json!({
            "id":NATIVE_SUPERVISOR,"thread_id":NATIVE_SUPERVISOR,"owner_user_id":"owner",
            "source_module":"ctox","source_record_type":"workjet_project","source_record_id":"project-1",
            "is_deleted":false,"metadata":{
                "workjet_supervisor_contract":"ctox.workjet.supervisor_binding.v1",
                "workjet_supervisor":{"project_id":"project-1","thread_id":NATIVE_SUPERVISOR,
                    "thread_key":format!("business-os/threads/{NATIVE_SUPERVISOR}")}
            }
        }),
    )
}

fn autonomous_binding() -> Value {
    let mut value = serde_json::to_value(binding()).unwrap();
    value["sourceSupervisorThreadId"] = json!(NATIVE_SUPERVISOR);
    value["executionPolicy"] =
        json!({"mode":"autonomous-worktree","projectId":"project-1","revision":1});
    value
}

fn autonomous_fixture() -> anyhow::Result<tempfile::TempDir> {
    let root = fixture()?;
    policy(root.path(), "autonomous_worktree", 1)?;
    native_supervisor(root.path())?;
    Ok(root)
}

fn autonomous_issue(root: &Path) -> anyhow::Result<Value> {
    call(
        root,
        "owner",
        json!({"action":"issue","binding":autonomous_binding(),"ttl_seconds":300}),
    )
}

#[test]
fn remote_worker_execution_policy_fixture_and_descriptor_are_strict() -> anyhow::Result<()> {
    use super::super::super::super::workjet_worker_execution_policy_contract::validate_fixture;
    let fixture: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json"
    ))?;
    for case in fixture["valid_cases"].as_array().unwrap() {
        validate_fixture(case["type"].as_str().unwrap(), case["value"].clone())
            .map_err(anyhow::Error::msg)?;
    }
    for case in fixture["invalid_cases"].as_array().unwrap() {
        assert!(validate_fixture(case["type"].as_str().unwrap(), case["value"].clone()).is_err());
    }
    let descriptor = serde_json::to_value(descriptor())?;
    let spec = &descriptor["inputSchema"]["properties"]["binding"]["properties"]["executionPolicy"];
    assert_eq!(spec["additionalProperties"], false);
    assert_eq!(
        spec["properties"]["mode"]["enum"],
        json!(["autonomous-worktree"])
    );
    assert_eq!(spec["properties"]["revision"]["minimum"], 1);
    assert_eq!(
        spec["properties"]["revision"]["maximum"],
        9_007_199_254_740_991_u64
    );
    Ok(())
}

#[test]
fn remote_worker_execution_policy_current_native_root_roundtrip_and_replay() -> anyhow::Result<()> {
    let root = autonomous_fixture()?;
    let issued = autonomous_issue(root.path())?;
    assert_eq!(issued["binding"], autonomous_binding());
    assert_eq!(autonomous_issue(root.path())?, issued);
    let claimed = call(
        root.path(),
        "owner",
        operation("claim", &issued, Some("execution-1")),
    )?;
    assert_eq!(claimed["binding"], issued["binding"]);
    assert_eq!(
        call(
            root.path(),
            "owner",
            operation("claim", &issued, Some("execution-1"))
        )?,
        claimed
    );
    assert_eq!(
        call(
            root.path(),
            "owner",
            operation("revalidate", &claimed, Some("execution-1"))
        )?,
        claimed
    );
    let mut renewal = operation("renew", &claimed, Some("execution-1"));
    renewal["renewal_sequence"] = json!(1);
    renewal["ttl_seconds"] = json!(300);
    let renewed = call(root.path(), "owner", renewal.clone())?;
    assert_eq!(renewed["renewalSequence"], 1);
    assert_eq!(renewed["binding"], issued["binding"]);
    assert_eq!(call(root.path(), "owner", renewal)?, renewed);
    assert_eq!(
        redact_receipt(renewed.clone())?["binding"]["executionPolicy"],
        renewed["binding"]["executionPolicy"]
    );
    let mut substituted = operation("revalidate", &renewed, Some("execution-1"));
    substituted["binding"]["executionPolicy"]["revision"] = json!(2);
    assert!(call(root.path(), "owner", substituted).is_err());
    assert_eq!(
        call(root.path(), "owner", operation("revoke", &renewed, None))?["state"],
        "revoked"
    );
    Ok(())
}

#[test]
fn remote_worker_execution_policy_absent_preserves_legacy_permit_fingerprint() -> anyhow::Result<()>
{
    let root = fixture()?;
    let issued = issue(root.path())?;
    assert!(issued["binding"].get("executionPolicy").is_none());
    policy(root.path(), "autonomous_worktree", 1)?;
    native_supervisor(root.path())?;
    // Opting in a project never silently changes an already omitted policy path.
    assert_eq!(issue(root.path())?, issued);
    let claimed = call(
        root.path(),
        "owner",
        operation("claim", &issued, Some("execution-1")),
    )?;
    assert_eq!(
        claimed["authorityFingerprint"],
        issued["authorityFingerprint"]
    );
    assert!(claimed["binding"].get("executionPolicy").is_none());
    Ok(())
}

#[test]
fn remote_worker_execution_policy_rechecks_all_authority_operations_after_reset(
) -> anyhow::Result<()> {
    for (mode, revision) in [
        ("default", 2),
        ("autonomous_worktree", 3),
        ("autonomous_worktree", 0),
    ] {
        let root = autonomous_fixture()?;
        let issued = autonomous_issue(root.path())?;
        let claimed = call(
            root.path(),
            "owner",
            operation("claim", &issued, Some("execution-1")),
        )?;
        policy(root.path(), mode, revision)?;
        assert!(autonomous_issue(root.path()).is_err());
        for action in ["claim", "revalidate", "renew"] {
            let mut args = operation(action, &claimed, Some("execution-1"));
            if action == "renew" {
                args["renewal_sequence"] = json!(1);
                args["ttl_seconds"] = json!(300);
            }
            assert!(
                call(root.path(), "owner", args).is_err(),
                "{mode}/{revision}: {action}"
            );
        }
        // Reducing authority cannot be obstructed by reset or stale generation.
        assert_eq!(
            call(root.path(), "owner", operation("revoke", &claimed, None))?["state"],
            "revoked"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_execution_policy_requires_real_native_supervisor_provenance() -> anyhow::Result<()>
{
    let root = fixture()?;
    policy(root.path(), "autonomous_worktree", 1)?;
    assert!(
        autonomous_issue(root.path()).is_err(),
        "unenrolled native root"
    );
    native_supervisor(root.path())?;
    let issued = autonomous_issue(root.path())?;
    for actor in ["foreign"] {
        assert!(call(
            root.path(),
            actor,
            json!({
                "action":"issue","binding":autonomous_binding(),"ttl_seconds":300
            })
        )
        .is_err());
    }
    let mut external_parent = autonomous_binding();
    external_parent["sourceSupervisorThreadId"] = json!("external-workjet-parent");
    assert!(
        call(
            root.path(),
            "owner",
            json!({
                "action":"issue","binding":external_parent,"ttl_seconds":300
            })
        )
        .is_err(),
        "external parent label cannot claim native enrollment"
    );

    for (key, value) in [
        ("owner_user_id", json!("foreign")),
        ("source_record_id", json!("another-project")),
        ("is_deleted", json!(true)),
    ] {
        let mut thread = store::outbound_load_record(
            &store::open_store(root.path())?,
            "user_threads",
            NATIVE_SUPERVISOR,
        )?
        .unwrap();
        let before = thread[key].clone();
        thread[key] = value;
        record(
            root.path(),
            "user_threads",
            NATIVE_SUPERVISOR,
            thread.clone(),
        )?;
        assert!(
            call(
                root.path(),
                "owner",
                operation("claim", &issued, Some("execution-1"))
            )
            .is_err(),
            "changed native provenance {key}"
        );
        thread[key] = before;
        record(root.path(), "user_threads", NATIVE_SUPERVISOR, thread)?;
    }
    store::open_store(root.path())?.execute(
        "DELETE FROM workjet_supervisor_bindings WHERE project_id=?1",
        ["project-1"],
    )?;
    assert!(call(
        root.path(),
        "owner",
        operation("claim", &issued, Some("execution-1"))
    )
    .is_err());
    assert_eq!(
        call(root.path(), "owner", operation("revoke", &issued, None))?["state"],
        "revoked"
    );
    Ok(())
}

#[test]
fn remote_worker_execution_policy_rejects_null_extra_fields_and_other_project() -> anyhow::Result<()>
{
    let root = autonomous_fixture()?;
    let fixture: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json"
    ))?;
    let mut invalid_refs: Vec<Value> = fixture["invalid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["value"].clone())
        .collect();
    invalid_refs
        .push(json!({"mode":"autonomous-worktree","projectId":"foreign-project","revision":1}));
    for reference in invalid_refs {
        let mut binding = autonomous_binding();
        binding["executionPolicy"] = reference.clone();
        assert!(
            call(
                root.path(),
                "owner",
                json!({
                    "action":"issue","binding":binding,"ttl_seconds":300
                })
            )
            .is_err(),
            "accepted {reference}"
        );
    }
    let table_created: bool = store::open_store(root.path())?.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_remote_worker_admissions')",
        [], |row| row.get(0),
    )?;
    assert!(
        !table_created,
        "invalid binding reached native permit mutation"
    );
    Ok(())
}
