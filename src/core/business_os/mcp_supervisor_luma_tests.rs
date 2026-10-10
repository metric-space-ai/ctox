// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
pub(super) const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
// This is a real model ID from the authenticated Claude GET /models receipt
// g3-claude-live-models-20261009.json. The following DB observations are fixtures,
// never evidence of a real holder execution.
const MODEL: &str = "claude-opus-5-5";

pub(super) fn fixture(selected: bool) -> anyhow::Result<(tempfile::TempDir, String)> {
    let (root, trusted) = workjet_worker_dispatch::meeting_test_fixture()?;
    if selected {
        // The native binding command publishes its domain effect into the
        // generated projection collection, just as an installed tenant does.
        let schemas: Value =
            serde_json::from_str(include_str!("business_os_schema_contract.json"))?;
        let version = schemas["workjet_worker_profile_bindings"]["version"]
            .as_u64()
            .context("binding projection schema")?;
        Connection::open(store::rxdb_store_path(root.path()))?.execute_batch(&format!(
            "CREATE TABLE ctox_business_os__workjet_worker_profile_bindings__v{version}
            (id TEXT PRIMARY KEY NOT NULL,revision TEXT,deleted INTEGER NOT NULL DEFAULT 0,
             lastWriteTime REAL NOT NULL DEFAULT 0,data TEXT NOT NULL);"
        ))?;
        let policy = store::open_store(root.path())?;
        store::upsert_business_record(
            &policy,
            "workjet_computers",
            "network-computer",
            1,
            json!({"id":"network-computer","owner_user_id":"owner","status":"assigned","hosting_mode":"workstation","is_deleted":false}),
        )?;
        let bound = crate::business_os::command_plane::accept_rxdb_business_command(
            root.path(),
            json!({"id":"bind-luma","module":"ctox","command_type":"ctox.workjet.worker_profile.bind",
            "payload":{"worker_profile_id":"project-luma","computer_id":"network-computer"},
            "client_context":{"actor":{"id":"owner","role":"chef"}}}),
        )?;
        anyhow::ensure!(bound["status"] == "completed", "{bound}");
        let selected = crate::business_os::command_plane::accept_rxdb_business_command(
            root.path(),
            json!({"id":"select-luma","module":"ctox","command_type":"ctox.workjet.project.upsert",
            "payload":{"project_id":"project","name":"Project","supervisor_luma_id":"project-luma"},
            "client_context":{"actor":{"id":"owner","role":"chef"}}}),
        )?;
        anyhow::ensure!(selected["status"] == "completed", "{selected}");
        configure(root.path(), configuration())?;
        policy.execute_batch(provider_federation::SCHEMA)?;
        let now = now_ms();
        policy.execute("INSERT INTO business_provider_federation_policy(owner_user_id,revision) VALUES ('owner',1)", [])?;
        policy.execute("INSERT INTO business_provider_federation_accounts
            (account_id,owner_user_id,holder_instance_id,provider,private_local_account_id,enabled,credential_ready,revision,observed_at_ms)
            VALUES ('native-account','owner','native-holder','claude','private-selector-not-exported',1,1,1,?1)", [now])?;
        policy.execute("INSERT INTO business_provider_federation_model_observations
            (account_id,account_revision,last_success_at_ms,models_json,last_attempt_json) VALUES ('native-account',1,?1,?2,?3)",
            params![now,json!([MODEL]).to_string(),json!({"checkedAtMs":now,"success":true,"httpStatus":200}).to_string()])?;
        policy.execute("INSERT INTO business_provider_federation_models(owner_user_id,provider,models_json) VALUES ('owner','claude',?1)",
            [json!([MODEL]).to_string()])?;
    }
    let id = required_arg(&trusted, "command_id")?;
    let command = crate::channels::business_command_projection(root.path(), &id)?;
    let token = issue_internal_command_session_token(
        root.path(),
        &id,
        command["payload_hash"].as_str().context("hash")?,
        "owner",
        "chef",
        "native-job-workspace",
        &json!({}),
    )?;
    let token = restrict_internal_command_session_to_workjet_supervisor(root.path(), &token)?;
    Ok((root, token))
}
fn configuration() -> Value {
    json!({"workerProfiles":[{"id":"project-luma","computerId":"network-computer","harness":"claude-code","llmRouteId":"project-route","modelId":MODEL}],
        "llmRoutes":[{"id":"project-route","gatewayAccountId":"holder-local-workjet-id",
            "nativeAccountReference":{"accountId":"native-account","holderInstanceId":"native-holder","accountRevision":1}}]})
}
fn configure(root: &Path, configuration: Value) -> anyhow::Result<()> {
    store::upsert_business_record(
        &store::open_store(root)?,
        "workjet_luma_configuration",
        "instance",
        1,
        json!({"id":"instance","schema_version":1,"revision":1,"configuration":configuration}),
    )?;
    Ok(())
}
fn code(error: anyhow::Error) -> &'static str {
    error
        .downcast_ref::<SupervisorLumaUnavailable>()
        .expect("typed unavailable")
        .code
}
fn routes(root: &Path) -> anyhow::Result<i64> {
    let core = Connection::open(crate::paths::core_db(root))?;
    let exists: bool = core.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_route_attempts')", [], |r| r.get(0))?;
    if !exists {
        return Ok(0);
    }
    Ok(core.query_row(
        "SELECT count(*) FROM workjet_supervisor_route_attempts",
        [],
        |r| r.get(0),
    )?)
}
#[test]
fn absent_selection_keeps_default_even_without_valid_luma_configuration() -> anyhow::Result<()> {
    let (root, token) = fixture(false)?;
    configure(
        root.path(),
        json!({"workerProfiles":"invalid","llmRoutes":null}),
    )?;
    require_executor(root.path(), Some(&token))?;
    require_executor(root.path(), None)?;
    assert_eq!(routes(root.path())?, 0);
    Ok(())
}
#[test]
fn selected_route_is_requested_evidence_and_never_fake_claude_execution() -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    for _ in 0..2 {
        assert_eq!(
            code(require_executor(root.path(), Some(&token)).unwrap_err()),
            "claude_code_holding_executor_unavailable"
        );
    }
    assert_eq!(routes(root.path())?, 1);
    let (requested, actual): (String, Option<String>) =
        Connection::open(crate::paths::core_db(root.path()))?.query_row(
            "SELECT requested_json,actual_json FROM workjet_supervisor_route_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
    let requested: Value = serde_json::from_str(&requested)?;
    assert_eq!(requested["model"], MODEL);
    assert_eq!(requested["harness"], "claude-code");
    assert_eq!(requested["route_id"], "project-route");
    assert_eq!(requested["computer_id"], "network-computer");
    assert_eq!(requested["supervisor_thread_id"], THREAD);
    assert!(actual.is_none());
    assert!(!requested
        .to_string()
        .contains("private-selector-not-exported"));
    assert!(!requested.to_string().contains("holder-local-workjet-id"));
    Ok(())
}
#[test]
fn legacy_workjet_account_id_does_not_become_a_native_account() -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    let mut config = configuration();
    config["llmRoutes"][0]
        .as_object_mut()
        .unwrap()
        .remove("nativeAccountReference");
    configure(root.path(), config)?;
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "missing_native_account_binding"
    );
    assert_eq!(routes(root.path())?, 0);
    Ok(())
}
#[test]
fn ambiguous_profile_and_route_are_rejected_without_attempt_evidence() -> anyhow::Result<()> {
    for list in ["workerProfiles", "llmRoutes"] {
        let (root, token) = fixture(true)?;
        let mut config = configuration();
        let entry = config[list][0].clone();
        config[list].as_array_mut().unwrap().push(entry);
        configure(root.path(), config)?;
        assert_eq!(
            code(require_executor(root.path(), Some(&token)).unwrap_err()),
            "ambiguous_supervisor_luma_reference"
        );
        assert_eq!(routes(root.path())?, 0);
    }
    Ok(())
}
#[test]
fn current_account_owner_revision_holder_and_model_policy_are_required() -> anyhow::Result<()> {
    for sql in [
        "UPDATE business_provider_federation_accounts SET owner_user_id='foreign'",
        "UPDATE business_provider_federation_accounts SET revision=2",
        "UPDATE business_provider_federation_accounts SET holder_instance_id='other-holder'",
        "UPDATE business_provider_federation_accounts SET enabled=0",
        "UPDATE business_provider_federation_models SET models_json='[]'",
    ] {
        let (root, token) = fixture(true)?;
        store::open_store(root.path())?.execute_batch(sql)?;
        let error = require_executor(root.path(), Some(&token)).unwrap_err();
        assert!(matches!(
            code(error),
            "supervisor_account_model_unavailable" | "supervisor_account_holder_changed"
        ));
        assert_eq!(routes(root.path())?, 0);
    }
    Ok(())
}
#[test]
fn replaced_native_lease_or_project_owner_cannot_capture_a_selection() -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    Connection::open(crate::paths::core_db(root.path()))?.execute(
        "UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'", [])?;
    assert!(require_executor(root.path(), Some(&token)).is_err());
    assert_eq!(routes(root.path())?, 0);
    let (root, token) = fixture(true)?;
    let policy = store::open_store(root.path())?;
    let mut project = store::outbound_load_record(&policy, "workjet_projects", "project")?.unwrap();
    project["owner_user_id"] = json!("foreign");
    store::upsert_business_record(&policy, "workjet_projects", "project", 2, project)?;
    assert!(require_executor(root.path(), Some(&token)).is_err());
    assert_eq!(routes(root.path())?, 0);
    Ok(())
}

