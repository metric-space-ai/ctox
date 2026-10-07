// Origin: CTOX
// License: AGPL-3.0-only

//! Native-owned Workjet project and working-copy command handlers.
//!
//! These handlers deliberately accept the authorized owner from their caller.
//! Working-copy computer and path identifiers describe the selected Workjet
//! computer and are opaque to CTOX: the backend must never interpret them as
//! host paths.

use super::domain_effect::{AppliedDomainEffect, DomainEffectAdmission};
use super::project_chats::{domain_references, Projection};
use super::store::{
    open_store, outbound_load_record, outbound_load_records_by_string_field,
    upsert_business_record, upsert_rxdb_collection_record, BusinessCommand,
};
use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

const PROJECTS_COLLECTION: &str = "workjet_projects";
const WORKING_COPIES_COLLECTION: &str = "workjet_working_copies";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectUpsertPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    name: String,
    #[serde(default)]
    description: ProjectField<String>,
    #[serde(default)]
    repo_url: ProjectField<String>,
    #[serde(default)]
    public_url: ProjectField<String>,
    #[serde(default)]
    info: ProjectField<ProjectInfo>,
    #[serde(default)]
    jour_fixe: ProjectField<JourFixe>,
    #[serde(default)]
    archived: Option<bool>,
}

// Missing keeps the current value; explicit null clears it. Option<T> alone
// cannot distinguish those two states in a partial project update.
#[derive(Debug)]
enum ProjectField<T> {
    Keep,
    Clear,
    Set(T),
}

impl<T> Default for ProjectField<T> {
    fn default() -> Self {
        Self::Keep
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ProjectField<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            Some(value) => Self::Set(value),
            None => Self::Clear,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JourFixe {
    weekday: u8,
    time: String,
    #[serde(default = "default_project_timezone")]
    timezone: String,
}

fn default_project_timezone() -> String {
    "Europe/Berlin".to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectListPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkingCopyUpsertPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    project_id: String,
    computer_id: String,
    #[serde(default)]
    path: Option<String>,
    active: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    working_copy_id: Option<String>,
}

pub(super) fn handle_workjet_project_store_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    _authorized_owner_email: Option<&str>,
    admission: Option<&DomainEffectAdmission>,
) -> anyhow::Result<Value> {
    // Policy/receipts retain the actor; only project ownership resolves to a
    // native-enrolled same-person identity. Project owners are never migrated.
    let owner = super::workjet_identity::owner(root, authorized_owner_user_id)?;
    let authorized_owner_user_id = owner.as_str();
    match command.command_type.as_str() {
        "ctox.workjet.project.list" => {
            handle_workjet_project_list_command(root, command, authorized_owner_user_id)
        }
        "ctox.workjet.project.upsert" => handle_workjet_project_upsert_command(
            root,
            command,
            authorized_owner_user_id,
            admission.context("new Workjet project mutation requires domain admission")?,
            None,
        ),
        "ctox.workjet.working_copy.upsert" => {
            handle_workjet_working_copy_upsert_command(root, command, authorized_owner_user_id)
        }
        other => anyhow::bail!("unsupported Workjet project command type: {other}"),
    }
}

pub(super) fn handle_workjet_project_list_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
) -> anyhow::Result<Value> {
    let payload: ProjectListPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.project.list payload")?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let limit = payload.limit.unwrap_or(100).clamp(1, 100);
    let conn = open_store(root)?;
    let mut projects = outbound_load_records_by_string_field(
        &conn,
        PROJECTS_COLLECTION,
        "owner_user_id",
        &owner_user_id,
    )?;
    projects.retain(|project| {
        project.get("is_deleted").and_then(Value::as_bool) != Some(true)
            && project.get("_deleted").and_then(Value::as_bool) != Some(true)
            && project.get("status").and_then(Value::as_str) == Some("active")
    });
    projects.sort_by(|left, right| {
        right
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .cmp(&left.get("updated_at_ms").and_then(Value::as_i64))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .cmp(&right.get("id").and_then(Value::as_str))
            })
    });
    let truncated = projects.len() > limit;
    let count = projects.len().min(limit);
    let project_ids = projects
        .iter()
        .take(limit)
        .map(|project| {
            project
                .get("id")
                .and_then(Value::as_str)
                .context("active Workjet project has no id")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": PROJECTS_COLLECTION,
        "owner_user_id": owner_user_id,
        "count": count,
        "project_ids": project_ids,
        "truncated": truncated,
    }))
}

