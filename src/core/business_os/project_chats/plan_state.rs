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
