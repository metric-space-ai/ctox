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
use serde::Deserialize;
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
    description: Option<String>,
    #[serde(default)]
    archived: bool,
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
    authorized_owner_email: Option<&str>,
    admission: Option<&DomainEffectAdmission>,
) -> anyhow::Result<Value> {
    if command.command_type != "ctox.workjet.project.upsert" {
        migrate_signed_owner_alias(root, authorized_owner_user_id, authorized_owner_email)?;
    }
    match command.command_type.as_str() {
        "ctox.workjet.project.list" => {
            handle_workjet_project_list_command(root, command, authorized_owner_user_id)
        }
        "ctox.workjet.project.upsert" => handle_workjet_project_upsert_command(
            root,
            command,
            authorized_owner_user_id,
            admission.context("new Workjet project mutation requires domain admission")?,
            authorized_owner_email,
        ),
        "ctox.workjet.working_copy.upsert" => {
            handle_workjet_working_copy_upsert_command(root, command, authorized_owner_user_id)
        }
        other => anyhow::bail!("unsupported Workjet project command type: {other}"),
    }
}

fn migrate_signed_owner_alias(
    root: &Path,
    authorized_owner_user_id: &str,
    authorized_owner_email: Option<&str>,
) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    let mut projections = Vec::new();
    apply_signed_owner_alias_migration(
        &conn,
        authorized_owner_user_id,
        authorized_owner_email,
        &mut projections,
    )?;
    super::project_chats::publish(root, &projections)
}

fn apply_signed_owner_alias_migration(
    conn: &Connection,
    authorized_owner_user_id: &str,
    authorized_owner_email: Option<&str>,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<()> {
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let Some(owner_email) = authorized_owner_email else {
        return Ok(());
    };
    let owner_email = bounded_required(owner_email, "owner_email", 320)?.to_ascii_lowercase();
    anyhow::ensure!(
        owner_email.contains('@'),
        "owner_email must be an email address"
    );
    if owner_email == owner_user_id.to_ascii_lowercase() {
        return Ok(());
    }

    let now = super::store::now_ms() as i64;
    for collection in [PROJECTS_COLLECTION, WORKING_COPIES_COLLECTION] {
        let records = outbound_load_records_by_string_field(
            &conn,
            collection,
            "owner_user_id",
            &owner_email,
        )?;
        for mut record in records {
            if record.get("is_deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let record_id = record
                .get("id")
                .and_then(Value::as_str)
                .with_context(|| format!("{collection} owner migration record has no id"))?
                .to_owned();
            record["owner_user_id"] = Value::String(owner_user_id.clone());
            record["updated_at_ms"] = Value::from(now);
            persist_idempotently(conn, collection, &record_id, now, record)?;
            projections.push(Projection {
                collection,
                id: record_id,
            });
        }
    }
    Ok(())
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
    projects.retain(|project| project.get("is_deleted").and_then(Value::as_bool) != Some(true));
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
    Ok(serde_json::json!({
        "ok": true,
        "collection": PROJECTS_COLLECTION,
        "count": count,
        "truncated": truncated,
    }))
}

pub(super) fn handle_workjet_project_upsert_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    admission: &DomainEffectAdmission,
    authorized_owner_email: Option<&str>,
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
    let description = optional_bounded(payload.description, "description", 4096)?;

    let mut conn = open_store(root)?;
    let applied = admission.apply(&mut conn, |transaction| {
        let mut chat_projections = Vec::new();
        apply_signed_owner_alias_migration(
            transaction,
            &owner_user_id,
            authorized_owner_email,
            &mut chat_projections,
        )?;
        let now = super::store::now_ms() as i64;
        let existing = outbound_load_record(&transaction, PROJECTS_COLLECTION, &project_id)?;
        ensure_owned(existing.as_ref(), &owner_user_id, "project")?;
        let created_at_ms = existing
            .as_ref()
            .and_then(|record| record.get("created_at_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(now);
        let status = if payload.archived {
            "archived"
        } else {
            "active"
        };
        let archived_at_ms = if payload.archived {
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
        if let Some(description) = description {
            project["description"] = Value::String(description);
        }
        if payload.archived {
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
            (PROJECTS_COLLECTION, 0),
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
    fn signed_email_alias_migrates_projects_and_working_copies_to_stable_user_id(
    ) -> anyhow::Result<()> {
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
        assert_eq!(listed["count"], 1);

        let conn = open_store(root.path())?;
        let project = outbound_load_record(&conn, PROJECTS_COLLECTION, "project-email")?
            .context("migrated project")?;
        assert_eq!(project["owner_user_id"], stable_user_id);
        let working_copy = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, working_copy_id)?
            .context("migrated working copy")?;
        assert_eq!(working_copy["owner_user_id"], stable_user_id);
        assert_eq!(
            load_rxdb_collection_record(root.path(), PROJECTS_COLLECTION, "project-email")?
                .context("project projection")?["owner_user_id"],
            stable_user_id
        );
        assert_eq!(
            load_rxdb_collection_record(root.path(), WORKING_COPIES_COLLECTION, working_copy_id,)?
                .context("working-copy projection")?["owner_user_id"],
            stable_user_id
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
