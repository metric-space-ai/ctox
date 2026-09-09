// Origin: CTOX
// License: Apache-2.0

use super::app_runtime;
use super::desktop_files::{upsert_desktop_file_with_policy, DesktopFileContentPolicy};
use super::rxdb_peer::{
    latest_rxdb_collection_table, now_ms, projection_sleep_secs, sqlite_quote_identifier,
    sqlite_table_has_column, sync_business_users_with_database, sync_module_catalog_with_database,
    sync_runtime_settings_with_database, sync_ticket_state_with_database,
    sync_workspace_branding_with_database, upsert_business_record_projection,
    BUSINESS_COMMAND_ACTIVE_POLL_SECS, BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS,
    BUSINESS_COMMAND_IDLE_POLL_SECS, COMMAND_PLANE_METRICS,
};
use super::rxdb_peer_browser::{
    apply_browser_runtime_command, is_browser_runtime_command, mark_browser_runtime_command_failed,
};
use super::rxdb_peer_commands::{
    command_id_from_document, incremental_upsert_document_with_envelope,
    project_appsec_command_result, project_support_command_result, project_threads_command_result,
};
use super::rxdb_peer_domain_recovery;
use super::rxdb_peer_intake_state::{
    business_command_document_is_terminal, is_pending_desktop_file_replication_error,
    resolve_business_command_intake_failure_history, PendingBusinessCommandIntakeOutcome,
};
pub(super) use super::rxdb_peer_intake_state::{
    is_transient_business_command_store_error, transient_business_command_retry_document,
};
use super::rxdb_peer_projections::{
    record_native_peer_loop_result, BUSINESS_COMMANDS_LOOP_METRICS,
};
use super::store;
use anyhow::Context;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use rxdb::rx_database::RxDatabase;
use serde_json::json;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BusinessCommandsSourceStamp {
    pub(super) table: BusinessCommandsTableStamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BusinessCommandsTableStamp {
    pub(super) table_name: Option<String>,
    pub(super) pending_count: i64,
    pub(super) latest_pending_lwt_bits: u64,
}

pub(super) async fn business_commands_source_change(
    root: &Path,
    last_source_stamp: &mut Option<BusinessCommandsSourceStamp>,
) -> anyhow::Result<Option<BusinessCommandsSourceStamp>> {
    let source_stamp = business_commands_source_stamp(root).await?;
    if last_source_stamp.as_ref() == Some(&source_stamp) {
        return Ok(None);
    }
    // Commit the stamp that was actually inspected before intake starts. A
    // browser push can race the native terminal projection and write the same
    // command back to `pending_sync`. Committing a post-intake refresh would
    // bless that late overwrite as already observed and put the consumer to
    // sleep until the fallback poll. Keeping the pre-intake stamp makes the
    // next immediate round observe and canonically replay the command.
    *last_source_stamp = Some(source_stamp.clone());
    if source_stamp.table.pending_count == 0 {
        return Ok(None);
    }
    Ok(Some(source_stamp))
}

pub(super) async fn refresh_business_commands_source_stamp(
    root: &Path,
    last_source_stamp: &mut Option<BusinessCommandsSourceStamp>,
) -> anyhow::Result<()> {
    *last_source_stamp = Some(business_commands_source_stamp(root).await?);
    Ok(())
}

pub(super) async fn business_commands_source_stamp(
    root: &Path,
) -> anyhow::Result<BusinessCommandsSourceStamp> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        Ok(BusinessCommandsSourceStamp {
            table: business_commands_table_stamp(&root)?,
        })
    })
    .await
    .context("join business commands source stamp")?
}

