use super::*;
use serde_json::json;

fn binding() -> Binding {
    serde_json::from_value(json!({
        "requestId":"request-1","requestDigest":"a".repeat(64),
        "sourceEnvironmentId":"source-env","sourceSupervisorThreadId":"supervisor-1",
        "sourceInstanceId":"source-instance","projectId":"project-1",
        "targetEnvironmentId":"target-env","targetConnectionId":"connection-1",
        "targetInstanceId":"target-instance","targetComputerId":"computer-1",
        "repositoryUrl":"https://github.com/example/repository.git","repositoryHead":"b".repeat(40),
        "workspaceKey":"request-1",
        "credentialRef":{"environmentId":"source-env","accountId":"account-1"},
        "providerRef":{"environmentId":"source-env","provider":"provider-1"},
        "modelRef":{"environmentId":"source-env","provider":"provider-1","modelId":"model-1"},
        "capabilities":["repository_read","repository_write","run_checks","open_pull_request"]
    }))
    .unwrap()
}
fn gateway(owner: &str) -> Value {
    json!({"auth_source":"ctox_dev_managed_mcp_token","channel":"ctox_dev_managed_mcp",
        "surface":"workjet","actor":owner,"role":"chef","workspace":"source-instance"})
}
fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    store::tests::seed_business_user(root.path(), "owner", "chef")?;
    store::tests::seed_business_user(root.path(), "foreign", "chef")?;
    save_mcp_policy(root.path(), &default_mcp_policy())?;
    record(
        root.path(),
        "workjet_projects",
        "project-1",
        json!({"id":"project-1",
        "owner_user_id":"owner","status":"active","is_deleted":false,
        "repo_url":"https://github.com/example/repository"}),
    )?;
    record(
        root.path(),
        "workjet_computers",
        "computer-1",
        json!({"id":"computer-1",
        "owner_user_id":"owner","status":"assigned","is_deleted":false,"agentless":false,
        "hosting_mode":"workstation","capability_epoch":1}),
    )?;
    Ok(root)
}
fn record(root: &Path, collection: &str, id: &str, value: Value) -> anyhow::Result<()> {
    super::super::super::store_workjet_projects::persist_idempotently(
        &store::open_store(root)?,
        collection,
        id,
        now_ms(),
        value,
    )
    .map(|_| ())
}
fn call(root: &Path, actor: &str, args: Value) -> anyhow::Result<Value> {
    super::super::call_tool_inner(root, TOOL, args, Some(&gateway(actor)))
}
fn issue(root: &Path) -> anyhow::Result<Value> {
    call(
        root,
        "owner",
        json!({"action":"issue","binding":binding(),"ttl_seconds":300}),
    )
}
fn operation(action: &str, receipt: &Value, execution: Option<&str>) -> Value {
    let mut args =
        json!({"action":action,"permit_id":receipt["permitId"],"binding":receipt["binding"]});
    if let Some(execution) = execution {
        args["execution_id"] = json!(execution);
    }
    args
}

#[test]
fn remote_worker_real_mcp_issue_claim_retry_and_revalidate() -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    assert_eq!(permit["ownerUserId"], "owner");
    assert_eq!(permit["state"], "issued");
    assert_eq!(
        issue(root.path())?,
        permit,
        "lost issue ACK must not extend or duplicate authority"
    );
    assert!(call(
        root.path(),
        "owner",
        operation("revalidate", &permit, Some("execution-1"))
    )
    .is_err());
    let claim = operation("claim", &permit, Some("execution-1"));
    let claimed = call(root.path(), "owner", claim.clone())?;
    assert_eq!(
        call(root.path(), "owner", claim)?,
        claimed,
        "lost claim ACK is idempotent"
    );
    assert_eq!(
        call(
            root.path(),
            "owner",
            operation("revalidate", &permit, Some("execution-1"))
        )?,
        claimed
    );
    assert!(call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-2"))
    )
    .is_err());
    assert!(call(
        root.path(),
        "owner",
        operation("revalidate", &permit, Some("execution-2"))
    )
    .is_err());
    let conn = store::open_store(root.path())?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM workjet_remote_worker_admissions",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn remote_worker_immutable_tuple_and_foreign_owner_rejected_on_service_path() -> anyhow::Result<()>
{
    let root = fixture()?;
    let permit = issue(root.path())?;
    assert!(call(
        root.path(),
        "foreign",
        operation("claim", &permit, Some("execution-1"))
    )
    .is_err());
    let pointers = [
        "/requestId",
        "/requestDigest",
        "/sourceEnvironmentId",
        "/sourceSupervisorThreadId",
        "/sourceInstanceId",
        "/projectId",
        "/targetEnvironmentId",
        "/targetConnectionId",
        "/targetInstanceId",
        "/targetComputerId",
        "/repositoryUrl",
        "/repositoryHead",
        "/workspaceKey",
        "/credentialRef/accountId",
        "/credentialRef/environmentId",
        "/providerRef/provider",
        "/providerRef/environmentId",
        "/modelRef/modelId",
        "/modelRef/provider",
        "/modelRef/environmentId",
    ];
    for pointer in pointers {
        let mut args = operation("claim", &permit, Some("execution-1"));
        *args["binding"].pointer_mut(pointer).unwrap() = json!("changed");
        assert!(
            call(root.path(), "owner", args).is_err(),
            "tampered binding {pointer} accepted"
        );
    }
    let mut args = operation("claim", &permit, Some("execution-1"));
    args["binding"]["capabilities"] = json!(["repository_read"]);
    assert!(call(root.path(), "owner", args).is_err());
    // Failed attempts leave the single permit unclaimed.
    assert_eq!(issue(root.path())?["state"], "issued");
    Ok(())
}

