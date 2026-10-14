// Origin: CTOX
// License: AGPL-3.0-only
use super::super::store_workjet_projects::tests::{
    create_workjet_rxdb_projection_tables, handle_workjet_project_upsert_command,
};
use super::*;
use tempfile::tempdir;

fn command(kind: &str, id: &str, payload: Value) -> BusinessCommand {
    BusinessCommand {
        id: Some(id.into()),
        module: "ctox".into(),
        command_type: format!("ctox.workjet.exit_model.{kind}"),
        record_id: Some("project-1".into()),
        payload,
        client_context: json!({}),
        origin: CommandOrigin::TrustedLocal,
    }
}
fn project(root: &Path) -> anyhow::Result<()> {
    create_workjet_rxdb_projection_tables(root)?;
    let c = BusinessCommand {
        command_type: "ctox.workjet.project.upsert".into(),
        ..command(
            "create",
            "create",
            json!({"project_id":"project-1","name":"Fixture"}),
        )
    };
    handle_workjet_project_upsert_command(root, &c, "owner-1")?;
    Ok(())
}
fn mutate(root: &Path, c: &BusinessCommand, owner: &str) -> anyhow::Result<Value> {
    let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&c.payload)?));
    let admission = DomainEffectAdmission::newly_claimed(c.id.as_deref().unwrap(), &hash, owner)?;
    handle_command(root, c, owner, Some(&admission))
}
#[test]
fn history_reopen_idempotence_and_foreign_owner_denial() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let read = command("read", "read", json!({"project_id":"project-1"}));
    assert_eq!(
        handle_command(root, &read, "owner-1", None)?["assessment"]["status"],
        "not_started"
    );
    assert!(handle_command(root, &read, "foreign", None).is_err());
    let blocked = command(
        "refresh",
        "refresh",
        json!({"project_id":"project-1","as_of":"2026-10-08"}),
    );
    let first = mutate(root, &blocked, "owner-1")?;
    assert_eq!(first["assessment"]["status"], "blocked");
    assert!(first["assessment"]["result"].is_null());
    assert_eq!(mutate(root, &blocked, "owner-1")?, first);
    assert!(mutate(root, &blocked, "foreign").is_err());
    let submit = command(
        "submit",
        "submit",
        json!({"project_id":"project-1","as_of":"2026-10-08","inputs":engine::fixture()}),
    );
    let final_run = mutate(root, &submit, "owner-1")?;
    assert_eq!(final_run["assessment"]["status"], "provisional");
    assert_eq!(
        final_run["assessment"]["result"]["expected_exit_equity_eur"],
        8050.0
    );
    let reopened = handle_command(root, &read, "owner-1", None)?;
    assert_eq!(reopened, final_run);
    assert_eq!(
        reopened["assessment"]["history"].as_array().unwrap().len(),
        2
    );
    assert_eq!(reopened["assessment"]["history"][1]["status"], "blocked");
    assert_eq!(
        reopened["assessment"]["history"][1]["run_id"],
        first["assessment"]["run_id"]
    );
    let conflict = command(
        "submit",
        "submit",
        json!({"project_id":"project-1","as_of":"2026-11-08","inputs":engine::fixture()}),
    );
    assert!(mutate(root, &conflict, "owner-1").is_err());
    let list = super::super::store_workjet_projects::handle_workjet_project_list_command(
        root,
        &BusinessCommand {
            command_type: "ctox.workjet.project.list".into(),
            record_id: None,
            payload: json!({}),
            ..read
        },
        "owner-1",
    )?;
    assert_eq!(list["exit_models"]["project-1"], reopened["assessment"]);
    let conn = reader(root)?;
    let projected = outbound_load_record(&conn, "workjet_projects", "project-1")?
        .context("project projection missing")?;
    assert!(projected.get("exit_model").is_none());
    assert!(serde_json::to_vec(&projected)?.len() <= 8192);
    Ok(())
}
#[test]
fn proposal_is_retained_without_imaginary_allocations() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let proposal = json!({"hours_per_week":20.0,"monthly_budget_eur":300.0,"comparison_mode":"equal_resources"});
    let first = mutate(
        root,
        &command(
            "refresh",
            "a",
            json!({"project_id":"project-1","as_of":"2026-10-08","resources":proposal}),
        ),
        "owner-1",
    )?;
    assert_eq!(first["assessment"]["status"], "blocked");
    assert!(first["assessment"]["plan_summary"].is_null());
    assert!(first["assessment"]["result"].is_null());
    let next = mutate(
        root,
        &command(
            "refresh",
            "b",
            json!({"project_id":"project-1","as_of":"2026-11-08"}),
        ),
        "owner-1",
    )?;
    assert_eq!(next["assessment"]["resource_proposal"], proposal);
    assert_eq!(next["assessment"]["exit_date"], "2031-11-08");
    Ok(())
}

