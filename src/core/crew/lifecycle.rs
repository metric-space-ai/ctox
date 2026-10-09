use super::*;

#[cfg(test)]
thread_local! {
    pub(super) static AFTER_RETENTION_READS: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        std::cell::RefCell::new(None);
}

/// Install in migration and when the independently initialized flow ledger appears.
/// Old best-effort emissions could race before PR-2b introduced this constraint.
pub(crate) fn ensure_selection_event_index(conn: &Connection) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ctox_harness_flow_events')",
        [], |r| r.get(0),
    )?;
    if !exists {
        // The flow ledger is independently and lazily initialized. Creating it
        // during queue schema setup would activate unrelated pump paths before
        // any worker starts. Admission installs the index before its first event.
        return Ok(());
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_crew_selection_diagnostic_time
        ON ctox_harness_flow_events(created_at)
        WHERE event_kind IN ('crew_selected','crew_selection_unavailable')
          AND COALESCE(json_extract(metadata_json,'$.repaired'),0)=0;",
    )?;
    let indexed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_crew_selection_event_attempt')",
        [], |r| r.get(0),
    )?;
    if indexed {
        return Ok(());
    }
    let mut removed = 0;
    loop {
        let changed = conn.execute(
            "DELETE FROM ctox_harness_flow_events WHERE event_id IN (
                SELECT event_id FROM (
                    SELECT event_id, ROW_NUMBER() OVER (
                        PARTITION BY json_extract(metadata_json,'$.attempt_id')
                        ORDER BY created_at,event_id
                    ) AS ordinal
                    FROM ctox_harness_flow_events
                    WHERE event_kind='crew_selected'
                      AND json_extract(metadata_json,'$.attempt_id') IS NOT NULL
                ) WHERE ordinal>1 LIMIT 128
            )",
            [],
        )?;
        removed += changed;
        if changed == 0 {
            break;
        }
    }
    if removed > 0 {
        eprintln!(
            "[ctox crew] removed {removed} duplicate selection events; oldest evidence retained"
        );
    }
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_crew_selection_event_attempt
         ON ctox_harness_flow_events(json_extract(metadata_json,'$.attempt_id'))
         WHERE event_kind='crew_selected';",
    )?;
    Ok(())
}

