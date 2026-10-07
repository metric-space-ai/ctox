// Origin: CTOX
// License: AGPL-3.0-only
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
        "hosting_mode":"workstation","capability_epoch":1,
        "capability_config":[{"kind":"build","ssh_endpoint_ref":"build-endpoint-1",
            "slots":1,"jobs":2,"lane_root":"/build-lane","disk_floor_gib":60,
            "toolchains":["rust"]}]}),
    )?;
    call(
        root.path(),
        "owner",
        json!({"action":"register_target","target":target_binding()}),
    )?;
    Ok(root)
}
fn target_binding() -> Value {
    let binding = serde_json::to_value(binding()).unwrap();
    let mut target = json!({});
    for key in [
        "sourceEnvironmentId",
        "targetEnvironmentId",
        "targetConnectionId",
        "targetInstanceId",
        "targetComputerId",
    ] {
        target[key] = binding[key].clone();
    }
    target
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
fn enrollment() -> Value {
    json!({"action":"enroll_target",
        "target":{"sourceEnvironmentId":"source-env","targetEnvironmentId":"new-execution-environment",
            "targetConnectionId":"connection-1","targetInstanceId":"target-instance"},
        "computer":{"displayName":"Remote build computer","hostingMode":"self_hosted",
            "buildCapability":{"ssh_endpoint_ref":"native-build-endpoint","slots":1,"jobs":2,
                "lane_root":"/build-lane","disk_floor_gib":60,"toolchains":["rust"]}}})
}
fn computer_count(root: &Path) -> anyhow::Result<usize> {
    Ok(store::outbound_load_records_by_string_field(
        &store::open_store(root)?,
        "workjet_computers",
        "owner_user_id",
        "owner",
    )?
    .len())
}

#[test]
fn remote_worker_target_enrollment_issues_one_real_native_computer_and_projection(
) -> anyhow::Result<()> {
    let root = fixture()?;
    std::fs::create_dir_all(root.path().join("runtime"))?;
    let projection = rusqlite::Connection::open(store::rxdb_store_path(root.path()))?;
    projection.execute_batch(
        "CREATE TABLE ctox_business_os__workjet_computers__v0 (
        id TEXT PRIMARY KEY NOT NULL, revision TEXT, deleted INTEGER NOT NULL DEFAULT 0,
        lastWriteTime REAL NOT NULL DEFAULT 0, data TEXT NOT NULL);",
    )?;
    drop(projection);
    let enroll = enrollment();
    let enrolled = call(root.path(), "owner", enroll.clone())?;
    let id = enrolled["target"]["targetComputerId"].as_str().unwrap();
    uuid::Uuid::parse_str(id)?;
    assert_ne!(id, "new-execution-environment");
    assert_eq!(
        call(root.path(), "owner", enroll.clone())?,
        enrolled,
        "lost enrollment ACK cannot create another native computer"
    );
    let persisted =
        store::outbound_load_record(&store::open_store(root.path())?, "workjet_computers", id)?
            .unwrap();
    assert_eq!(persisted["owner_user_id"], "owner");
    assert_eq!(persisted["status"], "assigned");
    assert_eq!(persisted["capability_config"][0]["kind"], "build");
    let projected =
        store::load_rxdb_collection_record(root.path(), "workjet_computers", id)?.unwrap();
    assert_eq!(projected["owner_user_id"], "owner");
    assert!(
        projected.get("capability_config").is_none(),
        "operational config stays native"
    );
    assert_eq!(computer_count(root.path())?, 2);
    for pointer in [
        "/computer/displayName",
        "/computer/buildCapability/jobs",
        "/target/targetConnectionId",
    ] {
        let mut changed = enroll.clone();
        *changed.pointer_mut(pointer).unwrap() = if pointer.ends_with("jobs") {
            json!(3)
        } else {
            json!("changed-intent")
        };
        assert!(
            call(root.path(), "owner", changed).is_err(),
            "changed enrollment {pointer} accepted"
        );
    }
    let mut request_binding = serde_json::to_value(binding())?;
    request_binding["targetEnvironmentId"] = enrolled["target"]["targetEnvironmentId"].clone();
    request_binding["targetComputerId"] = json!(id);
    assert!(call(
        root.path(),
        "owner",
        json!({"action":"issue","binding":request_binding,"ttl_seconds":300})
    )
    .is_ok());
    Ok(())
}

