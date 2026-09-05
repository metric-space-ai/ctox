// Origin: CTOX
// License: Apache-2.0

use super::*;

/// Repair only stale active projections. Canonical queue/command state is never
/// changed here. Holding the attached transaction fences lease admission and
/// projection writes against this recovery decision.
pub(crate) fn reconcile_stale_queue_projections(
    root: &Path,
    active_keys: &HashSet<String>,
) -> anyhow::Result<usize> {
    if !crate::paths::core_db(root).is_file()
        || (!business_os_store_path(root).is_file() && !rxdb_store_path(root).is_file())
    {
        return Ok(0);
    }
    drop(open_store(root)?);
    let mut conn = channels::open_channel_db(&crate::paths::core_db(root))?;
    attach_business_os_projection_store(root, &conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let now = now_ms() as i64;
    let cutoff = now.saturating_sub(BUSINESS_OS_QUEUE_ORPHAN_REPAIR_AGE_MS);
    let mut candidates = BTreeMap::<String, (Value, i64)>::new();
    {
        let mut statement = tx.prepare(
            "SELECT record_id, payload_json, updated_at_ms FROM business_os_projection.business_records
             WHERE collection='ctox_queue_tasks' AND deleted=0
               AND json_valid(payload_json)
               AND json_extract(payload_json, '$.status') IN ('running','leased')",
        )?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })? {
            let (id, raw_json, updated) = row?;
            candidates.insert(id, (serde_json::from_str(&raw_json)?, updated));
        }
    }
    // Native-only documents are invisible to the historical repair CLI. Read
    // the actual collection, with its current schema version, as well.
    let native_table = attached_rxdb_collection_table(&tx, "ctox_queue_tasks")?;
    let mut native_current_sql = None;
    if let Some(table) = native_table {
        let columns = attached_rxdb_table_columns(&tx, &table)?;
        let last_write = if columns.contains("lastWriteTime") {
            "lastWriteTime"
        } else {
            "0"
        };
        let deleted = if columns.contains("deleted") {
            "deleted"
        } else {
            "COALESCE(json_extract(data, '$._deleted'), 0)"
        };
        let qualified = format!(
            "business_os_rxdb_projection.{}",
            sqlite_quote_identifier(&table)
        );
        native_current_sql = Some(format!(
            "SELECT data, {deleted}, COALESCE(json_extract(data, '$.updated_at_ms'), {last_write}, 0)
             FROM {qualified} WHERE id=?1"
        ));
        let mut statement = tx.prepare(&format!(
            "SELECT id, data, CAST(COALESCE(json_extract(data, '$.updated_at_ms'), {last_write}, 0) AS INTEGER) FROM {qualified}
             WHERE json_valid(data) AND {deleted}=0
               AND json_extract(data, '$.status') IN ('running','leased')"
        ))?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })? {
            let (id, raw_json, updated) = row?;
            candidates.insert(id, (serde_json::from_str(&raw_json)?, updated));
        }
    }
    let mut repaired = 0;
    for (id, (mut payload, mut projection_updated)) in candidates {
        // Never resurrect a native tombstone or overwrite a terminal
        // native document merely because its business_records mirror is stale.
        if let Some(sql) = &native_current_sql {
            if let Some((raw_json, deleted, updated)) = tx
                .query_row(sql, params![id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                })
                .optional()?
            {
                let current: Value = serde_json::from_str(&raw_json)?;
                if deleted
                    || !matches!(
                        current.get("status").and_then(Value::as_str),
                        Some("running" | "leased")
                    )
                {
                    continue;
                }
                payload = current;
                projection_updated = updated as i64;
            }
        }
        let command_id = first_string_field(&payload, &["command_id"]);
        // `id` is the original QueueTaskView.message_key. The redundant field
        // was historically absent; its absence alone says nothing about liveness.
        let mut keys = vec![id.clone()];
        for field in ["message_key", "task_id"] {
            if let Some(key) = payload.get(field).and_then(Value::as_str) {
                if !key.trim().is_empty() && !keys.iter().any(|item| item == key) {
                    keys.push(key.to_string());
                }
            }
        }
        if let Some(command_id) = command_id.as_deref() {
            if let Some(key) = tx
                .query_row(
                    "SELECT task_id FROM business_command_task_links WHERE command_id=?1",
                    params![command_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        let mut source = None;
        let mut live = false;
        for key in &keys {
            let route = tx
                .query_row(
                    "SELECT route_status,
                        route_status='leased' AND COALESCE(trim(lease_owner),'') <> ''
                        AND datetime(COALESCE(NULLIF(trim(lease_expires_at),''),
                                     datetime(leased_at, '+15 minutes'))) > datetime('now')
                 FROM communication_routing_state WHERE message_key=?1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<bool>>(1)?.unwrap_or(false),
                        ))
                    },
                )
                .optional()?;
            if let Some((status, owns_live_lease)) = route {
                if owns_live_lease
                    || (active_keys.contains(key)
                        && matches!(status.as_str(), "leased" | "running"))
                {
                    live = true;
                    break;
                }
                if source.is_none() {
                    source = Some((key.clone(), status));
                }
            } else if active_keys.contains(key) {
                live = true;
                break;
            }
        }
        if live {
            continue;
        }
        // A known canonical outcome wins immediately. Only an orphan/expired
        // lease needs the grace period; rewriting an incorrect projection must
        // not postpone a durable cancellation indefinitely.
        let stale = projection_updated <= cutoff;
        let (route_status, reason) = if let Some((key, status)) = &source {
            payload["message_key"] = Value::String(key.clone());
            match status.as_str() {
                "pending" | "handled" | "failed" | "cancelled" | "blocked" => (
                    status.clone(),
                    format!("Stale running projection reconciled from canonical queue state {status}."),
                ),
                _ if stale => ("failed".to_string(),
                    "Stale running projection has no live canonical queue lease after the projection TTL.".to_string()),
                _ => continue,
            }
        } else {
            let command_status = if let Some(command_id) = command_id.as_deref() {
                tx.query_row(
                    "SELECT status FROM business_os_projection.business_commands WHERE command_id=?1",
                    params![command_id],
                    |row| row.get::<_, String>(0),
                ).optional()?
            } else {
                None
            };
            match command_status.as_deref().and_then(projection_route_status_for_command_status) {
                Some(status) if matches!(status, "handled" | "failed" | "cancelled" | "blocked") => (
                    status.to_string(), "Stale running projection reconciled from terminal business command.".to_string()),
                _ if stale => ("failed".to_string(),
                    "Orphaned running projection: no canonical queue row or terminal business command found after the projection TTL.".to_string()),
                _ => continue,
            }
        };
        let status = normalize_queue_status(&route_status);
        payload["status"] = Value::String(status.to_string());
        payload["task_status"] = Value::String(status.to_string());
        payload["route_status"] = Value::String(route_status.clone());
        payload["execution_phase"] =
            Value::String(queue_projection_execution_phase(&route_status, None));
        payload["terminal_status"] =
            Value::String(queue_projection_terminal_status(&route_status).to_string());
        payload["repair_note"] = Value::String(reason.clone());
        payload["status_note"] = Value::String(reason.clone());
        payload["lease_owner"] = Value::Null;
        payload["leased_at"] = Value::Null;
        payload["lease_expires_at"] = Value::Null;
        payload["lease_worker_id"] = Value::Null;
        payload["_deleted"] = Value::Bool(false);
        payload["_meta"]["lwt"] = Value::from(now);
        if route_status == "failed" {
            payload["error"] = Value::String(reason);
        }
        upsert_attached_business_record(&tx, "ctox_queue_tasks", &id, now, payload.clone())?;
        upsert_attached_rxdb_record(&tx, "ctox_queue_tasks", &id, now, payload)?;
        repaired += 1;
    }
    tx.commit()?;
    Ok(repaired)
}

#[cfg(test)]
#[path = "store_queue_reconcile_tests.rs"]
mod tests;