/// One bounded maintenance batch. Unfinished active work is never evicted.
/// Legacy rows with durable finalization evidence did run and are not orphans.
pub(crate) fn retain_attempts(conn: &Connection, now: i64) -> Result<()> {
    let cutoff = chrono::DateTime::from_timestamp_millis(now.saturating_sub(15 * 60 * 1000))
        .context("invalid crew sweep timestamp")?
        .to_rfc3339();
    let has_finalizations: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='worker_attempt_finalizations')",
        [], |r| r.get(0),
    )?;
    if has_finalizations {
        conn.execute(
            "UPDATE crew_attempts SET started_at=selected_at WHERE attempt_id IN (
                SELECT a.attempt_id FROM crew_attempts a
                WHERE a.started_at IS NULL AND EXISTS (
                    SELECT 1 FROM worker_attempt_finalizations f WHERE f.attempt_id=a.attempt_id
                ) ORDER BY a.selected_at,a.attempt_id LIMIT 128
            )",
            [],
        )?;
    }
    // Candidate discovery is read-only and may be expensive on a populated
    // ledger. Do it without a transaction; each deletion revalidates its
    // authority under its own short IMMEDIATE writer reservation below.
    let orphans = conn
        .prepare(
            "SELECT attempt_id,task_id,member_id,selected_at FROM crew_attempts
         WHERE started_at IS NULL AND finalized_at IS NULL AND selected_at<?1
           AND NOT EXISTS (SELECT 1 FROM communication_routing_state r
               WHERE r.message_key=crew_attempts.task_id AND r.route_status='leased')
         ORDER BY selected_at,attempt_id LIMIT 128",
        )?
        .query_map([&cutoff], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_events: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ctox_harness_flow_events')", [], |r| r.get(0),
    )?;
    #[cfg(test)]
    AFTER_RETENTION_READS.with(|slot| {
        if let Some(callback) = slot.borrow_mut().as_mut() {
            callback();
        }
    });
    for (attempt, task, member, selected) in orphans {
        let tx = crate::persistence::SqliteWriteTransaction::begin(conn, "crew.retention_orphan")?;
        // A task can start or be leased after candidate discovery. Neither its
        // attempt nor selection event may be removed using the old snapshot.
        let still_orphan: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM crew_attempts a WHERE attempt_id=?1
             AND task_id=?2 AND member_id=?3 AND selected_at=?4
             AND started_at IS NULL AND finalized_at IS NULL
             AND NOT EXISTS(SELECT 1 FROM communication_routing_state r
                 WHERE r.message_key=a.task_id AND r.route_status='leased'))",
            params![attempt, task, member, selected],
            |r| r.get(0),
        )?;
        if !still_orphan {
            continue;
        }
        // Do not discard finalizations outside this batch's migration window.
        let current_finalizations = has_finalizations || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='worker_attempt_finalizations')",
            [], |r|r.get::<_, bool>(0),
        )?;
        if current_finalizations
            && tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM worker_attempt_finalizations WHERE attempt_id=?1)",
                [&attempt],
                |r| r.get::<_, bool>(0),
            )?
        {
            continue;
        }
        if has_events {
            tx.execute(
                "INSERT OR IGNORE INTO crew_projection_tombstones(event_id)
                SELECT event_id FROM ctox_harness_flow_events WHERE event_kind='crew_selected'
                AND json_extract(metadata_json,'$.attempt_id')=?1 LIMIT 128",
                [&attempt],
            )?;
            tx.execute(
                "DELETE FROM ctox_harness_flow_events WHERE event_kind='crew_selected'
                AND json_extract(metadata_json,'$.attempt_id')=?1",
                [&attempt],
            )?;
        }
        tx.execute("DELETE FROM crew_attempts WHERE attempt_id=?1", [&attempt])?;
        tx.execute(
            "UPDATE crew_members SET stats_json=json_set(stats_json,'$.last_active_at',(
            SELECT COALESCE(started_at,finalized_at) FROM crew_attempts WHERE member_id=?1
            AND (started_at IS NOT NULL OR finalized_at IS NOT NULL)
            ORDER BY selected_at DESC,attempt_id DESC LIMIT 1
        )),updated_at=?3 WHERE id=?1 AND json_extract(stats_json,'$.last_active_at')=?2",
            params![member, selected, chrono::Utc::now().to_rfc3339()],
        )?;
        tx.execute(
            "UPDATE communication_routing_state SET crew_member_id=NULL
            WHERE message_key=?1 AND crew_member_id=?2 AND NOT EXISTS (
                SELECT 1 FROM crew_attempts WHERE task_id=?1 AND member_id=?2
                AND (started_at IS NOT NULL OR finalized_at IS NOT NULL)
            )",
            params![task, member],
        )?;
        tx.commit()?;
    }
    // One bounded autocommit statement: no orphan read snapshot to promote,
    // and no write lock retained across the preceding candidates.
    conn.execute(
        "DELETE FROM crew_attempts WHERE attempt_id IN (
            SELECT a.attempt_id FROM crew_attempts a
            WHERE a.finalized_at IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM communication_routing_state r
                  WHERE r.message_key=a.task_id AND r.route_status NOT IN ('handled','failed','cancelled'))
              AND a.attempt_id NOT IN (
                  SELECT attempt_id FROM crew_attempts WHERE finalized_at IS NOT NULL
                  ORDER BY finalized_at DESC,attempt_id DESC LIMIT 500
              )
            ORDER BY a.finalized_at,a.attempt_id LIMIT 128
        )", [],
    )?;
    Ok(())
}
