// Origin: CTOX
// License: AGPL-3.0-only

//! Native prompt configuration. Numeric KPI values are never browser input.
//! Source binding/calculation is a separate supervisor tool, not a guessed
//! number in a project card or a side effect of saving project metadata.
use super::domain_effect::{AppliedDomainEffect, DomainEffectAdmission};
use super::store::{open_store, BusinessCommand};
use super::workjet_project_kpis_contract::{
    ConfigureKpisRequest, KpiPrompt, KpiRecord, KpiResult, KpiState, ProjectKpis,
    ReadKpisRequest, WireValidate,
};
use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const STATE_TABLE: &str = "workjet_project_kpi_state";
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workjet_project_kpi_state (
    project_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL, state_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workjet_project_kpi_prompt_revisions (
    project_id TEXT NOT NULL, kpi_id TEXT NOT NULL, last_revision INTEGER NOT NULL,
    PRIMARY KEY(project_id,kpi_id)
);
CREATE TABLE IF NOT EXISTS workjet_project_kpi_operations (
    operation_id TEXT PRIMARY KEY, owner_user_id TEXT NOT NULL,
    intent_hash TEXT NOT NULL, result_json TEXT NOT NULL
);";

pub(super) fn is_command(kind: &str) -> bool {
    matches!(kind, "ctox.workjet.project.kpis.configure" | "ctox.workjet.project.kpis.read")
}

// Command-bus routing metadata is outside the generated request contract.
// Remove only that existing, typed field; every other unknown field fails.
fn payload(command: &BusinessCommand) -> anyhow::Result<Value> {
    let mut value = command.payload.clone();
    let object = value.as_object_mut().context("KPI payload must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let channel = channel.as_str().context("inbound_channel must be a string")?;
        ensure!(!channel.trim().is_empty() && channel.chars().count() <= 256,
            "invalid inbound_channel");
    }
    Ok(value)
}

fn empty(project_id: &str) -> ProjectKpis {
    ProjectKpis { project_id: project_id.to_owned(), revision: 0, items: vec![] }
}

fn load(conn: &Connection, project_id: &str, owner: &str) -> anyhow::Result<ProjectKpis> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [STATE_TABLE], |row| row.get(0))?;
    if !exists { return Ok(empty(project_id)); }
    let stored: Option<(String, String)> = conn.query_row(
        "SELECT owner_user_id,state_json FROM workjet_project_kpi_state WHERE project_id=?1",
        [project_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
    let Some((stored_owner, data)) = stored else { return Ok(empty(project_id)); };
    ensure!(stored_owner == owner, "KPI owner conflicts with current project ownership");
    let result: ProjectKpis = serde_json::from_str(&data)?;
    ensure!(result.project_id == project_id, "KPI project identity conflicts");
    result.validate().map_err(anyhow::Error::msg)?;
    Ok(result)
}

fn require_project(conn: &Connection, actor: &str, project_id: &str) -> anyhow::Result<String> {
    let owner = super::workjet_identity::owner_from_connection(conn, actor)?;
    let project = super::project_chats::owned_project(conn, project_id, &owner, true)?;
    ensure!(project["_deleted"] != true, "project is deleted");
    ensure!(project["id"] == project_id, "project_id must use its canonical spelling");
    Ok(owner)
}

pub(super) fn handle_command(root: &Path, command: &BusinessCommand, actor: &str,
    admission: Option<&DomainEffectAdmission>) -> anyhow::Result<Value> {
    if let Some(record_id) = command.record_id.as_deref() {
        ensure!(command.payload["project_id"].as_str() == Some(record_id),
            "KPI record_id must match the typed project_id");
    }
    match command.command_type.as_str() {
        "ctox.workjet.project.kpis.read" => {
            let request: ReadKpisRequest = serde_json::from_value(payload(command)?)?;
            request.validate().map_err(anyhow::Error::msg)?;
            let conn = open_store(root)?;
            let owner = require_project(&conn, actor, &request.project_id)?;
            let state = load(&conn, &request.project_id, &owner)?;
            Ok(json!({"ok":true,"kpis":state}))
        }
        "ctox.workjet.project.kpis.configure" => {
            let request: ConfigureKpisRequest = serde_json::from_value(payload(command)?)?;
            request.validate().map_err(anyhow::Error::msg)?;
            let mut conn = open_store(root)?;
            let applied = admission.context("KPI configuration requires domain admission")?
                .apply(&mut conn, |tx| configure(tx, actor, &request))?;
            Ok(applied.result)
        }
        _ => anyhow::bail!("unsupported project KPI command"),
    }
}

fn configure(conn: &Connection, actor: &str, request: &ConfigureKpisRequest)
    -> anyhow::Result<AppliedDomainEffect> {
    let owner = require_project(conn, actor, &request.project_id)?;
    conn.execute_batch(SCHEMA)?;
    let intent = format!("{:x}", Sha256::digest(serde_json::to_vec(request)?));
    let replay: Option<(String,String,String)> = conn.query_row(
        "SELECT owner_user_id,intent_hash,result_json FROM workjet_project_kpi_operations WHERE operation_id=?1",
        [&request.operation_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    if let Some((previous_owner, previous_intent, result)) = replay {
        ensure!(previous_owner == owner && previous_intent == intent,
            "KPI operation identity was reused with different intent");
        return Ok(AppliedDomainEffect {result:serde_json::from_str(&result)?,projections:vec![]});
    }
    let previous = load(conn, &request.project_id, &owner)?;
    ensure!(previous.revision == request.expected_revision, "KPI revision conflict");
    let revision = previous.revision.checked_add(1).context("KPI revision exhausted")?;
    let mut items = Vec::new();
    for prompt in &request.prompts {
        if let Some(old) = previous.items.iter().find(|item|
            item.prompt.kpi_id == prompt.kpi_id && item.prompt.prompt == prompt.prompt) {
            items.push(old.clone());
            continue;
        }
        // The watermark survives removal/clear: reusing the same KPI id may
        // never accept an old supervisor result for an earlier prompt.
        let old_revision: u64 = conn.query_row(
            "SELECT last_revision FROM workjet_project_kpi_prompt_revisions WHERE project_id=?1 AND kpi_id=?2",
            params![request.project_id,prompt.kpi_id], |row| row.get(0)).optional()?.unwrap_or(0);
        let prompt_revision = old_revision.checked_add(1).context("prompt revision exhausted")?;
        prompt_revision.validate().map_err(anyhow::Error::msg)?;
        conn.execute("INSERT INTO workjet_project_kpi_prompt_revisions(project_id,kpi_id,last_revision)
            VALUES(?1,?2,?3) ON CONFLICT(project_id,kpi_id) DO UPDATE SET last_revision=excluded.last_revision",
            params![request.project_id,prompt.kpi_id,prompt_revision])?;
        items.push(KpiRecord {
            prompt: KpiPrompt {kpi_id:prompt.kpi_id.clone(),prompt:prompt.prompt.clone(),
                revision:prompt_revision},
            result:KpiResult {status:KpiState::MissingSource,snapshot:None,
                reason_code:Some("source_not_bound".into()),
                message:Some("The project supervisor has not bound a verified metric source for this prompt.".into())},
        });
    }
    let state = ProjectKpis {project_id:request.project_id.clone(),revision,items};
    state.validate().map_err(anyhow::Error::msg)?;
    let result = json!({"ok":true,"kpis":state});
    conn.execute("INSERT INTO workjet_project_kpi_state(project_id,owner_user_id,state_json)
        VALUES(?1,?2,?3) ON CONFLICT(project_id) DO UPDATE SET state_json=excluded.state_json",
        params![request.project_id,owner,serde_json::to_string(&state)?])?;
    conn.execute("INSERT INTO workjet_project_kpi_operations(operation_id,owner_user_id,intent_hash,result_json)
        VALUES(?1,?2,?3,?4)",params![request.operation_id,owner,intent,serde_json::to_string(&result)?])?;
    Ok(AppliedDomainEffect {result,projections:vec![]})
}

#[cfg(test)]
mod tests;
