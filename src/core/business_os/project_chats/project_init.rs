// Origin: CTOX
// License: AGPL-3.0-only

//! One-click project onboarding (#653). The owner command only composes
//! existing native commands: project upsert (default Regeltermin), exit-model
//! refresh (5-year research turn) and one registered Supervisor turn running
//! the project-init skill (KPIs, first parent worker). Every step derives its
//! command id from the init operation, so a replay returns current progress
//! instead of creating new work. A failed step never aborts later steps.
use super::super::{threads, workjet_identity};
use super::*;
use crate::business_os::store;
use rusqlite::OptionalExtension;

pub(in crate::business_os) const COMMAND: &str = "ctox.workjet.project.init";
const SKILL: &str =
    include_str!("../../../skills/system/mission_orchestration/project-init/SKILL.md");
/// Monday 09:00 Europe/Berlin: German default week start and working hours.
pub(super) const DEFAULT_WEEKDAY: u64 = 1;
pub(super) const DEFAULT_TIME: &str = "09:00";
pub(super) const DEFAULT_TIMEZONE: &str = "Europe/Berlin";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    project_id: String,
    #[serde(default)]
    as_of: Option<String>,
    #[serde(default)]
    hours_per_week: Option<f64>,
    #[serde(default)]
    monthly_budget_eur: Option<f64>,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

fn step(id: &str, state: &str, detail: impl Into<String>) -> Value {
    let (de, en) = match id {
        "analysed" => ("Projekt analysiert", "Project analysed"),
        "regeltermin" => ("Regeltermin angelegt", "Regular meeting scheduled"),
        "exit" => ("Exit-Bewertung gestartet", "Exit evaluation started"),
        "supervisor" => ("Supervisor beauftragt", "Supervisor briefed"),
        "kpis" => ("KPIs angelegt", "KPIs created"),
        "worker" => ("Erster Worker gestartet", "First worker started"),
        _ => (id, id),
    };
    json!({"id":id,"state":state,"label":{"de":de,"en":en},"detail":detail.into()})
}

fn submit(
    root: &Path,
    session: &store::BusinessOsSession,
    command: Value,
) -> anyhow::Result<Value> {
    let mut command = command;
    command["client_context"] =
        json!({"actor":threads::actor_payload(session),"source":"native-workjet-project-init"});
    let accepted = store::accept_rxdb_business_command_with_origin(
        root,
        command,
        store::CommandOrigin::TrustedLocal,
    )?;
    ensure!(
        accepted["status"] == "completed",
        "{}",
        accepted["error_message"]
            .as_str()
            .unwrap_or("command was not completed")
    );
    Ok(accepted)
}

