// Origin: CTOX
// License: AGPL-3.0-only

//! Per-user order of the project gallery. The order is a server-side
//! preference of the authenticated user; project existence and ownership are
//! not re-checked here, so stale ids are ignored by the browser and never
//! expose another user's project.
use super::store::{business_os_store_path, open_store, BusinessCommand};
use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const MAX_PROJECTS: usize = 500;
const MAX_ID_CHARS: usize = 128;
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workjet_project_gallery_order (
    owner_user_id TEXT PRIMARY KEY,
    revision INTEGER NOT NULL,
    project_ids_json TEXT NOT NULL
);";

pub(super) fn is_command(kind: &str) -> bool {
    matches!(
        kind,
        "ctox.workjet.project.gallery.order.read" | "ctox.workjet.project.gallery.order.set"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GalleryOrder {
    pub(crate) revision: u64,
    pub(crate) project_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetGalleryOrderRequest {
    operation_id: String,
    expected_revision: u64,
    project_ids: Vec<String>,
}

// Command-bus routing metadata is outside the request contract.
fn payload(command: &BusinessCommand) -> anyhow::Result<Value> {
    let mut value = command.payload.clone();
    let object = value
        .as_object_mut()
        .context("gallery order payload must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let channel = channel
            .as_str()
            .context("inbound_channel must be a string")?;
        ensure!(
            !channel.trim().is_empty() && channel.chars().count() <= 256,
            "invalid inbound_channel"
        );
    }
    Ok(value)
}

fn validate_id(id: &str, field: &str) -> anyhow::Result<()> {
    ensure!(!id.trim().is_empty(), "{field} must not be blank");
    ensure!(
        id.chars().count() <= MAX_ID_CHARS,
        "{field} exceeds {MAX_ID_CHARS} characters"
    );
    Ok(())
}

fn validate_order(project_ids: &[String]) -> anyhow::Result<()> {
    ensure!(
        project_ids.len() <= MAX_PROJECTS,
        "gallery order exceeds {MAX_PROJECTS} projects"
    );
    let mut seen = std::collections::HashSet::new();
    for id in project_ids {
        validate_id(id, "project_id")?;
        ensure!(seen.insert(id.as_str()), "gallery order repeats a project");
    }
    Ok(())
}

fn load(conn: &Connection, owner: &str) -> anyhow::Result<GalleryOrder> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_project_gallery_order')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(GalleryOrder {
            revision: 0,
            project_ids: vec![],
        });
    }
    let stored: Option<(u64, String)> = conn
        .query_row(
            "SELECT revision,project_ids_json FROM workjet_project_gallery_order WHERE owner_user_id=?1",
            [owner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match stored {
        None => Ok(GalleryOrder {
            revision: 0,
            project_ids: vec![],
        }),
        Some((revision, data)) => {
            let project_ids: Vec<String> = serde_json::from_str(&data)?;
            validate_order(&project_ids)?;
            Ok(GalleryOrder {
                revision,
                project_ids,
            })
        }
    }
}

pub(super) fn handle_command(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
) -> anyhow::Result<Value> {
    match command.command_type.as_str() {
        "ctox.workjet.project.gallery.order.read" => {
            ensure!(
                payload(command)?
                    .as_object()
                    .is_some_and(|object| object.is_empty()),
                "gallery order read takes no fields"
            );
            let reader = Connection::open_with_flags(
                business_os_store_path(root),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            reader.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
            let owner = super::workjet_identity::owner_from_connection(&reader, actor)?;
            let order = load(&reader, &owner)?;
            Ok(json!({"ok":true,"order":order}))
        }
        "ctox.workjet.project.gallery.order.set" => {
            let request: SetGalleryOrderRequest = serde_json::from_value(payload(command)?)?;
            validate_id(&request.operation_id, "operation_id")?;
            validate_order(&request.project_ids)?;
            let mut conn = open_store(root)?;
            let tx = conn.transaction()?;
            tx.execute_batch(SCHEMA)?;
            let owner = super::workjet_identity::owner_from_connection(&tx, actor)?;
            let previous = load(&tx, &owner)?;
            ensure!(
                previous.revision == request.expected_revision,
                "gallery order revision conflict"
            );
            let revision = previous
                .revision
                .checked_add(1)
                .context("gallery order revision exhausted")?;
            tx.execute(
                "INSERT INTO workjet_project_gallery_order(owner_user_id,revision,project_ids_json)
                VALUES(?1,?2,?3)
                ON CONFLICT(owner_user_id) DO UPDATE SET
                    revision=excluded.revision, project_ids_json=excluded.project_ids_json",
                params![
                    owner,
                    revision,
                    serde_json::to_string(&request.project_ids)?
                ],
            )?;
            tx.commit()?;
            let order = GalleryOrder {
                revision,
                project_ids: request.project_ids,
            };
            Ok(json!({"ok":true,"order":order}))
        }
        _ => anyhow::bail!("unsupported project gallery order command"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_rejects_repeated_and_blank_ids() {
        assert!(validate_order(&["a".into(), "a".into()]).is_err());
        assert!(validate_order(&["  ".into()]).is_err());
        assert!(validate_order(&["a".into(), "b".into()]).is_ok());
    }

    #[test]
    fn order_rejects_more_than_the_gallery_limit() {
        let ids: Vec<String> = (0..=MAX_PROJECTS).map(|i| format!("p{i}")).collect();
        assert!(validate_order(&ids).is_err());
    }

    #[test]
    fn set_request_rejects_unknown_fields() {
        let value =
            json!({"operation_id":"op","expected_revision":0,"project_ids":[],"owner_user_id":"x"});
        assert!(serde_json::from_value::<SetGalleryOrderRequest>(value).is_err());
    }
}