#[test]
fn research_reply_parsing_keeps_one_object_without_trusting_a_second() {
    let blocked = json!({"exit_model_blocked":{"missing_inputs":["source:prices"]}});
    let raw = blocked.to_string();
    assert_eq!(parse_research_reply(&raw), Some(blocked.clone()));
    assert_eq!(
        parse_research_reply(&format!("```json\n{raw}\n```")),
        Some(blocked.clone())
    );
    assert_eq!(
        parse_research_reply(&format!("{raw}\n\nThe files are available in the app.")),
        Some(blocked)
    );
    assert!(parse_research_reply(&format!("{raw}\n{raw}")).is_none());
}

#[test]
fn terminal_research_retains_a_valid_resource_proposal_without_claiming_it_is_adoptable(
) -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let conn = open_store(root)?;
    conn.execute_batch(SCHEMA)?;
    let original = assessment("project-1", "research-run", "2026-10-08", None, None, true)?;
    conn.execute(
        "INSERT INTO workjet_exit_model_runs(run_id,project_id,owner_user_id,assessment_json,research_command_id) VALUES(?1,?2,?3,?4,?5)",
        params!["research-run", "project-1", "owner-1", original.to_string(), "research-cmd"],
    )?;
    drop(conn);
    let reply = json!({
        "exit_model_blocked":{"missing_inputs":["source:terminal_equity_prices"]},
        "resource_proposal":{"hours_per_week":7,"monthly_budget_eur":0,"comparison_mode":"project_specific"},
        "resource_proposal_reason":"Observed owner time only; prices are missing."
    });
    complete_research(root, "research-cmd", &format!("```json\n{reply}\n```"))?;
    let conn = reader(root)?;
    let state = read_state(&conn, "project-1", "owner-1")?;
    assert_eq!(state["resource_proposal"], reply["resource_proposal"]);
    assert_eq!(state["findings"][0]["code"], "resource_proposal_recorded");
    assert_eq!(state["status"], "blocked");
    assert!(state["plan_summary"].is_null());
    assert_eq!(
        conn.query_row(
            "SELECT inputs_json FROM workjet_exit_model_runs WHERE run_id=?1",
            [state["run_id"].as_str().unwrap()],
            |row| row.get::<_, Option<String>>(0)
        )?,
        None
    );
    Ok(())
}

#[test]
fn terminal_research_names_invalid_resource_mode_and_preserves_evidence_gaps() -> anyhow::Result<()>
{
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let conn = open_store(root)?;
    conn.execute_batch(SCHEMA)?;
    let original = assessment("project-1", "research-run", "2026-10-08", None, None, true)?;
    conn.execute(
        "INSERT INTO workjet_exit_model_runs(run_id,project_id,owner_user_id,assessment_json,research_command_id) VALUES(?1,?2,?3,?4,?5)",
        params!["research-run", "project-1", "owner-1", original.to_string(), "research-cmd"],
    )?;
    drop(conn);
    let reply = json!({
        "exit_model_blocked":{"missing_inputs":["source:terminal_equity_prices"]},
        "resource_proposal":{"hours_per_week":7,"monthly_budget_eur":0,"comparison_mode":"status_quo"},
        "resource_proposal_reason":"Observed owner time only; prices are missing."
    });
    complete_research(root, "research-cmd", &reply.to_string())?;
    let conn = reader(root)?;
    let state = read_state(&conn, "project-1", "owner-1")?;
    assert_eq!(state["findings"][0]["code"], "resource_proposal_invalid");
    assert_eq!(
        state["missing_inputs"],
        json!(["source:terminal_equity_prices"])
    );
    assert!(state.get("resource_proposal").is_none());
    Ok(())
}