#[test]
fn same_lease_cannot_rewrite_its_requested_route() -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "claude_code_holding_executor_unavailable"
    );
    let policy = store::open_store(root.path())?;
    let mut record =
        store::outbound_load_record(&policy, "workjet_luma_configuration", "instance")?.unwrap();
    record["revision"] = json!(2);
    store::upsert_business_record(&policy, "workjet_luma_configuration", "instance", 2, record)?;
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "supervisor_selection_changed_during_lease"
    );
    assert_eq!(routes(root.path())?, 1);
    Ok(())
}

#[test]
fn withdrawn_or_foreign_computer_and_stale_catalog_cannot_capture() -> anyhow::Result<()> {
    for sql in [
        "UPDATE business_provider_federation_model_observations SET last_success_at_ms=0",
        "UPDATE business_provider_federation_model_observations SET last_attempt_json='{\"success\":false}'",
    ] {
        let (root, token) = fixture(true)?;
        store::open_store(root.path())?.execute_batch(sql)?;
        assert_eq!(code(require_executor(root.path(), Some(&token)).unwrap_err()), "supervisor_account_model_unavailable");
        assert_eq!(routes(root.path())?, 0);
    }
    for (key, value) in [("owner_user_id", "foreign"), ("status", "unassigned")] {
        let (root, token) = fixture(true)?;
        let policy = store::open_store(root.path())?;
        let mut computer =
            store::outbound_load_record(&policy, "workjet_computers", "network-computer")?.unwrap();
        computer[key] = json!(value);
        store::upsert_business_record(
            &policy,
            "workjet_computers",
            "network-computer",
            2,
            computer,
        )?;
        assert!(require_executor(root.path(), Some(&token)).is_err());
        assert_eq!(routes(root.path())?, 0);
    }
    Ok(())
}

