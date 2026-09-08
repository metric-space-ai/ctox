// The business-command plane inside mission: claim/complete/progress of
// control commands, the saga step machinery, worker results, reviews,
// outbox rows, projections, diagnostics, intake failures, retention and
// storage audits and one-time legacy-data migrations.

use super::{
    attach_queue_projection_store, canonical_queue_route_status,
    create_queue_task_with_metadata_tx, current_queue_route_status, ensure_queue_account,
    epoch_millis, load_queue_task_from_conn, now_iso_string, open_channel_db,
    refresh_queue_projection_tasks, resolve_db_path, sanitize_path_component, set_routing_status,
    sha256_hex, BusinessCommandClaimRequest, BusinessCommandControlClaim,
    BusinessCommandOutboxEvent, BusinessCommandQueueClaim, QueueRouteStatus,
    QueueTaskCreateRequest, TerminalPolicyGrant,
};
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) fn claim_business_command_with_queue(
    root: &Path,
    claim: BusinessCommandClaimRequest,
    request: QueueTaskCreateRequest,
) -> Result<BusinessCommandQueueClaim> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    // Admission reads and writes one aggregate. Reserve the writer before
    // reading so concurrent admissions cannot both observe no active work,
    // or fail upgrading a stale deferred snapshot with SQLITE_BUSY.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let existing = tx
        .query_row(
            "SELECT idempotency_key, payload_hash, execution_phase, projection_version
             FROM business_command_aggregates
             WHERE command_id = ?1",
            params![claim.command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let (aggregate_exists, from_phase, accepted_version) =
        if let Some((idempotency_key, payload_hash, phase, version)) = existing {
            anyhow::ensure!(
                idempotency_key == claim.idempotency_key && payload_hash == claim.payload_hash,
                "idempotency_conflict: command id was already claimed with different intent"
            );
            let task_id = tx
                .query_row(
                    "SELECT task_id FROM business_command_task_links WHERE command_id = ?1",
                    params![claim.command_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(task_id) = task_id {
                if claim.command_type == super::auth_assist::REQUEST_TYPE
                    && super::auth_assist::preserve_request(&tx, &task_id, &claim.command_id)?
                {
                    let task = load_queue_task_from_conn(&tx, &task_id)?
                        .context("auth-assist task missing")?;
                    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
                }
                let task = load_queue_task_from_conn(&tx, &task_id)?
                    .context("claimed queue command task link points to a missing task")?;
                tx.commit()?;
                return Ok(BusinessCommandQueueClaim {
                    task,
                    already_claimed: true,
                });
            }
            anyhow::ensure!(
                phase == "waiting_dependencies",
                "claimed queue command is missing its atomic task link"
            );
            (true, phase, version.saturating_add(1))
        } else {
            (false, "local".to_string(), 1)
        };

    let superseded_by = if claim.command_type == "outbound.research.adapters.reconcile" {
        let digest = claim
            .intent
            .pointer("/payload/configuration_digest")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        match digest {
            Some(digest) => tx
                .query_row(
                    "SELECT a.command_id, l.task_id
                 FROM business_command_aggregates a
                 JOIN business_command_task_links l ON l.command_id=a.command_id
                 JOIN communication_routing_state r ON r.message_key=l.task_id
                 WHERE a.command_type='outbound.research.adapters.reconcile'
                   AND a.module=?1 AND a.record_id=?2 AND a.execution_phase!='terminal'
                   AND json_extract(a.intent_json,'$.payload.configuration_digest')=?3
                   AND COALESCE(json_extract(a.intent_json,'$.native_authorization'),'null')=?4
                   AND COALESCE(json_extract(a.intent_json,'$.client_context'),'null')=?5
                   AND r.route_status IN ('pending','leased','review_rework','blocked')
                 ORDER BY a.created_at_ms,a.command_id LIMIT 1",
                    params![
                        claim.module,
                        claim.record_id,
                        digest,
                        serde_json::to_string(
                            claim
                                .intent
                                .get("native_authorization")
                                .unwrap_or(&Value::Null)
                        )?,
                        serde_json::to_string(
                            claim.intent.get("client_context").unwrap_or(&Value::Null)
                        )?
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?,
            None => None,
        }
    } else {
        None
    };
    let now_ms = epoch_millis();
    if from_phase != "local" {
        crate::command_lifecycle::validate_execution_phase_transition(&from_phase, "accepted")?;
    }
    crate::command_lifecycle::validate_execution_phase_transition("accepted", "queued")?;
    if !aggregate_exists {
        tx.execute(
            "INSERT INTO business_command_aggregates
            (command_id, idempotency_key, payload_hash, module, command_type, record_id,
             execution_mode, execution_phase, terminal_status, attempt, projection_version,
             intent_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queue', 'accepted', 'none', 0, 1, ?7, ?8, ?9)",
            params![
                claim.command_id,
                claim.idempotency_key,
                claim.payload_hash,
                claim.module,
                claim.command_type,
                claim.record_id,
                serde_json::to_string(&claim.intent)?,
                claim.created_at_ms,
                now_ms,
            ],
        )?;
    } else {
        tx.execute(
            "UPDATE business_command_aggregates
             SET execution_phase = 'accepted', projection_version = ?2, updated_at_ms = ?3
             WHERE command_id = ?1",
            params![claim.command_id, accepted_version, now_ms],
        )?;
    }
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, 'accepted', 'none', 'canonical command admission', '{}', ?4)",
        params![claim.command_id, accepted_version, from_phase, now_ms],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &claim.command_id,
        accepted_version,
        "command.accepted",
        &json!({
            "command_id": claim.command_id,
            "execution_mode": "queue",
            "execution_phase": "accepted",
            "terminal_status": "none",
            "projection_version": accepted_version,
        }),
        now_ms,
    )?;
    let mut task = create_queue_task_with_metadata_tx(&tx, request)?;
    let queued_version = accepted_version.saturating_add(1);
    tx.execute(
        "INSERT INTO business_command_task_links (command_id, task_id, created_at_ms)
         VALUES (?1, ?2, ?3)",
        params![claim.command_id, task.message_key, now_ms],
    )?;
    tx.execute(
        "INSERT INTO business_command_effects
            (command_id, effect_key, status, claimed_at_ms, updated_at_ms)
         VALUES (?1, ?2, 'claimed', ?3, ?3)",
        params![
            claim.command_id,
            format!("queue:{}", task.message_key),
            now_ms
        ],
    )?;
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = 'queued', projection_version = ?2, updated_at_ms = ?3
         WHERE command_id = ?1",
        params![claim.command_id, queued_version, now_ms],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, 'queued', 'none', 'atomic queue admission', ?4, ?5)",
        params![
            claim.command_id,
            queued_version,
            "accepted",
            serde_json::to_string(&json!({ "task_id": task.message_key }))?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &claim.command_id,
        queued_version,
        "command.admitted",
        &json!({
            "command_id": claim.command_id,
            "execution_mode": "queue",
            "execution_task_id": task.message_key,
            "execution_phase": "queued",
            "terminal_status": "none",
            "projection_version": queued_version,
        }),
        now_ms,
    )?;
    if claim.command_type == super::auth_assist::REQUEST_TYPE
        && super::auth_assist::preserve_request(&tx, &task.message_key, &claim.command_id)?
    {
        task = load_queue_task_from_conn(&tx, &task.message_key)?
            .context("auth-assist task missing")?;
    }
    if let Some((command_id, task_id)) = superseded_by {
        let reason = format!("Adapterabgleich bereits offen: {task_id}");
        transition_business_command_for_task_in_transaction(
            &tx,
            &task.message_key,
            "cancelled",
            Some(&json!({"superseded_by_command_id":command_id,"superseded_by_task_id":task_id})),
            Some("adapter_reconciliation_superseded"),
            Some(&reason),
            &reason,
        )?;
        task = load_queue_task_from_conn(&tx, &task.message_key)?
            .context("superseded task missing")?;
    }
    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
    tx.commit()?;
    Ok(BusinessCommandQueueClaim {
        task,
        already_claimed: false,
    })
}

const MODULE_VISIBILITY_SAGA_STEPS: &[(&str, &str, &str)] = &[
    (
        "persist_visibility",
        "module_visibility:persist",
        "module_visibility:restore",
    ),
    (
        "project_catalog",
        "module_visibility:project",
        "module_visibility:reproject",
    ),
];

pub(super) fn registered_business_command_saga(
    command_type: &str,
) -> Option<(
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
)> {
    match command_type {
        "ctox.module.set_visible" => {
            Some(("ctox.module.visibility.v1", MODULE_VISIBILITY_SAGA_STEPS))
        }
        _ => None,
    }
}

pub(super) fn register_business_command_saga_tx(
    tx: &Transaction<'_>,
    command_id: &str,
    command_type: &str,
    now_ms: i64,
) -> Result<()> {
    let Some((saga_kind, steps)) = registered_business_command_saga(command_type) else {
        return Ok(());
    };
    let saga_id = format!("saga:{command_id}");
    tx.execute(
        "INSERT OR IGNORE INTO business_command_sagas
            (saga_id, command_id, saga_kind, phase, current_step, total_steps, compensation_status, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'forward', 0, ?4, 'not_started', ?5, ?5)",
        params![saga_id, command_id, saga_kind, steps.len() as i64, now_ms],
    )?;
    for (index, (name, forward_key, compensation_key)) in steps.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO business_command_saga_steps
                (saga_id, step_index, step_name, forward_effect_key, compensation_effect_key, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![saga_id, index as i64, name, forward_key, compensation_key, now_ms],
        )?;
    }
    Ok(())
}

/// Register the immutable definition and ordered effects for a runtime-loaded
/// app action. Runtime actions intentionally share the same durable saga
/// tables and terminal owner as compiled commands; only their definition is
/// loaded from the validated module package.
pub(crate) fn start_runtime_business_command_saga(
    root: &Path,
    command_id: &str,
    module_id: &str,
    action_name: &str,
    definition_hash: &str,
    definition: &Value,
    step_names: &[String],
) -> Result<()> {
    anyhow::ensure!(
        !step_names.is_empty(),
        "app action saga requires at least one step"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let now_ms = epoch_millis();
    let saga_id = format!("saga:{command_id}");
    tx.execute(
        "INSERT OR IGNORE INTO business_app_action_snapshots
            (command_id, module_id, action_name, definition_hash, definition_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            command_id,
            module_id,
            action_name,
            definition_hash,
            serde_json::to_string(definition)?,
            now_ms,
        ],
    )?;
    let existing: (String, String) = tx.query_row(
        "SELECT definition_hash, definition_json FROM business_app_action_snapshots WHERE command_id = ?1",
        params![command_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    anyhow::ensure!(
        existing.0 == definition_hash && existing.1 == serde_json::to_string(definition)?,
        "app_action_definition_changed: command already admitted with another definition"
    );
    tx.execute(
        "INSERT OR IGNORE INTO business_command_sagas
            (saga_id, command_id, saga_kind, phase, current_step, total_steps, compensation_status, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 'forward', 0, ?4, 'not_started', ?5, ?5)",
        params![
            saga_id,
            command_id,
            format!("ctox.app.action.v1:{module_id}:{action_name}:{definition_hash}"),
            step_names.len() as i64,
            now_ms,
        ],
    )?;
    for (index, name) in step_names.iter().enumerate() {
        let forward_key = format!("{command_id}:{definition_hash}:{index}:forward");
        let compensation_key = format!("{command_id}:{definition_hash}:{index}:compensation");
        tx.execute(
            "INSERT OR IGNORE INTO business_command_saga_steps
                (saga_id, step_index, step_name, forward_effect_key, compensation_effect_key, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![saga_id, index as i64, name, forward_key, compensation_key, now_ms],
        )?;
    }
    let registered_steps: i64 = tx.query_row(
        "SELECT COUNT(*) FROM business_command_saga_steps WHERE saga_id = ?1",
        params![saga_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        registered_steps == step_names.len() as i64,
        "app_action_definition_changed: registered saga step count differs"
    );
    tx.commit()?;
    Ok(())
}

pub(crate) fn runtime_business_command_action_snapshot(
    root: &Path,
    command_id: &str,
) -> Result<Option<Value>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT definition_json FROM business_app_action_snapshots WHERE command_id = ?1",
            params![command_id],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}

pub(crate) fn business_command_saga_status(root: &Path, command_id: &str) -> Result<Option<Value>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let saga_id = format!("saga:{command_id}");
    let saga: Option<(String, String, i64, i64)> = conn
        .query_row(
            "SELECT phase, compensation_status, current_step, total_steps
             FROM business_command_sagas WHERE saga_id = ?1",
            params![saga_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((phase, compensation_status, current_step, total_steps)) = saga else {
        return Ok(None);
    };
    let error_message: Option<String> = conn
        .query_row(
            "SELECT error_message FROM business_command_saga_steps
             WHERE saga_id = ?1 AND error_message IS NOT NULL
             ORDER BY CASE WHEN compensation_status = 'failed' THEN 0 ELSE 1 END, step_index DESC
             LIMIT 1",
            params![format!("saga:{command_id}")],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Some(json!({
        "phase": phase,
        "compensation_status": compensation_status,
        "current_step": current_step,
        "total_steps": total_steps,
        "error_message": error_message,
    })))
}

pub(crate) fn start_business_command_saga(
    root: &Path,
    command_id: &str,
    command_type: &str,
) -> Result<()> {
    anyhow::ensure!(
        registered_business_command_saga(command_type).is_some(),
        "no native saga definition registered for `{command_type}`"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    register_business_command_saga_tx(&tx, command_id, command_type, epoch_millis())?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn claim_business_command_saga_step(
    root: &Path,
    command_id: &str,
    step_name: &str,
    compensation: bool,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let saga_id = format!("saga:{command_id}");
    let column = if compensation {
        "compensation_status"
    } else {
        "forward_status"
    };
    let attempts = if compensation {
        "compensation_attempts"
    } else {
        "forward_attempts"
    };
    let step: Option<(String, i64, String)> = tx.query_row(
        &format!("SELECT s.{column}, s.step_index, g.phase FROM business_command_saga_steps s JOIN business_command_sagas g ON g.saga_id = s.saga_id WHERE s.saga_id = ?1 AND s.step_name = ?2"),
        params![saga_id, step_name],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let (status, step_index, saga_phase) =
        step.with_context(|| format!("no registered saga step `{step_name}` for `{command_id}`"))?;
    if status == "completed" {
        tx.commit()?;
        return Ok(false);
    }
    if compensation {
        anyhow::ensure!(
            saga_phase == "compensating" || saga_phase == "manual_intervention",
            "saga is not compensating"
        );
        anyhow::ensure!(
            status != "not_required",
            "compensation was not requested for saga step `{step_name}`"
        );
        let later_pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM business_command_saga_steps
             WHERE saga_id = ?1 AND step_index > ?2 AND compensation_status IN ('pending', 'claimed', 'failed')",
            params![saga_id, step_index],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            later_pending == 0,
            "saga compensation must run in reverse step order"
        );
    } else {
        anyhow::ensure!(saga_phase == "forward", "saga is not in forward phase");
        let earlier_incomplete: i64 = tx.query_row(
            "SELECT COUNT(*) FROM business_command_saga_steps
             WHERE saga_id = ?1 AND step_index < ?2 AND forward_status != 'completed'",
            params![saga_id, step_index],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            earlier_incomplete == 0,
            "saga forward steps must run in registered order"
        );
    }
    let now_ms = epoch_millis();
    tx.execute(
        &format!("UPDATE business_command_saga_steps SET {column} = 'claimed', {attempts} = {attempts} + 1, updated_at_ms = ?3 WHERE saga_id = ?1 AND step_name = ?2"),
        params![saga_id, step_name, now_ms],
    )?;
    tx.execute(
        "UPDATE business_command_sagas SET current_step = (SELECT step_index FROM business_command_saga_steps WHERE saga_id = ?1 AND step_name = ?2), updated_at_ms = ?3 WHERE saga_id = ?1",
        params![saga_id, step_name, now_ms],
    )?;
    tx.commit()?;
    Ok(true)
}

pub(crate) fn business_command_saga_step_evidence(
    root: &Path,
    command_id: &str,
    step_name: &str,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let raw: String = conn.query_row(
        "SELECT evidence_json FROM business_command_saga_steps WHERE saga_id = ?1 AND step_name = ?2",
        params![format!("saga:{command_id}"), step_name],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}

pub(crate) fn business_command_saga_pending_compensation_steps(
    root: &Path,
    command_id: &str,
) -> Result<Vec<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let saga_id = format!("saga:{command_id}");
    let mut stmt = conn.prepare(
        "SELECT step_name FROM business_command_saga_steps
         WHERE saga_id = ?1 AND compensation_status IN ('pending', 'claimed')
         ORDER BY step_index ASC",
    )?;
    let rows = stmt.query_map(params![saga_id], |row| row.get::<_, String>(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn record_business_command_saga_step_evidence(
    root: &Path,
    command_id: &str,
    step_name: &str,
    evidence: &Value,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let changed = conn.execute(
        "UPDATE business_command_saga_steps SET evidence_json = ?3, updated_at_ms = ?4
         WHERE saga_id = ?1 AND step_name = ?2 AND forward_status = 'claimed'",
        params![
            format!("saga:{command_id}"),
            step_name,
            serde_json::to_string(evidence)?,
            epoch_millis(),
        ],
    )?;
    anyhow::ensure!(changed == 1, "saga step `{step_name}` is not claimed");
    Ok(())
}

pub(crate) fn complete_business_command_saga_step(
    root: &Path,
    command_id: &str,
    step_name: &str,
    compensation: bool,
    evidence: &Value,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let saga_id = format!("saga:{command_id}");
    let column = if compensation {
        "compensation_status"
    } else {
        "forward_status"
    };
    let now_ms = epoch_millis();
    let changed = tx.execute(
        &format!("UPDATE business_command_saga_steps SET {column} = 'completed', evidence_json = ?3, error_message = NULL, updated_at_ms = ?4 WHERE saga_id = ?1 AND step_name = ?2 AND {column} IN ('claimed', 'completed')"),
        params![saga_id, step_name, serde_json::to_string(evidence)?, now_ms],
    )?;
    anyhow::ensure!(
        changed == 1,
        "saga step `{step_name}` was not durably claimed"
    );
    if compensation {
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM business_command_saga_steps WHERE saga_id = ?1 AND compensation_status IN ('pending', 'claimed', 'failed')",
            params![saga_id], |row| row.get(0),
        )?;
        if pending == 0 {
            tx.execute("UPDATE business_command_sagas SET phase = 'compensated', compensation_status = 'completed', updated_at_ms = ?2 WHERE saga_id = ?1", params![saga_id, now_ms])?;
        }
    } else {
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM business_command_saga_steps WHERE saga_id = ?1 AND forward_status != 'completed'",
            params![saga_id], |row| row.get(0),
        )?;
        if pending == 0 {
            tx.execute("UPDATE business_command_sagas SET phase = 'completed', current_step = total_steps, updated_at_ms = ?2 WHERE saga_id = ?1", params![saga_id, now_ms])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn fail_business_command_saga_step(
    root: &Path,
    command_id: &str,
    step_name: &str,
    error_message: &str,
    compensation: bool,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let saga_id = format!("saga:{command_id}");
    let column = if compensation {
        "compensation_status"
    } else {
        "forward_status"
    };
    let now_ms = epoch_millis();
    tx.execute(
        &format!("UPDATE business_command_saga_steps SET {column} = 'failed', error_message = ?3, updated_at_ms = ?4 WHERE saga_id = ?1 AND step_name = ?2"),
        params![saga_id, step_name, error_message, now_ms],
    )?;
    if compensation {
        tx.execute("UPDATE business_command_sagas SET phase = 'manual_intervention', compensation_status = 'failed', updated_at_ms = ?2 WHERE saga_id = ?1", params![saga_id, now_ms])?;
    } else {
        tx.execute(
            "UPDATE business_command_saga_steps SET compensation_status = 'pending', updated_at_ms = ?2
             WHERE saga_id = ?1 AND forward_status = 'completed'",
            params![saga_id, now_ms],
        )?;
        let compensation_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM business_command_saga_steps WHERE saga_id = ?1 AND compensation_status = 'pending'",
            params![saga_id], |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE business_command_sagas SET phase = ?2, compensation_status = ?3, updated_at_ms = ?4 WHERE saga_id = ?1",
            params![
                saga_id,
                if compensation_count == 0 { "compensated" } else { "compensating" },
                if compensation_count == 0 { "completed" } else { "pending" },
                now_ms,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn claim_business_command_waiting_dependencies(
    root: &Path,
    claim: BusinessCommandClaimRequest,
    missing_dependencies: &Value,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let existing = tx
        .query_row(
            "SELECT idempotency_key, payload_hash FROM business_command_aggregates WHERE command_id = ?1",
            params![claim.command_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((idempotency_key, payload_hash)) = existing {
        anyhow::ensure!(
            idempotency_key == claim.idempotency_key && payload_hash == claim.payload_hash,
            "idempotency_conflict: command id was already claimed with different intent"
        );
        tx.commit()?;
        return Ok(());
    }
    let now_ms = epoch_millis();
    tx.execute(
        "INSERT INTO business_command_aggregates
            (command_id, idempotency_key, payload_hash, module, command_type, record_id,
             execution_mode, execution_phase, terminal_status, attempt, projection_version,
             intent_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queue', 'waiting_dependencies', 'none', 0, 1, ?7, ?8, ?9)",
        params![
            claim.command_id,
            claim.idempotency_key,
            claim.payload_hash,
            claim.module,
            claim.command_type,
            claim.record_id,
            serde_json::to_string(&claim.intent)?,
            claim.created_at_ms,
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, 1, 'local', 'waiting_dependencies', 'none', 'required replicated data is unavailable', ?2, ?3)",
        params![claim.command_id, serde_json::to_string(missing_dependencies)?, now_ms],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &claim.command_id,
        1,
        "command.waiting_dependencies",
        &json!({
            "command_id": claim.command_id,
            "execution_mode": "queue",
            "execution_phase": "waiting_dependencies",
            "terminal_status": "none",
            "projection_version": 1,
            "missing_dependencies": missing_dependencies,
        }),
        now_ms,
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn claim_business_control_command(
    root: &Path,
    claim: BusinessCommandClaimRequest,
) -> Result<BusinessCommandControlClaim> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let existing = tx
        .query_row(
            "SELECT idempotency_key, payload_hash, terminal_status, result_json, execution_phase
             FROM business_command_aggregates
             WHERE command_id = ?1",
            params![claim.command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    if let Some((idempotency_key, payload_hash, terminal_status, result_json, phase)) = existing {
        anyhow::ensure!(
            idempotency_key == claim.idempotency_key && payload_hash == claim.payload_hash,
            "idempotency_conflict: command id was already claimed with different intent"
        );
        let result = result_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?;
        tx.commit()?;
        return Ok(BusinessCommandControlClaim {
            disposition: if phase == "terminal" {
                "terminal"
            } else {
                "uncertain"
            },
            result,
            terminal_status: (terminal_status != "none").then_some(terminal_status),
        });
    }

    let now_ms = epoch_millis();
    tx.execute(
        "INSERT INTO business_command_aggregates
            (command_id, idempotency_key, payload_hash, module, command_type, record_id,
             execution_mode, execution_phase, terminal_status, attempt, projection_version,
             intent_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'control', 'accepted', 'none', 0, 1, ?7, ?8, ?9)",
        params![
            claim.command_id,
            claim.idempotency_key,
            claim.payload_hash,
            claim.module,
            claim.command_type,
            claim.record_id,
            serde_json::to_string(&claim.intent)?,
            claim.created_at_ms,
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_effects
            (command_id, effect_key, status, claimed_at_ms, updated_at_ms)
         VALUES (?1, ?2, 'claimed', ?3, ?3)",
        params![
            claim.command_id,
            format!("control:{}", claim.command_type),
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, 1, 'local', 'accepted', 'none', 'durable control claim', '{}', ?2)",
        params![claim.command_id, now_ms],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &claim.command_id,
        1,
        "command.claimed",
        &json!({
            "command_id": claim.command_id,
            "execution_mode": "control",
            "execution_phase": "accepted",
            "terminal_status": "none",
            "projection_version": 1,
        }),
        now_ms,
    )?;
    tx.commit()?;
    Ok(BusinessCommandControlClaim {
        disposition: "new",
        result: None,
        terminal_status: None,
    })
}

pub(crate) fn complete_business_control_command(
    root: &Path,
    command_id: &str,
    terminal_status: &str,
    result: &Value,
    error_message: Option<&str>,
) -> Result<()> {
    anyhow::ensure!(
        crate::command_lifecycle::terminal_status_is_outcome(terminal_status),
        "invalid control command terminal status"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let (phase, version, command_type) = tx.query_row(
        "SELECT execution_phase, projection_version, command_type
         FROM business_command_aggregates WHERE command_id = ?1",
        params![command_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    if phase == "terminal" {
        tx.commit()?;
        return Ok(());
    }
    let saga_phase: Option<String> = tx
        .query_row(
            "SELECT phase FROM business_command_sagas WHERE command_id = ?1",
            params![command_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(saga_phase) = saga_phase.as_deref() {
        if terminal_status == "completed" {
            anyhow::ensure!(
                saga_phase == "completed",
                "terminal success rejected: command saga is `{saga_phase}`"
            );
        } else {
            anyhow::ensure!(
                matches!(saga_phase, "compensated" | "manual_intervention"),
                "terminal failure rejected until saga compensation is durably captured (phase `{saga_phase}`)"
            );
        }
    }
    crate::command_lifecycle::validate_execution_phase_transition(&phase, "terminal")?;
    let now = now_iso_string();
    let mut linked_task_id = None;
    if let Some(task_id) = tx
        .query_row(
            "SELECT task_id FROM business_command_task_links WHERE command_id = ?1",
            params![command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        // Only a linked queue task needs the attached projection stores.
        // Keep this decision under the same transaction as the link lookup
        // and queue settlement; unrelated control commands stay core-only.
        attach_queue_projection_store(root, &tx)?;
        linked_task_id = Some(task_id.clone());
        let current_route =
            canonical_queue_route_status(&current_queue_route_status(&tx, &task_id)?)?;
        if matches!(
            current_route,
            QueueRouteStatus::Leased | QueueRouteStatus::Running
        ) {
            let settled_route = match terminal_status {
                "completed" => QueueRouteStatus::Handled,
                "cancelled" => QueueRouteStatus::Cancelled,
                _ => QueueRouteStatus::Failed,
            };
            let reason = "linked queue task settled with terminal control command";
            let status_note = (settled_route == QueueRouteStatus::Failed).then(|| {
                error_message
                    .or_else(|| result.get("error").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(reason)
            });
            set_routing_status(
                &tx,
                &task_id,
                settled_route.as_str(),
                &now,
                "business-control-command-terminal-owner",
                reason,
                status_note,
                (settled_route == QueueRouteStatus::Handled)
                    .then_some(TerminalPolicyGrant::business_command_reviewed_terminal_success()),
            )?;
        }
    }
    let next_version = version.saturating_add(1);
    let now_ms = epoch_millis();
    let error_code = result.get("error_code").and_then(Value::as_str);
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = 'terminal', terminal_status = ?2,
             projection_version = ?3, result_json = ?4, error_code = ?5,
             error_message = ?6, retryable = 0, updated_at_ms = ?7
         WHERE command_id = ?1",
        params![
            command_id,
            terminal_status,
            next_version,
            serde_json::to_string(result)?,
            error_code,
            error_message,
            now_ms,
        ],
    )?;
    tx.execute(
        "UPDATE business_command_effects
         SET status = ?3, result_json = ?4, error_message = ?5, updated_at_ms = ?6
         WHERE command_id = ?1 AND effect_key = ?2",
        params![
            command_id,
            format!("control:{command_type}"),
            if terminal_status == "completed" {
                "completed"
            } else {
                "failed"
            },
            serde_json::to_string(result)?,
            error_message,
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, 'terminal', ?4, 'control effect outcome persisted', ?5, ?6)",
        params![
            command_id,
            next_version,
            phase,
            terminal_status,
            serde_json::to_string(result)?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        command_id,
        next_version,
        "command.terminal",
        &json!({
            "command_id": command_id,
            "execution_mode": "control",
            "execution_phase": "terminal",
            "terminal_status": terminal_status,
            "projection_version": next_version,
            "result": result,
        }),
        now_ms,
    )?;
    if let Some(task_id) = linked_task_id.as_deref() {
        if let Some(task) = load_queue_task_from_conn(&tx, task_id)? {
            refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn progress_business_control_command(
    root: &Path,
    command_id: &str,
    execution_phase: &str,
    result: &Value,
) -> Result<()> {
    anyhow::ensure!(
        matches!(execution_phase, "running"),
        "invalid control command progress phase"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let (phase, version) = tx.query_row(
        "SELECT execution_phase, projection_version
         FROM business_command_aggregates WHERE command_id = ?1",
        params![command_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    if phase == "terminal" || phase == execution_phase {
        tx.commit()?;
        return Ok(());
    }
    crate::command_lifecycle::validate_execution_phase_transition(&phase, execution_phase)?;
    let next_version = version.saturating_add(1);
    let now_ms = epoch_millis();
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = ?2, terminal_status = 'none', projection_version = ?3,
             result_json = ?4, retryable = 0,
             attempt = attempt + CASE WHEN execution_phase != ?2 THEN 1 ELSE 0 END,
             updated_at_ms = ?5
         WHERE command_id = ?1",
        params![
            command_id,
            execution_phase,
            next_version,
            serde_json::to_string(result)?,
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'none', 'control effect progress persisted', ?5, ?6)",
        params![
            command_id,
            next_version,
            phase,
            execution_phase,
            serde_json::to_string(result)?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        command_id,
        next_version,
        "command.progress",
        &json!({
            "command_id": command_id,
            "execution_mode": "control",
            "execution_phase": execution_phase,
            "terminal_status": "none",
            "projection_version": next_version,
            "result": result,
        }),
        now_ms,
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn transition_business_command_for_task(
    root: &Path,
    task_id: &str,
    route_status: &str,
    result: Option<&Value>,
    error_code: Option<&str>,
    error_message: Option<&str>,
    reason: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.transaction()?;
    let transitioned = transition_business_command_for_task_in_transaction(
        &tx,
        task_id,
        route_status,
        result,
        error_code,
        error_message,
        reason,
    )?;
    if let Some(task) = load_queue_task_from_conn(&tx, task_id)? {
        refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
    }
    tx.commit()?;
    Ok(transitioned)
}

pub(super) fn transition_business_command_for_task_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    route_status: &str,
    result: Option<&Value>,
    error_code: Option<&str>,
    error_message: Option<&str>,
    reason: &str,
) -> Result<bool> {
    let command_id = tx
        .query_row(
            "SELECT command_id FROM business_command_task_links WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(command_id) = command_id else {
        return Ok(false);
    };
    let (from_phase, prior_terminal, version) = tx.query_row(
        "SELECT execution_phase, terminal_status, projection_version
         FROM business_command_aggregates WHERE command_id = ?1",
        params![command_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )?;
    let normalized_route = canonical_queue_route_status(route_status)?;
    let persisted_route = if normalized_route == QueueRouteStatus::Running {
        QueueRouteStatus::Leased
    } else {
        normalized_route
    };
    let (to_phase, terminal_status) = match normalized_route {
        QueueRouteStatus::Handled => ("terminal", "completed"),
        QueueRouteStatus::Failed => ("terminal", "failed"),
        QueueRouteStatus::Cancelled => ("terminal", "cancelled"),
        QueueRouteStatus::Leased => ("leased", "none"),
        QueueRouteStatus::Running => ("running", "none"),
        QueueRouteStatus::Blocked => ("blocked", "none"),
        QueueRouteStatus::Pending
            if matches!(
                from_phase.as_str(),
                "leased" | "running" | "awaiting_review" | "validating" | "retry_wait"
            ) =>
        {
            ("retry_wait", "none")
        }
        QueueRouteStatus::Pending | QueueRouteStatus::ReviewRework => ("queued", "none"),
    };
    if from_phase == "terminal" {
        anyhow::ensure!(
            terminal_status == prior_terminal || terminal_status == "none",
            "terminal command transition conflict for task `{task_id}`"
        );
        // lease-1 (F-002): a terminal command whose queue route is still
        // `leased`/`running` is an orphaned lease — the worker that owned it
        // disappeared and previously NO path cleared the row: this function
        // returned here without touching routing, so every lease sweep and
        // boot recovery reported the row released while it stayed `leased`
        // forever (acked_at NULL, zero workers, UI showing healthy progress).
        // Settle the route to match the durable terminal command so queue
        // state, command state, and UI projections agree. Idempotent: the
        // settled row is terminal and is no longer selected by lease sweeps,
        // and the already-terminal command is never re-queued or duplicated.
        let current_route =
            canonical_queue_route_status(&current_queue_route_status(tx, task_id)?)?;
        if matches!(
            current_route,
            QueueRouteStatus::Leased | QueueRouteStatus::Running
        ) {
            let settled_route = match prior_terminal.as_str() {
                "completed" => QueueRouteStatus::Handled,
                "cancelled" => QueueRouteStatus::Cancelled,
                _ => QueueRouteStatus::Failed,
            };
            let settle_reason = format!(
                "orphaned queue lease settled to match terminal command ({prior_terminal}): {reason}"
            );
            set_routing_status(
                tx,
                task_id,
                settled_route.as_str(),
                &now_iso_string(),
                "business-command-terminal-owner",
                &settle_reason,
                Some(settle_reason.as_str()),
                (settled_route == QueueRouteStatus::Handled)
                    .then_some(TerminalPolicyGrant::business_command_reviewed_terminal_success()),
            )?;
        }
        return Ok(true);
    }
    crate::command_lifecycle::validate_execution_phase_transition(&from_phase, to_phase)?;
    if terminal_status == "completed" {
        let review_passed = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM business_command_results result_row
                JOIN business_command_aggregates aggregate_row
                  ON aggregate_row.command_id = result_row.command_id
                WHERE result_row.command_id = ?1
                  AND result_row.attempt = MAX(aggregate_row.attempt, 1)
                  AND result_row.review_status = 'passed'
                  AND result_row.validation_status = 'passed'
            )",
            params![command_id],
            |row| row.get::<_, bool>(0),
        )?;
        anyhow::ensure!(
            from_phase == "validating" && review_passed,
            "command completion requires persisted typed result plus passed review and validation"
        );
    }
    let now = now_iso_string();
    if persisted_route == QueueRouteStatus::Leased {
        let owned_lease = tx
            .query_row(
                "SELECT lease_owner, leased_at, lease_expires_at
                 FROM communication_routing_state
                 WHERE message_key=?1 AND route_status='leased'",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .is_some_and(|(owner, leased_at, expires_at)| {
                owner.is_some_and(|value| !value.trim().is_empty())
                    && leased_at.is_some_and(|value| !value.trim().is_empty())
                    && expires_at.is_some_and(|value| !value.trim().is_empty())
            });
        anyhow::ensure!(
            owned_lease,
            "business command `{command_id}` requires an owned, expiring queue lease before `{to_phase}`"
        );
    } else {
        set_routing_status(
            &tx,
            task_id,
            persisted_route.as_str(),
            &now,
            "business-command-terminal-owner",
            reason,
            error_message
                .or_else(|| (normalized_route == QueueRouteStatus::Failed).then_some(reason)),
            (persisted_route == QueueRouteStatus::Handled)
                .then_some(TerminalPolicyGrant::business_command_reviewed_terminal_success()),
        )?;
    }
    let next_version = version.saturating_add(1);
    let now_ms = epoch_millis();
    let result_json = result.map(serde_json::to_string).transpose()?;
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = ?2, terminal_status = ?3, projection_version = ?4,
             result_json = COALESCE(?5, result_json), error_code = ?6, error_message = ?7,
             retryable = CASE WHEN ?2 IN ('blocked', 'retry_wait') THEN 1 ELSE 0 END,
             attempt = attempt + CASE WHEN ?2 = 'running' AND execution_phase != 'running' THEN 1 ELSE 0 END,
             updated_at_ms = ?8
         WHERE command_id = ?1",
        params![
            command_id,
            to_phase,
            terminal_status,
            next_version,
            result_json,
            error_code,
            error_message,
            now_ms,
        ],
    )?;
    if to_phase == "terminal" {
        tx.execute(
            "UPDATE business_command_effects
             SET status = ?3, result_json = COALESCE(?4, result_json), error_message = ?5, updated_at_ms = ?6
             WHERE command_id = ?1 AND effect_key = ?2",
            params![
                command_id,
                format!("queue:{task_id}"),
                if terminal_status == "completed" { "completed" } else { "failed" },
                result_json,
                error_message,
                now_ms,
            ],
        )?;
    }
    let review_correlation = tx
        .query_row(
            "SELECT review_evidence_json FROM business_command_results
             WHERE command_id = ?1 ORDER BY attempt DESC LIMIT 1",
            params![command_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            command_id,
            next_version,
            from_phase,
            to_phase,
            terminal_status,
            reason,
            serde_json::to_string(&json!({
                "task_id": task_id,
                "route_status": persisted_route.as_str(),
                "result": result,
                "error_code": error_code,
                "error_message": error_message,
                "review_correlation": review_correlation,
            }))?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &command_id,
        next_version,
        if to_phase == "terminal" {
            "command.terminal"
        } else {
            "command.progress"
        },
        &json!({
            "command_id": command_id,
            "execution_task_id": task_id,
            "execution_phase": to_phase,
            "terminal_status": terminal_status,
            "projection_version": next_version,
            "review_correlation": review_correlation,
        }),
        now_ms,
    )?;
    Ok(true)
}

pub(crate) fn retry_failed_app_create_business_command(
    root: &Path,
    command_id: &str,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.transaction()?;
    let (task_id, command_type, from_phase, terminal_status, projection_version) = tx
        .query_row(
            "SELECT link.task_id, aggregate.command_type, aggregate.execution_phase,
                    aggregate.terminal_status, aggregate.projection_version
             FROM business_command_aggregates aggregate
             JOIN business_command_task_links link ON link.command_id = aggregate.command_id
             WHERE aggregate.command_id = ?1",
            params![command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
        .with_context(|| format!("business command `{command_id}` was not found"))?;
    anyhow::ensure!(
        command_type == "ctox.business_os.app.create",
        "business command `{command_id}` is not an app-create command"
    );
    anyhow::ensure!(
        from_phase == "terminal" && terminal_status == "failed",
        "business command `{command_id}` is not a terminal failed app-create command"
    );
    let route_status = canonical_queue_route_status(&current_queue_route_status(&tx, &task_id)?)?;
    anyhow::ensure!(
        route_status == QueueRouteStatus::Failed,
        "app-create command `{command_id}` has non-failed queue task `{task_id}` ({})",
        route_status.as_str()
    );

    let now = now_iso_string();
    let now_ms = epoch_millis();
    let next_version = projection_version.saturating_add(1);
    let reason =
        "operator retried terminal app-create command after recoverable infrastructure failure";
    set_routing_status(
        &tx,
        &task_id,
        QueueRouteStatus::Pending.as_str(),
        &now,
        "business-command-app-retry",
        reason,
        Some(reason),
        None,
    )?;
    tx.execute(
        "UPDATE communication_routing_state
         SET failure_attempt_count = 0, failure_class = NULL, retry_not_before = NULL,
             hold_reason = NULL, last_error = NULL
         WHERE message_key = ?1",
        params![task_id],
    )?;
    tx.execute(
        "UPDATE communication_messages
         SET observed_at = ?2,
             metadata_json = json_set(metadata_json, '$.status_note', ?3, '$.sort_at', ?2)
         WHERE message_key = ?1",
        params![task_id, now, reason],
    )?;
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = 'queued', terminal_status = 'none',
             projection_version = ?2, result_json = NULL, error_code = NULL,
             error_message = NULL, retryable = 0, updated_at_ms = ?3
         WHERE command_id = ?1",
        params![command_id, next_version, now_ms],
    )?;
    tx.execute(
        "UPDATE business_command_effects
         SET status = 'claimed', result_json = NULL, error_message = NULL, updated_at_ms = ?2
         WHERE command_id = ?1 AND effect_key = ?3",
        params![command_id, now_ms, format!("queue:{task_id}")],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status,
             reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, 'terminal', 'queued', 'none', ?3, ?4, ?5)",
        params![
            command_id,
            next_version,
            reason,
            serde_json::to_string(&json!({
                "task_id": task_id,
                "previous_terminal_status": terminal_status,
                "retry_kind": "operator_app_create_retry"
            }))?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        command_id,
        next_version,
        "command.progress",
        &json!({
            "command_id": command_id,
            "execution_task_id": task_id,
            "execution_phase": "queued",
            "terminal_status": "none",
            "projection_version": next_version,
            "retry_kind": "operator_app_create_retry"
        }),
        now_ms,
    )?;
    let task = load_queue_task_from_conn(&tx, &task_id)?
        .with_context(|| format!("failed to reload app-create queue task `{task_id}`"))?;
    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
    tx.commit()?;
    Ok(json!({
        "ok": true,
        "command_id": command_id,
        "task_id": task_id,
        "status": "queued",
        "task_status": "pending",
        "projection_version": next_version
    }))
}

pub(crate) fn persist_business_command_worker_result(
    root: &Path,
    task_id: &str,
    user_reply: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let row = tx
        .query_row(
            "SELECT aggregate_row.command_id, aggregate_row.execution_phase,
                    aggregate_row.projection_version, MAX(aggregate_row.attempt, 1)
             FROM business_command_task_links link
             JOIN business_command_aggregates aggregate_row ON aggregate_row.command_id = link.command_id
             WHERE link.task_id = ?1",
            params![task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((command_id, from_phase, version, attempt)) = row else {
        tx.commit()?;
        return Ok(false);
    };
    if from_phase == "terminal" {
        tx.commit()?;
        return Ok(true);
    }
    crate::command_lifecycle::validate_execution_phase_transition(&from_phase, "awaiting_review")?;
    let existing = tx
        .query_row(
            "SELECT user_reply FROM business_command_results WHERE command_id = ?1 AND attempt = ?2",
            params![command_id, attempt],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        anyhow::ensure!(
            existing == user_reply,
            "worker result for command `{command_id}` attempt {attempt} is immutable"
        );
        tx.commit()?;
        return Ok(true);
    }
    let now_ms = epoch_millis();
    let result = json!({
        "command_id": command_id,
        "execution_task_id": task_id,
        "attempt": attempt,
        "status": "succeeded",
        "user_message": user_reply,
        // Compatibility alias for existing Business OS projections. The
        // canonical lifecycle-v2 field is `user_message`.
        "user_reply": user_reply,
        "structured_output": Value::Null,
        "artifacts": [],
        "writebacks": [],
        "verification_claims": [],
        "retry": Value::Null,
        "error": null,
    });
    tx.execute(
        "INSERT INTO business_command_results
            (command_id, attempt, status, user_reply, created_at_ms)
         VALUES (?1, ?2, 'succeeded', ?3, ?4)",
        params![command_id, attempt, user_reply, now_ms],
    )?;
    let next_version = version.saturating_add(1);
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = 'awaiting_review', attempt = ?2, projection_version = ?3,
             result_json = ?4, updated_at_ms = ?5
         WHERE command_id = ?1",
        params![
            command_id,
            attempt,
            next_version,
            serde_json::to_string(&result)?,
            now_ms,
        ],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, 'awaiting_review', 'none', 'typed worker result persisted before review', ?4, ?5)",
        params![
            command_id,
            next_version,
            from_phase,
            serde_json::to_string(&json!({"task_id": task_id, "attempt": attempt}))?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &command_id,
        next_version,
        "command.result_persisted",
        &json!({
            "command_id": command_id,
            "execution_task_id": task_id,
            "execution_phase": "awaiting_review",
            "projection_version": next_version,
        }),
        now_ms,
    )?;
    tx.commit()?;
    crate::business_os::harness_cockpit::schedule_refresh(root);
    Ok(true)
}

pub(crate) fn record_business_command_review(
    root: &Path,
    task_id: &str,
    review_status: &str,
    validation_status: &str,
    evidence: &Value,
) -> Result<bool> {
    anyhow::ensure!(
        matches!(review_status, "passed" | "failed" | "held"),
        "invalid command review status"
    );
    anyhow::ensure!(
        matches!(validation_status, "passed" | "failed" | "pending"),
        "invalid command validation status"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let row = tx
        .query_row(
            "SELECT aggregate_row.command_id, aggregate_row.execution_phase,
                    aggregate_row.projection_version, MAX(aggregate_row.attempt, 1)
             FROM business_command_task_links link
             JOIN business_command_aggregates aggregate_row ON aggregate_row.command_id = link.command_id
             WHERE link.task_id = ?1",
            params![task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((command_id, from_phase, version, attempt)) = row else {
        tx.commit()?;
        return Ok(false);
    };
    anyhow::ensure!(from_phase != "terminal", "cannot review a terminal command");
    let retryable_hold = review_status == "held"
        && evidence
            .get("retryable_hold")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let changed = tx.execute(
        "UPDATE business_command_results
         SET review_status = ?3, validation_status = ?4, review_evidence_json = ?5,
             reviewed_at_ms = ?6
         WHERE command_id = ?1 AND attempt = ?2",
        params![
            command_id,
            attempt,
            review_status,
            validation_status,
            serde_json::to_string(evidence)?,
            epoch_millis(),
        ],
    )?;
    anyhow::ensure!(
        changed == 1,
        "typed worker result is required before review"
    );
    let to_phase = if review_status == "passed" && validation_status == "passed" {
        "validating"
    } else if review_status == "failed" || validation_status == "failed" || retryable_hold {
        "retry_wait"
    } else {
        "blocked"
    };
    crate::command_lifecycle::validate_execution_phase_transition(&from_phase, to_phase)?;
    let next_version = version.saturating_add(1);
    let now_ms = epoch_millis();
    tx.execute(
        "UPDATE business_command_aggregates
         SET execution_phase = ?2, projection_version = ?3,
             retryable = CASE WHEN ?2 IN ('retry_wait', 'blocked') THEN 1 ELSE 0 END,
             updated_at_ms = ?4
         WHERE command_id = ?1",
        params![command_id, to_phase, next_version, now_ms],
    )?;
    tx.execute(
        "INSERT INTO business_command_transitions
            (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'none', 'completion review and validation recorded', ?5, ?6)",
        params![
            command_id,
            next_version,
            from_phase,
            to_phase,
            serde_json::to_string(evidence)?,
            now_ms,
        ],
    )?;
    insert_business_command_outbox_rows(
        &tx,
        &command_id,
        next_version,
        "command.reviewed",
        &json!({
            "command_id": command_id,
            "execution_task_id": task_id,
            "execution_phase": to_phase,
            "projection_version": next_version,
        }),
        now_ms,
    )?;
    tx.commit()?;
    Ok(true)
}

pub(super) fn insert_business_command_outbox_rows(
    tx: &Transaction<'_>,
    command_id: &str,
    projection_version: i64,
    event_type: &str,
    payload: &Value,
    created_at_ms: i64,
) -> Result<()> {
    let payload_json = serde_json::to_string(payload)?;
    for destination in ["business-os", "rxdb"] {
        let event_id = format!("cmd-outbox:{command_id}:{projection_version}:{destination}");
        tx.execute(
            "INSERT OR IGNORE INTO business_command_outbox
                (event_id, command_id, projection_version, destination, event_type, payload_json,
                 status, attempts, next_attempt_at_ms, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0, 0, ?7)",
            params![
                event_id,
                command_id,
                projection_version,
                destination,
                event_type,
                payload_json,
                created_at_ms,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn pending_business_command_outbox(
    root: &Path,
    limit: usize,
) -> Result<Vec<BusinessCommandOutboxEvent>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let now_ms = epoch_millis();
    let mut stmt = conn.prepare(
        "SELECT event_id, command_id, projection_version, destination, event_type, attempts
         FROM business_command_outbox
         WHERE status IN ('pending', 'failed') AND next_attempt_at_ms <= ?1
         ORDER BY created_at_ms ASC, event_id ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![now_ms, limit.max(1) as i64], |row| {
        Ok(BusinessCommandOutboxEvent {
            event_id: row.get(0)?,
            command_id: row.get(1)?,
            projection_version: row.get(2)?,
            destination: row.get(3)?,
            event_type: row.get(4)?,
            attempts: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(crate) fn business_command_projection(root: &Path, command_id: &str) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let (
        module,
        command_type,
        record_id,
        payload_hash,
        execution_mode,
        execution_phase,
        terminal_status,
        attempt,
        projection_version,
        intent_json,
        result_json,
        error_code,
        error_message,
        retryable,
        created_at_ms,
        updated_at_ms,
    ) = conn.query_row(
        "SELECT module, command_type, record_id,
                payload_hash, execution_mode, execution_phase, terminal_status, attempt,
                projection_version, intent_json, result_json, error_code, error_message,
                retryable, created_at_ms, updated_at_ms
         FROM business_command_aggregates WHERE command_id = ?1",
        params![command_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, u32>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, i64>(13)? != 0,
                row.get::<_, i64>(14)?,
                row.get::<_, i64>(15)?,
            ))
        },
    )?;
    let mut projection: Value = serde_json::from_str(&intent_json)?;
    anyhow::ensure!(
        projection.is_object(),
        "canonical command intent must be an object"
    );
    let task_id = conn
        .query_row(
            "SELECT task_id FROM business_command_task_links WHERE command_id = ?1",
            params![command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default();
    let saga = conn
        .query_row(
            "SELECT saga_id, phase, current_step, total_steps, compensation_status
         FROM business_command_sagas WHERE command_id = ?1",
            params![command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let status = if execution_phase == "terminal" {
        terminal_status.as_str()
    } else if execution_phase == "waiting_dependencies" {
        "waiting_dependencies"
    } else {
        "accepted"
    };
    let object = projection.as_object_mut().expect("object checked above");
    object.insert("id".to_string(), Value::String(command_id.to_string()));
    object.insert(
        "command_id".to_string(),
        Value::String(command_id.to_string()),
    );
    object.insert("module".to_string(), Value::String(module));
    object.insert("command_type".to_string(), Value::String(command_type));
    object.insert("record_id".to_string(), Value::String(record_id));
    object.insert("contract_version".to_string(), Value::from(2));
    object.insert("status".to_string(), Value::String(status.to_string()));
    object.insert(
        "replication_phase".to_string(),
        Value::String("native_observed".to_string()),
    );
    object.insert(
        "execution_mode".to_string(),
        Value::String(execution_mode.clone()),
    );
    object.insert(
        "execution_phase".to_string(),
        Value::String(execution_phase.clone()),
    );
    object.insert(
        "terminal_status".to_string(),
        Value::String(terminal_status.clone()),
    );
    object.insert(
        "execution_task_id".to_string(),
        Value::String(task_id.clone()),
    );
    object.insert("task_id".to_string(), Value::String(task_id.clone()));
    if !task_id.is_empty() {
        if let Some(progress) =
            crate::lcm::run_task_execution_progress_for_task(&db_path, &task_id)?
        {
            object.insert("execution_progress".to_string(), progress);
        }
    }
    if let Some((saga_id, saga_phase, saga_step, saga_total_steps, compensation_status)) = saga {
        object.insert("saga_id".to_string(), Value::String(saga_id));
        object.insert("saga_phase".to_string(), Value::String(saga_phase));
        object.insert("saga_step".to_string(), Value::from(saga_step));
        object.insert(
            "saga_total_steps".to_string(),
            Value::from(saga_total_steps),
        );
        object.insert(
            "compensation_status".to_string(),
            Value::String(compensation_status),
        );
        if execution_phase != "terminal" {
            object.insert("pending_consistency".to_string(), Value::Bool(true));
        }
    }
    let (route_status, task_status) = if execution_phase == "terminal" {
        match terminal_status.as_str() {
            "completed" => (QueueRouteStatus::Handled, "completed"),
            "cancelled" => (QueueRouteStatus::Cancelled, "cancelled"),
            _ => (QueueRouteStatus::Failed, "failed"),
        }
    } else {
        match execution_phase.as_str() {
            "leased" | "running" | "awaiting_review" | "validating" => {
                (QueueRouteStatus::Leased, "running")
            }
            "blocked" | "waiting_dependencies" => (QueueRouteStatus::Blocked, "blocked"),
            _ => (QueueRouteStatus::Pending, "queued"),
        }
    };
    object.insert(
        "route_status".to_string(),
        Value::String(route_status.as_str().to_string()),
    );
    object.insert(
        "task_status".to_string(),
        Value::String(task_status.to_string()),
    );
    object.insert("attempt".to_string(), Value::from(attempt));
    object.insert(
        "projection_version".to_string(),
        Value::from(projection_version),
    );
    object.insert("payload_hash".to_string(), Value::String(payload_hash));
    object.insert("retryable".to_string(), Value::Bool(retryable));
    object.insert("created_at_ms".to_string(), Value::from(created_at_ms));
    object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    if let Some(raw) = result_json {
        let result: Value = serde_json::from_str(&raw)?;
        if let Some(result_object) = result.as_object() {
            for field in ["outbound_text", "response", "answer"] {
                if let Some(value) = result_object.get(field) {
                    object.insert(field.to_string(), value.clone());
                }
            }
        }
        object.insert("result".to_string(), result);
    }
    if let Some(value) = error_code {
        object.insert("error_code".to_string(), Value::String(value));
    }
    if let Some(value) = error_message {
        object.insert("error_message".to_string(), Value::String(value));
    }
    Ok(projection)
}

pub(crate) fn inspect_business_command(root: &Path, command_id: &str) -> Result<Option<Value>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let exists = conn
        .query_row(
            "SELECT 1 FROM business_command_aggregates WHERE command_id = ?1",
            params![command_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let mut command = business_command_projection(root, command_id)?;
    redact_command_secrets(&mut command);
    let task_id = conn
        .query_row(
            "SELECT task_id FROM business_command_task_links WHERE command_id = ?1",
            params![command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let mut transitions = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT projection_version, from_phase, to_phase, terminal_status, reason,
                evidence_json, created_at_ms
         FROM business_command_transitions WHERE command_id = ?1
         ORDER BY projection_version ASC",
    )?;
    let rows = stmt.query_map(params![command_id], |row| {
        Ok(json!({
            "projection_version": row.get::<_, i64>(0)?,
            "from_phase": row.get::<_, String>(1)?,
            "to_phase": row.get::<_, String>(2)?,
            "terminal_status": row.get::<_, String>(3)?,
            "reason": row.get::<_, String>(4)?,
            "evidence": serde_json::from_str::<Value>(&row.get::<_, String>(5)?)
                .unwrap_or(Value::Null),
            "created_at_ms": row.get::<_, i64>(6)?,
        }))
    })?;
    for row in rows {
        transitions.push(row?);
    }
    let dependencies = command
        .pointer("/payload/dependencies")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let attachments = command
        .pointer("/payload/attachments")
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok(Some(json!({
        "schema": "ctox.business_os.command_context.v1",
        "command": command,
        "execution_task_id": task_id,
        "dependencies": dependencies,
        "attachments": attachments,
        "transitions": transitions,
    })))
}

pub(crate) fn inspect_business_command_for_task(
    root: &Path,
    task_id: &str,
) -> Result<Option<Value>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let command_id = conn
        .query_row(
            "SELECT command_id FROM business_command_task_links WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    command_id
        .as_deref()
        .map(|command_id| inspect_business_command(root, command_id))
        .transpose()
        .map(Option::flatten)
}

pub(super) fn redact_command_secrets(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in [
                "capability_token",
                "authorization",
                "access_token",
                "refresh_token",
                "secret",
            ] {
                if object.contains_key(key) {
                    object.insert(key.to_string(), Value::String("[REDACTED]".to_string()));
                }
            }
            for child in object.values_mut() {
                redact_command_secrets(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_command_secrets(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn mark_business_command_outbox_delivered(root: &Path, event_id: &str) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    Ok(conn.execute(
        "UPDATE business_command_outbox
         SET status = 'delivered', delivered_at_ms = ?2, last_error = NULL
         WHERE event_id = ?1 AND status != 'delivered'",
        params![event_id, epoch_millis()],
    )? > 0)
}

pub(crate) fn mark_business_command_outbox_failed(
    root: &Path,
    event_id: &str,
    error: &str,
    max_attempts: u32,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let attempts = tx.query_row(
        "SELECT attempts + 1 FROM business_command_outbox WHERE event_id = ?1",
        params![event_id],
        |row| row.get::<_, u32>(0),
    )?;
    let dead_letter = attempts >= max_attempts.max(1);
    let backoff_ms = 250_i64.saturating_mul(1_i64 << attempts.min(8));
    tx.execute(
        "UPDATE business_command_outbox
         SET status = ?2, attempts = ?3, next_attempt_at_ms = ?4, last_error = ?5
         WHERE event_id = ?1",
        params![
            event_id,
            if dead_letter { "dead_letter" } else { "failed" },
            attempts,
            epoch_millis().saturating_add(backoff_ms),
            error,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// Kurzlebiger Cache fuer die Kern-Diagnose.
///
/// Diese Funktion faehrt sechs Abfragen, darunter zwei LEFT JOINs ueber
/// `business_command_aggregates` x `business_command_task_links` x
/// `communication_messages`, und oeffnet dafuer je Aufruf eine eigene
/// DB-Verbindung. Sie haengt an `native_peer_status` und darueber am
/// `sync_config_for_browser` — also an JEDEM Laden der Business-OS-Shell.
///
/// Gemessen am 06.08. auf einer gewachsenen Instanz (Kern-DB 1,1 GB): alle
/// vier HTTP-Worker standen gleichzeitig in genau diesem `sqlite3Select`,
/// der Port nahm Verbindungen an und beantwortete keine. Seitenaufbau 33 s,
/// API-Aufrufe 44 s, Reloads ueber 300 s. Nebenbei war das die Quelle des
/// Verbindungslecks: 1.366 offene Deskriptoren auf einer einzigen Datei.
///
/// Der Wert ist eine Gesundheitsmomentaufnahme, kein autoritativer Zustand —
/// der einzige Konsument auf dem heissen Pfad liest daraus `oldest_outbox_age_ms`
/// und toleriert ausdruecklich `null`. Ein paar Sekunden Alter sind fachlich
/// unerheblich; ein blockierter Seitenaufbau ist es nicht.
const CORE_DIAGNOSTICS_TTL: Duration = Duration::from_secs(3);

#[allow(clippy::type_complexity)]
static CORE_DIAGNOSTICS_CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, Value)>>> =
    OnceLock::new();

pub(crate) fn business_command_core_diagnostics(root: &Path) -> Result<Value> {
    let cache = CORE_DIAGNOSTICS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = root.to_path_buf();
    {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((measured_at, value)) = guard.get(&key) {
            if measured_at.elapsed() < CORE_DIAGNOSTICS_TTL {
                return Ok(value.clone());
            }
        }
    }
    let fresh = business_command_core_diagnostics_uncached(root)?;
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(key, (Instant::now(), fresh.clone()));
    Ok(fresh)
}

fn business_command_core_diagnostics_uncached(root: &Path) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let aggregate_count = conn.query_row(
        "SELECT COUNT(*) FROM business_command_aggregates",
        [],
        |row| row.get::<_, u64>(0),
    )?;
    let queue_commands_without_link = conn.query_row(
        "SELECT COUNT(*) FROM business_command_aggregates aggregate_row
         LEFT JOIN business_command_task_links link ON link.command_id = aggregate_row.command_id
         WHERE aggregate_row.execution_mode = 'queue'
           AND aggregate_row.execution_phase NOT IN ('waiting_dependencies', 'terminal')
           AND link.command_id IS NULL",
        [],
        |row| row.get::<_, u64>(0),
    )?;
    let links_without_task = conn.query_row(
        "SELECT COUNT(*) FROM business_command_task_links link
         LEFT JOIN communication_messages task ON task.message_key = link.task_id
         WHERE task.message_key IS NULL",
        [],
        |row| row.get::<_, u64>(0),
    )?;
    let (pending_outbox, dead_letter_outbox, oldest_pending_created_at_ms) = conn.query_row(
        "SELECT
            COALESCE(SUM(CASE WHEN status IN ('pending', 'failed') THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN status = 'dead_letter' THEN 1 ELSE 0 END), 0),
            MIN(CASE WHEN status IN ('pending', 'failed') THEN created_at_ms END)
         FROM business_command_outbox",
        [],
        |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    let uncertain_effects = conn.query_row(
        "SELECT COUNT(*) FROM business_command_effects WHERE status = 'uncertain'",
        [],
        |row| row.get::<_, u64>(0),
    )?;
    let (open_intake_failures, exhausted_intake_failures) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN exhausted = 1 THEN 1 ELSE 0 END), 0)
         FROM business_command_intake_failures WHERE resolved_at_ms IS NULL",
        [],
        |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
    )?;
    let oldest_outbox_age_ms =
        oldest_pending_created_at_ms.map(|created| epoch_millis().saturating_sub(created).max(0));
    Ok(json!({
        "aggregate_count": aggregate_count,
        "queue_commands_without_link": queue_commands_without_link,
        "links_without_task": links_without_task,
        "orphan_link_count": queue_commands_without_link.saturating_add(links_without_task),
        "pending_outbox": pending_outbox,
        "dead_letter_outbox": dead_letter_outbox,
        "oldest_outbox_age_ms": oldest_outbox_age_ms,
        "uncertain_effects": uncertain_effects,
        "open_intake_failures": open_intake_failures,
        "exhausted_intake_failures": exhausted_intake_failures,
        "duplicate_effect_count": 0,
    }))
}

pub(crate) fn record_business_command_intake_failure(
    root: &Path,
    claim: BusinessCommandClaimRequest,
    error_message: &str,
    retry_budget: u32,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let tx = conn.transaction()?;
    let existing_exhausted_attempt = tx
        .query_row(
            "SELECT attempt
             FROM business_command_intake_failures
             WHERE command_id = ?1 AND resolved_at_ms IS NULL AND exhausted = 1
             ORDER BY attempt DESC
             LIMIT 1",
            params![claim.command_id],
            |row| row.get::<_, u32>(0),
        )
        .optional()?;
    let now_ms = epoch_millis();
    let (attempt, exhausted) = if let Some(attempt) = existing_exhausted_attempt {
        tx.execute(
            "UPDATE business_command_intake_failures
             SET observed_at_ms = ?3, error_message = ?4
             WHERE command_id = ?1 AND attempt = ?2 AND resolved_at_ms IS NULL AND exhausted = 1",
            params![claim.command_id, attempt, now_ms, error_message],
        )?;
        (attempt, true)
    } else {
        let attempt = tx.query_row(
            "SELECT COALESCE(MAX(attempt), 0) + 1
             FROM business_command_intake_failures
             WHERE command_id = ?1 AND resolved_at_ms IS NULL",
            params![claim.command_id],
            |row| row.get::<_, u32>(0),
        )?;
        let exhausted = attempt >= retry_budget.max(1);
        tx.execute(
            "INSERT INTO business_command_intake_failures
                (command_id, attempt, error_message, exhausted, observed_at_ms, resolved_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                claim.command_id,
                attempt,
                error_message,
                if exhausted { 1 } else { 0 },
                now_ms,
            ],
        )?;
        (attempt, exhausted)
    };
    let canonical = tx
        .query_row(
            "SELECT idempotency_key, payload_hash, execution_phase, terminal_status,
                    projection_version
             FROM business_command_aggregates WHERE command_id = ?1",
            params![claim.command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let canonical_exists = canonical.is_some();
    let idempotency_conflict =
        canonical
            .as_ref()
            .is_some_and(|(idempotency_key, payload_hash, _, _, _)| {
                idempotency_key != &claim.idempotency_key || payload_hash != &claim.payload_hash
            });
    let canonical_already_terminal =
        canonical
            .as_ref()
            .is_some_and(|(_, _, phase, terminal_status, _)| {
                phase == "terminal" || terminal_status != "none"
            });
    let mut canonical_failure_created = false;
    let mut next_projection_version = 1_i64;
    let mut prior_phase = "native_observed".to_string();
    if exhausted && !idempotency_conflict && !canonical_already_terminal {
        let failure_result = json!({
            "ok": false,
            "error_code": "native_unavailable",
            "error_message": error_message,
        });
        if let Some((_, _, phase, _, projection_version)) = canonical.as_ref() {
            prior_phase = phase.clone();
            next_projection_version = projection_version.saturating_add(1);
            tx.execute(
                "UPDATE business_command_aggregates
                 SET execution_phase = 'terminal', terminal_status = 'failed', attempt = ?2,
                     projection_version = ?3, result_json = ?4,
                     error_code = 'native_unavailable', error_message = ?5,
                     retryable = 0, updated_at_ms = ?6
                 WHERE command_id = ?1 AND execution_phase != 'terminal'",
                params![
                    claim.command_id,
                    attempt,
                    next_projection_version,
                    serde_json::to_string(&failure_result)?,
                    error_message,
                    now_ms,
                ],
            )?;
        } else {
            tx.execute(
                "INSERT INTO business_command_aggregates
                    (command_id, idempotency_key, payload_hash, module, command_type, record_id,
                     execution_mode, execution_phase, terminal_status, attempt, projection_version,
                     intent_json, result_json, error_code, error_message, retryable,
                     created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'control', 'terminal', 'failed', ?7, 1,
                         ?8, ?9, 'native_unavailable', ?10, 0, ?11, ?12)",
                params![
                    claim.command_id,
                    claim.idempotency_key,
                    claim.payload_hash,
                    claim.module,
                    claim.command_type,
                    claim.record_id,
                    attempt,
                    serde_json::to_string(&claim.intent)?,
                    serde_json::to_string(&failure_result)?,
                    error_message,
                    claim.created_at_ms,
                    now_ms,
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO business_command_transitions
                (command_id, projection_version, from_phase, to_phase, terminal_status, reason, evidence_json, created_at_ms)
             VALUES (?1, ?2, ?3, 'terminal', 'failed', 'native intake retry budget exhausted', ?4, ?5)",
            params![
                claim.command_id,
                next_projection_version,
                prior_phase,
                serde_json::to_string(&json!({
                    "attempt": attempt,
                    "error_message": error_message,
                }))?,
                now_ms,
            ],
        )?;
        insert_business_command_outbox_rows(
            &tx,
            &claim.command_id,
            next_projection_version,
            "command.intake_exhausted",
            &json!({
                "command_id": claim.command_id,
                "execution_phase": "terminal",
                "terminal_status": "failed",
                "projection_version": next_projection_version,
            }),
            now_ms,
        )?;
        canonical_failure_created = true;
    }
    tx.commit()?;
    let terminal_projection_ready = exhausted
        && (canonical_failure_created || canonical_already_terminal || idempotency_conflict);
    let failure_document = if canonical_failure_created || canonical_already_terminal {
        business_command_projection(root, &claim.command_id)?
    } else if idempotency_conflict {
        intake_failure_projection(
            &claim.intent,
            &claim.command_id,
            attempt,
            now_ms,
            "idempotency_conflict",
            "command id was already claimed with different immutable intent",
        )
    } else {
        claim.intent
    };
    Ok(json!({
        "command_id": claim.command_id,
        "attempt": attempt,
        "exhausted": exhausted,
        "canonical_exists": canonical_exists,
        "canonical_failure_created": canonical_failure_created,
        "canonical_already_terminal": canonical_already_terminal,
        "idempotency_conflict": idempotency_conflict,
        "terminal_projection_ready": terminal_projection_ready,
        "failure_document": failure_document,
    }))
}

fn intake_failure_projection(
    intent: &Value,
    command_id: &str,
    attempt: u32,
    updated_at_ms: i64,
    error_code: &str,
    error_message: &str,
) -> Value {
    let mut projection = intent.clone();
    if !projection.is_object() {
        projection = json!({});
    }
    let object = projection.as_object_mut().expect("projection is an object");
    let updated_at_ms = object
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .map(|previous| updated_at_ms.max(previous.saturating_add(1)))
        .unwrap_or(updated_at_ms);
    object.insert("id".to_string(), Value::String(command_id.to_string()));
    object.insert(
        "command_id".to_string(),
        Value::String(command_id.to_string()),
    );
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
    object.insert("attempt".to_string(), Value::from(attempt));
    object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    object.insert("retryable".to_string(), Value::Bool(false));
    object.insert(
        "error_code".to_string(),
        Value::String(error_code.to_string()),
    );
    object.insert(
        "error_message".to_string(),
        Value::String(error_message.to_string()),
    );
    projection
}

pub(crate) fn resolve_business_command_intake_failures(
    root: &Path,
    command_id: &str,
) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    Ok(conn.execute(
        "UPDATE business_command_intake_failures SET resolved_at_ms = ?2
         WHERE command_id = ?1 AND resolved_at_ms IS NULL",
        params![command_id, epoch_millis()],
    )?)
}

pub(crate) fn business_command_retention_maintenance(root: &Path, apply: bool) -> Result<Value> {
    const LARGE_RESULT_BYTES: usize = 64 * 1024;
    const DELIVERED_OUTBOX_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let mut candidates = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT command_id, result_json FROM business_command_aggregates aggregate_row
             WHERE execution_phase = 'terminal'
               AND length(COALESCE(result_json, '')) > ?1
               AND NOT EXISTS (
                    SELECT 1 FROM business_command_outbox outbox
                    WHERE outbox.command_id = aggregate_row.command_id
                      AND outbox.status != 'delivered'
               )",
        )?;
        let rows = stmt.query_map(params![LARGE_RESULT_BYTES as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            candidates.push(row?);
        }
    }
    let artifact_root = root.join("runtime/business-command-artifacts");
    let mut externalized = 0_u64;
    if apply && !candidates.is_empty() {
        fs::create_dir_all(&artifact_root)?;
        let tx = conn.transaction()?;
        for (command_id, result_json) in &candidates {
            let digest = sha256_hex(result_json.as_bytes());
            let file_name = format!(
                "{}-{}.json",
                sanitize_path_component(command_id),
                &digest[..16]
            );
            let path = artifact_root.join(file_name);
            fs::write(&path, result_json)?;
            let reference = json!({
                "externalized": true,
                "artifact_ref": path.strip_prefix(root).unwrap_or(&path).display().to_string(),
                "sha256": digest,
                "size_bytes": result_json.len(),
            });
            tx.execute(
                "UPDATE business_command_aggregates SET result_json = ?2 WHERE command_id = ?1",
                params![command_id, serde_json::to_string(&reference)?],
            )?;
            externalized = externalized.saturating_add(1);
        }
        tx.commit()?;
    }
    let cutoff = epoch_millis().saturating_sub(DELIVERED_OUTBOX_RETENTION_MS);
    let delivered_outbox_candidates = conn.query_row(
        "SELECT COUNT(*) FROM business_command_outbox
         WHERE status = 'delivered' AND delivered_at_ms < ?1",
        params![cutoff],
        |row| row.get::<_, u64>(0),
    )?;
    let pruned_outbox = if apply {
        conn.execute(
            "DELETE FROM business_command_outbox
             WHERE status = 'delivered' AND delivered_at_ms < ?1",
            params![cutoff],
        )? as u64
    } else {
        0
    };
    Ok(json!({
        "apply": apply,
        "large_result_candidates": candidates.len(),
        "externalized_results": externalized,
        "delivered_outbox_candidates": delivered_outbox_candidates,
        "pruned_delivered_outbox": pruned_outbox,
        "aggregate_and_transition_evidence_deleted": 0,
        "policy": {
            "large_result_bytes": LARGE_RESULT_BYTES,
            "delivered_outbox_retention_ms": DELIVERED_OUTBOX_RETENTION_MS,
        },
    }))
}

const LEGACY_CANCELLED_QUEUE_COMMAND_MIGRATION: &str = "2026-07-31-legacy-cancelled-queue-command";

fn business_command_data_migration_applied(conn: &Connection, migration_id: &str) -> Result<bool> {
    let table_exists = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name = 'business_command_data_migrations'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM business_command_data_migrations WHERE migration_id = ?1
         )",
        params![migration_id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(Into::into)
}

fn cancelled_queue_command_rows(conn: &Connection) -> Result<Vec<(String, String, String)>> {
    let mut rows = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT link.command_id, link.task_id, aggregate_row.execution_phase
         FROM business_command_task_links link
         JOIN business_command_aggregates aggregate_row
           ON aggregate_row.command_id = link.command_id
         JOIN communication_routing_state route
           ON route.message_key = link.task_id
         WHERE aggregate_row.execution_mode = 'queue'
           AND aggregate_row.execution_phase != 'terminal'
           AND route.route_status = 'cancelled'",
    )?;
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        rows.push(row?);
    }
    Ok(rows)
}

fn terminal_failure_queue_command_rows(
    conn: &Connection,
) -> Result<Vec<(String, String, String, String, Option<String>)>> {
    let mut rows = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT link.command_id, link.task_id, aggregate_row.execution_phase,
                route.route_status, route.last_error
         FROM business_command_task_links link
         JOIN business_command_aggregates aggregate_row
           ON aggregate_row.command_id = link.command_id
         JOIN communication_routing_state route
           ON route.message_key = link.task_id
         WHERE aggregate_row.execution_mode = 'queue'
           AND aggregate_row.execution_phase != 'terminal'
           AND route.route_status IN ('failed', 'cancelled')",
    )?;
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })? {
        rows.push(row?);
    }
    Ok(rows)
}

fn resolvable_transient_intake_failures(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT failure.command_id
         FROM business_command_intake_failures failure
         JOIN business_command_aggregates aggregate_row
           ON aggregate_row.command_id = failure.command_id
         WHERE failure.resolved_at_ms IS NULL
           AND failure.exhausted = 0
           AND (
                lower(failure.error_message) LIKE '%database is locked%'
                OR lower(failure.error_message) LIKE '%database table is locked%'
                OR lower(failure.error_message) LIKE '%sqlite_busy%'
                OR lower(failure.error_message) LIKE '%cannot promote read transaction%'
           )
           AND EXISTS (
                SELECT 1 FROM business_command_outbox delivered
                WHERE delivered.command_id = failure.command_id
                  AND delivered.destination = 'rxdb'
                  AND delivered.status = 'delivered'
           )
           AND NOT EXISTS (
                SELECT 1 FROM business_command_outbox incomplete
                WHERE incomplete.command_id = failure.command_id
                  AND incomplete.status != 'delivered'
           )
         ORDER BY failure.command_id",
    )?;
    let mut command_ids = Vec::new();
    for row in stmt.query_map([], |row| row.get::<_, String>(0))? {
        command_ids.push(row?);
    }
    Ok(command_ids)
}

/// Audits the two storage inconsistencies for which no repository writer could
/// be identified (`missing_task_links` and `task_links_to_missing_tasks`). It
/// deliberately does not guess repairs for either class. With `apply`, it also
/// repairs rows left by legacy route-only cancellation writers exactly once per
/// database and records a durable migration marker.
pub(crate) fn audit_and_migrate_business_command_storage(
    root: &Path,
    apply: bool,
) -> Result<Value> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let mut missing_links = Vec::new();
    let mut missing_tasks = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT aggregate_row.command_id, aggregate_row.execution_phase
             FROM business_command_aggregates aggregate_row
             LEFT JOIN business_command_task_links link ON link.command_id = aggregate_row.command_id
             WHERE aggregate_row.execution_mode = 'queue'
               AND aggregate_row.execution_phase NOT IN ('waiting_dependencies', 'terminal')
               AND link.command_id IS NULL",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (command_id, phase) = row?;
            missing_links.push(json!({"command_id": command_id, "execution_phase": phase}));
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT link.command_id, link.task_id FROM business_command_task_links link
             LEFT JOIN communication_messages task ON task.message_key = link.task_id
             WHERE task.message_key IS NULL",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (command_id, task_id) = row?;
            missing_tasks.push(json!({"command_id": command_id, "execution_task_id": task_id}));
        }
    }

    let resolvable_intake_failures = resolvable_transient_intake_failures(&conn)?;
    let mut resolved_intake_failures = 0_u64;
    if apply && !resolvable_intake_failures.is_empty() {
        let tx = conn.transaction()?;
        let resolved_at_ms = epoch_millis();
        for command_id in &resolvable_intake_failures {
            resolved_intake_failures = resolved_intake_failures.saturating_add(tx.execute(
                "UPDATE business_command_intake_failures
                     SET resolved_at_ms = ?2
                     WHERE command_id = ?1 AND resolved_at_ms IS NULL AND exhausted = 0",
                params![command_id, resolved_at_ms],
            )?
                as u64);
        }
        tx.commit()?;
    }

    let mut migration_already_applied =
        business_command_data_migration_applied(&conn, LEGACY_CANCELLED_QUEUE_COMMAND_MIGRATION)?;
    let mut migration_applied_now = false;
    let mut cancelled_queue_command_drift = Vec::new();
    let mut repaired_cancelled_queue_commands = 0_u64;
    if apply && !migration_already_applied {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS business_command_data_migrations (
                migration_id TEXT PRIMARY KEY,
                applied_at_ms INTEGER NOT NULL,
                details_json TEXT NOT NULL DEFAULT '{}'
             );",
        )?;
        migration_already_applied = tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM business_command_data_migrations WHERE migration_id = ?1
             )",
            params![LEGACY_CANCELLED_QUEUE_COMMAND_MIGRATION],
            |row| row.get::<_, bool>(0),
        )?;
        if !migration_already_applied {
            cancelled_queue_command_drift = cancelled_queue_command_rows(&tx)?;
            for (_, task_id, _) in &cancelled_queue_command_drift {
                if transition_business_command_for_task_in_transaction(
                    &tx,
                    task_id,
                    "cancelled",
                    None,
                    None,
                    Some("queue task was already cancelled"),
                    "migrated legacy cancelled queue task",
                )? {
                    repaired_cancelled_queue_commands =
                        repaired_cancelled_queue_commands.saturating_add(1);
                }
            }
            tx.execute(
                "INSERT INTO business_command_data_migrations
                    (migration_id, applied_at_ms, details_json)
                 VALUES (?1, ?2, ?3)",
                params![
                    LEGACY_CANCELLED_QUEUE_COMMAND_MIGRATION,
                    epoch_millis(),
                    serde_json::to_string(&json!({
                        "candidate_count": cancelled_queue_command_drift.len(),
                        "repaired_count": repaired_cancelled_queue_commands,
                    }))?,
                ],
            )?;
            migration_applied_now = true;
        }
        tx.commit()?;
    } else if !migration_already_applied {
        cancelled_queue_command_drift = cancelled_queue_command_rows(&conn)?;
    }

    let mut terminal_failure_queue_command_drift = terminal_failure_queue_command_rows(&conn)?;
    let mut repaired_terminal_failure_queue_commands = 0_u64;
    if apply && !terminal_failure_queue_command_drift.is_empty() {
        let tx = conn.transaction()?;
        for (_, task_id, _, route_status, last_error) in &terminal_failure_queue_command_drift {
            let failure_reason = last_error
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("terminal queue route reconciled with linked business command");
            if transition_business_command_for_task_in_transaction(
                &tx,
                task_id,
                route_status,
                None,
                (route_status == "failed").then_some("queue_terminal_failure_reconciled"),
                Some(failure_reason),
                "reconciled terminal queue route with linked business command",
            )? {
                repaired_terminal_failure_queue_commands =
                    repaired_terminal_failure_queue_commands.saturating_add(1);
            }
        }
        tx.commit()?;
        terminal_failure_queue_command_drift = terminal_failure_queue_command_rows(&conn)?;
    }

    Ok(json!({
        "apply": apply,
        "missing_task_links": missing_links,
        "task_links_to_missing_tasks": missing_tasks,
        "resolvable_transient_intake_failures": resolvable_intake_failures,
        "resolved_transient_intake_failures": resolved_intake_failures,
        "cancelled_queue_command_drift": cancelled_queue_command_drift.iter().map(
            |(command_id, task_id, execution_phase)| json!({
                "command_id": command_id,
                "execution_task_id": task_id,
                "execution_phase": execution_phase,
                "queue_route_status": "cancelled",
            })
        ).collect::<Vec<_>>(),
        "legacy_cancelled_queue_command_migration": {
            "migration_id": LEGACY_CANCELLED_QUEUE_COMMAND_MIGRATION,
            "already_applied": migration_already_applied,
            "applied_now": migration_applied_now,
        },
        "repaired_cancelled_queue_commands": repaired_cancelled_queue_commands,
        "terminal_failure_queue_command_drift": terminal_failure_queue_command_drift.iter().map(
            |(command_id, task_id, execution_phase, route_status, _)| json!({
                "command_id": command_id,
                "execution_task_id": task_id,
                "execution_phase": execution_phase,
                "queue_route_status": route_status,
            })
        ).collect::<Vec<_>>(),
        "repaired_terminal_failure_queue_commands": repaired_terminal_failure_queue_commands,
        "unsafe_repairs_applied": 0,
    }))
}

/// Compatibility entry point for the Business OS `commands reconcile` CLI.
/// New internal callers should use [`audit_and_migrate_business_command_storage`].
pub(crate) fn reconcile_business_command_invariants(root: &Path, apply: bool) -> Result<Value> {
    audit_and_migrate_business_command_storage(root, apply)
}