#[test]
fn stored_plan_confirmation_is_one_owner_operation_with_a_revision_fence() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let mut inputs = engine::fixture();
    inputs.plan.confirmed = false;
    let proposed = mutate(
        root,
        &command(
            "submit",
            "proposal",
            json!({"project_id":"project-1","as_of":"2026-10-08","inputs":inputs}),
        ),
        "owner-1",
    )?;
    assert_eq!(
        proposed["assessment"]["missing_inputs"],
        json!(["confirmed_resource_plan"])
    );
    assert_eq!(proposed["assessment"]["plan_summary"]["confirmed"], false);
    assert!(proposed["assessment"]["result"].is_null());
    let confirm = command(
        "submit",
        "confirm",
        json!({"project_id":"project-1","confirm_run_id":proposed["assessment"]["run_id"]}),
    );
    assert!(mutate(root, &confirm, "foreign").is_err());
    let accepted = mutate(root, &confirm, "owner-1")?;
    assert_eq!(accepted["assessment"]["status"], "provisional");
    assert_eq!(accepted["assessment"]["plan_summary"]["confirmed"], true);
    assert_eq!(
        accepted["assessment"]["result"]["expected_exit_equity_eur"],
        8050.0
    );
    assert_eq!(
        accepted["assessment"]["confirmed_from_run_id"],
        proposed["assessment"]["run_id"]
    );
    assert_eq!(mutate(root, &confirm, "owner-1")?, accepted);
    let conn = reader(root)?;
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM workjet_exit_model_runs", [], |r| r
            .get::<_, i64>(0))?,
        2
    );
    let stale = command("submit", "second-owner-intent", confirm.payload.clone());
    assert!(mutate(root, &stale, "owner-1")
        .unwrap_err()
        .to_string()
        .contains("stale"));
    let changed = command(
        "submit",
        "confirm",
        json!({"project_id":"project-1","confirm_run_id":"different"}),
    );
    assert!(mutate(root, &changed, "owner-1").is_err());
    for key in ["resources", "inputs", "as_of"] {
        let mut payload = confirm.payload.clone();
        payload[key] = json!({});
        assert!(mutate(root, &command("submit", "override", payload), "owner-1").is_err());
    }
    Ok(())
}

#[test]
fn confirmation_requires_complete_current_evidence_not_just_a_resource_proposal(
) -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let mut inputs = engine::fixture();
    inputs.plan.confirmed = false;
    inputs.sources[0].valid_until = "2026-10-08".into();
    let expired = mutate(
        root,
        &command(
            "submit",
            "expired",
            json!({"project_id":"project-1","as_of":"2026-10-08","inputs":inputs}),
        ),
        "owner-1",
    )?;
    let err = mutate(
        root,
        &command(
            "submit",
            "confirm-expired",
            json!({"project_id":"project-1","confirm_run_id":expired["assessment"]["run_id"]}),
        ),
        "owner-1",
    )
    .unwrap_err();
    assert!(err.to_string().contains("current_source:fixture"), "{err}");
    let empty = mutate(
        root,
        &command("refresh", "empty", json!({"project_id":"project-1"})),
        "owner-1",
    )?;
    let err = mutate(
        root,
        &command(
            "submit",
            "confirm-empty",
            json!({"project_id":"project-1","confirm_run_id":empty["assessment"]["run_id"]}),
        ),
        "owner-1",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("no complete researched plan"),
        "{err}"
    );
    Ok(())
}