#[test]
fn remote_worker_invalid_or_revoked_enrollment_never_creates_another_assignment(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let enroll = enrollment();
    for pointer in [
        "/computer/hostingMode",
        "/computer/buildCapability/slots",
        "/target/sourceEnvironmentId",
    ] {
        let mut invalid = enroll.clone();
        *invalid.pointer_mut(pointer).unwrap() = match pointer {
            "/computer/hostingMode" => json!("managed_backend"),
            "/computer/buildCapability/slots" => json!(0),
            _ => json!("new-execution-environment"),
        };
        assert!(call(root.path(), "owner", invalid).is_err());
    }
    assert_eq!(
        computer_count(root.path())?,
        1,
        "rejected enrollment leaves no assigned computer"
    );
    let enrolled = call(root.path(), "owner", enroll.clone())?;
    let revoked = call(
        root.path(),
        "owner",
        json!({"action":"revoke_target","target_environment_id":"new-execution-environment",
            "expected_revision":enrolled["revision"]}),
    )?;
    assert_eq!(revoked["state"], "revoked");
    assert!(
        call(root.path(), "owner", enroll).is_err(),
        "a replay cannot reactivate or mint another computer after unpair"
    );
    assert_eq!(computer_count(root.path())?, 2);
    Ok(())
}

#[test]
fn remote_worker_target_resolution_is_explicit_native_owner_and_source_scoped() -> anyhow::Result<()>
{
    let root = fixture()?;
    let resolve = json!({"action":"resolve_target","target_environment_id":"target-env"});
    let resolved = call(root.path(), "owner", resolve.clone())?;
    assert_eq!(resolved["contract"], "ctox.workjet.remote-worker-target.v1");
    assert_eq!(resolved["target"], target_binding());
    assert_eq!(resolved["buildCapability"]["kind"], "build");
    assert_eq!(resolved["revision"], 1);
    assert_eq!(
        call(
            root.path(),
            "owner",
            json!({"action":"register_target","target":target_binding()})
        )?,
        resolved
    );
    assert!(call(root.path(), "foreign", resolve.clone()).is_err());
    let mut foreign_source = gateway("owner");
    foreign_source["workspace"] = json!("other-source-instance");
    assert!(
        super::super::call_tool_inner(root.path(), TOOL, resolve, Some(&foreign_source)).is_err()
    );
    for key in [
        "sourceEnvironmentId",
        "targetEnvironmentId",
        "targetConnectionId",
        "targetInstanceId",
        "targetComputerId",
    ] {
        let mut request_binding = serde_json::to_value(binding())?;
        request_binding[key] = json!("different-native-binding");
        assert!(
            call(
                root.path(),
                "owner",
                json!({"action":"issue","binding":request_binding,"ttl_seconds":300})
            )
            .is_err(),
            "unregistered target tuple field {key} was admitted"
        );
    }
    let mut unregistered = target_binding();
    unregistered["targetComputerId"] = json!("connection-target-env");
    assert!(
        call(
            root.path(),
            "owner",
            json!({"action":"register_target","target":unregistered})
        )
        .is_err(),
        "a presentation computer identifier cannot create native assignment"
    );
    Ok(())
}

#[test]
fn remote_worker_target_replacement_and_revocation_fence_existing_execution() -> anyhow::Result<()>
{
    let root = fixture()?;
    let permit = issue(root.path())?;
    call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1")),
    )?;
    let mut replacement = target_binding();
    replacement["targetConnectionId"] = json!("new-connection");
    let replace = json!({"action":"register_target","target":replacement,"expected_revision":1});
    let registered = call(root.path(), "owner", replace.clone())?;
    assert_eq!(registered["revision"], 2);
    assert_eq!(
        call(root.path(), "owner", replace)?,
        registered,
        "lost registration ACK is idempotent"
    );
    assert!(call(
        root.path(),
        "owner",
        operation("revalidate", &permit, Some("execution-1"))
    )
    .is_err());
    let mut renewal = operation("renew", &permit, Some("execution-1"));
    renewal["renewal_sequence"] = json!(1);
    renewal["ttl_seconds"] = json!(300);
    assert!(call(root.path(), "owner", renewal).is_err());
    assert!(
        call(
            root.path(),
            "owner",
            json!({"action":"register_target","target":target_binding(),"expected_revision":1})
        )
        .is_err(),
        "delayed replacement cannot undo a newer registration"
    );
    let revoke = json!({"action":"revoke_target","target_environment_id":"target-env","expected_revision":2});
    let revoked = call(root.path(), "owner", revoke.clone())?;
    assert_eq!(revoked["revision"], 3);
    assert_eq!(revoked["state"], "revoked");
    assert_eq!(call(root.path(), "owner", revoke)?, revoked);
    assert!(call(
        root.path(),
        "owner",
        json!({"action":"resolve_target","target_environment_id":"target-env"})
    )
    .is_err());
    let reactivated = call(
        root.path(),
        "owner",
        json!({"action":"register_target","target":replacement,"expected_revision":3}),
    )?;
    assert_eq!(reactivated["revision"], 4);
    assert!(call(
        root.path(),
        "owner",
        operation("revalidate", &permit, Some("execution-1"))
    )
    .is_err());
    assert_eq!(
        call(root.path(), "owner", operation("revoke", &permit, None))?["state"],
        "revoked"
    );
    Ok(())
}