fn owner_route_read(
    root: &Path,
    operation: &str,
    owner: &str,
    project: &str,
    thread: &str,
) -> anyhow::Result<Value> {
    crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":operation,"module":"ctox",
        "command_type":"ctox.workjet.project.supervisor.route.read.v1",
        "payload":{"project_id":project,"thread_id":thread},
        "client_context":{"actor":{"id":owner,"role":"chef","is_admin":true}}}),
    )
}

#[test]
fn configured_route_capabilities_are_separate_and_owner_bound() -> anyhow::Result<()> {
    let (root, _) = fixture(false)?;
    for (actor, project, thread, status) in [
        ("owner", "project", THREAD, "completed"),
        ("owner", "foreign", THREAD, "failed"),
        ("foreign", "project", THREAD, "failed"),
        ("owner", "project", "another-thread", "failed"),
    ] {
        let response = crate::business_os::command_plane::accept_rxdb_business_command(
            root.path(),
            json!({"id":format!("route-cap-{actor}-{project}-{thread}"),"module":"ctox",
            "command_type":"ctox.workjet.project.supervisor.route.capabilities.v1",
            "payload":{"project_id":project,"thread_id":thread},
            "client_context":{"actor":{"id":actor,"role":"chef"}}}),
        );
        if status != "completed" {
            assert!(
                response.is_err()
                    || response
                        .as_ref()
                        .is_ok_and(|value| value["status"] == "failed"),
                "{response:?}"
            );
            continue;
        }
        let response = response?;
        assert_eq!(response["status"], status, "{response}");
        if status == "completed" {
            assert_eq!(
                response["result"]["read_command"],
                "ctox.workjet.project.supervisor.route.read.v1"
            );
            assert_eq!(response["result"]["status"], "completed");
            assert_eq!(response["result"]["task_status"], "completed");
            assert_eq!(response["result"].as_object().unwrap().len(), 7);
        }
    }
    assert_eq!(routes(root.path())?, 0);
    Ok(())
}

