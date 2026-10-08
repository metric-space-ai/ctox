// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    crate::business_os::store_workjet_projects::tests::create_workjet_rxdb_projection_tables(
        root.path(),
    )?;
    let conn = store::open_store(root.path())?;
    let now = now_ms() as i64;
    for user in ["chatgpt:test-user", "foreign"] {
        conn.execute("INSERT INTO business_users (user_id,display_name,role,active,created_at_ms,updated_at_ms)
            VALUES (?1,?1,'admin',1,?2,?2)", params![user, now])?;
    }
    drop(conn);
    let mut policy = default_mcp_policy();
    policy.enabled = true;
    policy.allow_reads = true;
    policy.allow_writes = true;
    save_mcp_policy(root.path(), &policy)?;
    Ok(root)
}
fn request(key: &str) -> Value {
    json!({"project_id":"stable-project","name":"Project","repo_url":"https://github.com/metric-space-ai/molecularity",
        "info":{"goal":"Engine; demos; GitHub project page"},"idempotency_key":key,
        "_context":{"channel":"chatgpt_mcp","surface":"business_os_mcp","actor":"chatgpt:test-user",
            "workspace":"test-workspace","request_id":"project-test"}})
}
fn call(root: &Path, args: Value) -> anyhow::Result<Value> {
    call_tool(root, TOOL, args)
}

#[test]
fn mcp_project_upsert_creates_one_owned_project_and_replays_exact_receipt() -> anyhow::Result<()> {
    let root = fixture()?;
    let first = call(root.path(), request("create"))?;
    assert_eq!(first["project"]["id"], "stable-project");
    assert_eq!(first["project"]["owner_user_id"], "chatgpt:test-user");
    assert_eq!(
        first["project"]["info"]["goal"],
        "Engine; demos; GitHub project page"
    );
    assert!(first["group_chat_id"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(first["project"].get("public_url").is_none());
    assert_eq!(call(root.path(), request("create"))?, first);
    let projected =
        store::load_rxdb_collection_record(root.path(), "workjet_projects", "stable-project")?
            .context("project projection")?;
    assert_eq!(projected["repo_url"], first["project"]["repo_url"]);
    let conn = store::open_store(root.path())?;
    assert_eq!(
        store::outbound_load_records_by_string_field(
            &conn,
            "workjet_projects",
            "owner_user_id",
            "chatgpt:test-user"
        )?
        .len(),
        1
    );
    let mut changed = request("create");
    changed["name"] = json!("Different");
    assert!(call(root.path(), changed).is_err());
    assert_eq!(call(root.path(), request("create"))?, first);
    Ok(())
}

#[test]
fn mcp_project_upsert_cannot_take_foreign_project_or_accept_escape_fields() -> anyhow::Result<()> {
    let root = fixture()?;
    let before = call(root.path(), request("create"))?;
    let mut foreign = request("foreign");
    foreign["_context"]["actor"] = json!("foreign");
    assert!(call(root.path(), foreign).is_err());
    for field in [
        "owner_user_id",
        "terminal_input",
        "server_update",
        "credentials",
        "public_url",
    ] {
        let mut bad = request(field);
        bad[field] = json!("forbidden");
        assert!(call(root.path(), bad).is_err(), "{field}");
    }
    assert_eq!(call(root.path(), request("create"))?, before);
    Ok(())
}

#[test]
fn mcp_project_upsert_preserves_website_schedule_and_goal_when_omitted() -> anyhow::Result<()> {
    let root = fixture()?;
    let before = call(root.path(), request("create"))?;
    let conn = store::open_store(root.path())?;
    let mut row = before["project"].clone();
    row["public_url"] = json!("https://example.org/existing");
    row["jour_fixe"] = json!({"weekday":3,"time":"13:00","timezone":"Europe/Berlin"});
    store::upsert_business_record(
        &conn,
        "workjet_projects",
        "stable-project",
        now_ms() as i64,
        row,
    )?;
    drop(conn);
    let mut patch = request("rename");
    patch["name"] = json!("Renamed");
    patch.as_object_mut().unwrap().remove("info");
    patch.as_object_mut().unwrap().remove("repo_url");
    let after = call(root.path(), patch)?;
    assert_eq!(
        after["project"]["public_url"],
        "https://example.org/existing"
    );
    assert_eq!(after["project"]["jour_fixe"]["time"], "13:00");
    assert_eq!(after["project"]["info"], before["project"]["info"]);
    assert_eq!(after["project"]["repo_url"], before["project"]["repo_url"]);
    Ok(())
}

#[test]
fn mcp_project_upsert_denies_readonly_channel_and_restricted_command_session() -> anyhow::Result<()>
{
    let root = fixture()?;
    let mut policy = default_mcp_policy();
    policy.enabled = true;
    policy.allow_writes = false;
    save_mcp_policy(root.path(), &policy)?;
    assert!(call(root.path(), request("readonly")).is_err());
    assert_eq!(tool_policy_class(TOOL), McpToolPolicyClass::Write);
    let trusted = json!({"auth_source":MCP_INTERNAL_SESSION_AUTH_SOURCE,"command_id":"restricted","allowed_actions":[],"allowed_collections":["workjet_projects"]});
    assert!(
        enforce_internal_command_session_scope(TOOL, &request("scope"), Some(&trusted)).is_err()
    );
    Ok(())
}

#[test]
fn mcp_project_upsert_managed_scope_cannot_be_forged() -> anyhow::Result<()> {
    let root = fixture()?;
    let mut context =
        context_from_arguments_with_trusted_gateway_context(TOOL, &request("scope"), None)?;
    context.trusted_role_source = Some("ctox_dev_managed_mcp_token".to_owned());
    context.trusted_managed_read_scope = Some(ManagedMcpCollectionReadScope {
        allow_reads: true,
        allowed_collections: vec!["foreign_collection".to_owned()],
    });
    assert!(execute(root.path(), &context, &request("scope")).is_err());
    context.trusted_managed_read_scope = None;
    assert!(execute(root.path(), &context, &request("scope")).is_err());
    Ok(())
}