pub(super) fn business_commands_table_stamp(
    root: &Path,
) -> anyhow::Result<BusinessCommandsTableStamp> {
    let path = store::rxdb_store_path(root);
    if !path.exists() {
        return Ok(empty_business_commands_table_stamp(None));
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| {
        format!(
            "open Business OS RxDB store for command source stamp {}",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure business command source stamp busy_timeout")?;
    let Some(table) = latest_rxdb_collection_table(&conn, "business_commands")? else {
        return Ok(empty_business_commands_table_stamp(None));
    };
    let quoted = sqlite_quote_identifier(&table);
    let deleted_expr = if sqlite_table_has_column(&conn, &table, "deleted")? {
        "deleted"
    } else {
        "0"
    };
    let lwt_expr = if sqlite_table_has_column(&conn, &table, "lastWriteTime")? {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let stamp_sql = business_commands_table_stamp_sql(&quoted, deleted_expr, lwt_expr);
    let (mut pending_count, mut latest_pending_lwt): (i64, f64) = conn
        .query_row(&stamp_sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
        .with_context(|| format!("stamp pending business_commands rows in {table}"))?;
    if let Some(predicate) = rxdb_peer_domain_recovery::retry_predicate(
        &store::business_os_store_path(root),
        &conn,
        &quoted,
        deleted_expr,
    )? {
        let (count, latest): (i64, f64) = conn.query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(MAX({lwt_expr}), 0)
                      FROM {quoted} WHERE {deleted_expr} = 0 AND {predicate}"
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        pending_count += count;
        latest_pending_lwt = latest_pending_lwt.max(latest);
    }
    Ok(BusinessCommandsTableStamp {
        table_name: Some(table),
        pending_count,
        latest_pending_lwt_bits: latest_pending_lwt.to_bits(),
    })
}

fn business_commands_table_stamp_sql(
    quoted_table: &str,
    deleted_expr: &str,
    lwt_expr: &str,
) -> String {
    // Keep the mutually exclusive lifecycle states in separate branches. A
    // single OR expression made SQLite scan every live command even though
    // the RxDB table already has a (deleted, status) expression index.
    format!(
        "SELECT COUNT(*), COALESCE(MAX(candidate_lwt), 0)
         FROM (
           SELECT {lwt_expr} AS candidate_lwt
           FROM {quoted_table}
           WHERE {deleted_expr} = 0
             AND json_extract(data, '$.status') = 'pending_sync'
           UNION ALL
           SELECT {lwt_expr} AS candidate_lwt
           FROM {quoted_table}
           WHERE {deleted_expr} = 0
             AND json_extract(data, '$.status') = 'waiting_dependencies'
           UNION ALL
           SELECT {lwt_expr} AS candidate_lwt
           FROM {quoted_table}
           WHERE {deleted_expr} = 0
             AND json_extract(data, '$.status') = 'accepted'
             AND json_extract(data, '$.command_type') IN (
               'external_sql.sync.refresh',
               'external_sql.write',
               'outbound.research_source.generate_adapter',
               'outbound.research_source.test',
               'outbound.research_source.auth_assist',
               'web_stack.person_research'
             )
           UNION ALL
           SELECT {lwt_expr} AS candidate_lwt
           FROM {quoted_table}
           WHERE {deleted_expr} = 0
             AND json_extract(data, '$.status') = 'failed'
             AND COALESCE(json_extract(data, '$.terminal_status'), 'none') = 'none'
             AND json_extract(data, '$.command_type') IN (
               'external_sql.sync.refresh',
               'external_sql.write',
               'outbound.research_source.generate_adapter',
               'outbound.research_source.test',
               'outbound.research_source.auth_assist',
               'web_stack.person_research'
             )
         )"
    )
}

pub(super) fn empty_business_commands_table_stamp(
    table_name: Option<String>,
) -> BusinessCommandsTableStamp {
    BusinessCommandsTableStamp {
        table_name,
        pending_count: 0,
        latest_pending_lwt_bits: 0,
    }
}

pub(super) async fn consume_business_commands_loop(root: PathBuf, database: Arc<RxDatabase>) {
    // Per-command failure budget. A command that keeps failing to accept
    // (e.g. a corrupt document) used to abort the WHOLE round via `?`, get
    // re-sorted to the head on the next 1s tick, and starve every command
    // behind it — browser-issued commands then appeared to hang forever.
    let mut accept_failures: HashMap<String, u32> = HashMap::new();
    let mut last_source_stamp: Option<BusinessCommandsSourceStamp> = None;
    // Do not run the comparatively expensive invariant sweep before the first
    // command intake opportunity after peer startup.
    let mut consecutive_idle_rounds = 0u32;
    loop {
        let started = Instant::now();
        let result: anyhow::Result<usize> = async {
            if business_commands_source_change(&root, &mut last_source_stamp)
                .await?
                .is_some()
            {
                let consumed =
                    consume_pending_business_commands(&root, &database, &mut accept_failures)
                        .await?;
                return Ok(consumed);
            }

            Ok(0)
        }
        .await;
        record_native_peer_loop_result(&BUSINESS_COMMANDS_LOOP_METRICS, &result, started.elapsed());
        if schedule_business_command_intake_retry(&mut last_source_stamp, &accept_failures) {
            // A fully contended SQLite write can reject both acceptance and
            // the durable failure record. In that case the RxDB source row is
            // unchanged, so no notifier or source-stamp delta can wake us.
            // Keep the retry finite and paced instead of sleeping for the
            // 30-second idle safety poll or spinning at full CPU.
            consecutive_idle_rounds = 0;
            let retry_delay_ms = if accept_failures.values().all(|attempt| *attempt == 0) {
                1_000
            } else {
                250
            };
            tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
            continue;
        }
        match result {
            Ok(0) => {
                consecutive_idle_rounds = consecutive_idle_rounds.saturating_add(1);
            }
            Ok(_) => {
                consecutive_idle_rounds = 0;
                // Drain an already-arrived burst without imposing the former
                // one-second post-command sleep.  Once the queue is empty the
                // table-change notifier below becomes the bounded idle wait.
                continue;
            }
            Err(err) => {
                consecutive_idle_rounds = 0;
                eprintln!("[business-os] native rxdb command consumer failed: {err:#}");
            }
        }
        // The current empty result has already incremented the counter. Enter
        // the event-driven wait immediately; its timeout is only a bounded
        // fallback when the SQLite notifier is unavailable.
        wait_for_business_command_wake(&root, last_source_stamp.as_ref(), consecutive_idle_rounds)
            .await;
    }
}

pub(super) fn schedule_business_command_intake_retry(
    last_source_stamp: &mut Option<BusinessCommandsSourceStamp>,
    accept_failures: &HashMap<String, u32>,
) -> bool {
    if accept_failures.is_empty() {
        return false;
    }
    // Force the next loop iteration to rescan even when neither the failed
    // intake nor its failure audit could acquire the SQLite write lock.
    *last_source_stamp = None;
    true
}

pub(super) fn business_command_poll_sleep_secs(consecutive_idle_rounds: u32) -> u64 {
    projection_sleep_secs(
        BUSINESS_COMMAND_ACTIVE_POLL_SECS,
        BUSINESS_COMMAND_IDLE_POLL_SECS,
        BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS,
        consecutive_idle_rounds,
    )
}

pub(super) async fn wait_for_business_command_wake(
    root: &Path,
    last_source_stamp: Option<&BusinessCommandsSourceStamp>,
    consecutive_idle_rounds: u32,
) {
    let sleep_for = Duration::from_secs(business_command_poll_sleep_secs(consecutive_idle_rounds));
    if consecutive_idle_rounds < BUSINESS_COMMAND_IDLE_BACKOFF_AFTER_TICKS {
        tokio::time::sleep(sleep_for).await;
        return;
    }
    let Some(table_name) = last_source_stamp
        .and_then(|stamp| stamp.table.table_name.as_deref())
        .filter(|name| !name.is_empty())
    else {
        tokio::time::sleep(sleep_for).await;
        return;
    };
    let database_path = store::rxdb_store_path(root);
    let seen_generation = rxdb::storage::sqlite::instance::table_change_generation_for_path(
        &database_path,
        table_name,
    )
    .unwrap_or(0);
    // Close the lost-wakeup window between the source-stamp check in the
    // consumer loop and arming the table notifier. If a late browser push was
    // already reflected in the generation we just sampled, compare the
    // durable source stamp once more and return immediately when it differs.
    if business_commands_source_stamp(root)
        .await
        .is_ok_and(|current| Some(&current) != last_source_stamp)
    {
        return;
    }
    rxdb::storage::sqlite::instance::wait_for_table_change_for_path(
        &database_path,
        table_name,
        seen_generation,
        sleep_for,
    )
    .await;
}

/// How often a single command may fail `accept_pending_business_command`
/// before it is marked `failed` and dropped from the pending queue.
pub(super) const BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET: u32 = 5;
pub(super) const BUSINESS_COMMAND_RETRY_CANDIDATE_SQL: &str = r#"(
  json_extract(data, '$.status') IN ('pending_sync', 'waiting_dependencies')
  OR (
    (
      json_extract(data, '$.status') = 'accepted'
      OR (
        json_extract(data, '$.status') = 'failed'
        AND COALESCE(json_extract(data, '$.terminal_status'), 'none') = 'none'
      )
    )
    AND json_extract(data, '$.command_type') IN (
      'external_sql.sync.refresh',
      'external_sql.write',
      'outbound.research_source.generate_adapter',
      'outbound.research_source.test',
      'outbound.research_source.auth_assist',
      'web_stack.person_research'
    )
  )
)"#;

pub(super) async fn consume_pending_business_commands(
    root: &Path,
    database: &Arc<RxDatabase>,
    accept_failures: &mut HashMap<String, u32>,
) -> anyhow::Result<usize> {
    let rows = pending_business_command_documents(root, 25)
        .await
        .context("load pending business_commands from RxDB SQLite")?;
    let pending_count = rows.len();
    if pending_count == 0 {
        // A command can disappear while its in-memory fallback counter is
        // waiting for a lock retry. Do not let that stale counter keep the
        // consumer on the paced active path forever.
        accept_failures.clear();
    }
    for document in rows {
        COMMAND_PLANE_METRICS.record_attempt();
        // Isolate failures per command: one broken document must not stall
        // the entire queue (it would be re-sorted to the head every tick).
        let command_id = document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let intake_result = match accept_pending_business_command(root, database, document.clone())
            .await
        {
            Ok(PendingBusinessCommandIntakeOutcome::Accepted)
            | Ok(PendingBusinessCommandIntakeOutcome::Terminalized) => Ok(()),
            Ok(PendingBusinessCommandIntakeOutcome::CanonicalReplayed) => Err(anyhow::anyhow!(
                "canonical command replay remained nonterminal"
            )),
            Ok(PendingBusinessCommandIntakeOutcome::WaitingForReplication { error }) => {
                if !command_id.is_empty() {
                    accept_failures.insert(command_id.clone(), 0);
                }
                if document.get("last_retry_error").and_then(Value::as_str) != Some(error.as_str())
                {
                    if let Some(retry_document) = transient_business_command_retry_document(
                        &document,
                        error.as_str(),
                        document
                            .get("retry_attempt")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32,
                    ) {
                        let commands = database
                            .collection("business_commands")
                            .context("business_commands collection is not registered")?;
                        incremental_upsert_document_with_envelope(
                            &commands,
                            retry_document,
                            "desktop import awaiting replicated chunks",
                        )
                        .await?;
                    }
                }
                COMMAND_PLANE_METRICS
                    .retries_total
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Ok(PendingBusinessCommandIntakeOutcome::RetryableFailure { error }) => {
                Err(anyhow::anyhow!(error))
            }
            Err(error) => Err(error),
        };
        match intake_result {
            Ok(()) => {
                COMMAND_PLANE_METRICS.record_processed(&document);
                if !command_id.is_empty() {
                    accept_failures.remove(&command_id);
                    resolve_business_command_intake_failure_history(root, &command_id).await;
                }
            }
            Err(err) => {
                COMMAND_PLANE_METRICS
                    .errors_total
                    .fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "[business-os] accepting business command `{command_id}` failed: {err:#}"
                );
                if command_id.is_empty() {
                    continue;
                }
                let failure_root = root.to_path_buf();
                let failed_document = document.clone();
                let failure_message = format!("{err:#}");
                let persisted_failure_message = failure_message.clone();
                let persisted_failure = match tokio::task::spawn_blocking(move || {
                    store::record_business_command_intake_failure(
                        &failure_root,
                        &failed_document,
                        &persisted_failure_message,
                        BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET,
                    )
                })
                .await
                {
                    Ok(Ok(value)) => value,
                    Ok(Err(persist_error)) => {
                        eprintln!(
                            "[business-os] persisting intake failure for `{command_id}` failed: {persist_error:#}"
                        );
                        let fallback_attempt = accept_failures
                            .get(&command_id)
                            .copied()
                            .unwrap_or_default()
                            .saturating_add(1);
                        json!({
                            "attempt": fallback_attempt,
                            "exhausted": false,
                            "canonical_exists": false,
                            "canonical_failure_created": false,
                        })
                    }
                    Err(join_error) => {
                        eprintln!(
                            "[business-os] joining intake failure persistence for `{command_id}` failed: {join_error}"
                        );
                        let fallback_attempt = accept_failures
                            .get(&command_id)
                            .copied()
                            .unwrap_or_default()
                            .saturating_add(1);
                        json!({
                            "attempt": fallback_attempt,
                            "exhausted": false,
                            "canonical_exists": false,
                            "canonical_failure_created": false,
                        })
                    }
                };
                let failures = persisted_failure
                    .get("attempt")
                    .and_then(Value::as_u64)
                    .unwrap_or(1) as u32;
                accept_failures.insert(command_id.clone(), failures);
                let exhausted = persisted_failure
                    .get("exhausted")
                    .and_then(Value::as_bool)
                    .unwrap_or(failures >= BUSINESS_COMMAND_ACCEPT_RETRY_BUDGET);
                if !exhausted {
                    if let Some(retry_document) = transient_business_command_retry_document(
                        &document,
                        failure_message.as_str(),
                        failures,
                    ) {
                        let commands = database
                            .collection("business_commands")
                            .context("business_commands collection is not registered")?;
                        incremental_upsert_document_with_envelope(
                            &commands,
                            retry_document,
                            "retryable business_command",
                        )
                        .await
                        .map_err(|error| {
                            anyhow::anyhow!(
                                "requeue transient business_command {command_id}: {error}"
                            )
                        })?;
                        COMMAND_PLANE_METRICS
                            .retries_total
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                }
                if exhausted {
                    if persisted_failure
                        .get("domain_effect_applied")
                        .and_then(Value::as_bool)
                        == Some(true)
                    {
                        // Keep the existing paced retry alive without projecting
                        // a new accepted/failed document on every failed delivery.
                        // Zero selects the existing one-second replication wait.
                        accept_failures.insert(command_id.clone(), 0);
                        continue;
                    }
                    COMMAND_PLANE_METRICS
                        .exhausted_total
                        .fetch_add(1, Ordering::Relaxed);
                    accept_failures.remove(&command_id);
                    let terminal_projection_ready = persisted_failure
                        .get("terminal_projection_ready")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if terminal_projection_ready {
                        let failed_patch = persisted_failure
                            .get("failure_document")
                            .cloned()
                            .unwrap_or_else(|| document.clone());
                        let commands = database
                            .collection("business_commands")
                            .context("business_commands collection is not registered")?;
                        if let Err(write_err) = incremental_upsert_document_with_envelope(
                            &commands,
                            failed_patch,
                            "failed business_command",
                        )
                        .await
                        {
                            eprintln!(
                                "[business-os] marking command `{command_id}` failed did not stick: {write_err}"
                            );
                        } else {
                            resolve_business_command_intake_failure_history(root, &command_id)
                                .await;
                        }
                    } else if persisted_failure
                        .get("canonical_exists")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        // Acceptance may already be canonical while only its
                        // RxDB projection is failing. Never overwrite that
                        // aggregate with a projection-only terminal failure.
                        if let Err(write_err) = upsert_business_record_projection(
                            root.to_path_buf(),
                            database,
                            "business_commands",
                            command_id.clone(),
                        )
                        .await
                        {
                            eprintln!(
                                "[business-os] replaying canonical command `{command_id}` after projection failure did not stick: {write_err:#}"
                            );
                        }
                    }
                } else {
                    COMMAND_PLANE_METRICS
                        .retries_total
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    Ok(pending_count)
}

pub(super) async fn pending_business_command_documents(
    root: &Path,
    limit: usize,
) -> anyhow::Result<Vec<Value>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || pending_business_command_documents_sync(&root, limit))
        .await
        .context("join pending business_commands SQLite load")?
}

pub(super) fn pending_business_command_documents_sync(
    root: &Path,
    limit: usize,
) -> anyhow::Result<Vec<Value>> {
    let path = store::rxdb_store_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| {
        format!(
            "open Business OS RxDB store for pending commands {}",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("configure pending business command busy_timeout")?;
    let Some(table) = latest_rxdb_collection_table(&conn, "business_commands")? else {
        return Ok(Vec::new());
    };
    let quoted = sqlite_quote_identifier(&table);
    let deleted_expr = if sqlite_table_has_column(&conn, &table, "deleted")? {
        "deleted"
    } else {
        "0"
    };
    let lwt_expr = if sqlite_table_has_column(&conn, &table, "lastWriteTime")? {
        "COALESCE(lastWriteTime, 0)"
    } else {
        "CAST(COALESCE(json_extract(data, '$._meta.lwt'), json_extract(data, '$.updated_at_ms'), 0) AS REAL)"
    };
    let oldest_limit = limit.saturating_add(1) / 2;
    let retry_predicate = match rxdb_peer_domain_recovery::retry_predicate(
        &store::business_os_store_path(root),
        &conn,
        &quoted,
        deleted_expr,
    )? {
        Some(applied) => format!("({BUSINESS_COMMAND_RETRY_CANDIDATE_SQL} OR {applied})"),
        None => BUSINESS_COMMAND_RETRY_CANDIDATE_SQL.to_owned(),
    };
    let newest_limit = limit.saturating_sub(oldest_limit);
    let mut documents = Vec::new();
    let mut seen_ids = HashSet::new();
    for (direction, batch_limit) in [("ASC", oldest_limit), ("DESC", newest_limit)] {
        if batch_limit == 0 {
            continue;
        }
        let mut stmt = conn
            .prepare(&format!(
                "SELECT data
                 FROM {quoted}
                 WHERE {deleted_expr} = 0
                   AND {retry_predicate}
                 ORDER BY {lwt_expr} {direction}
                 LIMIT ?1"
            ))
            .with_context(|| {
                format!("prepare pending business_commands {direction} scan in {table}")
            })?;
        let rows = stmt
            .query_map([batch_limit as i64], |row| row.get::<_, String>(0))
            .with_context(|| format!("query pending business_commands {direction} in {table}"))?;
        for row in rows {
            let raw = row.context("read pending business_command row")?;
            let document = serde_json::from_str::<Value>(&raw)
                .with_context(|| format!("parse pending business_command JSON in {table}"))?;
            let id = document
                .get("command_id")
                .or_else(|| document.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() || seen_ids.insert(id) {
                documents.push(document);
            }
        }
    }
    Ok(documents)
}

/// Verjaehrungsfrist fuer pendente Befehle. Am 19.08.2026 hat der erste
/// erfolgreiche Upgrade-Neustart seit Wochen den gesamten Befehls-Rueckstau
/// der alten Version ausgefuehrt: Befehle vom 22. Juli und 3. August wurden
/// um 12:46 erneut "accepted", darunter ein alter Lead-Schreiber, der um
/// 12:47:38 alle 19 Leads der Kampagne "Chemie-Unternehmen Recherche
/// 2026-08-16" mit dem Juli-Stand ueberschrieb. Ein Befehl, den wochenlang
/// niemand ausgefuehrt hat, ist keine Arbeit mehr, sondern eine Zeitbombe.
const STALE_BUSINESS_COMMAND_MAX_AGE_MS: u64 = 6 * 60 * 60 * 1_000;

fn business_command_created_at_ms(document: &Value) -> Option<u64> {
    ["created_at_ms", "updated_at_ms"]
        .iter()
        .find_map(|key| document.get(*key).and_then(Value::as_u64))
        .filter(|value| *value > 0)
}

/// Ein Befehl, der nie mehr gelingen kann, darf nicht ewig wiederholt werden.
/// idempotency_conflict hiess bisher RetryableFailure: der Intake sortierte
/// das Dokument jede Runde wieder nach vorn und verstopfte mit dem
/// Wiederholungskampf die Replikation aller uebrigen Collections.
fn business_command_store_error_is_permanent(err: &anyhow::Error) -> bool {
    format!("{err:#}").contains("idempotency_conflict")
}

/// Die Felder, die einen Befehl endgueltig terminal machen.
///
/// Es reicht NICHT, `status` auf "failed" zu setzen:
/// [`BUSINESS_COMMAND_RETRY_CANDIDATE_SQL`] laesst ein `failed`-Dokument
/// weiterhin als Wiederholungskandidat gelten, solange `terminal_status` fehlt.
/// Auf einer Produktivinstanz drehten deshalb zwei ueber 500 Stunden alte
/// Befehle rund 20-mal pro Sekunde durch die Verjaehrung, und der Dienst
/// verbrannte dauerhaft einen vollen Kern, nur um sie wieder und wieder
/// abzulehnen. `execution_phase` und `terminal_status` muessen dabei laut
/// `validate_terminal_pair` gemeinsam gesetzt werden.
fn apply_terminal_failure_fields(object: &mut serde_json::Map<String, Value>, error: &str) {
    object.insert("status".to_string(), Value::String("failed".to_string()));
    object.insert(
        "task_status".to_string(),
        Value::String("failed".to_string()),
    );
    object.insert(
        "execution_phase".to_string(),
        Value::String("terminal".to_string()),
    );
    object.insert(
        "terminal_status".to_string(),
        Value::String("failed".to_string()),
    );
    object.insert(
        "error_code".to_string(),
        Value::String("deadline_exceeded".to_string()),
    );
    object.insert("error".to_string(), Value::String(error.to_string()));
    object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
}

async fn terminalize_business_command_document(
    database: &Arc<RxDatabase>,
    document: &Value,
    error: &str,
) -> anyhow::Result<()> {
    let mut marked = document.clone();
    if let Some(object) = marked.as_object_mut() {
        object.remove("_rev");
        object.remove("_meta");
        apply_terminal_failure_fields(object, error);
    }
    // Nicht collection.incremental_upsert: das schrieb am 20.08.2026 ins Leere
    // (Dokument blieb pending), und die Verjaehrung verweigerte dieselben zwei
    // Zombie-Befehle im Sekundentakt — eine Endlosschleife im Journal. Der
    // kanonische Projektionsschreiber prueft den Bestand und schreibt die
    // Statusaenderung wirklich.
    let collection = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    incremental_upsert_document_with_envelope(&collection, marked, "stale business command")
        .await
        .map_err(|err| anyhow::anyhow!("terminalize stale business command: {err}"))?;
    Ok(())
}

pub(super) async fn accept_pending_business_command(
    root: &Path,
    database: &Arc<RxDatabase>,
    document: Value,
) -> anyhow::Result<PendingBusinessCommandIntakeOutcome> {
    // Reconcile from durable evidence before interpreting browser-authored
    // command type, age or payload. This branch cannot invoke a handler.
    let recovery_root = root.to_path_buf();
    let recovery_id = command_id_from_document(&document)?;
    let recovered = tokio::task::spawn_blocking(move || {
        super::command_plane::recover_applied_domain_effect_for_intake(&recovery_root, &recovery_id)
    })
    .await
    .context("join applied domain effect recovery")??;
    if recovered.is_some() {
        return Ok(PendingBusinessCommandIntakeOutcome::Terminalized);
    }
    // Verjaehrung VOR jeder Ausfuehrung — auch vor Browser-Kommandos.
    if let Some(created_at) = business_command_created_at_ms(&document) {
        let age = (now_ms() as u64).saturating_sub(created_at);
        if age > STALE_BUSINESS_COMMAND_MAX_AGE_MS {
            let fehler = format!(
                "stale_command_expired: Befehl ist {} Stunden alt und wird nicht mehr ausgefuehrt",
                age / 3_600_000
            );
            eprintln!(
                "[business-os] verweigere Replay eines veralteten Befehls {}: {fehler}",
                document.get("id").and_then(Value::as_str).unwrap_or("?")
            );
            terminalize_business_command_document(database, &document, &fehler).await?;
            return Ok(PendingBusinessCommandIntakeOutcome::Terminalized);
        }
    }
    let command_type = document
        .get("command_type")
        .or_else(|| document.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let command_payload = document.get("payload").cloned().unwrap_or(Value::Null);

    if is_browser_runtime_command(&command_type) {
        let command_id = document
            .get("command_id")
            .or_else(|| document.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let accepted = json!({
            "id": command_id,
            "command_id": command_id,
            "status": "accepted",
            "task_status": "accepted"
        });
        if let Err(err) = apply_browser_runtime_command(root, database, &document, &accepted).await
        {
            eprintln!("[business-os] browser runtime command failed: {err:#}");
            mark_browser_runtime_command_failed(database, &command_id, &command_payload, &err)
                .await?;
            return Ok(PendingBusinessCommandIntakeOutcome::Terminalized);
        }
        return Ok(PendingBusinessCommandIntakeOutcome::Accepted);
    }

    let root = root.to_path_buf();
    let accept_root = root.clone();
    let document_for_store = document.clone();
    // This document was replicated from a browser/device peer over WebRTC/RxDB:
    // its client_context (incl. actor) is attacker-controllable, so it is tagged
    // ReplicatedPeer and cannot authorize a privileged role without a verified
    // capability token (see store::rxdb_session_from_command).
    let intake_probe = document_for_store
        .pointer("/client_context/command_timing_probe")
        .and_then(Value::as_bool)
        .filter(|requested| *requested)
        .map(|_| {
            (
                Instant::now(),
                command_id_from_document(&document_for_store).ok(),
            )
        });
    let accepted_result = tokio::task::spawn_blocking(move || {
        let queue_wait_ms = intake_probe
            .as_ref()
            .map(|(started, _)| started.elapsed().as_secs_f64() * 1_000.0);
        let result = store::accept_rxdb_business_command_with_origin(
            &accept_root,
            document_for_store,
            store::CommandOrigin::ReplicatedPeer,
        );
        if let Some(((started, command_id), queue_wait_ms)) = intake_probe.zip(queue_wait_ms) {
            eprintln!(
                "command_intake_queue_sample={}",
                json!({
                    "command_id": command_id,
                    "queue_wait_ms": queue_wait_ms,
                    "store_execution_ms":
                        started.elapsed().as_secs_f64() * 1_000.0 - queue_wait_ms,
                    "ok": result.is_ok(),
                })
            );
        }
        result
    })
    .await;

    let mut accepted = match accepted_result {
        Ok(Ok(val)) => val,
        Ok(Err(err))
            if command_type == "ctox.business_os.app.create"
                && command_payload
                    .get("import_source")
                    .and_then(|source| source.get("kind"))
                    .and_then(Value::as_str)
                    == Some("desktop-folder")
                && is_pending_desktop_file_replication_error(&err) =>
        {
            return Ok(PendingBusinessCommandIntakeOutcome::WaitingForReplication {
                error: format!(
                    "waiting for replicated desktop import files before app creation: {err:#}"
                ),
            });
        }
        Ok(Err(err)) if is_transient_business_command_store_error(&err) => {
            return Ok(PendingBusinessCommandIntakeOutcome::RetryableFailure {
                error: format!("transient native business command store contention: {err:#}"),
            });
        }
        Ok(Err(err)) if business_command_store_error_is_permanent(&err) => {
            eprintln!("[business-os] permanent command store error, terminalizing: {err:#}");
            terminalize_business_command_document(
                database,
                &document,
                &format!("permanent_store_error: {err:#}"),
            )
            .await?;
            return Ok(PendingBusinessCommandIntakeOutcome::Terminalized);
        }
        Ok(Err(err)) => {
            eprintln!("[business-os] native business command store execution failed: {err:#}");
            return Ok(PendingBusinessCommandIntakeOutcome::RetryableFailure {
                error: format!("native business command store execution failed: {err:#}"),
            });
        }
        Err(err) => {
            return Ok(PendingBusinessCommandIntakeOutcome::RetryableFailure {
                error: format!("joining native business command store execution failed: {err}"),
            });
        }
    };

    if command_type == app_runtime::APP_ACTION_COMMAND_TYPE
        && accepted.get("already_accepted").and_then(Value::as_bool) != Some(true)
    {
        let snapshot = accepted
            .get("_app_action_snapshot")
            .cloned()
            .context("app_runtime_reconfiguring: admitted action has no immutable snapshot")?;
        let execution = app_runtime::execute(
            root.as_path(),
            database,
            &command_id_from_document(&document)?,
            &snapshot,
        )
        .await?;
        let mut result = execution.result;
        if let Some(object) = result.as_object_mut() {
            if let Some(code) = execution.error_code {
                object.insert("error_code".to_owned(), Value::String(code.to_owned()));
            }
            if let Some(message) = execution.error_message {
                object.insert("error".to_owned(), Value::String(message));
            }
        }
        accepted = store::finalize_runtime_app_action(
            root.as_path(),
            &document,
            execution.status,
            result,
        )?;
    }

    let command_id = accepted
        .get("command_id")
        .or_else(|| accepted.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("accepted command is missing command_id")?;

    if accepted
        .get("already_accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        upsert_business_record_projection(
            root.to_path_buf(),
            database,
            "business_commands",
            command_id.clone(),
        )
        .await?;
        if let Some(task_id) = accepted
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_business_record_projection(
                root.to_path_buf(),
                database,
                "ctox_queue_tasks",
                task_id.to_string(),
            )
            .await?;
        }
        if let Some(chat_id) = accepted
            .get("chat_id")
            .or_else(|| {
                accepted
                    .get("result")
                    .and_then(|result| result.get("chat_id"))
            })
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_business_record_projection(
                root.to_path_buf(),
                database,
                "business_chats",
                chat_id.to_string(),
            )
            .await?;
        }
        return Ok(if business_command_document_is_terminal(&accepted) {
            PendingBusinessCommandIntakeOutcome::Terminalized
        } else {
            PendingBusinessCommandIntakeOutcome::CanonicalReplayed
        });
    }

    let commands = database
        .collection("business_commands")
        .context("business_commands collection is not registered")?;
    let accepted_status = accepted
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let existing_status = document.get("status").and_then(Value::as_str).unwrap_or("");
    if accepted_status == "already_accepted"
        && !existing_status.is_empty()
        && existing_status != "pending_sync"
    {
        return Ok(PendingBusinessCommandIntakeOutcome::CanonicalReplayed);
    }
    let mut next = if document.is_object() {
        document.clone()
    } else {
        json!({ "id": command_id, "command_id": command_id })
    };
    if let Some(obj) = next.as_object_mut() {
        obj.insert(
            "status".to_string(),
            Value::String(accepted_status.to_string()),
        );
        if accepted.get("task_id").is_some() || !obj.contains_key("task_id") {
            obj.insert(
                "task_id".to_string(),
                accepted
                    .get("task_id")
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new())),
            );
        }
        obj.insert(
            "task_status".to_string(),
            accepted.get("task_status").cloned().unwrap_or_else(|| {
                obj.get("status")
                    .cloned()
                    .unwrap_or(Value::String("accepted".to_string()))
            }),
        );
        if let Some(result) = accepted.get("result") {
            obj.insert("result".to_string(), result.clone());
        }
        for key in ["outbound_text", "response", "answer", "summary"] {
            if let Some(value) = accepted.get(key) {
                obj.insert(key.to_string(), value.clone());
            }
        }
        for key in ["report_id", "report_status"] {
            if let Some(value) = accepted.get(key) {
                obj.insert(key.to_string(), value.clone());
            }
        }
        obj.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    }
    enrich_native_command_lifecycle(&mut next, &accepted)?;
    if next.get("contract_version").and_then(Value::as_u64) == Some(2) {
        let persist_root = root.clone();
        let persisted = next.clone();
        tokio::task::spawn_blocking(move || {
            store::persist_business_command_lifecycle_projection(&persist_root, &persisted)
        })
        .await
        .context("join native command lifecycle projection persistence")??;
    }
    incremental_upsert_document_with_envelope(&commands, next, "accepted business_command")
        .await
        .map_err(|err| anyhow::anyhow!("upsert accepted business_command {command_id}: {err}"))?;

    if let Some(task_id) = accepted.get("task_id").and_then(Value::as_str) {
        if !task_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "ctox_queue_tasks",
                task_id.to_string(),
            )
            .await?;
        }
    }
    if let Some(report_id) = accepted.get("report_id").and_then(Value::as_str) {
        if !report_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "business_module_reports",
                report_id.to_string(),
            )
            .await?;
            upsert_business_record_projection(
                root.clone(),
                database,
                "ctox_bug_reports",
                report_id.to_string(),
            )
            .await?;
        }
    }
    if let Some(source_file_ids) = accepted
        .get("result")
        .and_then(|result| result.get("source_file_ids"))
        .and_then(Value::as_array)
    {
        for source_file_id in source_file_ids.iter().filter_map(Value::as_str) {
            if !source_file_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_source_files",
                    source_file_id.to_string(),
                )
                .await?;
            }
        }
    }
    if command_type == "ctox.business_os.user.upsert" {
        sync_business_users_with_database(&root, database).await?;
    }
    if command_type == "ctox.runtime_settings.save" {
        sync_runtime_settings_with_database(&root, database).await?;
    }
    if command_type == "ctox.business_os.branding.update" {
        sync_workspace_branding_with_database(&root, database).await?;
    }
    if command_type.starts_with("ctox.ticket.") {
        sync_ticket_state_with_database(&root, database).await?;
    }
    if command_type.starts_with("support.") {
        project_support_command_result(root.clone(), database, &accepted).await?;
    }
    if command_type.starts_with("ctox.appsec.") {
        project_appsec_command_result(root.clone(), database, &accepted).await?;
    }
    if command_type.starts_with("threads.") {
        project_threads_command_result(root.clone(), database, &accepted).await?;
    }
    if command_type == "ctox.file.materialize" {
        if let Some(materialized_path) = accepted
            .get("result")
            .and_then(|result| result.get("path"))
            .or_else(|| command_payload.get("path"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            upsert_desktop_file_with_policy(
                &root,
                database,
                PathBuf::from(materialized_path),
                DesktopFileContentPolicy::Eager,
                true,
            )
            .await
            .with_context(|| {
                format!("project materialized desktop file {materialized_path} into native RxDB")
            })?;
        }
    }
    if matches!(
        command_type.as_str(),
        "ctox.module.save"
            | "ctox.module.delete"
            | "ctox.module.install_template"
            | "ctox.module.assign_founder"
            | "ctox.module.release"
            | "ctox.module.rollback"
            | "ctox.module.rollback_version"
            | "ctox.module.repair_lifecycle_projection"
            | "ctox.app_store.install"
            | "ctox.app_store.uninstall"
    ) {
        sync_module_catalog_with_database(&root, database).await?;
    }
    let mut projected_acl = false;
    if let Some(acl_ids) = accepted
        .get("result")
        .and_then(|result| result.get("business_module_acl_ids"))
        .and_then(Value::as_array)
    {
        for acl_id in acl_ids.iter().filter_map(Value::as_str) {
            if !acl_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_acl",
                    acl_id.to_string(),
                )
                .await?;
                projected_acl = true;
            }
        }
    }
    if !projected_acl && command_type == "ctox.module.assign_founder" {
        let module_id = command_payload
            .get("module_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let user_id = command_payload
            .get("user_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !module_id.is_empty() && !user_id.is_empty() {
            upsert_business_record_projection(
                root.clone(),
                database,
                "business_module_acl",
                format!("{module_id}:founder:{user_id}"),
            )
            .await?;
        }
    }
    if let Some(release_ids) = accepted
        .get("result")
        .and_then(|result| result.get("business_module_release_ids"))
        .and_then(Value::as_array)
    {
        for release_id in release_ids.iter().filter_map(Value::as_str) {
            if !release_id.is_empty() {
                upsert_business_record_projection(
                    root.clone(),
                    database,
                    "business_module_releases",
                    release_id.to_string(),
                )
                .await?;
            }
        }
    }
    Ok(if business_command_document_is_terminal(&accepted) {
        PendingBusinessCommandIntakeOutcome::Terminalized
    } else {
        PendingBusinessCommandIntakeOutcome::Accepted
    })
}

pub(super) fn enrich_native_command_lifecycle(
    document: &mut Value,
    accepted: &Value,
) -> anyhow::Result<()> {
    if document.get("contract_version").and_then(Value::as_u64) != Some(2) {
        return Ok(());
    }
    let object = document
        .as_object_mut()
        .context("v2 business command lifecycle document must be an object")?;
    let status = accepted
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("accepted");
    let execution_mode = accepted
        .get("execution_mode")
        .and_then(Value::as_str)
        .unwrap_or("queue")
        .to_string();
    let execution_task_id = accepted
        .get("execution_task_id")
        .or_else(|| {
            (execution_mode == "queue")
                .then_some(accepted.get("task_id"))
                .flatten()
        })
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let terminal_status = if crate::command_lifecycle::terminal_status_is_outcome(status) {
        status
    } else {
        "none"
    };
    let execution_phase = if terminal_status != "none" {
        "terminal"
    } else if execution_mode == "queue" && !execution_task_id.is_empty() {
        "queued"
    } else {
        "accepted"
    };
    let previous_version = object
        .get("projection_version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let target_task_id = accepted
        .get("target_task_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let target_record_id = accepted
        .get("target_record_id")
        .and_then(Value::as_str)
        .or_else(|| object.get("record_id").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();

    object.insert("execution_mode".to_string(), Value::String(execution_mode));
    object.insert(
        "execution_task_id".to_string(),
        Value::String(execution_task_id),
    );
    object.insert("target_task_id".to_string(), Value::String(target_task_id));
    object.insert(
        "target_record_id".to_string(),
        Value::String(target_record_id),
    );
    object.insert(
        "replication_phase".to_string(),
        Value::String("native_observed".to_string()),
    );
    object.insert(
        "execution_phase".to_string(),
        Value::String(execution_phase.to_string()),
    );
    object.insert(
        "terminal_status".to_string(),
        Value::String(terminal_status.to_string()),
    );
    object.insert(
        "projection_version".to_string(),
        Value::from(previous_version.saturating_add(1)),
    );
    object
        .entry("attempt".to_string())
        .or_insert_with(|| Value::from(0_u64));
    if terminal_status == "failed" {
        object.insert(
            "error_code".to_string(),
            Value::String("command_terminal_failure".to_string()),
        );
        object.insert("retryable".to_string(), Value::Bool(false));
    }
    crate::command_lifecycle::validate_document(document).map_err(|error| anyhow::anyhow!(error))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_terminal_failure_fields, business_commands_table_stamp_sql,
        BUSINESS_COMMAND_RETRY_CANDIDATE_SQL,
    };
    use rusqlite::{params, Connection};
    use serde_json::json;

    #[test]
    fn business_commands_stamp_uses_an_index_for_every_lifecycle_branch() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE business_commands (
                id TEXT PRIMARY KEY,
                deleted INTEGER NOT NULL,
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            CREATE INDEX business_commands_json__deleted__status
            ON business_commands (
                deleted,
                json_extract(data, '$.status')
            );
            CREATE INDEX business_commands_json__deleted__command_type
            ON business_commands (
                deleted,
                json_extract(data, '$.command_type')
            );
            "#,
        )
        .expect("create command stamp test schema");

        let sql = business_commands_table_stamp_sql(
            "business_commands",
            "deleted",
            "COALESCE(lastWriteTime, 0)",
        );
        let mut stmt = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("prepare command stamp query plan");
        let plan = stmt
            .query_map([], |row| row.get::<_, String>(3))
            .expect("query command stamp plan")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect command stamp plan");
        let indexed_branches = plan
            .iter()
            .filter(|detail| detail.contains("SEARCH business_commands USING INDEX"))
            .count();

        assert_eq!(indexed_branches, 4, "query plan: {plan:#?}");
        assert!(
            !plan.iter().any(|detail| detail == "SCAN business_commands"),
            "query plan: {plan:#?}"
        );
    }

    #[test]
    fn a_terminalized_command_stops_being_a_retry_candidate() {
        // Die Luecke, die auf der Produktivinstanz einen ganzen Kern kostete:
        // das SQL-Praedikat war korrekt und der Schreiber schrieb wirklich --
        // aber er setzte `terminal_status` nicht, also blieb das Dokument
        // Kandidat und drehte ~20-mal pro Sekunde durch die Verjaehrung.
        // Dieser Test bindet beide Seiten aneinander.
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE business_commands (
                id TEXT PRIMARY KEY,
                deleted INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create table");

        // Ein Zombie, wie er real vorlag: verjaehrt, aber noch Kandidat.
        let mut zombie = json!({
            "id": "cmd_thesen_source_zombie",
            "status": "failed",
            "command_type": "outbound.research_source.test",
        });
        let vorher: i64 = {
            conn.execute(
                "INSERT INTO business_commands (id, deleted, data) VALUES ('z', 0, ?1)",
                params![zombie.to_string()],
            )
            .expect("insert zombie");
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM business_commands
                     WHERE deleted = 0 AND {BUSINESS_COMMAND_RETRY_CANDIDATE_SQL}"
                ),
                [],
                |row| row.get(0),
            )
            .expect("count before")
        };
        assert_eq!(vorher, 1, "ohne terminal_status muss er Kandidat sein");

        apply_terminal_failure_fields(
            zombie.as_object_mut().expect("object"),
            "stale_command_expired: Befehl ist 569 Stunden alt",
        );
        conn.execute(
            "UPDATE business_commands SET data = ?1 WHERE id = 'z'",
            params![zombie.to_string()],
        )
        .expect("update terminalized");

        let nachher: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM business_commands
                     WHERE deleted = 0 AND {BUSINESS_COMMAND_RETRY_CANDIDATE_SQL}"
                ),
                [],
                |row| row.get(0),
            )
            .expect("count after");
        assert_eq!(
            nachher, 0,
            "nach der Terminalisierung darf er NICHT mehr aufgegriffen werden"
        );

        // execution_phase und terminal_status muessen zusammenpassen,
        // sonst weist validate_terminal_pair das Dokument zurueck.
        assert_eq!(zombie["execution_phase"], "terminal");
        assert_eq!(zombie["terminal_status"], "failed");
    }

    #[test]
    fn business_commands_stamp_union_matches_retry_candidate_semantics() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE business_commands (
                id TEXT PRIMARY KEY,
                deleted INTEGER NOT NULL,
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create command stamp test table");
        let recoverable_types = [
            "external_sql.sync.refresh",
            "external_sql.write",
            "outbound.research_source.generate_adapter",
            "outbound.research_source.test",
            "outbound.research_source.auth_assist",
            "web_stack.person_research",
        ];
        let mut documents = vec![
            json!({"status": "pending_sync", "command_type": "any.command"}),
            json!({"status": "waiting_dependencies", "command_type": "any.command"}),
            json!({"status": "completed", "command_type": "external_sql.write"}),
            json!({"status": "accepted", "command_type": "office.document.create"}),
            json!({"status": "failed", "terminal_status": "failed", "command_type": "external_sql.write"}),
        ];
        for command_type in recoverable_types {
            documents.push(json!({"status": "accepted", "command_type": command_type}));
            documents.push(json!({
                "status": "failed",
                "terminal_status": "none",
                "command_type": command_type
            }));
        }
        for (index, document) in documents.iter().enumerate() {
            conn.execute(
                "INSERT INTO business_commands (id, deleted, lastWriteTime, data) VALUES (?1, 0, ?2, ?3)",
                params![format!("cmd-{index}"), index as f64 + 1.0, document.to_string()],
            )
            .expect("insert command stamp test document");
        }

        let legacy: (i64, f64) = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(MAX(lastWriteTime), 0)
                     FROM business_commands
                     WHERE deleted = 0 AND {BUSINESS_COMMAND_RETRY_CANDIDATE_SQL}"
                ),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query legacy command stamp predicate");
        let union: (i64, f64) = conn
            .query_row(
                &business_commands_table_stamp_sql("business_commands", "deleted", "lastWriteTime"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query indexable command stamp union");

        assert_eq!(union, legacy);
        assert_eq!(union.0, 14);
    }
}
