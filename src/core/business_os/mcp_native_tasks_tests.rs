//! Contract fixtures use real native queue leases, Crew attempts, signed sessions and SQLite.
use super::*;

fn fixture(root: &Path, role: &str) -> anyhow::Result<BusinessOsSession> {
    let (capability, _) = store::issue_business_os_capability_token_for_managed_user(
        root,
        "native-actor",
        "Native actor",
        role,
        now_ms(),
    )?;
    for command in ["e49-parent", "e49-foreign"] {
        store::accept_rxdb_business_command_with_origin(
            root,
            serde_json::json!({
                "id":command,"module":"outbound-lead-generation","command_type":"business_os.chat.task",
                "payload":{"instruction":"Inspect assigned task","mode":"data","external_executor":{"executor_id":"native-executor","harness":"codex","timeout_seconds":600}},
                "client_context":{"capability_token":capability}
            }),
            store::CommandOrigin::ReplicatedPeer,
        )?;
    }
    let conn = rusqlite::Connection::open(crate::paths::core_db(root))?;
    let task: String = conn.query_row(
        "SELECT task_id FROM business_command_task_links WHERE command_id='e49-parent'",
        [],
        |r| r.get(0),
    )?;
    conn.execute("UPDATE communication_routing_state SET crew_assigned_member_id='crew-pico' WHERE message_key=?1", [&task])?;
    crate::mission::channels::lease_queue_task(root, &task, "native-worker")?;
    crate::crew::prepare_attempt(
        root,
        &[task],
        "native-worker",
        "e49-attempt",
        Some("e49-thread"),
        &serde_json::json!({}),
        None,
        "Inspect assigned task",
        None,
    )?
    .context("crew attempt missing")?;
    let parent = crate::mission::channels::business_command_projection(root, "e49-parent")?;
    let token = issue_internal_command_session_token(
        root,
        "e49-parent",
        parent["payload_hash"].as_str().unwrap(),
        "native-actor",
        role,
        "native-workspace",
        &serde_json::json!({}),
    )?;
    let token = restrict_internal_command_session_to_crew(root, &token)?;
    let token = bind_internal_command_session_to_crew_attempt(
        root,
        &token,
        "e49-attempt",
        "e49-plan",
    )?;
    let claims = decode_internal_command_session_token(root, &token)?;
    let conn = crew_execution::open(root)?;
    conn.execute("INSERT INTO workjet_external_crew_attempts(attempt_id,command_id,executor_id,harness,claims_json,prompt,deadline_ms,state) VALUES('e49-attempt','e49-parent','native-executor','codex',?1,'Inspect assigned task',?2,'offered')", params![serde_json::to_string(&claims)?, now_ms()+600_000])?;
    let session = BusinessOsSession {
        ok: true,
        authenticated: true,
        auth_required: false,
        login_url: None,
        reason: None,
        user: Some(crate::business_os::session::BusinessOsSessionUser {
            id: "native-actor".into(),
            display_name: "Native actor".into(),
            role: role.into(),
            is_admin: role == "admin",
        }),
    };
    Ok(session)
}
pub(crate) fn with_fixture(role: &'static str, body: impl FnOnce(&Path, &BusinessOsSession) -> anyhow::Result<()> + Send + 'static) -> anyhow::Result<()> {
    let root = std::sync::Arc::new(tempfile::tempdir()?);
    let retained_root = root.clone();
    let result = crate::app_capabilities::cap3::tests::run_with_core_owners(root.path().to_path_buf(), move || {
        let session = fixture(root.path(), role)?;
        body(root.path(), &session)
    });
    drop(retained_root);
    result
}
fn native_context(role: &str) -> McpChannelRequestContext {
    McpChannelRequestContext {channel:"native_v4".into(),surface:"native".into(),actor:"native-actor".into(),workspace:"native_v4".into(),tool:"list".into(),request_id:"test".into(),confirmation_state:McpConfirmationState::NotRequired,trusted_role:Some(role.into()),trusted_role_source:Some("retained_native_authority".into()),trusted_managed_read_scope:None,trusted_managed_instance_id:None}
}
fn attempt(command: &str) -> Value {
    serde_json::json!({"command_id":command,"executor_id":"native-executor","attempt_id":"e49-attempt"})
}
fn invoke(
    root: &Path,
    session: &BusinessOsSession,
    suffix: &str,
    args: Value,
) -> anyhow::Result<Value> {
    execute(
        root,
        "native-invocation",
        &format!("ctox.crew.execution.{suffix}"),
        &args,
        session,
    )
}
fn error_code(error: anyhow::Error) -> String {
    serde_json::from_str::<Value>(&error.to_string()).expect("typed native error")["code"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn e49_native_rights_cannot_be_replaced_by_private_crew_read() -> anyhow::Result<()> {
    with_fixture("founder", |root, session| {
    let args = serde_json::json!({"command_id":"e49-parent","executor_id":"native-executor"});
    // Founder may read private Crew state, but has no execution-management right.
    let context = McpChannelRequestContext {
        channel: "native_v4".into(),
        surface: "native".into(),
        actor: "native-actor".into(),
        workspace: "native_v4".into(),
        tool: "list".into(),
        request_id: "test".into(),
        confirmation_state: McpConfirmationState::NotRequired,
        trusted_role: Some("founder".into()),
        trusted_role_source: Some("retained_native_authority".into()),
        trusted_managed_read_scope: None,
        trusted_managed_instance_id: None,
    };
    assert!(
        !crew_execution::list(root, &context, &args)?["offers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        error_code(
            invoke(root, &session, "list", args)
                .expect_err("private read cannot grant execution rights")
        ),
        "forbidden"
    );
    Ok(())
    })
}

#[test]
fn e49_claim_consumes_offer_once_and_foreign_execution_report_is_forbidden() -> anyhow::Result<()> {
    with_fixture("admin", |root, session| {
    let offered = invoke(
        root,
        &session,
        "list",
        serde_json::json!({"command_id":"e49-parent","executor_id":"native-executor"}),
    )?;
    assert_eq!(offered["offers"][0]["state"], "offered");
    let claimed = invoke(root, &session, "claim", attempt("e49-parent"))?;
    assert_eq!(claimed["attempt_id"], "e49-attempt");
    assert!(
        claimed.get("command_session").is_none(),
        "native result must not persist a bearer token"
    );
    assert_eq!(
        error_code(
            invoke(root, &session, "claim", attempt("e49-parent"))
                .expect_err("second native claim must be rejected")
        ),
        "conflict"
    );
    let legacy = native_context("admin");
    let replay = crew_execution::claim(root, &legacy, &attempt("e49-parent"))?;
    let replay_again = crew_execution::claim(root, &legacy, &attempt("e49-parent"))?;
    assert_eq!(replay["attempt_id"], replay_again["attempt_id"]);
    let mut foreign = attempt("e49-foreign");
    foreign["reply"] = serde_json::json!("foreign candidate");
    assert_eq!(
        error_code(
            invoke(root, &session, "report", foreign)
                .expect_err("same owner cannot report under another execution")
        ),
        "forbidden"
    );
    let mut plan = attempt("e49-parent");
    plan["steps"] = serde_json::json!([{"label":"Inspect source","status":"in_progress"}]);
    assert!(invoke(root, &session, "plan.update", plan)?.is_object());
    let mut reply = attempt("e49-parent");
    reply["reply"] = serde_json::json!("Native candidate");
    assert_eq!(
        invoke(root, &session, "report", reply)?["accepted"],
        true
    );
    Ok(())
    })
}

#[test]
fn e49_task_target_is_atomic_and_idempotency_binds_full_intent() -> anyhow::Result<()> {
    with_fixture("admin", |root, session| {
    let args = serde_json::json!({"title":"Native task","instruction":"Inspect source","idempotency_key":"native-request","target":{"kind":"crew","member_id":"crew-pico","executor_id":"native-executor","harness":"codex","timeout_seconds":600},"payload":{"project_ref":"app-owned-reference"}});
    let first = execute(root, "create-1", "ctox.task.create", &args, &session)?;
    let replay = execute(root, "create-2", "ctox.task.create", &args, &session)?;
    assert_eq!(first["task_id"], replay["task_id"]);
    let task =
        crate::mission::channels::load_queue_task(root, first["task_id"].as_str().unwrap())?
            .unwrap();
    assert_eq!(task.crew_assigned_member_id.as_deref(), Some("crew-pico"));
    let mut changed = args.clone();
    changed["instruction"] = serde_json::json!("Different intent");
    assert!(execute(
        root,
        "create-3",
        "ctox.task.create",
        &changed,
        &session
    )
    .is_err());
    let mut invalid = args.clone();
    invalid["idempotency_key"] = serde_json::json!("invalid-target");
    invalid["target"]["member_id"] = serde_json::json!("missing-member");
    assert!(execute(
        root,
        "create-4",
        "ctox.task.create",
        &invalid,
        &session
    )
    .is_err());
    let id = format!(
        "native-task:{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&("native-actor", "invalid-target"))?)
    );
    assert!(
        crate::mission::channels::inspect_business_command(root, &id)?.is_none(),
        "failed target assignment must roll back queue and command"
    );
    let mut native = args;
    native["idempotency_key"] = serde_json::json!("native-target");
    native["target"] = serde_json::json!({"kind":"native"});
    assert!(execute(
        root,
        "create-5",
        "ctox.task.create",
        &native,
        &session
    )?["task_id"]
        .is_string());
    Ok(())
    })
}