pub(super) fn handle_workjet_project_upsert_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    admission: &DomainEffectAdmission,
    _authorized_owner_email: Option<&str>,
) -> anyhow::Result<Value> {
    let payload: ProjectUpsertPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.project.upsert payload")?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let project_id = payload
        .project_id
        .or_else(|| command.record_id.clone())
        .context("project_id is required")?;
    let project_id = bounded_required(&project_id, "project_id", 128)?;
    let name = bounded_required(&payload.name, "name", 256)?;
    let description = project_field_value(payload.description, |value| {
        Ok(Value::String(
            optional_bounded(Some(value), "description", 4096)?.unwrap_or_default(),
        ))
    })?;
    let repo_url = project_field_value(payload.repo_url, |value| project_url(value, "repo_url"))?;
    let public_url =
        project_field_value(payload.public_url, |value| project_url(value, "public_url"))?;
    let info = project_field_value(payload.info, |mut info| {
        info.description = optional_project_info_text(info.description, "info.description", 4096)?;
        info.goal = optional_project_info_text(info.goal, "info.goal", 4096)?;
        info.phase = optional_bounded(info.phase, "info.phase", 128)?;
        info.status = optional_bounded(info.status, "info.status", 128)?;
        Ok(serde_json::to_value(info)?)
    })?;
    let jour_fixe = project_field_value(payload.jour_fixe, |mut meeting| {
        anyhow::ensure!(
            (1..=7).contains(&meeting.weekday),
            "jour_fixe.weekday must be ISO 1..7 (Monday..Sunday)"
        );
        let time = meeting.time.as_bytes();
        anyhow::ensure!(
            time.len() == 5
                && time[2] == b':'
                && [time[0], time[1], time[3], time[4]]
                    .iter()
                    .all(u8::is_ascii_digit),
            "jour_fixe.time must be HH:mm"
        );
        let hour: u8 = meeting.time[..2].parse()?;
        let minute: u8 = meeting.time[3..].parse()?;
        anyhow::ensure!(hour < 24 && minute < 60, "jour_fixe.time must be HH:mm");
        meeting.timezone = bounded_required(&meeting.timezone, "jour_fixe.timezone", 128)?;
        let _: chrono_tz::Tz = meeting
            .timezone
            .parse()
            .map_err(|_| anyhow::anyhow!("jour_fixe.timezone must be an IANA timezone"))?;
        Ok(serde_json::to_value(meeting)?)
    })?;

    let mut conn = open_store(root)?;
    let applied = admission.apply(&mut conn, |transaction| {
        let mut chat_projections = Vec::new();
        let now = super::store::now_ms() as i64;
        let existing = outbound_load_record(&transaction, PROJECTS_COLLECTION, &project_id)?;
        ensure_owned(existing.as_ref(), &owner_user_id, "project")?;
        let created_at_ms = existing
            .as_ref()
            .and_then(|record| record.get("created_at_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(now);
        let archived = payload.archived.unwrap_or_else(|| {
            existing
                .as_ref()
                .and_then(|record| record.get("status"))
                .and_then(Value::as_str)
                == Some("archived")
        });
        let status = if archived { "archived" } else { "active" };
        let archived_at_ms = if archived {
            existing
                .as_ref()
                .filter(|record| record.get("status").and_then(Value::as_str) == Some("archived"))
                .and_then(|record| record.get("archived_at_ms"))
                .and_then(Value::as_i64)
                .unwrap_or(now)
        } else {
            0
        };
        let mut project = serde_json::json!({
            "id": project_id,
            "name": name,
            "status": status,
            "owner_user_id": owner_user_id,
            "created_at_ms": created_at_ms,
            "updated_at_ms": now,
            "is_deleted": false,
        });
        for (field, patch) in [
            ("description", description),
            ("repo_url", repo_url),
            ("public_url", public_url),
            ("info", info),
            ("jour_fixe", jour_fixe),
        ] {
            match patch {
                ProjectField::Keep => {
                    if let Some(value) = existing.as_ref().and_then(|record| record.get(field)) {
                        project[field] = value.clone();
                    }
                }
                ProjectField::Clear => {}
                ProjectField::Set(value) => project[field] = value,
            }
        }
        if archived {
            project["archived_at_ms"] = Value::from(archived_at_ms);
        }

        let project =
            persist_idempotently(&transaction, PROJECTS_COLLECTION, &project_id, now, project)?;
        let group_chat_id = super::project_chats::ensure_default_group(
            transaction,
            &project,
            &mut chat_projections,
        )?;
        chat_projections.push(Projection {
            collection: PROJECTS_COLLECTION,
            id: project_id.clone(),
        });
        Ok(AppliedDomainEffect {
            result: serde_json::json!({
                "ok": true,
                "collection": PROJECTS_COLLECTION,
                "project": project,
                "group_chat_id": group_chat_id,
            }),
            projections: domain_references(chat_projections),
        })
    })?;
    Ok(applied.result)
}

pub(super) fn handle_workjet_working_copy_upsert_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
) -> anyhow::Result<Value> {
    let payload: WorkingCopyUpsertPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.working_copy.upsert payload")?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    let project_id = bounded_required(&payload.project_id, "project_id", 128)?;
    let label = optional_bounded(payload.label, "label", 256)?;
    anyhow::ensure!(
        payload.active != payload.working_copy_id.is_some(),
        "active working copies require a path and detached working copies require working_copy_id"
    );

    let requested_working_copy_id = payload
        .working_copy_id
        .as_deref()
        .map(|id| bounded_required(id, "working_copy_id", 160))
        .transpose()?;

    let conn = open_store(root)?;
    let project = outbound_load_record(&conn, PROJECTS_COLLECTION, &project_id)?
        .context("Workjet project not found")?;
    ensure_owned(Some(&project), &owner_user_id, "project")?;
    anyhow::ensure!(
        !payload.active || project.get("status").and_then(Value::as_str) != Some("archived"),
        "cannot attach a working copy to an archived project"
    );
    let (working_copy_id, opaque_path) = if payload.active {
        let opaque_path = bounded_required(
            payload.path.as_deref().context("path is required")?,
            "path",
            4096,
        )?;
        (
            deterministic_working_copy_id(&project_id, &computer_id, &opaque_path),
            opaque_path,
        )
    } else {
        anyhow::ensure!(
            payload.path.is_none(),
            "path must be omitted when detaching"
        );
        let working_copy_id = requested_working_copy_id.context("working_copy_id is required")?;
        let existing = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?
            .context("working copy not found")?;
        let path = existing
            .get("path")
            .and_then(Value::as_str)
            .context("working copy has no path")?
            .to_owned();
        (working_copy_id, path)
    };
    let existing = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?;
    ensure_owned(existing.as_ref(), &owner_user_id, "working copy")?;
    if let Some(existing) = existing.as_ref() {
        anyhow::ensure!(
            existing.get("project_id").and_then(Value::as_str) == Some(project_id.as_str()),
            "working_copy_id belongs to a different project"
        );
        anyhow::ensure!(
            existing.get("computer_id").and_then(Value::as_str) == Some(computer_id.as_str()),
            "working_copy_id belongs to a different computer"
        );
    }

    let label = label.or_else(|| {
        existing
            .as_ref()
            .and_then(|record| record.get("label"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let now = super::store::now_ms() as i64;
    let created_at_ms = existing
        .as_ref()
        .and_then(|record| record.get("created_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let mut working_copy = serde_json::json!({
        "id": working_copy_id,
        "project_id": project_id,
        "computer_id": computer_id,
        "path": opaque_path,
        "status": if payload.active { "active" } else { "detached" },
        "owner_user_id": owner_user_id,
        "created_at_ms": created_at_ms,
        "updated_at_ms": now,
        "is_deleted": false,
    });
    if let Some(label) = label {
        working_copy["label"] = Value::String(label);
    }

    let working_copy = persist_and_project_idempotently(
        root,
        &conn,
        WORKING_COPIES_COLLECTION,
        &working_copy_id,
        now,
        working_copy,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": WORKING_COPIES_COLLECTION,
        "working_copy": working_copy,
    }))
}

fn deterministic_working_copy_id(project_id: &str, computer_id: &str, path: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [project_id, computer_id, path] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("workjet_wc_{:x}", hasher.finalize())
}

pub(super) fn persist_idempotently(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    now: i64,
    desired: Value,
) -> anyhow::Result<Value> {
    if let Some(existing) = outbound_load_record(conn, collection, record_id)? {
        if stable_record_content(&existing) == stable_record_content(&desired) {
            return Ok(existing);
        }
    }
    upsert_business_record(conn, collection, record_id, now, desired)?;
    outbound_load_record(conn, collection, record_id)?
        .with_context(|| format!("failed to reload {collection} record {record_id}"))
}

fn persist_and_project_idempotently(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
    now: i64,
    desired: Value,
) -> anyhow::Result<Value> {
    let record = persist_idempotently(conn, collection, record_id, now, desired)?;
    let updated_at_ms = record
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(now);
    upsert_rxdb_collection_record(root, collection, record_id, updated_at_ms, record.clone())?;
    Ok(record)
}

fn stable_record_content(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("_rev");
        object.remove("_deleted");
        object.remove("updated_at_ms");
        object.remove("verified_at_ms");
    }
    value
}

fn ensure_owned(existing: Option<&Value>, owner_user_id: &str, kind: &str) -> anyhow::Result<()> {
    if let Some(existing) = existing {
        anyhow::ensure!(
            existing.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id),
            "{kind} belongs to a different owner"
        );
    }
    Ok(())
}

fn project_field_value<T>(
    field: ProjectField<T>,
    validate: impl FnOnce(T) -> anyhow::Result<Value>,
) -> anyhow::Result<ProjectField<Value>> {
    Ok(match field {
        ProjectField::Keep => ProjectField::Keep,
        ProjectField::Clear => ProjectField::Clear,
        ProjectField::Set(value) => ProjectField::Set(validate(value)?),
    })
}

fn project_url(value: String, field: &str) -> anyhow::Result<Value> {
    let value = bounded_required(&value, field, 2048)?;
    let url = url::Url::parse(&value)
        .with_context(|| format!("{field} must be an absolute HTTP(S) URL"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "{field} must be an HTTP(S) URL without credentials"
    );
    Ok(Value::String(value))
}

fn bounded_required(value: &str, field: &str, max_chars: usize) -> anyhow::Result<String> {
    let value = value.trim();
    validate_bounded(value, field, max_chars, false)?;
    Ok(value.to_owned())
}

fn optional_bounded(
    value: Option<String>,
    field: &str,
    max_chars: usize,
) -> anyhow::Result<Option<String>> {
    value
        .map(|value| {
            let value = value.trim();
            validate_bounded(value, field, max_chars, true)?;
            Ok(value.to_owned())
        })
        .transpose()
}

fn optional_project_info_text(
    value: Option<String>,
    field: &str,
    max_chars: usize,
) -> anyhow::Result<Option<String>> {
    value
        .map(|value| {
            let value = value.trim();
            anyhow::ensure!(
                value.chars().count() <= max_chars,
                "{field} exceeds {max_chars} characters"
            );
            anyhow::ensure!(
                !value
                    .chars()
                    .any(|character| character.is_control()
                        && !matches!(character, '\n' | '\r' | '\t')),
                "{field} contains control characters"
            );
            Ok(value.to_owned())
        })
        .transpose()
}

fn validate_bounded(
    value: &str,
    field: &str,
    max_chars: usize,
    allow_empty: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        allow_empty || !value.is_empty(),
        "{field} must not be empty"
    );
    anyhow::ensure!(
        value.chars().count() <= max_chars,
        "{field} exceeds {max_chars} characters"
    );
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "{field} contains control characters"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::business_os::store::{load_rxdb_collection_record, rxdb_store_path, CommandOrigin};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn command(command_type: &str, payload: Value) -> BusinessCommand {
        BusinessCommand {
            id: Some("cmd-workjet-test".to_owned()),
            module: "ctox".to_owned(),
            command_type: command_type.to_owned(),
            record_id: None,
            payload,
            client_context: json!({}),
            origin: CommandOrigin::TrustedLocal,
        }
    }

    // Component fixtures exercise the domain adapter without claiming to test
    // command admission. End-to-end recovery tests use the real command plane.
    pub(crate) fn handle_workjet_project_upsert_command(
        root: &Path,
        command: &BusinessCommand,
        owner: &str,
    ) -> anyhow::Result<Value> {
        let operation = format!(
            "fixture-{:x}",
            Sha256::digest(
                format!("{}:{}:{}", command.command_type, owner, command.payload).as_bytes()
            )
        );
        let admission = DomainEffectAdmission::newly_claimed(&operation, &operation, owner)?;
        let result =
            super::handle_workjet_project_upsert_command(root, command, owner, &admission, None)?;
        let conn = open_store(root)?;
        let effect = super::super::domain_effect::load(&conn, &operation, &operation, owner)?
            .context("fixture receipt missing")?;
        for reference in effect.projections {
            let record = outbound_load_record(&conn, &reference.collection, &reference.id)?
                .context("fixture source missing")?;
            upsert_rxdb_collection_record(
                root,
                &reference.collection,
                &reference.id,
                record["updated_at_ms"].as_i64().unwrap_or(0),
                record,
            )?;
        }
        Ok(result)
    }

    fn create_project(root: &Path) -> anyhow::Result<Value> {
        handle_workjet_project_upsert_command(
            root,
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id": "project-1", "name": "Project One"}),
            ),
            "owner-1",
        )
    }

    pub(crate) fn create_workjet_rxdb_projection_tables(root: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(root.join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root))?;
        for (collection, version) in [
            (PROJECTS_COLLECTION, 1),
            (WORKING_COPIES_COLLECTION, 0),
            (super::super::project_chats::CHATS, 0),
            (super::super::project_chats::THREADS, 1),
        ] {
            conn.execute(
                &format!(
                    "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v{version} (
                        id TEXT PRIMARY KEY NOT NULL,
                        revision TEXT,
                        deleted INTEGER NOT NULL DEFAULT 0,
                        lastWriteTime REAL NOT NULL DEFAULT 0,
                        data TEXT NOT NULL
                    )"
                ),
                [],
            )?;
        }
        Ok(())
    }

    #[test]
    fn project_payloads_accept_command_bus_routing_metadata() -> anyhow::Result<()> {
        serde_json::from_value::<ProjectListPayload>(json!({
            "limit": 100,
            "inbound_channel": "ctox"
        }))?;
        serde_json::from_value::<ProjectUpsertPayload>(json!({
            "project_id": "project-1",
            "name": "Project One",
            "inbound_channel": "ctox"
        }))?;
        serde_json::from_value::<WorkingCopyUpsertPayload>(json!({
            "project_id": "project-1",
            "computer_id": "computer-1",
            "path": "/workspace/project-1",
            "active": true,
            "inbound_channel": "ctox"
        }))?;
        Ok(())
    }

    #[test]
    fn project_configuration_persists_native_and_projected_values() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_rxdb_projection_tables(root.path())?;
        let request = json!({"project_id":"project-1","name":"Project One",
            "repo_url":"https://github.com/metric-space-ai/ctox",
            "public_url":"https://ctox.dev",
            "info":{"description":"Work daemon","goal":"Usable projects\nPersist after reopening","phase":"delivery","status":"active"},
            "jour_fixe":{"weekday":1,"time":"09:30"}});
        let first = handle_workjet_project_upsert_command(
            root.path(),
            &command("ctox.workjet.project.upsert", request.clone()),
            "owner-1",
        )?;
        let second = handle_workjet_project_upsert_command(
            root.path(),
            &command("ctox.workjet.project.upsert", request),
            "owner-1",
        )?;
        assert_eq!(first["project"], second["project"]);
        assert_eq!(first["project"]["jour_fixe"]["timezone"], "Europe/Berlin");
        let conn = open_store(root.path())?;
        let persisted = outbound_load_record(&conn, PROJECTS_COLLECTION, "project-1")?
            .context("project persisted")?;
        assert_eq!(
            persisted["info"]["goal"],
            "Usable projects\nPersist after reopening"
        );
        drop(conn);
        let projected = load_rxdb_collection_record(root.path(), PROJECTS_COLLECTION, "project-1")?
            .context("project projected")?;
        for field in ["repo_url", "public_url", "info", "jour_fixe"] {
            assert_eq!(persisted[field], projected[field]);
        }
        Ok(())
    }

    #[test]
    fn project_configuration_partial_updates_preserve_and_null_clears() -> anyhow::Result<()> {
        let root = tempdir()?;
        let initial = json!({"project_id":"project-1","name":"Project One","description":"Original",
            "repo_url":"https://github.com/metric-space-ai/ctox","public_url":"https://ctox.dev",
            "info":{"goal":"Keep this"},"jour_fixe":{"weekday":7,"time":"23:59","timezone":"UTC"},"archived":true});
        let first = handle_workjet_project_upsert_command(
            root.path(),
            &command("ctox.workjet.project.upsert", initial),
            "owner-1",
        )?;
        let renamed = handle_workjet_project_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id":"project-1","name":"Renamed"}),
            ),
            "owner-1",
        )?;
        assert_eq!(renamed["project"]["status"], "archived");
        for field in [
            "description",
            "repo_url",
            "public_url",
            "info",
            "jour_fixe",
            "archived_at_ms",
        ] {
            assert_eq!(renamed["project"][field], first["project"][field]);
        }
        let cleared = handle_workjet_project_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id":"project-1","name":"Renamed","repo_url":null,"public_url":null,"info":null,"jour_fixe":null,"archived":false}),
            ),
            "owner-1",
        )?;
        for field in [
            "repo_url",
            "public_url",
            "info",
            "jour_fixe",
            "archived_at_ms",
        ] {
            assert!(cleared["project"].get(field).is_none());
        }
        assert_eq!(cleared["project"]["description"], "Original");
        assert_eq!(cleared["project"]["status"], "active");
        Ok(())
    }

    #[test]
    fn project_configuration_rejects_invalid_fields_without_mutation() -> anyhow::Result<()> {
        let root = tempdir()?;
        let before = create_project(root.path())?;
        for patch in [
            json!({"repo_url":"javascript:alert(1)"}),
            json!({"public_url":"https://user:secret@example.test"}),
            json!({"repo_url":"relative/path"}),
            json!({"jour_fixe":{"weekday":0,"time":"09:00"}}),
            json!({"jour_fixe":{"weekday":8,"time":"09:00"}}),
            json!({"jour_fixe":{"weekday":1,"time":"9:00"}}),
            json!({"jour_fixe":{"weekday":1,"time":"24:00"}}),
            json!({"jour_fixe":{"weekday":1,"time":"09:60"}}),
            json!({"jour_fixe":{"weekday":1,"time":"09:00","timezone":"Invalid/Timezone"}}),
            json!({"info":{"owner_user_id":"foreign"}}),
            json!({"owner_user_id":"foreign"}),
        ] {
            let mut payload = json!({"project_id":"project-1","name":"Must not change"});
            payload
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(handle_workjet_project_upsert_command(
                root.path(),
                &command("ctox.workjet.project.upsert", payload),
                "owner-1"
            )
            .is_err());
        }
        let conn = open_store(root.path())?;
        let current = outbound_load_record(&conn, PROJECTS_COLLECTION, "project-1")?
            .context("unchanged project")?;
        assert_eq!(current, before["project"]);
        Ok(())
    }

    #[test]
    fn project_configuration_cannot_edit_another_owners_project() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_project(root.path())?;
        let payload = json!({"project_id":"project-1","name":"Forged","jour_fixe":{"weekday":2,"time":"09:30"},"info":{"goal":"Replace"}});
        let error = handle_workjet_project_upsert_command(
            root.path(),
            &command("ctox.workjet.project.upsert", payload),
            "foreign-owner",
        )
        .unwrap_err();
        assert!(error.to_string().contains("different owner"));
        Ok(())
    }

    #[test]
    fn project_upsert_stamps_native_fields_and_is_idempotent() -> anyhow::Result<()> {
        let root = tempdir()?;
        let first = create_project(root.path())?;
        let second = create_project(root.path())?;
        let first = &first["project"];
        let second = &second["project"];
        assert_eq!(first["owner_user_id"], "owner-1");
        assert_eq!(first["status"], "active");
        assert_eq!(first["_rev"], second["_rev"]);
        assert_eq!(first["updated_at_ms"], second["updated_at_ms"]);
        Ok(())
    }

    #[test]
    fn project_and_working_copy_upserts_repair_native_rxdb_projection() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_rxdb_projection_tables(root.path())?;
        create_project(root.path())?;
        let working_copy = handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-1",
                    "computer_id": "computer-1",
                    "path": "/workspace/project-1",
                    "active": true
                }),
            ),
            "owner-1",
        )?;
        let working_copy_id = working_copy["working_copy"]["id"]
            .as_str()
            .context("working copy id")?;

        let projected_project =
            load_rxdb_collection_record(root.path(), PROJECTS_COLLECTION, "project-1")?
                .context("project projection")?;
        assert_eq!(projected_project["owner_user_id"], "owner-1");
        let projected_working_copy =
            load_rxdb_collection_record(root.path(), WORKING_COPIES_COLLECTION, working_copy_id)?
                .context("working copy projection")?;
        assert_eq!(projected_working_copy["project_id"], "project-1");
        assert_eq!(projected_working_copy["computer_id"], "computer-1");
        assert_eq!(projected_working_copy["path"], "/workspace/project-1");

        create_project(root.path())?;
        handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-1",
                    "computer_id": "computer-1",
                    "path": "/workspace/project-1",
                    "active": true
                }),
            ),
            "owner-1",
        )?;
        assert!(load_rxdb_collection_record(
            root.path(),
            WORKING_COPIES_COLLECTION,
            working_copy_id,
        )?
        .is_some());
        Ok(())
    }

    #[test]
    fn signed_email_alias_does_not_rewrite_project_or_working_copy_owners() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_rxdb_projection_tables(root.path())?;
        let email = "michael.welsch@metric-space.ai";
        let stable_user_id = "196a89ba-ee86-4413-885c-04ca60e6f291";
        handle_workjet_project_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id": "project-email", "name": "greppy"}),
            ),
            email,
        )?;
        let working_copy = handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-email",
                    "computer_id": "computer-1",
                    "path": "/workspace/greppy",
                    "active": true
                }),
            ),
            email,
        )?;
        let working_copy_id = working_copy["working_copy"]["id"]
            .as_str()
            .context("working copy id")?;

        let listed = handle_workjet_project_store_command(
            root.path(),
            &command("ctox.workjet.project.list", json!({"limit": 100})),
            stable_user_id,
            Some(email),
            None,
        )?;
        assert_eq!(listed["count"], 0);

        let conn = open_store(root.path())?;
        let project = outbound_load_record(&conn, PROJECTS_COLLECTION, "project-email")?
            .context("migrated project")?;
        assert_eq!(project["owner_user_id"], email);
        let working_copy = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, working_copy_id)?
            .context("migrated working copy")?;
        assert_eq!(working_copy["owner_user_id"], email);
        assert_eq!(
            load_rxdb_collection_record(root.path(), PROJECTS_COLLECTION, "project-email")?
                .context("project projection")?["owner_user_id"],
            email
        );
        assert_eq!(
            load_rxdb_collection_record(root.path(), WORKING_COPIES_COLLECTION, working_copy_id,)?
                .context("working-copy projection")?["owner_user_id"],
            email
        );
        Ok(())
    }

    #[test]
    fn project_upsert_rejects_unknown_native_fields_and_controls() -> anyhow::Result<()> {
        let root = tempdir()?;
        let spoofed = command(
            "ctox.workjet.project.upsert",
            json!({
                "project_id": "project-1",
                "name": "Project One",
                "owner_user_id": "attacker"
            }),
        );
        assert!(handle_workjet_project_upsert_command(root.path(), &spoofed, "owner-1").is_err());
        let control = command(
            "ctox.workjet.project.upsert",
            json!({"project_id": "bad\nproject", "name": "Project One"}),
        );
        assert!(handle_workjet_project_upsert_command(root.path(), &control, "owner-1").is_err());
        Ok(())
    }

    #[test]
    fn project_list_is_owner_scoped_bounded_and_does_not_require_working_copies(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        create_project(root.path())?;
        handle_workjet_project_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id": "other-project", "name": "Other"}),
            ),
            "owner-2",
        )?;
        let listed = handle_workjet_project_list_command(
            root.path(),
            &command("ctox.workjet.project.list", json!({"limit": 100})),
            "owner-1",
        )?;
        assert_eq!(listed["count"], 1);
        assert!(listed.get("projects").is_none());
        assert!(outbound_load_record(
            &open_store(root.path())?,
            WORKING_COPIES_COLLECTION,
            "project-1"
        )?
        .is_none());
        Ok(())
    }

    #[test]
    fn project_list_returns_only_twelve_active_ids_from_sixteen_owner_rows() -> anyhow::Result<()> {
        let root = tempdir()?;
        for index in 0..16 {
            handle_workjet_project_upsert_command(
                root.path(),
                &command(
                    "ctox.workjet.project.upsert",
                    json!({
                        "project_id": format!("project-{index:02}"),
                        "name": format!("Project {index}"),
                        "archived": index >= 12,
                    }),
                ),
                "owner-1",
            )?;
        }
        handle_workjet_project_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.project.upsert",
                json!({"project_id": "foreign-active", "name": "Foreign"}),
            ),
            "owner-2",
        )?;
        let listed = handle_workjet_project_list_command(
            root.path(),
            &command("ctox.workjet.project.list", json!({"limit": 12})),
            "owner-1",
        )?;
        assert_eq!(listed["count"], 12);
        assert_eq!(listed["truncated"], false);
        assert!(listed.get("projects").is_none());
        let mut ids: Vec<_> = listed["project_ids"]
            .as_array()
            .context("active id window missing")?
            .iter()
            .map(|id| id.as_str().context("invalid project id"))
            .collect::<anyhow::Result<_>>()?;
        ids.sort();
        let expected: Vec<_> = (0..12).map(|index| format!("project-{index:02}")).collect();
        assert_eq!(ids, expected);
        let bounded = handle_workjet_project_list_command(
            root.path(),
            &command("ctox.workjet.project.list", json!({"limit": 10})),
            "owner-1",
        )?;
        assert_eq!(bounded["count"], 10);
        assert_eq!(bounded["project_ids"].as_array().unwrap().len(), 10);
        assert_eq!(bounded["truncated"], true);
        let conn = open_store(root.path())?;
        assert_eq!(
            outbound_load_records_by_string_field(
                &conn,
                PROJECTS_COLLECTION,
                "owner_user_id",
                "owner-1",
            )?
            .len(),
            16,
            "archived rows remain durable",
        );
        Ok(())
    }

    #[test]
    fn working_copy_requires_project_and_has_deterministic_id() -> anyhow::Result<()> {
        let root = tempdir()?;
        let guest_path = "guest://computer-1/workspaces/project-one";
        let missing = command(
            "ctox.workjet.working_copy.upsert",
            json!({
                "project_id": "missing",
                "computer_id": "computer-1",
                "path": guest_path,
                "active": true
            }),
        );
        assert!(
            handle_workjet_working_copy_upsert_command(root.path(), &missing, "owner-1").is_err()
        );

        create_project(root.path())?;
        let upsert = command(
            "ctox.workjet.working_copy.upsert",
            json!({
                "project_id": "project-1",
                "computer_id": "computer-1",
                "path": guest_path,
                "active": true
            }),
        );
        let first = handle_workjet_working_copy_upsert_command(root.path(), &upsert, "owner-1")?;
        let second = handle_workjet_working_copy_upsert_command(root.path(), &upsert, "owner-1")?;
        assert!(first["working_copy"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("workjet_wc_")));
        assert_eq!(first["working_copy"]["id"], second["working_copy"]["id"]);
        assert_eq!(
            first["working_copy"]["_rev"],
            second["working_copy"]["_rev"]
        );
        assert_eq!(first["working_copy"]["computer_id"], "computer-1");
        assert_eq!(first["working_copy"]["path"], guest_path);
        assert!(first["working_copy"].get("verified_at_ms").is_none());
        Ok(())
    }

    #[test]
    fn working_copy_treats_guest_path_as_opaque_and_rejects_active_supplied_ids(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        create_project(root.path())?;
        let opaque = handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-1",
                    "computer_id": "computer-1",
                    "path": "Z:\\Workjet\\not-on-the-ctox-host",
                    "active": true
                }),
            ),
            "owner-1",
        )?;
        assert_eq!(
            opaque["working_copy"]["path"],
            "Z:\\Workjet\\not-on-the-ctox-host"
        );

        let supplied_id = command(
            "ctox.workjet.working_copy.upsert",
            json!({
                "project_id": "project-1",
                "computer_id": "computer-1",
                "path": "guest-path",
                "active": true,
                "working_copy_id": "browser-chosen"
            }),
        );
        assert!(
            handle_workjet_working_copy_upsert_command(root.path(), &supplied_id, "owner-1")
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn working_copy_retry_does_not_duplicate_and_second_computer_is_distinct() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        create_project(root.path())?;
        let first_computer = command(
            "ctox.workjet.working_copy.upsert",
            json!({
                "project_id": "project-1",
                "computer_id": "computer-1",
                "path": "guest://shared/project-one",
                "active": true
            }),
        );
        let first =
            handle_workjet_working_copy_upsert_command(root.path(), &first_computer, "owner-1")?;
        let retry =
            handle_workjet_working_copy_upsert_command(root.path(), &first_computer, "owner-1")?;
        let second = handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-1",
                    "computer_id": "computer-2",
                    "path": "guest://shared/project-one",
                    "active": true
                }),
            ),
            "owner-1",
        )?;
        assert_eq!(first["working_copy"]["id"], retry["working_copy"]["id"]);
        assert_ne!(first["working_copy"]["id"], second["working_copy"]["id"]);

        let copies = outbound_load_records_by_string_field(
            &open_store(root.path())?,
            WORKING_COPIES_COLLECTION,
            "project_id",
            "project-1",
        )?;
        assert_eq!(copies.len(), 2);
        Ok(())
    }

    #[test]
    fn working_copy_detach_is_idempotent_without_host_path_access() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_project(root.path())?;
        let attached = handle_workjet_working_copy_upsert_command(
            root.path(),
            &command(
                "ctox.workjet.working_copy.upsert",
                json!({
                    "project_id": "project-1",
                    "computer_id": "computer-1",
                    "path": "/opaque/guest/checkout",
                    "label": "Primary checkout",
                    "active": true
                }),
            ),
            "owner-1",
        )?;
        let working_copy_id = attached["working_copy"]["id"]
            .as_str()
            .context("working-copy id missing")?
            .to_owned();
        let detach = command(
            "ctox.workjet.working_copy.upsert",
            json!({
                "project_id": "project-1",
                "computer_id": "computer-1",
                "active": false,
                "working_copy_id": working_copy_id
            }),
        );
        let first = handle_workjet_working_copy_upsert_command(root.path(), &detach, "owner-1")?;
        let second = handle_workjet_working_copy_upsert_command(root.path(), &detach, "owner-1")?;
        assert_eq!(first["working_copy"]["status"], "detached");
        assert_eq!(first["working_copy"]["label"], "Primary checkout");
        assert_eq!(
            first["working_copy"]["_rev"],
            second["working_copy"]["_rev"]
        );
        Ok(())
    }
}
