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
    Ok(())
}
#[test]
fn proposal_is_retained_without_imaginary_allocations() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    project(root)?;
    let proposal =
        json!({"hours_per_week":20,"monthly_budget_eur":300,"comparison_mode":"equal_resources"});
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
