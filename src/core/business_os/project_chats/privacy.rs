// Origin: CTOX
// License: AGPL-3.0-only
use super::{is_command, is_owned_collection, owned_project, CHATS, MEMBERS};
use crate::business_os::store::{open_store, outbound_load_record, BusinessCommand};
use anyhow::ensure;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn chat_id(value: &str) -> bool {
    value.starts_with("workjet_private_") || value.starts_with("workjet_group_")
}

fn profile_binding_id(value: &str) -> bool {
    value.starts_with("workjet_profile_")
}

/// Only typed envelope/reference fields are followed. Message body text is not
/// interpreted as an identity or silently turned into an authorization grant.
fn references(document: &Value, result: &mut BTreeSet<String>) {
    let Some(object) = document.as_object() else {
        return;
    };
    for (key, value) in object {
        if key == "id"
            || key == "record_id"
            || key == "binding_id"
            || key.ends_with("chat_id")
            || key.ends_with("thread_id")
        {
            if let Some(value) = value.as_str() {
                if chat_id(value) || profile_binding_id(value) {
                    result.insert(value.to_owned());
                }
            }
        }
    }
    for field in [
        "payload",
        "client_context",
        "result",
        "metadata",
        "source_context",
        "source",
    ] {
        if let Some(value) = object.get(field) {
            references(value, result);
        }
    }
}

fn project_command(document: &Value) -> bool {
    document
        .get("command_type")
        .and_then(Value::as_str)
        .is_some_and(|kind| is_command(kind) && kind.starts_with("ctox.workjet.project."))
}

// These existing projections form a bounded chain: run/event → queue →
// business command. Follow their typed IDs so private results cannot bypass
// the chat policy merely by omitting a direct thread_id.
fn associations<'a>(collection: &str, document: &'a Value) -> Vec<(&'static str, &'a str)> {
    let mut result = Vec::new();
    if matches!(
        collection,
        "ctox_queue_tasks"
            | "ctox_runs"
            | "ctox_harness_events"
            | "outbound_approvals"
            | "outbound_messages"
    ) {
        for key in ["command_id", "ctox_command_id"] {
            if let Some(id) = document[key].as_str().filter(|id| !id.is_empty()) {
                result.push(("business_commands", id));
            }
        }
    }
    if matches!(
        collection,
        "ctox_runs" | "ctox_harness_events" | "outbound_approvals" | "outbound_messages"
    ) {
        if let Some(id) = document["task_id"].as_str().filter(|id| !id.is_empty()) {
            result.push(("ctox_queue_tasks", id));
        }
    }
    result
}

pub(in crate::business_os) fn has_restricted_reference(collection: &str, document: &Value) -> bool {
    if is_owned_collection(collection) || project_command(document) {
        return true;
    }
    let mut ids = BTreeSet::new();
    references(document, &mut ids);
    !ids.is_empty() || !associations(collection, document).is_empty()
}

/// A constraint on the normal policy, never a replacement permission grant.
/// None preserves the existing behavior of records outside this new contract.
pub(in crate::business_os) fn document_visible_to_actor(
    root: &Path,
    collection: &str,
    document: &Value,
    user_id: &str,
) -> Option<bool> {
    if !has_restricted_reference(collection, document) {
        return None;
    }
    let Ok(conn) = open_store(root) else {
        return Some(false);
    };
    visible_in_store(&conn, collection, document, user_id)
}

pub(super) fn visible_in_store(
    conn: &Connection,
    collection: &str,
    document: &Value,
    user_id: &str,
) -> Option<bool> {
    if user_id.is_empty() {
        return Some(false);
    }
    let mut references_to_check = BTreeSet::new();
    references(document, &mut references_to_check);
    if is_owned_collection(collection) {
        let Some(id) = document["id"].as_str() else {
            return Some(false);
        };
        let Ok(Some(stored)) = outbound_load_record(conn, collection, id) else {
            return Some(false);
        };
        if stored["owner_user_id"] != user_id || stored["is_deleted"] == true {
            return Some(false);
        }
        if collection == CHATS || collection == MEMBERS {
            let Some(project_id) = stored["project_id"].as_str() else {
                return Some(false);
            };
            if owned_project(conn, project_id, user_id, false).is_err() {
                return Some(false);
            }
        }
    }
    if project_command(document) {
        let Some(project_id) = document["payload"]["project_id"].as_str() else {
            return Some(false);
        };
        if owned_project(conn, project_id, user_id, false).is_err() {
            return Some(false);
        }
    }
    let mut restricted = is_owned_collection(collection)
        || project_command(document)
        || !references_to_check.is_empty();
    for (related_collection, id) in associations(collection, document) {
        match outbound_load_record(conn, related_collection, id) {
            Ok(Some(related)) => {
                match visible_in_store(conn, related_collection, &related, user_id) {
                    Some(false) => return Some(false),
                    Some(true) => restricted = true,
                    None => {}
                }
            }
            // Missing legacy references retain the existing policy. New chat
            // producers must persist their direct thread identity as well;
            // absence of a relation never grants access to a reserved chat ID.
            Ok(None) => {}
            Err(_) => return Some(false),
        }
    }
    for id in &references_to_check {
        let related_collection = if chat_id(id) {
            CHATS
        } else {
            super::super::worker_profile_bindings::COLLECTION
        };
        let Ok(Some(relation)) = outbound_load_record(conn, related_collection, id) else {
            return Some(false);
        };
        if relation["owner_user_id"] != user_id || relation["is_deleted"] == true {
            return Some(false);
        }
        if chat_id(id) {
            let Some(project_id) = relation["project_id"].as_str() else {
                return Some(false);
            };
            if owned_project(conn, project_id, user_id, false).is_err() {
                return Some(false);
            }
        }
    }
    restricted.then_some(true)
}

pub(in crate::business_os) fn command_access_check(
    root: &Path,
    command: &BusinessCommand,
    authenticated_user: &str,
) -> anyhow::Result<()> {
    let document = serde_json::to_value(command)?;
    let access =
        document_visible_to_actor(root, "business_commands", &document, authenticated_user);
    ensure!(
        access != Some(false),
        "Workjet chat is unavailable to this user or instance"
    );
    if access.is_none() {
        return Ok(());
    }
    // User↔worker private chats never grant another human access through the
    // generic Threads assignee/mention/notification fields. Project worker
    // membership has its own command and is not a human participant grant.
    if command.command_type.starts_with("threads.") {
        for key in [
            "target_user_ids",
            "participant_ids",
            "watcher_user_ids",
            "assignee_user_ids",
        ] {
            if let Some(values) = command.payload.get(key).and_then(Value::as_array) {
                ensure!(
                    values
                        .iter()
                        .all(|value| value.as_str() == Some(authenticated_user)),
                    "Workjet chat cannot add another human participant"
                );
            }
        }
        for key in [
            "target_user_id",
            "assigned_user_id",
            "user_id",
            "reviewer_user_id",
        ] {
            if let Some(value) = command.payload.get(key).and_then(Value::as_str) {
                ensure!(
                    value.is_empty() || value == authenticated_user,
                    "Workjet chat cannot assign another human participant"
                );
            }
        }
    }
    Ok(())
}
