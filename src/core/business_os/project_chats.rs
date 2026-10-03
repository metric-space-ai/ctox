// Origin: CTOX
// License: AGPL-3.0-only

//! Workjet project/chat relationships. Messages remain in the existing Threads
//! store; these native-owned records carry identity and membership, not content.
use super::domain_effect::{AppliedDomainEffect, DomainEffectAdmission, DomainRecordRef};
use super::store::{
    open_store, outbound_load_record, upsert_rxdb_collection_record, BusinessCommand,
};
use anyhow::{ensure, Context};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

mod privacy;
pub(super) use privacy::{
    command_access_check, document_visible_to_actor, has_restricted_reference,
    VisibilityReadContext,
};

#[cfg(test)]
mod tests;

pub(super) const CHATS: &str = "workjet_project_chats";
pub(super) const MEMBERS: &str = "workjet_project_workers";
pub(super) const THREADS: &str = "user_threads";
pub(super) const CONTRACT: &str = "workjet-project-chats.v1";

#[derive(Clone, Debug)]
pub(super) struct Projection {
    pub collection: &'static str,
    pub id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPayload {
    project_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerPayload {
    project_id: String,
    worker_profile_id: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatPayload {
    project_id: String,
    worker_profile_id: String,
    title: String,
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
}

pub(super) fn is_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.workjet.project.chat.ensure"
            | "ctox.workjet.project.worker.add"
            | "ctox.workjet.project.worker.remove"
            | "ctox.workjet.project.chat.create"
            | "ctox.workjet.worker_profile.bind"
            | "ctox.workjet.worker_profile.unbind"
    )
}

pub(super) fn is_owned_collection(collection: &str) -> bool {
    matches!(
        collection,
        CHATS | MEMBERS | super::worker_profile_bindings::COLLECTION
    )
}

pub(super) fn handle_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner: &str,
    admission: &DomainEffectAdmission,
) -> anyhow::Result<Value> {
    let owner = required(authorized_owner, "authenticated user", 256)?;
    super::worker_profile_bindings::validate_crew_reference(root, command)?;
    let mut conn = open_store(root)?;
    let applied = admission.apply(&mut conn, |tx| {
        let mut projections = Vec::new();
        let result = apply_command(root, tx, command, &owner, &mut projections)?;
        Ok(AppliedDomainEffect {
            result,
            projections: domain_references(projections),
        })
    })?;
    Ok(applied.result)
}

pub(super) fn domain_references(projections: Vec<Projection>) -> Vec<DomainRecordRef> {
    projections
        .into_iter()
        .map(|projection| DomainRecordRef {
            collection: projection.collection.to_owned(),
            id: projection.id,
        })
        .collect()
}