pub(in crate::business_os) fn handle(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
) -> anyhow::Result<Value> {
    let payload: Payload = serde_json::from_value(command.payload.clone())?;
    let operation = required(
        command.id.as_deref().context("command id is required")?,
        "command id",
        128,
    )?;
    let conn = open_store(root)?;
    let owner = workjet_identity::owner_from_connection(&conn, actor)?;
    let project = owned_project(&conn, &payload.project_id, &owner, true)?;
    let project_id = text(&project, "id")?.to_owned();
    ensure!(
        command
            .record_id
            .as_deref()
            .is_none_or(|id| id == project_id),
        "record_id must match project_id"
    );
    let bound: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_supervisor_bindings')", [], |r| r.get(0))?;
    let thread: Option<String> = if bound {
        conn.query_row(
            "SELECT thread_id FROM workjet_supervisor_bindings WHERE project_id=?1 AND owner_user_id=?2",
            rusqlite::params![project_id, owner],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    drop(conn);
    let session = store::active_domain_recovery_session(root, &owner)?;
    let mut steps = Vec::new();
    let has_source = ["repository_url", "repo_url"]
        .iter()
        .any(|k| project[*k].as_str().is_some_and(|v| !v.trim().is_empty()))
        || project["info"]["local_path"].as_str().is_some();
    steps.push(step(
        "analysed",
        if thread.is_some() {
            "queued"
        } else {
            "blocked"
        },
        if has_source {
            "repository"
        } else {
            "project metadata"
        },
    ));

    // Regeltermin: keep an owner-chosen slot; otherwise set the German default.
    let regeltermin = if project.get("jour_fixe").is_some_and(|v| !v.is_null()) {
        Ok("kept")
    } else {
        submit(
            root,
            &session,
            json!({"id":stable_id("workjet_project_init_jf",&[&operation]),"module":"ctox",
              "command_type":"ctox.workjet.project.upsert","record_id":project_id,
              "payload":{"project_id":project_id,"name":project["name"],
                "jour_fixe":{"weekday":DEFAULT_WEEKDAY,"time":DEFAULT_TIME,"timezone":DEFAULT_TIMEZONE}}}),
        )
        .map(|_| "created")
    };
    steps.push(match regeltermin {
        Ok(state) => step("regeltermin", "done", state),
        Err(error) => step("regeltermin", "failed", error.to_string()),
    });

    // Exit evaluation: the existing refresh admits its own research turn.
    let as_of = payload.as_of.clone().unwrap_or_else(|| {
        chrono::Utc::now()
            .with_timezone(&chrono_tz::Europe::Berlin)
            .format("%Y-%m-%d")
            .to_string()
    });
    let exit = submit(
        root,
        &session,
        json!({"id":stable_id("workjet_project_init_exit",&[&operation]),"module":"ctox",
          "command_type":"ctox.workjet.exit_model.refresh","record_id":project_id,
          "payload":{"project_id":project_id,"as_of":as_of,"resources":{
            "hours_per_week":payload.hours_per_week.unwrap_or(20.0),
            "monthly_budget_eur":payload.monthly_budget_eur.unwrap_or(0.0),
            "comparison_mode":"equal_resources"}}}),
    );
    steps.push(match exit {
        Ok(_) if thread.is_some() => step("exit", "queued", "research turn admitted"),
        Ok(_) => step("exit", "blocked", "supervisor_not_bound"),
        Err(error) => step("exit", "failed", error.to_string()),
    });

    // One Supervisor turn with the skill; KPIs and the worker run inside it.
    let Some(thread) = thread else {
        steps.push(step("supervisor", "blocked", "supervisor_not_bound"));
        steps.push(step("kpis", "blocked", "supervisor_not_bound"));
        steps.push(step("worker", "blocked", "supervisor_not_bound"));
        return Ok(
            json!({"ok":true,"project_id":project_id,"operation_id":operation,"steps":steps}),
        );
    };
    let goal = format!(
        "ProjectInit for project {project_id}, operation_id {operation}. {}",
        SKILL.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    let turn = submit(
        root,
        &session,
        json!({"id":stable_id("workjet_project_init_turn",&[&operation]),"module":"ctox",
          "command_type":"ctox.workjet.project.supervisor.turn.submit","record_id":project_id,
          "payload":{"project_id":project_id,"thread_id":thread,"goal":goal}}),
    );
    match turn {
        Ok(accepted) => {
            let task = accepted["result"]["turn"]["task_id"].as_str().unwrap_or("");
            steps.push(step("supervisor", "queued", task));
            steps.push(step("kpis", "queued", "supervisor"));
            steps.push(step("worker", "queued", "supervisor"));
        }
        Err(error) => {
            let reason = error.to_string();
            steps.push(step("supervisor", "failed", reason.clone()));
            steps.push(step("kpis", "blocked", reason.clone()));
            steps.push(step("worker", "blocked", reason));
        }
    }
    Ok(json!({"ok":true,"project_id":project_id,"operation_id":operation,"steps":steps}))
}

#[cfg(test)]
#[path = "project_init_tests.rs"]
mod tests;
