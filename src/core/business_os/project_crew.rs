//! Resolve a private Workjet project's Crew identity from native relationships.
use super::{project_chats, store, worker_profile_bindings};
use anyhow::{ensure, Context};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;

/// A queue metadata field or a client-selected member is not an identity grant.
/// Follow the canonical command/task link, then the authenticated project chat.
pub(crate) fn project_crew_member_for_task(
    root: &Path,
    task_id: &str,
) -> anyhow::Result<Option<String>> {
    let core = Connection::open_with_flags(
        crate::paths::core_db(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    // A legacy/non-command queue may have no command ledger at all. Do not
    // initialize a store or infer a private chat from its prompt/metadata.
    let has_links: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_command_task_links')",
        [], |row| row.get(0),
    )?;
    if !has_links {
        return Ok(None);
    }
    let commands = core
        .prepare("SELECT command_id FROM business_command_task_links WHERE task_id=?1 LIMIT 2")?
        .query_map([task_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        commands.len() <= 1,
        "project Crew task has ambiguous command links"
    );
    let Some(command_id) = commands.into_iter().next() else {
        return Ok(None);
    };
    let command = crate::mission::channels::business_command_projection(root, &command_id)?;
    let Some(chat_id) = command
        .pointer("/payload/thread_id")
        .and_then(Value::as_str)
        .filter(|id| id.starts_with("workjet_private_"))
    else {
        return Ok(None);
    };
    ensure!(
        command["command_type"] == "business_os.chat.task",
        "private Workjet Crew requires a chat task"
    );
    let authority = store::revalidate_business_command_execution_authorization(root, &command_id)?;
    let owner = authority
        .pointer("/actor/id")
        .and_then(Value::as_str)
        .context("project Crew owner is missing")?;
    let mut conn = Connection::open_with_flags(
        store::business_os_store_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let tx = conn.transaction()?;
    let target = target_for_chat(&tx, owner, chat_id)?;
    tx.commit()?;
    let member = target.member_id;
    if let Some(expected) = command
        .pointer("/payload/workjet_crew_member_id")
        .and_then(Value::as_str)
    {
        ensure!(
            expected == member,
            "project Crew identity changed since submission"
        );
    }
    if let Some(expected) = command
        .pointer("/payload/external_executor/executor_id")
        .and_then(Value::as_str)
    {
        ensure!(
            expected == target.computer_id,
            "project executor changed since submission"
        );
    }
    ensure!(
        crate::crew::members(&core)?
            .iter()
            .any(|candidate| candidate.id == member && !candidate.archived),
        "project Crew member is unavailable or archived"
    );
    Ok(Some(member))
}

pub(super) struct ProjectCrewTarget {
    pub member_id: String,
    pub computer_id: String,
}

#[cfg(test)]
pub(super) fn member_for_chat(
    conn: &Connection,
    owner: &str,
    chat_id: &str,
) -> anyhow::Result<String> {
    Ok(target_for_chat(conn, owner, chat_id)?.member_id)
}

pub(super) fn target_for_chat(
    conn: &Connection,
    owner: &str,
    chat_id: &str,
) -> anyhow::Result<ProjectCrewTarget> {
    let relation = store::outbound_load_record(conn, project_chats::CHATS, chat_id)?
        .context("private project chat is unavailable")?;
    ensure!(
        relation["id"] == chat_id
            && relation["thread_id"] == chat_id
            && relation["kind"] == "private"
            && relation["owner_user_id"] == owner
            && relation["is_deleted"] != true,
        "private project chat identity differs"
    );
    let project_id = project_chats::text(&relation, "project_id")?;
    project_chats::owned_project(conn, project_id, owner, true)?;
    let profile = project_chats::text(&relation, "worker_profile_id")?;
    let membership_id = project_chats::stable_id("workjet_member", &[owner, project_id, profile]);
    let membership = store::outbound_load_record(conn, project_chats::MEMBERS, &membership_id)?
        .context("project worker membership is missing")?;
    ensure!(
        membership["owner_user_id"] == owner
            && membership["project_id"] == project_id
            && membership["worker_profile_id"] == profile
            && membership["status"] == "active"
            && membership["is_deleted"] != true,
        "project worker is not active"
    );
    let thread = store::outbound_load_record(conn, project_chats::THREADS, chat_id)?
        .context("project chat history is missing")?;
    ensure!(
        thread["owner_user_id"] == owner && thread["is_deleted"] != true,
        "project chat history is unavailable"
    );
    let binding = worker_profile_bindings::require_active(conn, owner, profile)?;
    Ok(ProjectCrewTarget {
        member_id: project_chats::text(&binding, "crew_member_id")?.to_owned(),
        computer_id: project_chats::text(&binding, "computer_id")?.to_owned(),
    })
}
