use super::*;

const MAX_REPAIR_ATTEMPTS: usize = 3;
const REPAIR_BACKOFF_SECONDS: i64 = 300;
const REPAIR_EXHAUSTED: &str =
    "scrape repair retry limit exhausted (3 attempts); operator intervention required";

/// Admission and creation share SQLite's write lock. Verification from a
/// running repair must reuse its lease, regardless of run, prompt or caller.
pub fn create_scrape_repair_queue_task(
    root: &Path,
    target_type: &str,
    target: &str,
    mut request: QueueTaskCreateRequest,
) -> Result<QueueTaskView> {
    let target_type = target_type.trim();
    let target = target.trim();
    anyhow::ensure!(
        !target_type.is_empty() && !target.is_empty(),
        "repair target must not be empty"
    );
    let mut conn = open_channel_db(&resolve_db_path(root, None))?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let history = {
        let mut query = tx.prepare(
            "SELECT m.message_key, r.route_status, r.updated_at
             FROM communication_messages m
             JOIN communication_routing_state r ON r.message_key=m.message_key
             WHERE m.account_key=?1 AND json_valid(m.metadata_json)
               AND json_extract(m.metadata_json, '$.scrape_repair.target_key')=?2
               AND COALESCE(json_extract(m.metadata_json, '$.scrape_repair.target_type'), 'scrape')=?3
             ORDER BY r.updated_at DESC, COALESCE(json_extract(m.metadata_json, '$.scrape_repair.attempt'), 0) DESC, m.message_key DESC",
        )?;
        let rows = query.query_map(params![QUEUE_ACCOUNT_KEY, target, target_type], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if let Some((key, _, _)) = history
        .iter()
        .find(|(_, status, _)| !matches!(status.as_str(), "handled" | "failed" | "cancelled"))
    {
        let task = load_queue_task_from_conn(&tx, key)?.context("open repair task disappeared")?;
        tx.commit()?;
        return Ok(task);
    }
    // A successful repair resets consecutive failures. Cancel is an operator
    // stop; a new probe must not resurrect it. Explicit queue release can.
    if let Some((key, status, _)) = history.first() {
        if status == "cancelled" {
            let task =
                load_queue_task_from_conn(&tx, key)?.context("cancelled repair disappeared")?;
            tx.commit()?;
            return Ok(task);
        }
    }
    let failures = history
        .iter()
        .take_while(|(_, status, _)| status == "failed")
        .count();
    if failures >= MAX_REPAIR_ATTEMPTS {
        let key = &history[0].0;
        tx.execute(
            "UPDATE communication_routing_state SET last_error=?2 WHERE message_key=?1 AND route_status='failed'",
            params![key, REPAIR_EXHAUSTED],
        )?;
        let task = load_queue_task_from_conn(&tx, key)?.context("failed repair disappeared")?;
        refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
        tx.commit()?;
        return Ok(task);
    }
    let metadata = request.extra_metadata.get_or_insert_with(|| json!({}));
    metadata["scrape_repair"]["target_key"] = json!(target);
    metadata["scrape_repair"]["target_type"] = json!(target_type);
    metadata["scrape_repair"]["attempt"] = json!(failures + 1);
    metadata["scrape_repair"]["max_attempts"] = json!(MAX_REPAIR_ATTEMPTS);
    let task = create_queue_task_with_metadata_tx(&tx, request)?;
    if failures > 0 {
        let prior_finished = chrono::DateTime::parse_from_rfc3339(&history[0].2)?;
        let retry_at = prior_finished
            + chrono::Duration::seconds(REPAIR_BACKOFF_SECONDS * (1_i64 << (failures - 1)));
        tx.execute(
            "UPDATE communication_routing_state SET retry_not_before=?2 WHERE message_key=?1",
            params![task.message_key, retry_at.to_rfc3339()],
        )?;
    }
    let task =
        load_queue_task_from_conn(&tx, &task.message_key)?.context("repair task disappeared")?;
    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
    tx.commit()?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(run: usize) -> QueueTaskCreateRequest {
        QueueTaskCreateRequest {
            title: "repair scrape target maps-google-com".to_string(),
            prompt: format!("Repair failure from run {run}"),
            thread_key: format!("caller-{run}"),
            workspace_root: None,
            priority: "normal".to_string(),
            suggested_skill: None,
            parent_message_key: None,
            extra_metadata: Some(json!({"scrape_repair": {"run_id": run}})),
        }
    }

    #[test]
    fn incident_scrape_repair_three_submissions_have_one_open_identity() {
        let root = std::env::temp_dir().join(format!("ctox-repair-dedup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let first = create_scrape_repair_queue_task(&root, "scrape", "maps-google-com", request(0))
            .unwrap();
        lease_queue_task(&root, &first.message_key, "worker").unwrap();
        for run in 1..3 {
            let task =
                create_scrape_repair_queue_task(&root, "scrape", "maps-google-com", request(run))
                    .unwrap();
            assert_eq!(task.message_key, first.message_key);
            assert_eq!(task.route_status, "leased");
        }
        assert_eq!(
            list_queue_tasks(&root, &["pending".into(), "leased".into()], 100)
                .unwrap()
                .len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incident_scrape_repair_stops_after_three_failures_with_durable_backoff() {
        let root =
            std::env::temp_dir().join(format!("ctox-repair-budget-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut keys = Vec::new();
        for attempt in 0..3 {
            let task = create_scrape_repair_queue_task(
                &root,
                "scrape",
                "maps-google-com",
                request(attempt),
            )
            .unwrap();
            assert!(!keys.contains(&task.message_key));
            let conn = open_channel_db(&resolve_db_path(&root, None)).unwrap();
            let retry: Option<String> = conn
                .query_row(
                    "SELECT retry_not_before FROM communication_routing_state WHERE message_key=?1",
                    params![task.message_key],
                    |row| row.get(0),
                )
                .unwrap();
            if attempt == 0 {
                assert!(retry.is_none());
            } else {
                let delay = chrono::DateTime::parse_from_rfc3339(&retry.unwrap())
                    .unwrap()
                    .signed_duration_since(chrono::Utc::now())
                    .num_seconds();
                assert!(delay >= (300 * (1 << (attempt - 1))) - 10);
                assert!(lease_queue_task(&root, &task.message_key, "worker").is_err());
            }
            drop(conn);
            ack_leased_messages_with_failure_reason(
                &root,
                &[task.message_key.clone()],
                "failed",
                "repair verification failed",
            )
            .unwrap();
            keys.push(task.message_key);
        }
        let exhausted =
            create_scrape_repair_queue_task(&root, "scrape", "maps-google-com", request(4))
                .unwrap();
        assert_eq!(exhausted.message_key, *keys.last().unwrap());
        assert_eq!(exhausted.route_status, "failed");
        assert_eq!(exhausted.status_note.as_deref(), Some(REPAIR_EXHAUSTED));
        assert!(
            list_queue_tasks(&root, &["pending".into(), "leased".into()], 100)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            list_queue_tasks(&root, &["failed".into()], 100)
                .unwrap()
                .len(),
            3
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