#[test]
fn remote_worker_source_epoch_and_account_deactivation_fence_claims() -> anyhow::Result<()> {
    for mutation in [
        "UPDATE business_users SET active=0 WHERE user_id='owner'",
        "UPDATE business_users SET role='user' WHERE user_id='owner'",
        "UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'",
    ] {
        let root = fixture()?;
        let permit = issue(root.path())?;
        store::open_store(root.path())?.execute(mutation, [])?;
        assert!(call(
            root.path(),
            "owner",
            operation("claim", &permit, Some("execution-1"))
        )
        .is_err());
    }
    Ok(())
}

#[test]
fn remote_worker_current_computer_project_and_revoke_checked_after_claim() -> anyhow::Result<()> {
    for (collection, record_id, pointer, value) in [
        (
            "workjet_computers",
            "computer-1",
            "status",
            json!("unassigned"),
        ),
        (
            "workjet_computers",
            "computer-1",
            "owner_user_id",
            json!("foreign"),
        ),
        (
            "workjet_computers",
            "computer-1",
            "capability_epoch",
            json!(2),
        ),
        ("workjet_computers", "computer-1", "agentless", json!(true)),
        ("workjet_computers", "computer-1", "_deleted", json!(true)),
        ("workjet_projects", "project-1", "status", json!("archived")),
        (
            "workjet_projects",
            "project-1",
            "owner_user_id",
            json!("foreign"),
        ),
        (
            "workjet_projects",
            "project-1",
            "repo_url",
            json!("https://github.com/example/foreign"),
        ),
    ] {
        let root = fixture()?;
        let permit = issue(root.path())?;
        call(
            root.path(),
            "owner",
            operation("claim", &permit, Some("execution-1")),
        )?;
        let conn = store::open_store(root.path())?;
        let mut current = store::outbound_load_record(&conn, collection, record_id)?.unwrap();
        current[pointer] = value;
        drop(conn);
        record(root.path(), collection, record_id, current)?;
        assert!(
            call(
                root.path(),
                "owner",
                operation("revalidate", &permit, Some("execution-1"))
            )
            .is_err(),
            "revoked {collection}/{pointer} accepted"
        );
    }
    let root = fixture()?;
    let permit = issue(root.path())?;
    call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1")),
    )?;
    assert_eq!(
        call(root.path(), "owner", operation("revoke", &permit, None))?["state"],
        "revoked"
    );
    assert!(call(
        root.path(),
        "owner",
        operation("revalidate", &permit, Some("execution-1"))
    )
    .is_err());
    assert!(
        issue(root.path()).is_err(),
        "same request cannot be reissued after revoke"
    );
    Ok(())
}

#[test]
fn remote_worker_expiry_cannot_be_renewed_by_replaying_issue() -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    let mut expired = permit.clone();
    expired["expiresAtMs"] = json!(now_ms() - 1);
    store::open_store(root.path())?.execute(
        "UPDATE workjet_remote_worker_admissions SET receipt_json=?1",
        params![serde_json::to_string(&expired)?],
    )?;
    assert!(issue(root.path()).is_err());
    assert!(call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1"))
    )
    .is_err());
    Ok(())
}

#[test]
fn remote_worker_caller_context_and_command_session_cannot_mint_authority() -> anyhow::Result<()> {
    let root = fixture()?;
    let mut forged = json!({"action":"issue","binding":binding(),"ttl_seconds":300});
    forged["_context"] = gateway("owner");
    assert!(super::super::call_tool_inner(root.path(), TOOL, forged, None).is_err());
    let internal = json!({"auth_source":MCP_INTERNAL_SESSION_AUTH_SOURCE,"channel":"ctox_internal_business_command",
        "actor":"owner","role":"chef","workspace":"source-instance","command_id":"command-1"});
    assert!(super::super::call_tool_inner(
        root.path(),
        TOOL,
        json!({"action":"issue","binding":binding(),"ttl_seconds":300}),
        Some(&internal)
    )
    .is_err());
    let mut policy = default_mcp_policy();
    policy.allow_writes = false;
    save_mcp_policy(root.path(), &policy)?;
    assert!(
        issue(root.path()).is_err(),
        "new permit tool must be a write-policy operation"
    );
    Ok(())
}

#[test]
fn remote_worker_no_paths_secret_urls_or_cross_gateway_refs() -> anyhow::Result<()> {
    let root = fixture()?;
    for (pointer, value) in [
        ("/workspaceKey", "../escape"),
        ("/repositoryUrl", "https://secret@example.com/repo"),
        ("/repositoryUrl", "https://example.com/repo?token=secret"),
        ("/credentialRef/environmentId", "target-env"),
        ("/modelRef/provider", "foreign-provider"),
    ] {
        let mut args = json!({"action":"issue","binding":binding(),"ttl_seconds":300});
        *args["binding"].pointer_mut(pointer).unwrap() = json!(value);
        assert!(call(root.path(), "owner", args).is_err());
    }
    let mut args = json!({"action":"issue","binding":binding(),"ttl_seconds":300});
    args["binding"]["capabilities"] = json!(["merge_pull_request"]);
    assert!(call(root.path(), "owner", args).is_err());
    Ok(())
}
