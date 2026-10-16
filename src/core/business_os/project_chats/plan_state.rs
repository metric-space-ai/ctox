// Origin: CTOX
// License: AGPL-3.0-only
//! Read-only plan facts; identities come from the current native project goal.
use anyhow::Result;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;

pub(in crate::business_os) fn available(core: &Connection) -> Result<bool> {
    Ok(core.query_row("SELECT COUNT(*)=3 FROM sqlite_master WHERE type='table' AND name IN ('planned_steps','planned_goals','workjet_project_goal_definitions')", [], |r| r.get(0))?)
}
pub(in crate::business_os) fn failed_from_core(
    core: &Connection,
    owner: &str,
    project: &str,
    thread: &str,
) -> Result<Option<String>> {
    if !available(core)? {
        return Ok(None);
    }
    Ok(core
        .query_row(
            "SELECT s.step_id FROM workjet_project_goal_definitions d
        JOIN planned_goals g ON g.goal_id=d.goal_id AND g.thread_key=d.supervisor_thread_key
        JOIN planned_steps s ON s.goal_id=g.goal_id
        WHERE d.owner_user_id=?1 AND d.project_id=?2 AND d.supervisor_thread_key=?3
          AND g.status IN ('active','blocked','failed') AND s.status='failed'
        ORDER BY s.step_order,s.step_id LIMIT 1",
            params![owner, project, thread],
            |r| r.get(0),
        )
        .optional()?)
}
pub(in crate::business_os) fn failed(
    root: &Path,
    owner: &str,
    project: &str,
    thread: &str,
) -> Result<Option<String>> {
    let path = crate::paths::core_db(root);
    if !path.exists() {
        return Ok(None);
    }
    let core = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    failed_from_core(&core, owner, project, thread)
}

/// Patch just the derived metadata under the existing writer reservation.
/// Never replace a concurrent message/title/role update with an older snapshot.
pub(in crate::business_os) fn project_threads(root: &Path, core: &Connection) -> Result<()> {
    use crate::business_os::store;
    use serde_json::json;
    if !store::business_os_store_path(root).is_file() {
        return Ok(());
    }
    let conn = store::open_store(root)?;
    let bindings: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_supervisor_bindings')",
        [],
        |r| r.get(0),
    )?;
    if !bindings {
        return Ok(());
    }
    let rows = conn
        .prepare("SELECT project_id,owner_user_id,thread_id FROM workjet_supervisor_bindings")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (project, owner, thread) in rows {
        if super::owned_project(&conn, &project, &owner, false).is_err() {
            continue;
        }
        let failed = failed_from_core(
            core,
            &owner,
            &project,
            &format!("business-os/threads/{thread}"),
        )?;
        let tx =
            crate::persistence::SqliteWriteTransaction::begin(&conn, "plan.thread_projection")?;
        let Some(mut record) = store::outbound_load_record(&tx, "user_threads", &thread)? else {
            continue;
        };
        if record["owner_user_id"] != owner || record["source_record_id"] != project {
            continue;
        }
        let value = json!(failed);
        let changed = record.pointer("/metadata/failed_step_id") != Some(&value);
        if changed {
            if !record["metadata"].is_object() {
                record["metadata"] = json!({});
            }
            record["metadata"]["failed_step_id"] = value;
            record["updated_at_ms"] = json!(store::now_ms());
            store::upsert_business_record(
                &tx,
                "user_threads",
                &thread,
                record["updated_at_ms"].as_i64().unwrap(),
                record.clone(),
            )?;
        }
        tx.commit()?;
        // Re-deliver a failed mirror without emitting another update for the
        // unchanged thread on every worker/plan notification. Read the latest
        // source after the patch so a concurrent chat edit is not overwritten.
        let mut mirror = store::BusinessProjectionWriter::open(root)?;
        if let Some(current) =
            store::outbound_load_record(mirror.source_connection(), "user_threads", &thread)?
        {
            if !mirror.stored_projection_matches("user_threads", &thread, &current)? {
                store::upsert_rxdb_collection_record(
                    root,
                    "user_threads",
                    &thread,
                    current["updated_at_ms"].as_i64().unwrap_or(0),
                    current,
                )?;
            }
        }
    }
    Ok(())
}

/// Source timestamps, never inferred from worker prose or projection time.
fn millis(value: Option<&str>) -> Option<i64> {
    let ms = chrono::DateTime::parse_from_rfc3339(value?)
        .ok()?
        .timestamp_millis();
    (1..=9_007_199_254_740_991).contains(&ms).then_some(ms)
}