#[test]
fn remote_worker_target_missing_registration_and_retired_computer_fail_closed() -> anyhow::Result<()>
{
    let root = fixture()?;
    let permit = issue(root.path())?;
    store::open_store(root.path())?.execute("DELETE FROM workjet_remote_worker_targets", [])?;
    assert!(
        issue(root.path()).is_err(),
        "legacy unregistered permit cannot bypass enrollment"
    );
    assert!(call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1"))
    )
    .is_err());
    let registered = call(
        root.path(),
        "owner",
        json!({"action":"register_target","target":target_binding()}),
    )?;
    let conn = store::open_store(root.path())?;
    let mut computer =
        store::outbound_load_record(&conn, "workjet_computers", "computer-1")?.unwrap();
    computer["status"] = json!("unassigned");
    record(root.path(), "workjet_computers", "computer-1", computer)?;
    assert!(call(
        root.path(),
        "owner",
        json!({"action":"resolve_target","target_environment_id":"target-env"})
    )
    .is_err());
    assert!(call(
        root.path(),
        "owner",
        json!({"action":"register_target","target":target_binding()})
    )
    .is_err());
    let revoke = json!({"action":"revoke_target","target_environment_id":"target-env",
        "expected_revision":registered["revision"]});
    assert!(call(root.path(), "foreign", revoke.clone()).is_err());
    assert_eq!(call(root.path(), "owner", revoke)?["state"], "revoked");
    Ok(())
}

#[test]
fn remote_worker_requires_typed_build_capability_and_fences_its_removal() -> anyhow::Result<()> {
    let storage = json!([{"kind":"storage","endpoint_ref":"storage-endpoint-1",
        "protocol":"ssh","root":"/artifacts","quota_gib":null,"purposes":["artifacts"]}]);
    let invalid_build = json!([{"kind":"build","ssh_endpoint_ref":"build-endpoint-1",
        "slots":0,"jobs":2,"lane_root":"/build-lane","disk_floor_gib":60,
        "toolchains":["rust"]}]);
    for config in [Value::Null, json!([]), storage, invalid_build] {
        let root = fixture()?;
        let permit = issue(root.path())?;
        let claimed = call(
            root.path(),
            "owner",
            operation("claim", &permit, Some("execution-1")),
        )?;
        let conn = store::open_store(root.path())?;
        let mut computer =
            store::outbound_load_record(&conn, "workjet_computers", "computer-1")?.unwrap();
        // Presentation chips cannot replace a removed/malformed native grant.
        computer["capabilities"] = json!(["build", "codex"]);
        computer["capability_config"] = config;
        record(root.path(), "workjet_computers", "computer-1", computer)?;
        assert!(issue(root.path()).is_err());
        assert!(call(
            root.path(),
            "owner",
            operation("claim", &permit, Some("execution-1"))
        )
        .is_err());
        assert!(call(
            root.path(),
            "owner",
            operation("revalidate", &claimed, Some("execution-1"))
        )
        .is_err());
        let mut renew = operation("renew", &claimed, Some("execution-1"));
        renew["renewal_sequence"] = json!(1);
        renew["ttl_seconds"] = json!(300);
        assert!(call(root.path(), "owner", renew).is_err());
        // Reducing authority still works after the build grant disappears.
        assert_eq!(
            call(root.path(), "owner", operation("revoke", &claimed, None))?["state"],
            "revoked"
        );
    }
    Ok(())
}

#[test]
fn remote_worker_receipt_preserves_only_typed_account_reference() -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    assert_eq!(permit["binding"], serde_json::to_value(binding())?);
    // Other MCP tools/records do not gain an exemption by mimicking our schema.
    let generic = redact_mcp_response(json!({
        "credentialRef": {"environmentId":"source-env","accountId":"account-1"},
        "password":"test-secret", "nested":{"token":"test-token"}
    }));
    assert_eq!(generic["credentialRef"], REDACTED_MCP_VALUE);
    assert_eq!(generic["password"], REDACTED_MCP_VALUE);
    assert_eq!(generic["nested"]["token"], REDACTED_MCP_VALUE);
    for pointer in ["/binding/credentialRef", "/binding", ""] {
        let mut injected = permit.clone();
        let object = injected
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap();
        object.insert("secret".to_owned(), json!("test-secret"));
        assert!(
            redact_receipt(injected).is_err(),
            "unknown secret field {pointer}"
        );
    }
    let target = redact_receipt(json!({
        "contract": target::CONTRACT, "credentialRef":{"accountId":"test-secret"},
        "target":{"targetComputerId":"computer-1"}, "token":"test-token"
    }))?;
    assert_eq!(target["credentialRef"], REDACTED_MCP_VALUE);
    assert_eq!(target["token"], REDACTED_MCP_VALUE);
    assert_eq!(target["target"]["targetComputerId"], "computer-1");
    let mut wrong_contract = permit;
    wrong_contract["contract"] = json!("untrusted");
    assert!(redact_receipt(wrong_contract).is_err());
    Ok(())
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