#[test]
fn configured_route_read_preserves_default_and_legacy_capabilities_without_creating_attempts(
) -> anyhow::Result<()> {
    let (root, _) = fixture(false)?;
    configure(
        root.path(),
        json!({"workerProfiles":false,"llmRoutes":false}),
    )?;
    let response = owner_route_read(root.path(), "read-default", "owner", "project", THREAD)?;
    assert_eq!(response["status"], "completed");
    let result = &response["result"];
    assert_eq!(result["schema"], "ctox.workjet.supervisor.route-display.v1");
    assert_eq!(result["configured"], Value::Null);
    assert_eq!(result["actual"], Value::Null);
    assert_eq!(result["source"], Value::Null);
    assert_eq!(routes(root.path())?, 0);
    let capabilities = crate::business_os::command_plane::accept_rxdb_business_command(
        root.path(),
        json!({"id":"legacy-capabilities","module":"ctox",
        "command_type":"ctox.workjet.project.supervisor.turn.capabilities",
        "payload":{"project_id":"project","thread_id":THREAD},
        "client_context":{"actor":{"id":"owner","role":"chef"}}}),
    )?;
    assert_eq!(capabilities["status"], "completed");
    let keys: std::collections::BTreeSet<_> = capabilities["result"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "ok",
            "contract",
            "binding",
            "turn_kinds",
            "default_turn_kind",
            "status",
            "task_status"
        ]
        .into_iter()
        .collect()
    );
    Ok(())
}

