// Origin: CTOX
// License: AGPL-3.0-only
use super::*;

fn fixture(jour_fixe: Value) -> anyhow::Result<tempfile::TempDir> {
    let root = super::weekly_reports::fixture()?;
    let conn = open_store(root.path())?;
    let mut project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    project["jour_fixe"] = jour_fixe;
    store::upsert_business_record(&conn, "workjet_projects", "project", 3, project)?;
    Ok(root)
}

fn init(root: &Path, id: &str) -> anyhow::Result<Value> {
    let accepted = crate::business_os::command_plane::accept_rxdb_business_command(
        root,
        json!({"id":id,"module":"ctox","command_type":super::project_init::COMMAND,"record_id":"project",
          "payload":{"project_id":"project","as_of":"2026-10-13"},
          "client_context":{"actor":{"id":"owner","role":"admin","is_admin":true}}}),
    )?;
    ensure!(accepted["status"] == "completed", "init failed: {accepted}");
    Ok(accepted["result"].clone())
}

fn states(result: &Value) -> Vec<(String, String)> {
    result["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_owned(),
                s["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn project_init_sets_default_regeltermin_and_queues_exit_and_supervisor() -> anyhow::Result<()> {
    let root = fixture(Value::Null)?;
    let result = init(root.path(), "init-1")?;
    let steps = states(&result);
    let ids: Vec<_> = steps.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "analysed",
            "regeltermin",
            "exit",
            "supervisor",
            "kpis",
            "worker"
        ]
    );
    assert!(steps.iter().all(|(_, s)| s != "failed"), "{result}");
    assert_eq!(result["steps"][1]["detail"], "created");
    assert_eq!(result["steps"][1]["label"]["de"], "Regeltermin angelegt");
    let conn = open_store(root.path())?;
    let project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    assert_eq!(
        project["jour_fixe"],
        json!({"weekday":1,"time":"09:00","timezone":"Europe/Berlin"})
    );
    let research: i64 = conn.query_row(
        "SELECT COUNT(*) FROM workjet_exit_model_runs WHERE project_id='project' AND research_command_id IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(research, 1);
    drop(conn);
    // Replay returns the same progress without new research or turns.
    let replay = init(root.path(), "init-1")?;
    assert_eq!(states(&replay), steps);
    Ok(())
}

#[test]
fn project_init_keeps_owner_regeltermin() -> anyhow::Result<()> {
    let slot = json!({"weekday":3,"time":"14:00","timezone":"Europe/Berlin"});
    let root = fixture(slot.clone())?;
    let result = init(root.path(), "init-2")?;
    assert_eq!(result["steps"][1]["detail"], "kept");
    let conn = open_store(root.path())?;
    let project = outbound_load_record(&conn, "workjet_projects", "project")?.unwrap();
    assert_eq!(project["jour_fixe"], slot);
    Ok(())
}