fn apply_command(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    owner: &str,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<Value> {
    let now = super::store::now_ms() as i64;
    match command.command_type.as_str() {
        "ctox.workjet.project.chat.ensure" => {
            let payload: ProjectPayload = serde_json::from_value(command.payload.clone())?;
            let project = owned_project(conn, &payload.project_id, owner, false)?;
            let group_id = ensure_default_group(conn, &project, projections)?;
            Ok(json!({"ok": true, "contract": CONTRACT, "group_chat_id": group_id}))
        }
        "ctox.workjet.project.worker.add" | "ctox.workjet.project.worker.remove" => {
            let payload: WorkerPayload = serde_json::from_value(command.payload.clone())?;
            let project = owned_project(conn, &payload.project_id, owner, true)?;
            let project_id = text(&project, "id")?;
            let worker_id = required(&payload.worker_profile_id, "worker_profile_id", 256)?;
            let adding = command.command_type.ends_with(".add");
            if adding {
                super::worker_profile_bindings::require_active(conn, owner, &worker_id)?;
            }
            let group_id = ensure_default_group(conn, &project, projections)?;
            let membership_id = stable_id("workjet_member", &[owner, project_id, &worker_id]);
            let existing = outbound_load_record(conn, MEMBERS, &membership_id)?;
            if !adding && existing.is_none() {
                return Ok(
                    json!({"ok": true, "contract": CONTRACT, "group_chat_id": group_id, "member": false}),
                );
            }
            let membership = json!({
                "id": membership_id, "project_id": project_id, "owner_user_id": owner,
                "worker_profile_id": worker_id, "group_chat_id": group_id,
                "status": if adding { "active" } else { "removed" },
                "created_at_ms": existing.as_ref().and_then(|v| v["created_at_ms"].as_i64()).unwrap_or(now),
                "updated_at_ms": now, "is_deleted": false,
            });
            persist(conn, MEMBERS, &membership_id, membership, projections)?;
            let first_chat_id = if adding {
                let id = stable_id("workjet_private", &[owner, project_id, &worker_id, "first"]);
                ensure_chat(
                    conn,
                    &id,
                    &project,
                    Some(&worker_id),
                    true,
                    "Chat",
                    projections,
                )?;
                Some(id)
            } else {
                None
            };
            Ok(json!({
                "ok": true, "contract": CONTRACT, "group_chat_id": group_id,
                "membership_id": membership_id, "first_chat_id": first_chat_id, "member": adding,
            }))
        }
        "ctox.workjet.project.chat.create" => {
            let payload: ChatPayload = serde_json::from_value(command.payload.clone())?;
            let project = owned_project(conn, &payload.project_id, owner, true)?;
            let project_id = text(&project, "id")?;
            let worker_id = required(&payload.worker_profile_id, "worker_profile_id", 256)?;
            let title = required(&payload.title, "title", 256)?;
            super::worker_profile_bindings::require_active(conn, owner, &worker_id)?;
            let membership_id = stable_id("workjet_member", &[owner, project_id, &worker_id]);
            let membership = outbound_load_record(conn, MEMBERS, &membership_id)?
                .context("add this worker to the project before opening another chat")?;
            ensure!(
                membership["status"] == "active",
                "worker is not an active project member"
            );
            let operation_id = required(
                command.id.as_deref().context("command id is required")?,
                "command id",
                256,
            )?;
            // A deliberate new command creates a distinct chat. Replaying that
            // command retains its chat and history; no name/model participates.
            let chat_id = stable_id(
                "workjet_private",
                &[owner, project_id, &worker_id, "explicit", &operation_id],
            );
            ensure_chat(
                conn,
                &chat_id,
                &project,
                Some(&worker_id),
                false,
                &title,
                projections,
            )?;
            Ok(json!({"ok": true, "contract": CONTRACT, "chat_id": chat_id}))
        }
        "ctox.workjet.worker_profile.bind" | "ctox.workjet.worker_profile.unbind" => {
            super::worker_profile_bindings::apply_command(root, conn, command, owner, projections)
        }
        _ => anyhow::bail!("unsupported Workjet project-chat command"),
    }
}

/// Called inside project upsert's existing domain transaction as well as by
/// explicit ensure for projects created before this contract was installed.
pub(super) fn ensure_default_group(
    conn: &Connection,
    project: &Value,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<String> {
    let project_id = text(project, "id")?;
    let owner = text(project, "owner_user_id")?;
    let chat_id = stable_id("workjet_group", &[owner, project_id]);
    ensure_chat(
        conn,
        &chat_id,
        project,
        None,
        true,
        text(project, "name")?,
        projections,
    )?;
    Ok(chat_id)
}

fn ensure_chat(
    conn: &Connection,
    chat_id: &str,
    project: &Value,
    worker_id: Option<&str>,
    initial: bool,
    title: &str,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<()> {
    let owner = text(project, "owner_user_id")?;
    let project_id = text(project, "id")?;
    let kind = if worker_id.is_some() {
        "private"
    } else {
        "group"
    };
    let relation = outbound_load_record(conn, CHATS, chat_id)?;
    let thread = outbound_load_record(conn, THREADS, chat_id)?;
    if let Some(relation) = relation {
        ensure!(
            relation["is_deleted"] != true
                && relation["owner_user_id"] == owner
                && relation["project_id"] == project_id
                && relation["kind"] == kind
                && relation["initial"] == initial
                && relation.get("worker_profile_id").and_then(Value::as_str) == worker_id,
            "chat identity conflicts with an existing relationship"
        );
        let thread = thread.context("existing chat history is unavailable; repair is required")?;
        ensure!(
            thread["is_deleted"] != true,
            "existing chat was deleted; it cannot be recreated"
        );
        projections.push(Projection {
            collection: CHATS,
            id: chat_id.to_owned(),
        });
        projections.push(Projection {
            collection: THREADS,
            id: chat_id.to_owned(),
        });
        return Ok(());
    }
    ensure!(
        thread.is_none(),
        "refusing to adopt unrelated existing chat history"
    );
    let now = super::store::now_ms() as i64;
    let mut relation = json!({
        "id": chat_id, "thread_id": chat_id, "project_id": project_id,
        "owner_user_id": owner, "kind": kind, "initial": initial,
        "created_at_ms": now, "updated_at_ms": now, "is_deleted": false,
    });
    if let Some(worker_id) = worker_id {
        relation["worker_profile_id"] = Value::String(worker_id.to_owned());
    }
    let thread = json!({
        "id": chat_id, "thread_id": chat_id, "title": title, "kind": "chat",
        "status": "open", "owner_user_id": owner, "created_by_id": owner,
        "participant_ids": [owner], "watcher_user_ids": [], "assigned_user_id": "",
        "source_module": "ctox", "source_record_type": "workjet_project",
        "source_record_id": project_id, "source_label": project["name"],
        "metadata": {"workjet_contract": CONTRACT},
        "created_at_ms": now, "updated_at_ms": now, "is_deleted": false,
        "last_message_id": "", "last_message_at_ms": 0, "pending_approval_count": 0,
        "last_seen_by_user": {}, "snoozed_until_ms": 0, "archived_at_ms": 0,
    });
    persist(conn, THREADS, chat_id, thread, projections)?;
    persist(conn, CHATS, chat_id, relation, projections)?;
    Ok(())
}

pub(super) fn owned_project(
    conn: &Connection,
    project_id: &str,
    owner: &str,
    require_active: bool,
) -> anyhow::Result<Value> {
    let project_id = required(project_id, "project_id", 128)?;
    let project = outbound_load_record(conn, "workjet_projects", &project_id)?
        .context("project is unavailable in this instance")?;
    ensure!(
        project["owner_user_id"] == owner && project["is_deleted"] != true,
        "project is unavailable to this user"
    );
    ensure!(
        !require_active || project["status"] == "active",
        "project is archived"
    );
    Ok(project)
}

pub(super) fn persist(
    conn: &Connection,
    collection: &'static str,
    id: &str,
    record: Value,
    projections: &mut Vec<Projection>,
) -> anyhow::Result<()> {
    let existing = outbound_load_record(conn, collection, id)?;
    let now = (super::store::now_ms() as i64).max(
        existing
            .as_ref()
            .and_then(|v| v["updated_at_ms"].as_i64())
            .unwrap_or(0)
            .saturating_add(1),
    );
    super::store_workjet_projects::persist_idempotently(conn, collection, id, now, record)?;
    projections.push(Projection {
        collection,
        id: id.to_owned(),
    });
    Ok(())
}

pub(super) fn publish(root: &Path, projections: &[Projection]) -> anyhow::Result<()> {
    let conn = open_store(root)?;
    for projection in projections {
        let record = outbound_load_record(&conn, projection.collection, &projection.id)?
            .context("committed chat projection is unavailable")?;
        let updated = record["updated_at_ms"]
            .as_i64()
            .context("projection has no timestamp")?;
        upsert_rxdb_collection_record(
            root,
            projection.collection,
            &projection.id,
            updated,
            record,
        )?;
    }
    Ok(())
}

pub(super) fn required(value: &str, field: &str, max: usize) -> anyhow::Result<String> {
    let value = value.trim();
    ensure!(
        !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control),
        "{field} must be nonempty, bounded text without control characters"
    );
    Ok(value.to_owned())
}

pub(super) fn text<'a>(value: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("missing {field}"))
}

pub(super) fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("{prefix}_{:x}", hash.finalize())
}