#[test]
fn configured_route_read_reveals_requested_facts_but_never_private_accounts_or_claimed_actual_execution(
) -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "claude_code_holding_executor_unavailable"
    );
    let core = Connection::open(crate::paths::core_db(root.path()))?;
    // A mere private column, even if non-null, is not a verified producer receipt.
    core.execute(
        "UPDATE workjet_supervisor_route_attempts SET actual_json=?1",
        [json!({"model": MODEL, "unverified_claim": true}).to_string()],
    )?;
    let response = owner_route_read(root.path(), "read-request", "owner", "project", THREAD)?;
    assert_eq!(response["status"], "completed", "{response}");
    let result = &response["result"];
    assert_eq!(result["configured"]["model"], MODEL);
    assert_eq!(result["configured"]["harness"], "claude-code");
    assert_eq!(result["actual"], Value::Null);
    assert_eq!(
        result["source"]["error_code"],
        "claude_code_holding_executor_unavailable"
    );
    assert_eq!(
        result["source"]["request_revision"].as_str().unwrap().len(),
        64
    );
    for secret in [
        "native-account",
        "native-holder",
        "holder-local-workjet-id",
        "private-selector-not-exported",
        "unverified_claim",
    ] {
        assert!(!result.to_string().contains(secret), "{secret}");
    }
    let policy = store::open_store(root.path())?;
    let mut record =
        store::outbound_load_record(&policy, "workjet_luma_configuration", "instance")?.unwrap();
    record["revision"] = json!(2);
    store::upsert_business_record(&policy, "workjet_luma_configuration", "instance", 2, record)?;
    let changed = owner_route_read(root.path(), "read-changed", "owner", "project", THREAD)?;
    assert_eq!(changed["status"], "completed", "{changed}");
    assert_eq!(changed["result"]["configured"]["configuration_revision"], 2);
    assert_eq!(changed["result"]["source"], Value::Null);
    Ok(())
}

#[test]
fn configured_route_read_uses_a_read_snapshot_beside_a_core_writer_and_never_repairs_schema(
) -> anyhow::Result<()> {
    let (root, _) = fixture(true)?;
    let mut writer = Connection::open(crate::paths::core_db(root.path()))?;
    writer.execute_batch("PRAGMA journal_mode=WAL;")?;
    let writer = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let response = read_configured_route(root.path(), "owner", "project", THREAD)?;
    assert_eq!(response["configured"]["model"], MODEL);
    assert_eq!(response["source"], Value::Null);
    assert_eq!(routes(root.path())?, 0);
    writer.rollback()?;
    Ok(())
}

#[test]
fn configured_route_read_refuses_foreign_owner_project_thread_and_stale_model_authority(
) -> anyhow::Result<()> {
    for (actor, project, thread) in [
        ("foreign", "project", THREAD),
        ("owner", "foreign", THREAD),
        ("owner", "project", "b7b5c13e-aa42-453b-a012-24b96a036033"),
    ] {
        let (root, _) = fixture(true)?;
        let response = owner_route_read(root.path(), "read-invalid", actor, project, thread);
        assert!(
            response.is_err()
                || response
                    .as_ref()
                    .is_ok_and(|r| r["status"] == "failed" || r["ok"] == false)
        );
    }
    let (root, _) = fixture(true)?;
    store::open_store(root.path())?.execute_batch(
        "UPDATE business_provider_federation_model_observations SET last_success_at_ms=0",
    )?;
    let error = owner_route_read(root.path(), "read-stale", "owner", "project", THREAD)
        .expect_err("a stale account is not valid route authority");
    let message = error.to_string();
    assert_eq!(
        message,
        "supervisor_account_model_unavailable: configured project Supervisor route is unavailable"
    );
    assert!(!message.contains("native-account"));
    assert!(!message.contains("private-selector-not-exported"));
    assert_eq!(routes(root.path())?, 0);
    Ok(())
}

#[test]
fn configured_route_display_native_fixture_corpus_rejects_private_and_unbound_claims(
) -> anyhow::Result<()> {
    let spec: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json"
    ))?;
    for (key, expected) in [("valid_cases", true), ("invalid_cases", false)] {
        for case in spec[key].as_array().unwrap() {
            assert_eq!(
                super::super::super::workjet_supervisor_route_display_contract::validate_fixture(
                    case["type"].as_str().unwrap(),
                    case["value"].clone()
                )
                .is_ok(),
                expected,
                "{case}"
            );
        }
    }
    Ok(())
}