fn renewal(permit: &Value, execution: &str, sequence: u64) -> Value {
    let mut request = operation("renew", permit, Some(execution));
    request["renewal_sequence"] = json!(sequence);
    request["ttl_seconds"] = json!(300);
    request
}

#[test]
fn remote_worker_renewal_keeps_one_execution_and_lost_ack_does_not_extend_again(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1")),
    )?;
    let mut short = permit.clone();
    short["state"] = json!("claimed");
    short["executionId"] = json!("execution-1");
    short["expiresAtMs"] = json!(now_ms() + 60_000);
    store::open_store(root.path())?.execute(
        "UPDATE workjet_remote_worker_admissions SET receipt_json=?1",
        params![serde_json::to_string(&short)?],
    )?;
    assert!(call(
        root.path(),
        "owner",
        renewal(&permit, "foreign-execution", 1)
    )
    .is_err());
    assert!(call(root.path(), "owner", renewal(&permit, "execution-1", 2)).is_err());
    let renewed = call(root.path(), "owner", renewal(&permit, "execution-1", 1))?;
    assert_eq!(renewed["permitId"], permit["permitId"]);
    assert_eq!(renewed["executionId"], "execution-1");
    assert_eq!(renewed["renewalSequence"], 1);
    assert!(renewed["expiresAtMs"].as_i64().unwrap() > short["expiresAtMs"].as_i64().unwrap());
    assert_eq!(
        call(root.path(), "owner", renewal(&permit, "execution-1", 1))?,
        renewed,
        "duplicate renewal must return its existing deadline"
    );
    let second = call(root.path(), "owner", renewal(&permit, "execution-1", 2))?;
    assert_eq!(second["renewalSequence"], 2);
    assert!(call(root.path(), "owner", renewal(&permit, "execution-1", 1)).is_err());
    assert!(call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-2"))
    )
    .is_err());
    store::open_store(root.path())?.execute(
        "UPDATE business_users SET capability_epoch=capability_epoch+1 WHERE user_id='owner'",
        [],
    )?;
    let denied = call(root.path(), "owner", renewal(&permit, "execution-1", 3)).unwrap_err();
    assert_eq!(
        denied.downcast_ref::<BusinessOsMcpError>().unwrap().code,
        BusinessOsMcpErrorCode::PermissionDenied
    );
    Ok(())
}

#[test]
fn remote_worker_expired_or_revoked_lease_cannot_renew_and_cancellation_is_idempotent(
) -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    let mut expired = call(
        root.path(),
        "owner",
        operation("claim", &permit, Some("execution-1")),
    )?;
    expired["expiresAtMs"] = json!(now_ms() - 1);
    store::open_store(root.path())?.execute(
        "UPDATE workjet_remote_worker_admissions SET receipt_json=?1",
        params![serde_json::to_string(&expired)?],
    )?;
    assert!(call(root.path(), "owner", renewal(&permit, "execution-1", 1)).is_err());
    record(
        root.path(),
        "workjet_computers",
        "computer-1",
        json!({"id":"computer-1",
        "owner_user_id":"owner","status":"unassigned","hosting_mode":"workstation"}),
    )?;
    let revoked = call(root.path(), "owner", operation("revoke", &permit, None))?;
    assert_eq!(revoked["state"], "revoked");
    assert_eq!(
        call(root.path(), "owner", operation("revoke", &permit, None))?,
        revoked
    );
    assert!(call(root.path(), "foreign", operation("revoke", &permit, None)).is_err());
    assert!(call(root.path(), "owner", renewal(&permit, "execution-1", 1)).is_err());
    Ok(())
}

#[test]
fn remote_worker_concurrent_claims_publish_only_one_execution() -> anyhow::Result<()> {
    let root = fixture()?;
    let permit = issue(root.path())?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let tasks = ["execution-1", "execution-2"]
        .into_iter()
        .map(|id| {
            let path = root.path().to_owned();
            let args = operation("claim", &permit, Some(id));
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                call(&path, "owner", args)
            })
        })
        .collect::<Vec<_>>();
    let replies = tasks
        .into_iter()
        .map(|task| task.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(replies.iter().filter(|reply| reply.is_ok()).count(), 1);
    let accepted = replies.into_iter().find_map(Result::ok).unwrap();
    let execution = accepted["executionId"].as_str().unwrap();
    assert_eq!(
        call(
            root.path(),
            "owner",
            operation("revalidate", &permit, Some(execution))
        )?,
        accepted
    );
    Ok(())
}