pub(in crate::business_os) fn project_steps(root: &Path, core: &Connection) -> Result<()> {
    use crate::business_os::store;
    use serde_json::{json, Value};
    use std::collections::BTreeSet;
    if !available(core)? {
        return Ok(());
    }
    let mut writer = store::BusinessProjectionWriter::open(root)?;
    let rows = core
        .prepare(
            "SELECT d.project_id,d.owner_user_id,d.supervisor_thread_key,d.goal_id,d.revision,
        g.status,g.source_prompt,s.step_id,s.title,s.status,s.updated_at,s.completed_at
        FROM workjet_project_goal_definitions d JOIN planned_goals g
        ON g.goal_id=d.goal_id AND g.thread_key=d.supervisor_thread_key
        JOIN planned_steps s ON s.goal_id=g.goal_id
        WHERE g.status IN ('active','blocked','failed','completed')
        ORDER BY d.project_id,s.step_order,s.step_id",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut current = BTreeSet::new();
    for (
        project,
        owner,
        thread,
        goal,
        revision,
        goal_status,
        prompt,
        id,
        title,
        status,
        updated,
        completed,
    ) in rows
    {
        if super::owned_project(writer.source_connection(), &project, &owner, false).is_err() {
            continue;
        }
        let Some(todo) = id.strip_prefix(&format!("{goal}::")) else {
            continue;
        };
        let prompt: Value = serde_json::from_str(&prompt)?;
        let Some(item) = prompt["items"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == todo))
        else {
            continue;
        };
        let mut doc = json!({"id":id,"goal_id":goal,"goal_revision":revision,"goal_status":goal_status,
            "step_id":todo,"status":status,"project_id":project,"owner":item["owner"],"owner_user_id":owner,
            "title":title,"failed_step_id":failed_from_core(core,&owner,&project,&thread)?});
        let stamp = millis(Some(&updated));
        if let Some(t) = stamp {
            doc["updated_at_ms"] = json!(t);
        }
        if status == "completed" {
            if let Some(t) = millis(completed.as_deref()) {
                doc["completed_at_ms"] = json!(t);
            }
        }
        if !writer.stored_projection_matches("planned_steps", &id, &doc)? {
            writer.upsert_source_projection("planned_steps", &id, stamp.unwrap_or(0), doc)?;
        }
        current.insert(id);
    }
    let old = writer
        .source_connection()
        .prepare(
            "SELECT record_id FROM business_records WHERE collection='planned_steps' AND deleted=0",
        )?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in old {
        if !current.contains(&id) {
            writer.tombstone_source_projection("planned_steps", &id, store::now_ms() as i64)?;
        }
    }
    Ok(())
}

/// Run before the generic Admin path. A projected row is no authority to read a
/// foreign/currently replaced goal, even if its owner/project fields are forged.
pub(in crate::business_os) fn visible(
    core: &Connection,
    policy: &Connection,
    doc: &serde_json::Value,
    user: &str,
) -> Result<bool> {
    if !available(core)? {
        return Ok(false);
    }
    let owner = crate::business_os::workjet_identity::owner_from_connection(policy, user)?;
    let Some(project) = doc["project_id"].as_str() else {
        return Ok(false);
    };
    if super::owned_project(policy, project, &owner, false).is_err() {
        return Ok(false);
    }
    Ok(core.query_row(
        "SELECT EXISTS(SELECT 1 FROM workjet_project_goal_definitions d
        JOIN planned_goals g ON g.goal_id=d.goal_id AND g.thread_key=d.supervisor_thread_key
        JOIN planned_steps s ON s.goal_id=g.goal_id
        WHERE d.project_id=?1 AND d.owner_user_id=?2 AND d.goal_id=?3 AND d.revision=?4
        AND s.step_id=?5 AND g.status IN ('active','blocked','failed','completed'))",
        params![
            project,
            owner,
            doc["goal_id"].as_str(),
            doc["goal_revision"].as_i64(),
            doc["id"].as_str()
        ],
        |r| r.get(0),
    )?)
}

pub(in crate::business_os) fn visible_from_root(
    root: &Path,
    doc: &serde_json::Value,
    user: &str,
) -> bool {
    let read = || -> Result<bool> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
        let policy = Connection::open_with_flags(
            crate::business_os::store::business_os_store_path(root),
            flags,
        )?;
        core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        policy.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        visible(&core, &policy, doc, user)
    };
    read().unwrap_or(false)
}

#[cfg(test)]
pub(in crate::business_os) fn run_contract(
    operation: impl FnOnce(tempfile::TempDir) -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let mut root = tempfile::tempdir()?;
    // The outer guard owns cleanup: all daemon handles must shut down first,
    // including when a contract assertion panics inside run_core_sync.
    let path = root.path().to_path_buf();
    let _cleanup = Cleanup(path.clone());
    root.disable_cleanup(true);
    crate::app_capabilities::cap3::tests::run_with_core_owners(path, move || operation(root))
}