#[test]
fn computed_route_v2_fixture_and_owner_commands_are_additive_reads() -> anyhow::Result<()> {
    let spec: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-supervisor-route-computation-v2.json"
    ))?;
    for (key, expected) in [("valid_cases", true), ("invalid_cases", false)] {
        for sample in spec[key].as_array().unwrap() {
            assert_eq!(super::super::super::workjet_supervisor_route_computation_contract::validate_fixture(
                sample["type"].as_str().unwrap(),sample["value"].clone()).is_ok(),expected,"{sample}");
        }
    }
    let (root, _) = fixture(true)?;
    for (i, command) in [
        "ctox.workjet.project.supervisor.route.read.v2",
        "ctox.workjet.project.supervisor.route.capabilities.v2",
    ]
    .iter()
    .enumerate()
    {
        for (owner, expected) in [("owner", "completed"), ("foreign", "failed")] {
            let result = crate::business_os::command_plane::accept_rxdb_business_command(
                root.path(),
                json!({"id":format!("v2-{i}-{owner}"),"module":"ctox","command_type":command,
                  "payload":{"project_id":"project","thread_id":THREAD},
                  "client_context":{"actor":{"id":owner,"role":"chef","is_admin":true}}}),
            )?;
            assert_eq!(result["status"], expected, "{result}");
            if owner == "owner" && i == 0 {
                assert!(
                    result["result"]["actual"].is_null(),
                    "selection is not computation"
                );
                assert_eq!(result["result"]["configured"]["luma_id"], "project-luma");
            }
        }
    }
    Ok(())
}

#[test]
fn clearing_a_sealed_luma_never_falls_back_in_the_same_lease() -> anyhow::Result<()> {
    let (root, token) = fixture(true)?;
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "claude_code_holding_executor_unavailable"
    );
    let cleared = crate::business_os::command_plane::accept_rxdb_business_command(
        root.path(),
        json!({"id":"clear-luma","module":"ctox","command_type":"ctox.workjet.project.upsert",
            "payload":{"project_id":"project","name":"Project","supervisor_luma_id":null},
            "client_context":{"actor":{"id":"owner","role":"chef"}}}),
    )?;
    anyhow::ensure!(cleared["status"] == "completed", "{cleared}");
    assert_eq!(
        code(require_executor(root.path(), Some(&token)).unwrap_err()),
        "supervisor_selection_changed_during_lease"
    );
    assert_eq!(routes(root.path())?, 1);

    // A new actual native lease may use the newly selected instance default.
    // Fixture-only replacement; production never receives caller lease fields.
    let trusted = verify_internal_command_session_token(root.path(), &token)?;
    let command_id = required_arg(&trusted, "command_id")?;
    Connection::open(crate::paths::core_db(root.path()))?.execute(
        "UPDATE communication_routing_state SET lease_worker_id='replacement-native-worker'
         WHERE message_key IN (SELECT task_id FROM business_command_task_links WHERE command_id=?1)",
        [&command_id],
    )?;
    let command = crate::channels::business_command_projection(root.path(), &command_id)?;
    let replacement = issue_internal_command_session_token(
        root.path(),
        &command_id,
        command["payload_hash"].as_str().context("hash")?,
        "owner",
        "chef",
        "native-job-workspace",
        &json!({}),
    )?;
    let replacement =
        restrict_internal_command_session_to_workjet_supervisor(root.path(), &replacement)?;
    let mut writer = Connection::open(crate::paths::core_db(root.path()))?;
    let pending_writer = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_executor(root.path(), Some(&replacement))?;
    assert_eq!(routes(root.path())?, 1);
    pending_writer.rollback()?;
    let actual: Option<String> = Connection::open(crate::paths::core_db(root.path()))?.query_row(
        "SELECT actual_json FROM workjet_supervisor_route_attempts",
        [],
        |r| r.get(0),
    )?;
    assert!(actual.is_none());
    Ok(())
}
