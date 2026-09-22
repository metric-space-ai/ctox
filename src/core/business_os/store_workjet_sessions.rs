// Origin: CTOX
// License: AGPL-3.0-only

//! Native-owned Workjet session register command handlers.
//!
//! `workjet_sessions` is the server-authoritative binding between a logical
//! Workjet session and its one active working copy. Transfer lifecycle writes
//! deliberately do not live in this package; `workjet_session_transfers` is
//! registered and projected here only as the Package-1 journal contract.

use super::store::{
    insert_business_event, open_store, outbound_load_record, outbound_load_records_by_string_field,
    upsert_business_record, upsert_rxdb_collection_record, BusinessCommand,
};
use anyhow::Context;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) const SESSIONS_COLLECTION: &str = "workjet_sessions";
pub(super) const TRANSFERS_COLLECTION: &str = "workjet_session_transfers";
const PROJECTS_COLLECTION: &str = "workjet_projects";
const WORKING_COPIES_COLLECTION: &str = "workjet_working_copies";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WorkjetSessionTransferTransition {
    pub(super) from: &'static str,
    pub(super) verb: &'static str,
    pub(super) to: &'static str,
}

/// The complete RFC §12 transfer state machine. Keeping verbs on every edge
/// makes illegal state changes rejectable without spreading lifecycle rules
/// across individual handlers.
pub(super) const WORKJET_SESSION_TRANSFER_STATES: [&str; 12] = [
    "pause_requested",
    "packing",
    "packed",
    "shipping",
    "applying",
    "applied",
    "switching",
    "resuming",
    "completed",
    "aborting",
    "rolled_back",
    "failed",
];

pub(super) const WORKJET_SESSION_TRANSFER_TRANSITIONS: [WorkjetSessionTransferTransition; 27] = [
    WorkjetSessionTransferTransition {
        from: "pause_requested",
        verb: "pause_ack",
        to: "packing",
    },
    WorkjetSessionTransferTransition {
        from: "pause_requested",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "pause_requested",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "packing",
        verb: "pack_complete",
        to: "packed",
    },
    WorkjetSessionTransferTransition {
        from: "packing",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "packing",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "packed",
        verb: "ship",
        to: "shipping",
    },
    WorkjetSessionTransferTransition {
        from: "packed",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "packed",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "shipping",
        verb: "apply_start",
        to: "applying",
    },
    WorkjetSessionTransferTransition {
        from: "shipping",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "shipping",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "applying",
        verb: "apply_complete",
        to: "applied",
    },
    WorkjetSessionTransferTransition {
        from: "applying",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "applying",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "applied",
        verb: "confirm_working_copy",
        to: "switching",
    },
    WorkjetSessionTransferTransition {
        from: "applied",
        verb: "abort",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "applied",
        verb: "timeout",
        to: "aborting",
    },
    WorkjetSessionTransferTransition {
        from: "switching",
        verb: "commit_complete",
        to: "resuming",
    },
    WorkjetSessionTransferTransition {
        from: "switching",
        verb: "abort",
        to: "failed",
    },
    WorkjetSessionTransferTransition {
        from: "switching",
        verb: "timeout",
        to: "failed",
    },
    WorkjetSessionTransferTransition {
        from: "switching",
        verb: "commit_failed",
        to: "failed",
    },
    WorkjetSessionTransferTransition {
        from: "resuming",
        verb: "resume_ack",
        to: "completed",
    },
    WorkjetSessionTransferTransition {
        from: "resuming",
        verb: "abort",
        to: "failed",
    },
    WorkjetSessionTransferTransition {
        from: "resuming",
        verb: "timeout",
        to: "failed",
    },
    WorkjetSessionTransferTransition {
        from: "aborting",
        verb: "compensation_complete",
        to: "rolled_back",
    },
    WorkjetSessionTransferTransition {
        from: "aborting",
        verb: "compensation_failed",
        to: "failed",
    },
];

pub(super) fn workjet_session_transfer_next_state(state: &str, verb: &str) -> Option<&'static str> {
    WORKJET_SESSION_TRANSFER_TRANSITIONS
        .iter()
        .find(|transition| transition.from == state && transition.verb == verb)
        .map(|transition| transition.to)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryOutcome {
    pub transfer_id: String,
    pub old_state: String,
    pub new_state: String,
    pub error_code: String,
}

pub fn run_workjet_session_transfer_recovery(root: &Path, now_ms: i64) -> Vec<RecoveryOutcome> {
    run_workjet_session_transfer_recovery_inner(root, now_ms).unwrap_or_default()
}

fn run_workjet_session_transfer_recovery_inner(
    root: &Path,
    now_ms: i64,
) -> anyhow::Result<Vec<RecoveryOutcome>> {
    let mut conn = open_store(root)?;
    let transfers = {
        let mut statement = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection=?1 AND deleted=0",
        )?;
        let rows = statement.query_map([TRANSFERS_COLLECTION], |row| row.get::<_, String>(0))?;
        let mut transfers = Vec::new();
        for row in rows {
            transfers.push(serde_json::from_str::<Value>(&row?)?);
        }
        transfers
    };
    let mut outcomes = Vec::new();
    for mut transfer in transfers {
        let Some(transfer_id) = transfer
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(old_state) = transfer
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if is_terminal_transfer_state(Some(&old_state)) {
            continue;
        }
        let Some(deadline_at_ms) = transfer.get("deadline_at_ms").and_then(Value::as_i64) else {
            continue;
        };
        if deadline_at_ms >= now_ms {
            continue;
        }
        let Some(session_id) = transfer
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        else {
            continue;
        };
        let precommit = matches!(
            old_state.as_str(),
            "pause_requested" | "packing" | "packed" | "shipping" | "applying" | "applied"
        );
        let (new_state, error_code, audit_event) = if precommit {
            let error_code = if old_state == "pause_requested" {
                "pause_timeout"
            } else {
                "transfer_expired"
            };
            transfer["recovery_aborting_at_ms"] = Value::from(now_ms);
            transfer["state"] = Value::String("rolled_back".to_owned());
            transfer["error_code"] = Value::String(error_code.to_owned());
            transfer["target_partial_cleanup_required"] = Value::Bool(true);
            transfer["rolled_back_at_ms"] = Value::from(now_ms);
            session["run_status"] = Value::String("running".to_owned());
            session
                .as_object_mut()
                .context("session must be an object")?
                .remove("active_transfer_id");
            (
                "rolled_back",
                error_code,
                "workjet.session.transfer.aborted",
            )
        } else if matches!(old_state.as_str(), "switching" | "resuming") {
            transfer["state"] = Value::String("failed".to_owned());
            transfer["error_code"] = Value::String("resume_timeout".to_owned());
            session["run_status"] = Value::String("transfer_failed".to_owned());
            (
                "failed",
                "resume_timeout",
                "workjet.session.transfer.manual_intervention",
            )
        } else {
            continue;
        };
        transfer["updated_at_ms"] = Value::from(now_ms);
        session["updated_at_ms"] = Value::from(now_ms);
        let tx = conn.transaction()?;
        upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now_ms, transfer)?;
        upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now_ms, session)?;
        insert_business_event(
            &tx,
            TRANSFERS_COLLECTION,
            &transfer_id,
            audit_event,
            serde_json::json!({
                "event_type": audit_event,
                "transfer_id": transfer_id,
                "session_id": session_id,
                "old_state": old_state,
                "state": new_state,
                "reason": "deadline_expired",
                "error_code": error_code,
                "observed_at_ms": now_ms,
            }),
            now_ms,
        )?;
        tx.commit()?;
        let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
            .context("failed to reload recovered Workjet session")?;
        let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
            .context("failed to reload recovered Workjet transfer")?;
        project_transfer_records(root, &session, &transfer)?;
        outcomes.push(RecoveryOutcome {
            transfer_id,
            old_state,
            new_state: new_state.to_owned(),
            error_code: error_code.to_owned(),
        });
    }
    Ok(outcomes)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionCreatePayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    project_id: String,
    working_copy_id: String,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    coding_session_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionListPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionDeletePayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferStartPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    session_id: String,
    target_computer_id: String,
    target_path: String,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferPauseAckPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    computer_id: String,
    fence_epoch: i64,
    #[serde(default)]
    last_terminal_turn_id: Option<String>,
    git_repository: bool,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferPackCompleteGitPayload {
    head: String,
    branch: String,
    base_commit: String,
    bundle_file_id: String,
    patch_file_id: String,
    patch_sha256: String,
    untracked_file_id: String,
    untracked_sha256: String,
    dirty: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferPackCompletePayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    computer_id: String,
    fence_epoch: i64,
    mode: String,
    manifest_file_id: String,
    artifact_file_ids: Vec<String>,
    artifact_generation_id: String,
    manifest_sha256: String,
    #[serde(default)]
    git: Option<TransferPackCompleteGitPayload>,
    #[serde(default)]
    tree_sha256: Option<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferApplyCompletePayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    computer_id: String,
    fence_epoch: i64,
    #[serde(default)]
    observed_head: Option<String>,
    observed_manifest_sha256: String,
    #[serde(default)]
    observed_tree_sha256: Option<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferConfirmWorkingCopyPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    computer_id: String,
    fence_epoch: i64,
    working_copy_id: String,
    path: String,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferResumeAckPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    computer_id: String,
    fence_epoch: i64,
    worker_instance_id: String,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferAbortPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    transfer_id: String,
    reason: String,
    idempotency_key: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferStatusPayload {
    #[serde(default, rename = "inbound_channel")]
    _inbound_channel: Option<String>,
    #[serde(default)]
    transfer_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

pub(super) fn handle_workjet_session_store_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    authorized_owner_email: Option<&str>,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    migrate_signed_owner_alias(root, authorized_owner_user_id, authorized_owner_email)?;
    match command.command_type.as_str() {
        "ctox.workjet.session.create" => {
            handle_workjet_session_create_command(root, command, authorized_owner_user_id)
        }
        "ctox.workjet.session.list" => {
            handle_workjet_session_list_command(root, command, authorized_owner_user_id)
        }
        "ctox.workjet.session.delete" => handle_workjet_session_delete_command(
            root,
            command,
            authorized_owner_user_id,
            can_manage_all_records,
        ),
        "ctox.workjet.session.transfer.start" => handle_workjet_session_transfer_start_command(
            root,
            command,
            authorized_owner_user_id,
            can_manage_all_records,
        ),
        "ctox.workjet.session.transfer.pause_ack" => {
            handle_workjet_session_transfer_pause_ack_command(
                root,
                command,
                authorized_owner_user_id,
                can_manage_all_records,
            )
        }
        "ctox.workjet.session.transfer.pack_complete" => {
            handle_workjet_session_transfer_pack_complete_command(
                root,
                command,
                authorized_owner_user_id,
                can_manage_all_records,
            )
        }
        "ctox.workjet.session.transfer.apply_complete" => {
            handle_workjet_session_transfer_apply_complete_command(
                root,
                command,
                authorized_owner_user_id,
                can_manage_all_records,
            )
        }
        "ctox.workjet.session.transfer.confirm_working_copy" => {
            handle_workjet_session_transfer_confirm_working_copy_command(
                root,
                command,
                authorized_owner_user_id,
                can_manage_all_records,
            )
        }
        "ctox.workjet.session.transfer.resume_ack" => {
            handle_workjet_session_transfer_resume_ack_command(
                root,
                command,
                authorized_owner_user_id,
                can_manage_all_records,
            )
        }
        "ctox.workjet.session.transfer.abort" => handle_workjet_session_transfer_abort_command(
            root,
            command,
            authorized_owner_user_id,
            can_manage_all_records,
        ),
        "ctox.workjet.session.transfer.status" => handle_workjet_session_transfer_status_command(
            root,
            command,
            authorized_owner_user_id,
            can_manage_all_records,
        ),
        other => anyhow::bail!("unsupported Workjet session command type: {other}"),
    }
}

pub(super) fn migrate_signed_owner_alias(
    root: &Path,
    authorized_owner_user_id: &str,
    authorized_owner_email: Option<&str>,
) -> anyhow::Result<()> {
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let Some(owner_email) = authorized_owner_email else {
        return Ok(());
    };
    let owner_email = bounded_required(owner_email, "owner_email", 320)?.to_ascii_lowercase();
    anyhow::ensure!(
        owner_email.contains('@'),
        "owner_email must be an email address"
    );
    if owner_email == owner_user_id.to_ascii_lowercase() {
        return Ok(());
    }

    let conn = open_store(root)?;
    let now = super::store::now_ms() as i64;
    for collection in [
        PROJECTS_COLLECTION,
        WORKING_COPIES_COLLECTION,
        SESSIONS_COLLECTION,
        TRANSFERS_COLLECTION,
    ] {
        let records = outbound_load_records_by_string_field(
            &conn,
            collection,
            "owner_user_id",
            &owner_email,
        )?;
        for mut record in records {
            if record.get("is_deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let record_id = record
                .get("id")
                .and_then(Value::as_str)
                .with_context(|| format!("{collection} owner migration record has no id"))?
                .to_owned();
            record["owner_user_id"] = Value::String(owner_user_id.clone());
            record["updated_at_ms"] = Value::from(now);
            persist_and_project_idempotently(root, &conn, collection, &record_id, now, record)?;
        }
    }
    Ok(())
}

pub(super) fn workjet_session_record_policy_scope(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    authorized_owner_email: Option<&str>,
) -> anyhow::Result<super::policy::BusinessOsScope> {
    migrate_signed_owner_alias(root, authorized_owner_user_id, authorized_owner_email)?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let conn = open_store(root)?;
    let (scope_collection, scope_id, owned_by_actor, assigned_to_actor) = match command
        .command_type
        .as_str()
    {
        "ctox.workjet.session.create" => {
            let payload = parse_create_payload(command)?;
            let project_id = bounded_required(&payload.project_id, "project_id", 128)?;
            let working_copy_id =
                bounded_required(&payload.working_copy_id, "working_copy_id", 160)?;
            let session_id =
                deterministic_session_id(&project_id, &working_copy_id, &owner_user_id);
            ensure_asserted_session_id(command, payload.session_id.as_deref(), &session_id)?;
            let project = outbound_load_record(&conn, PROJECTS_COLLECTION, &project_id)?;
            let working_copy =
                outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?;
            let owned = project.as_ref().is_some_and(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
            }) && working_copy.as_ref().is_some_and(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
                    && record.get("project_id").and_then(Value::as_str) == Some(project_id.as_str())
            });
            (SESSIONS_COLLECTION, session_id, owned, false)
        }
        "ctox.workjet.session.delete" => {
            let session_id = session_id_for_delete(command)?;
            let owned = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
                .as_ref()
                .is_some_and(|record| {
                    record.get("owner_user_id").and_then(Value::as_str)
                        == Some(owner_user_id.as_str())
                });
            (SESSIONS_COLLECTION, session_id, owned, false)
        }
        "ctox.workjet.session.transfer.start" => {
            let payload: TransferStartPayload = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.workjet.session.transfer.start payload")?;
            let session_id = bounded_required(&payload.session_id, "session_id", 160)?;
            let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?;
            let owned = session.as_ref().is_none_or(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
            });
            (SESSIONS_COLLECTION, session_id, owned, false)
        }
        "ctox.workjet.session.transfer.pause_ack" => {
            let payload: TransferPauseAckPayload = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.workjet.session.transfer.pause_ack payload")?;
            let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
            let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?;
            let owned = transfer.as_ref().is_none_or(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
            });
            (TRANSFERS_COLLECTION, transfer_id, owned, false)
        }
        "ctox.workjet.session.transfer.pack_complete" => {
            let payload: TransferPackCompletePayload =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.workjet.session.transfer.pack_complete payload")?;
            let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
            let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?;
            let owned = transfer.as_ref().is_none_or(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
            });
            (TRANSFERS_COLLECTION, transfer_id, owned, false)
        }
        "ctox.workjet.session.transfer.abort" => {
            let payload: TransferAbortPayload = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.workjet.session.transfer.abort payload")?;
            let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
            let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?;
            let owned = transfer.as_ref().is_none_or(|record| {
                record.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id.as_str())
            });
            (TRANSFERS_COLLECTION, transfer_id, owned, false)
        }
        "ctox.workjet.session.transfer.status" => {
            let payload: TransferStatusPayload = serde_json::from_value(command.payload.clone())
                .context("invalid ctox.workjet.session.transfer.status payload")?;
            anyhow::ensure!(
                payload.transfer_id.is_some() ^ payload.session_id.is_some(),
                "exactly one of transfer_id or session_id is required"
            );
            if let Some(transfer_id) = payload.transfer_id.as_deref() {
                let transfer_id = bounded_required(transfer_id, "transfer_id", 160)?;
                let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?;
                let owned = transfer.as_ref().is_none_or(|record| {
                    record.get("owner_user_id").and_then(Value::as_str)
                        == Some(owner_user_id.as_str())
                });
                (TRANSFERS_COLLECTION, transfer_id, owned, owned)
            } else {
                let session_id = bounded_required(
                    payload.session_id.as_deref().unwrap_or_default(),
                    "session_id",
                    160,
                )?;
                let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?;
                let owned = session.as_ref().is_none_or(|record| {
                    record.get("owner_user_id").and_then(Value::as_str)
                        == Some(owner_user_id.as_str())
                });
                (SESSIONS_COLLECTION, session_id, owned, owned)
            }
        }
        other => anyhow::bail!("{other} does not use a record-scoped policy decision"),
    };
    Ok(super::policy::BusinessOsScope {
        scope_type: super::policy::BusinessOsScopeType::Record,
        scope_id: Some(format!("{scope_collection}/{scope_id}")),
        assigned_to_actor,
        owned_by_actor,
    })
}

fn handle_workjet_session_create_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
) -> anyhow::Result<Value> {
    let payload = parse_create_payload(command)?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let project_id = bounded_required(&payload.project_id, "project_id", 128)?;
    let working_copy_id = bounded_required(&payload.working_copy_id, "working_copy_id", 160)?;
    let thread_id = optional_bounded(payload.thread_id, "thread_id", 160)?;
    let coding_session_id = optional_bounded(payload.coding_session_id, "coding_session_id", 128)?;
    let session_id = deterministic_session_id(&project_id, &working_copy_id, &owner_user_id);
    ensure_asserted_session_id(command, payload.session_id.as_deref(), &session_id)?;

    let conn = open_store(root)?;
    let project = outbound_load_record(&conn, PROJECTS_COLLECTION, &project_id)?
        .context("Workjet project not found")?;
    ensure_owned(Some(&project), &owner_user_id, "project", false)?;
    anyhow::ensure!(
        project.get("status").and_then(Value::as_str) != Some("archived"),
        "cannot create a session for an archived project"
    );
    let working_copy = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?
        .context("Workjet working copy not found")?;
    ensure_owned(Some(&working_copy), &owner_user_id, "working copy", false)?;
    anyhow::ensure!(
        working_copy.get("project_id").and_then(Value::as_str) == Some(project_id.as_str()),
        "working copy belongs to a different project"
    );
    anyhow::ensure!(
        working_copy.get("status").and_then(Value::as_str) == Some("active"),
        "Workjet session requires an active working copy"
    );
    let computer_id = bounded_required(
        working_copy
            .get("computer_id")
            .and_then(Value::as_str)
            .context("working copy has no computer_id")?,
        "computer_id",
        256,
    )?;

    for other in outbound_load_records_by_string_field(
        &conn,
        SESSIONS_COLLECTION,
        "working_copy_id",
        &working_copy_id,
    )? {
        if other.get("id").and_then(Value::as_str) == Some(session_id.as_str())
            || other.get("is_deleted").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        anyhow::ensure!(
            !matches!(
                other.get("run_status").and_then(Value::as_str),
                Some("running" | "resuming")
            ),
            "working copy already has an authoritative running session"
        );
    }

    let existing = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?;
    ensure_owned(existing.as_ref(), &owner_user_id, "session", false)?;
    let now = super::store::now_ms() as i64;
    let created_at_ms = existing
        .as_ref()
        .and_then(|record| record.get("created_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let updated_at_ms = existing
        .as_ref()
        .and_then(|record| record.get("updated_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let mut session = serde_json::json!({
        "id": session_id,
        "project_id": project_id,
        "working_copy_id": working_copy_id,
        "computer_id": computer_id,
        "run_status": "running",
        "fence_epoch": 0,
        "owner_user_id": owner_user_id,
        "created_at_ms": created_at_ms,
        "updated_at_ms": updated_at_ms,
        "is_deleted": false,
    });
    if let Some(thread_id) = thread_id {
        session["thread_id"] = Value::String(thread_id);
    }
    if let Some(coding_session_id) = coding_session_id {
        session["coding_session_id"] = Value::String(coding_session_id);
    }

    if let Some(existing) = existing.as_ref() {
        anyhow::ensure!(
            stable_record_content(existing) == stable_record_content(&session),
            "Workjet session already exists with different content"
        );
    }
    let session = persist_and_project_idempotently(
        root,
        &conn,
        SESSIONS_COLLECTION,
        &session_id,
        now,
        session,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": SESSIONS_COLLECTION,
        "session": session,
    }))
}

fn handle_workjet_session_list_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
) -> anyhow::Result<Value> {
    let payload: SessionListPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.session.list payload")?;
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let limit = payload.limit.unwrap_or(100).clamp(1, 100);
    let conn = open_store(root)?;
    let mut sessions = outbound_load_records_by_string_field(
        &conn,
        SESSIONS_COLLECTION,
        "owner_user_id",
        &owner_user_id,
    )?;
    sessions.retain(|session| session.get("is_deleted").and_then(Value::as_bool) != Some(true));
    sessions.sort_by(|left, right| {
        right
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .cmp(&left.get("updated_at_ms").and_then(Value::as_i64))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .cmp(&right.get("id").and_then(Value::as_str))
            })
    });
    let truncated = sessions.len() > limit;
    sessions.truncate(limit);
    Ok(serde_json::json!({
        "ok": true,
        "collection": SESSIONS_COLLECTION,
        "count": sessions.len(),
        "truncated": truncated,
        "sessions": sessions,
    }))
}

fn handle_workjet_session_delete_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let session_id = session_id_for_delete(command)?;
    let conn = open_store(root)?;
    let existing = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("Workjet session not found")?;
    ensure_owned(
        Some(&existing),
        &owner_user_id,
        "session",
        can_manage_all_records,
    )?;
    anyhow::ensure!(
        existing.get("active_transfer_id").is_none(),
        "cannot delete a session with an active transfer"
    );
    if existing.get("is_deleted").and_then(Value::as_bool) == Some(true) {
        return Ok(serde_json::json!({
            "ok": true,
            "collection": SESSIONS_COLLECTION,
            "session": existing,
        }));
    }
    let now = super::store::now_ms() as i64;
    let mut session = existing;
    session["is_deleted"] = Value::Bool(true);
    session["updated_at_ms"] = Value::from(now);
    let session = persist_and_project_idempotently(
        root,
        &conn,
        SESSIONS_COLLECTION,
        &session_id,
        now,
        session,
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": SESSIONS_COLLECTION,
        "session": session,
    }))
}

fn handle_workjet_session_transfer_start_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferStartPayload = match serde_json::from_value(command.payload.clone()) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                None,
                None,
                &format!("invalid transfer start payload: {error}"),
            ));
        }
    };
    let owner_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let session_id = bounded_required(&payload.session_id, "session_id", 160)?;
    let target_computer_id =
        bounded_required(&payload.target_computer_id, "target_computer_id", 256)?;
    let target_path = bounded_required(&payload.target_path, "target_path", 4096)?;
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    let transfer_id = deterministic_transfer_id(&session_id, &idempotency_key);

    let mut conn = open_store(root)?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            None,
            "Workjet session not found",
        ));
    };
    if session.get("is_deleted").and_then(Value::as_bool) == Some(true) {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            None,
            "Workjet session not found",
        ));
    }
    if !can_manage_all_records
        && session.get("owner_user_id").and_then(Value::as_str) != Some(owner_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            None,
            "Workjet session belongs to a different owner",
        ));
    }
    let session_owner = bounded_required(
        session
            .get("owner_user_id")
            .and_then(Value::as_str)
            .context("Workjet session has no owner_user_id")?,
        "owner_user_id",
        256,
    )?;

    if let Some(existing) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)? {
        let same_request = existing.get("session_id").and_then(Value::as_str)
            == Some(session_id.as_str())
            && existing.get("target_computer_id").and_then(Value::as_str)
                == Some(target_computer_id.as_str())
            && existing.get("target_path").and_then(Value::as_str) == Some(target_path.as_str());
        if !same_request {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                existing.get("state").and_then(Value::as_str),
                "transfer idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &existing);
    }

    if let Some(active_transfer_id) = session
        .get("active_transfer_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        let active = outbound_load_record(&conn, TRANSFERS_COLLECTION, active_transfer_id)?;
        let state = active
            .as_ref()
            .and_then(|record| record.get("state"))
            .and_then(Value::as_str);
        return Ok(transfer_error(
            "session_already_transferring",
            false,
            Some(active_transfer_id),
            state,
            "Workjet session already has an active transfer",
        ));
    }
    if session.get("run_status").and_then(Value::as_str) != Some("running") {
        return Ok(transfer_error(
            "session_not_running",
            false,
            Some(&transfer_id),
            None,
            "Workjet session is not running",
        ));
    }

    let source_working_copy_id = bounded_required(
        session
            .get("working_copy_id")
            .and_then(Value::as_str)
            .context("Workjet session has no working_copy_id")?,
        "source_working_copy_id",
        160,
    )?;
    let source_computer_id = bounded_required(
        session
            .get("computer_id")
            .and_then(Value::as_str)
            .context("Workjet session has no computer_id")?,
        "source_computer_id",
        256,
    )?;
    let Some(source_copy) =
        outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &source_working_copy_id)?
    else {
        return Ok(transfer_error(
            "source_working_copy_missing",
            false,
            Some(&transfer_id),
            None,
            "source working copy is missing",
        ));
    };
    if source_copy.get("status").and_then(Value::as_str) != Some("active")
        || source_copy.get("computer_id").and_then(Value::as_str)
            != Some(source_computer_id.as_str())
    {
        return Ok(transfer_error(
            "source_working_copy_missing",
            false,
            Some(&transfer_id),
            None,
            "source working copy binding is not active",
        ));
    }
    if source_computer_id == target_computer_id {
        return Ok(transfer_error(
            "target_computer_is_source",
            false,
            Some(&transfer_id),
            None,
            "target computer is the source computer",
        ));
    }

    let now = super::store::now_ms() as i64;
    let target = match super::store_workjet_computers::require_assigned_workjet_computer(
        &conn,
        &target_computer_id,
        &session_owner,
    ) {
        Ok(target) => target,
        Err(_) => {
            return Ok(transfer_error(
                "target_computer_unassigned",
                false,
                Some(&transfer_id),
                None,
                "target computer is not assigned to the session owner",
            ));
        }
    };
    let last_seen_at_ms = target
        .get("last_seen_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if target.get("replication_up").and_then(Value::as_bool) != Some(true)
        || now.saturating_sub(last_seen_at_ms) > 30_000
    {
        return Ok(transfer_error(
            "target_computer_offline",
            true,
            Some(&transfer_id),
            None,
            "target computer is offline",
        ));
    }

    for other in outbound_load_records_by_string_field(
        &conn,
        TRANSFERS_COLLECTION,
        "session_id",
        &session_id,
    )? {
        if !is_terminal_transfer_state(other.get("state").and_then(Value::as_str)) {
            return Ok(transfer_error(
                "session_already_transferring",
                false,
                other.get("id").and_then(Value::as_str),
                other.get("state").and_then(Value::as_str),
                "Workjet session already has an active transfer",
            ));
        }
    }

    let old_epoch = session
        .get("fence_epoch")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let fence_epoch = old_epoch
        .checked_add(1)
        .context("Workjet session fence_epoch overflow")?;
    let project_id = bounded_required(
        session
            .get("project_id")
            .and_then(Value::as_str)
            .context("Workjet session has no project_id")?,
        "project_id",
        128,
    )?;
    let deadline_at_ms = now.saturating_add(60_000);
    let transfer = serde_json::json!({
        "id": transfer_id,
        "session_id": session_id,
        "project_id": project_id,
        "source_working_copy_id": source_working_copy_id,
        "source_computer_id": source_computer_id,
        "target_computer_id": target_computer_id,
        "target_path": target_path,
        "state": "pause_requested",
        "fence_epoch": fence_epoch,
        "artifact_file_ids": [],
        "deadline_at_ms": deadline_at_ms,
        "created_at_ms": now,
        "updated_at_ms": now,
        "owner_user_id": session_owner,
        "is_deleted": false,
    });
    session["run_status"] = Value::String("pausing".to_owned());
    session["fence_epoch"] = Value::from(fence_epoch);
    session["active_transfer_id"] = Value::String(transfer_id.clone());
    session["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.started",
        serde_json::json!({
            "event_type": "workjet.session.transfer.started",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "source_computer_id": source_computer_id,
            "target_computer_id": target_computer_id,
            "fence_epoch": fence_epoch,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload started Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload started Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_pause_ack_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferPauseAckPayload = match serde_json::from_value(command.payload.clone()) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                command.payload.get("transfer_id").and_then(Value::as_str),
                None,
                &format!("invalid transfer pause_ack payload: {error}"),
            ));
        }
    };
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    // RFC §6.3 step 11: the ack names the last terminal turn when one ran;
    // a session whose worker never executed a turn has none to name.
    let last_terminal_turn_id = match payload.last_terminal_turn_id.as_deref() {
        Some(value) if !value.trim().is_empty() => {
            Some(bounded_required(value, "last_terminal_turn_id", 160)?)
        }
        _ => None,
    };
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    if payload.fence_epoch < 0 {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            None,
            "pause_ack fence_epoch must be nonnegative",
        ));
    }
    let normalized_payload = serde_json::json!({
        "transfer_id": transfer_id,
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "last_terminal_turn_id": last_terminal_turn_id,
        "git_repository": payload.git_repository,
        "idempotency_key": idempotency_key,
    });
    let payload_sha256 = normalized_payload_sha256(&normalized_payload)?;

    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    let state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet session not found",
        ));
    };

    if let Some(previous_payload_sha256) = previous_transfer_step_payload_sha256(
        &conn,
        &transfer_id,
        "workjet.session.transfer.fenced",
        &idempotency_key,
    )? {
        if previous_payload_sha256 != payload_sha256 {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&state),
                "pause_ack idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &transfer);
    }
    if state != "pause_requested" {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            Some(&state),
            "pause_ack is only valid in pause_requested",
        ));
    }
    let source_computer_id = transfer
        .get("source_computer_id")
        .and_then(Value::as_str)
        .context("Workjet transfer has no source_computer_id")?;
    if computer_id != source_computer_id {
        return Ok(transfer_error(
            "computer_actor_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "pause_ack computer is not the transfer source",
        ));
    }
    if transfer.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch) {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            Some(&state),
            "pause_ack fence_epoch does not match the transfer",
        ));
    }
    let next_state = workjet_session_transfer_next_state(&state, "pause_ack")
        .context("pause_ack transition missing from transfer state table")?;
    let now = super::store::now_ms() as i64;
    let packing_timeout_ms = if payload.git_repository {
        5 * 60_000
    } else {
        30 * 60_000
    };
    transfer["state"] = Value::String(next_state.to_owned());
    transfer["source_git_repository"] = Value::Bool(payload.git_repository);
    transfer["last_terminal_turn_id"] = last_terminal_turn_id
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    transfer["pause_acked_at_ms"] = Value::from(now);
    transfer["deadline_at_ms"] = Value::from(now.saturating_add(packing_timeout_ms));
    transfer["updated_at_ms"] = Value::from(now);
    session["run_status"] = Value::String("paused".to_owned());
    session["last_terminal_turn_id"] = last_terminal_turn_id
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null);
    session["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.fenced",
        serde_json::json!({
            "event_type": "workjet.session.transfer.fenced",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "fence_epoch": payload.fence_epoch,
            "last_turn": last_terminal_turn_id,
            "idempotency_key": idempotency_key,
            "payload_sha256": payload_sha256,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload pause-acked Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload pause-acked Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_pack_complete_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferPackCompletePayload = match serde_json::from_value(command.payload.clone())
    {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                command.payload.get("transfer_id").and_then(Value::as_str),
                None,
                &format!("invalid transfer pack_complete payload: {error}"),
            ));
        }
    };
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    let mode = bounded_required(&payload.mode, "mode", 16)?;
    let manifest_file_id = bounded_required(&payload.manifest_file_id, "manifest_file_id", 160)?;
    let artifact_generation_id = bounded_required(
        &payload.artifact_generation_id,
        "artifact_generation_id",
        160,
    )?;
    let manifest_sha256 = exact_lower_hex(&payload.manifest_sha256, "manifest_sha256", 64)?;
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    if payload.artifact_file_ids.len() > 64 {
        return Ok(transfer_error(
            "idempotency_conflict",
            false,
            Some(&transfer_id),
            None,
            "artifact_file_ids exceeds 64 entries",
        ));
    }
    let artifact_file_ids = payload
        .artifact_file_ids
        .iter()
        .map(|file_id| bounded_required(file_id, "artifact_file_id", 160))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if payload.fence_epoch < 0 {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            None,
            "pack_complete fence_epoch must be nonnegative",
        ));
    }

    let (git, tree_sha256) = match mode.as_str() {
        "git" => {
            if payload.tree_sha256.is_some() {
                return Ok(transfer_error(
                    "idempotency_conflict",
                    false,
                    Some(&transfer_id),
                    None,
                    "git pack_complete must not include tree_sha256",
                ));
            }
            let Some(git) = payload.git else {
                return Ok(transfer_error(
                    "git_pack_failed",
                    true,
                    Some(&transfer_id),
                    None,
                    "git pack_complete requires the git proof object",
                ));
            };
            let head = exact_lower_hex(&git.head, "git.head", 40)?;
            let branch = bounded_required(&git.branch, "git.branch", 256)?;
            let base_commit = exact_lower_hex(&git.base_commit, "git.base_commit", 40)?;
            let bundle_file_id = bounded_required(&git.bundle_file_id, "git.bundle_file_id", 160)?;
            let patch_file_id = bounded_required(&git.patch_file_id, "git.patch_file_id", 160)?;
            let patch_sha256 = exact_lower_hex(&git.patch_sha256, "git.patch_sha256", 64)?;
            let untracked_file_id =
                bounded_required(&git.untracked_file_id, "git.untracked_file_id", 160)?;
            let untracked_sha256 =
                exact_lower_hex(&git.untracked_sha256, "git.untracked_sha256", 64)?;
            let git = serde_json::json!({
                "head": head,
                "branch": branch,
                "base_commit": base_commit,
                "bundle_file_id": bundle_file_id,
                "patch_file_id": patch_file_id,
                "patch_sha256": patch_sha256,
                "untracked_file_id": untracked_file_id,
                "untracked_sha256": untracked_sha256,
                "dirty": git.dirty,
            });
            (Some(git), None)
        }
        "copy" => {
            if payload.git.is_some() {
                return Ok(transfer_error(
                    "idempotency_conflict",
                    false,
                    Some(&transfer_id),
                    None,
                    "copy pack_complete must not include a git proof object",
                ));
            }
            if artifact_file_ids.len() != 1 {
                return Ok(transfer_error(
                    "artifact_missing",
                    true,
                    Some(&transfer_id),
                    None,
                    "copy pack_complete requires exactly one archive artifact",
                ));
            }
            let Some(tree_sha256) = payload.tree_sha256 else {
                return Ok(transfer_error(
                    "artifact_missing",
                    true,
                    Some(&transfer_id),
                    None,
                    "copy pack_complete requires tree_sha256",
                ));
            };
            (
                None,
                Some(exact_lower_hex(&tree_sha256, "tree_sha256", 64)?),
            )
        }
        _ => {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                None,
                "pack_complete mode must be git or copy",
            ));
        }
    };

    let normalized_payload = serde_json::json!({
        "transfer_id": transfer_id,
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "mode": mode,
        "manifest_file_id": manifest_file_id,
        "artifact_file_ids": artifact_file_ids,
        "artifact_generation_id": artifact_generation_id,
        "manifest_sha256": manifest_sha256,
        "git": git,
        "tree_sha256": tree_sha256,
        "idempotency_key": idempotency_key,
    });
    let payload_sha256 = normalized_payload_sha256(&normalized_payload)?;

    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    let state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet session not found",
        ));
    };

    if let Some(previous_payload_sha256) = previous_transfer_step_payload_sha256(
        &conn,
        &transfer_id,
        "workjet.session.transfer.packed",
        &idempotency_key,
    )? {
        if previous_payload_sha256 != payload_sha256 {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&state),
                "pack_complete idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &transfer);
    }
    if let Some(existing_generation_id) = transfer
        .get("artifact_generation_id")
        .and_then(Value::as_str)
    {
        if existing_generation_id != artifact_generation_id {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&state),
                "pack_complete cannot reference a second artifact generation",
            ));
        }
    }
    if state != "packing" {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            Some(&state),
            "pack_complete is only valid in packing",
        ));
    }
    let source_computer_id = transfer
        .get("source_computer_id")
        .and_then(Value::as_str)
        .context("Workjet transfer has no source_computer_id")?;
    if computer_id != source_computer_id {
        return Ok(transfer_error(
            "computer_actor_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "pack_complete computer is not the transfer source",
        ));
    }
    if transfer.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch) {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            Some(&state),
            "pack_complete fence_epoch does not match the transfer",
        ));
    }
    if mode == "copy"
        && transfer
            .get("source_git_repository")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Ok(transfer_error(
            "copy_not_allowed_for_git_repo",
            false,
            Some(&transfer_id),
            Some(&state),
            "copy mode is not allowed for a Git source repository",
        ));
    }

    let packed_state = workjet_session_transfer_next_state(&state, "pack_complete")
        .context("pack_complete transition missing from transfer state table")?;
    let shipping_state = workjet_session_transfer_next_state(packed_state, "ship")
        .context("ship transition missing from transfer state table")?;
    let now = super::store::now_ms() as i64;
    transfer["state"] = Value::String(shipping_state.to_owned());
    transfer["mode"] = Value::String(mode.clone());
    transfer["manifest_file_id"] = Value::String(manifest_file_id.clone());
    transfer["manifest_sha256"] = Value::String(manifest_sha256.clone());
    transfer["artifact_file_ids"] = Value::Array(
        artifact_file_ids
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    transfer["artifact_generation_id"] = Value::String(artifact_generation_id.clone());
    if let Some(git) = git.clone() {
        transfer["git"] = git;
    }
    if let Some(tree_sha256) = tree_sha256.clone() {
        transfer["tree_sha256"] = Value::String(tree_sha256);
    }
    transfer["packed_at_ms"] = Value::from(now);
    transfer["deadline_at_ms"] = Value::from(now.saturating_add(10 * 60_000));
    transfer["updated_at_ms"] = Value::from(now);
    session["run_status"] = Value::String("transferring".to_owned());
    session["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.packed",
        serde_json::json!({
            "event_type": "workjet.session.transfer.packed",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "mode": mode,
            "manifest_sha256": manifest_sha256,
            "artifact_generation_id": artifact_generation_id,
            "idempotency_key": idempotency_key,
            "payload_sha256": payload_sha256,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload packed Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload packed Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_apply_complete_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferApplyCompletePayload =
        match serde_json::from_value(command.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(transfer_error(
                    "idempotency_conflict",
                    false,
                    command.payload.get("transfer_id").and_then(Value::as_str),
                    None,
                    &format!("invalid transfer apply_complete payload: {error}"),
                ));
            }
        };
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    let observed_head = payload
        .observed_head
        .as_deref()
        .map(|value| exact_lower_hex(value, "observed_head", 40))
        .transpose()?;
    let observed_manifest_sha256 = exact_lower_hex(
        &payload.observed_manifest_sha256,
        "observed_manifest_sha256",
        64,
    )?;
    let observed_tree_sha256 = payload
        .observed_tree_sha256
        .as_deref()
        .map(|value| exact_lower_hex(value, "observed_tree_sha256", 64))
        .transpose()?;
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    if payload.fence_epoch < 0 {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            None,
            "apply_complete fence_epoch must be nonnegative",
        ));
    }
    let normalized_payload = serde_json::json!({
        "transfer_id": transfer_id,
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "observed_head": observed_head,
        "observed_manifest_sha256": observed_manifest_sha256,
        "observed_tree_sha256": observed_tree_sha256,
        "idempotency_key": idempotency_key,
    });
    let payload_sha256 = normalized_payload_sha256(&normalized_payload)?;
    let mismatch_sha256 = normalized_payload_sha256(&serde_json::json!({
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "observed_head": observed_head,
        "observed_manifest_sha256": observed_manifest_sha256,
        "observed_tree_sha256": observed_tree_sha256,
    }))?;

    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    let state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet session not found",
        ));
    };

    for event_type in [
        "workjet.session.transfer.applied",
        "workjet.session.transfer.apply_mismatch",
    ] {
        if let Some(previous_payload_sha256) = previous_transfer_step_payload_sha256(
            &conn,
            &transfer_id,
            event_type,
            &idempotency_key,
        )? {
            if previous_payload_sha256 != payload_sha256 {
                return Ok(transfer_error(
                    "idempotency_conflict",
                    false,
                    Some(&transfer_id),
                    Some(&state),
                    "apply_complete idempotency key was reused with different payload",
                ));
            }
            return if event_type == "workjet.session.transfer.applied" {
                transfer_success_response(&session, &transfer)
            } else {
                Ok(transfer_error(
                    "apply_hash_mismatch",
                    true,
                    Some(&transfer_id),
                    Some(&state),
                    "target apply proof does not match the packed manifest",
                ))
            };
        }
    }
    if !matches!(state.as_str(), "shipping" | "applying") {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            Some(&state),
            "apply_complete is only valid in shipping or applying",
        ));
    }
    let target_computer_id = transfer
        .get("target_computer_id")
        .and_then(Value::as_str)
        .context("Workjet transfer has no target_computer_id")?;
    if computer_id != target_computer_id {
        return Ok(transfer_error(
            "computer_actor_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "apply_complete computer is not the transfer target",
        ));
    }
    if transfer.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch) {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            Some(&state),
            "apply_complete fence_epoch does not match the transfer",
        ));
    }

    let mode = transfer
        .get("mode")
        .and_then(Value::as_str)
        .context("Workjet transfer has no mode")?
        .to_owned();
    let manifest_matches = transfer.get("manifest_sha256").and_then(Value::as_str)
        == Some(observed_manifest_sha256.as_str());
    let content_matches = match mode.as_str() {
        "git" => {
            transfer
                .get("git")
                .and_then(|git| git.get("head"))
                .and_then(Value::as_str)
                == observed_head.as_deref()
        }
        "copy" => {
            transfer.get("tree_sha256").and_then(Value::as_str) == observed_tree_sha256.as_deref()
        }
        _ => false,
    };
    let now = super::store::now_ms() as i64;
    if !manifest_matches || !content_matches {
        if state == "shipping" {
            let applying_state = workjet_session_transfer_next_state(&state, "apply_start")
                .context("apply_start transition missing from transfer state table")?;
            transfer["state"] = Value::String(applying_state.to_owned());
        }
        let same_mismatch = transfer
            .get("apply_mismatch_sha256")
            .and_then(Value::as_str)
            == Some(mismatch_sha256.as_str());
        let mismatch_count = if same_mismatch {
            transfer
                .get("apply_mismatch_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .saturating_add(1)
        } else {
            1
        };
        transfer["apply_mismatch_sha256"] = Value::String(mismatch_sha256);
        transfer["apply_mismatch_count"] = Value::from(mismatch_count);
        transfer["deadline_at_ms"] = Value::from(now.saturating_add(if mode == "git" {
            15 * 60_000
        } else {
            60 * 60_000
        }));
        if mismatch_count >= 3 {
            transfer["state"] = Value::String("failed".to_owned());
            transfer["error_code"] = Value::String("apply_hash_mismatch".to_owned());
        }
        transfer["updated_at_ms"] = Value::from(now);
        let tx = conn.transaction()?;
        upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
        insert_business_event(
            &tx,
            TRANSFERS_COLLECTION,
            &transfer_id,
            "workjet.session.transfer.apply_mismatch",
            serde_json::json!({
                "event_type": "workjet.session.transfer.apply_mismatch",
                "transfer_id": transfer_id,
                "session_id": session_id,
                "idempotency_key": idempotency_key,
                "payload_sha256": payload_sha256,
                "apply_mismatch_count": mismatch_count,
                "observed_at_ms": now,
            }),
            now,
        )?;
        tx.commit()?;
        let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
            .context("failed to reload mismatched Workjet transfer")?;
        project_transfer_records(root, &session, &transfer)?;
        return Ok(transfer_error(
            "apply_hash_mismatch",
            true,
            Some(&transfer_id),
            transfer.get("state").and_then(Value::as_str),
            "target apply proof does not match the packed manifest",
        ));
    }

    let applying_state = if state == "shipping" {
        workjet_session_transfer_next_state(&state, "apply_start")
            .context("apply_start transition missing from transfer state table")?
    } else {
        state.as_str()
    };
    let applied_state = workjet_session_transfer_next_state(applying_state, "apply_complete")
        .context("apply_complete transition missing from transfer state table")?;
    transfer["state"] = Value::String(applied_state.to_owned());
    transfer["applied_at_ms"] = Value::from(now);
    transfer["observed_manifest_sha256"] = Value::String(observed_manifest_sha256.clone());
    if let Some(observed_head) = observed_head.clone() {
        transfer["observed_head"] = Value::String(observed_head);
    }
    if let Some(observed_tree_sha256) = observed_tree_sha256.clone() {
        transfer["observed_tree_sha256"] = Value::String(observed_tree_sha256);
    }
    transfer["deadline_at_ms"] = Value::from(now.saturating_add(60_000));
    transfer["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.applied",
        serde_json::json!({
            "event_type": "workjet.session.transfer.applied",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "observed_head": observed_head,
            "observed_manifest_sha256": observed_manifest_sha256,
            "observed_tree_sha256": observed_tree_sha256,
            "idempotency_key": idempotency_key,
            "payload_sha256": payload_sha256,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload applied Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_confirm_working_copy_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferConfirmWorkingCopyPayload =
        match serde_json::from_value(command.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(transfer_error(
                    "idempotency_conflict",
                    false,
                    command.payload.get("transfer_id").and_then(Value::as_str),
                    None,
                    &format!("invalid transfer confirm_working_copy payload: {error}"),
                ));
            }
        };
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    let working_copy_id = bounded_required(&payload.working_copy_id, "working_copy_id", 160)?;
    let path = bounded_required(&payload.path, "path", 4096)?;
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    if payload.fence_epoch < 0 {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            None,
            "confirm_working_copy fence_epoch must be nonnegative",
        ));
    }
    let normalized_payload = serde_json::json!({
        "transfer_id": transfer_id,
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "working_copy_id": working_copy_id,
        "path": path,
        "idempotency_key": idempotency_key,
    });
    let payload_sha256 = normalized_payload_sha256(&normalized_payload)?;

    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    let state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet session not found",
        ));
    };
    if let Some(previous_payload_sha256) = previous_transfer_step_payload_sha256(
        &conn,
        &transfer_id,
        "workjet.session.transfer.switched",
        &idempotency_key,
    )? {
        if previous_payload_sha256 != payload_sha256 {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&state),
                "confirm_working_copy idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &transfer);
    }
    if state != "applied" {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            Some(&state),
            "confirm_working_copy is only valid in applied",
        ));
    }
    let target_computer_id = bounded_required(
        transfer
            .get("target_computer_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no target_computer_id")?,
        "target_computer_id",
        256,
    )?;
    if computer_id != target_computer_id {
        return Ok(transfer_error(
            "computer_actor_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "confirm_working_copy computer is not the transfer target",
        ));
    }
    if transfer.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch) {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            Some(&state),
            "confirm_working_copy fence_epoch does not match the transfer",
        ));
    }
    let project_id = bounded_required(
        transfer
            .get("project_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no project_id")?,
        "project_id",
        128,
    )?;
    let target_path = bounded_required(
        transfer
            .get("target_path")
            .and_then(Value::as_str)
            .context("Workjet transfer has no target_path")?,
        "target_path",
        4096,
    )?;
    let expected_working_copy_id =
        deterministic_working_copy_id(&project_id, &target_computer_id, &target_path);
    if path != target_path || working_copy_id != expected_working_copy_id {
        return Ok(transfer_error(
            "working_copy_id_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "working_copy_id or path does not match the deterministic transfer target",
        ));
    }
    let source_working_copy_id = bounded_required(
        transfer
            .get("source_working_copy_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no source_working_copy_id")?,
        "source_working_copy_id",
        160,
    )?;
    let Some(mut source_working_copy) =
        outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &source_working_copy_id)?
    else {
        return Ok(transfer_error(
            "source_working_copy_missing",
            false,
            Some(&transfer_id),
            Some(&state),
            "source working copy is missing",
        ));
    };
    let session_owner = bounded_required(
        session
            .get("owner_user_id")
            .and_then(Value::as_str)
            .context("Workjet session has no owner_user_id")?,
        "owner_user_id",
        256,
    )?;
    let existing_target = outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?;
    if let Some(existing_target) = existing_target.as_ref() {
        let target_matches = existing_target.get("owner_user_id").and_then(Value::as_str)
            == Some(session_owner.as_str())
            && existing_target.get("project_id").and_then(Value::as_str)
                == Some(project_id.as_str())
            && existing_target.get("computer_id").and_then(Value::as_str)
                == Some(target_computer_id.as_str())
            && existing_target.get("path").and_then(Value::as_str) == Some(target_path.as_str());
        if !target_matches {
            return Ok(transfer_error(
                "working_copy_id_mismatch",
                false,
                Some(&transfer_id),
                Some(&state),
                "existing target working copy does not match the transfer target",
            ));
        }
    }

    let now = super::store::now_ms() as i64;
    let old_epoch = session
        .get("fence_epoch")
        .and_then(Value::as_i64)
        .context("Workjet session has no fence_epoch")?;
    let new_epoch = old_epoch
        .checked_add(1)
        .context("Workjet session fence_epoch overflow")?;
    let created_at_ms = existing_target
        .as_ref()
        .and_then(|record| record.get("created_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(now);
    let mut target_working_copy = serde_json::json!({
        "id": working_copy_id,
        "project_id": project_id,
        "computer_id": target_computer_id,
        "path": target_path,
        "status": "active",
        "verified_at_ms": now,
        "owner_user_id": session_owner,
        "created_at_ms": created_at_ms,
        "updated_at_ms": now,
        "is_deleted": false,
    });
    if let Some(label) = existing_target
        .as_ref()
        .and_then(|record| record.get("label"))
        .cloned()
    {
        target_working_copy["label"] = label;
    }
    source_working_copy["status"] = Value::String("detached".to_owned());
    source_working_copy["updated_at_ms"] = Value::from(now);
    session["working_copy_id"] = Value::String(working_copy_id.clone());
    session["computer_id"] = Value::String(target_computer_id.clone());
    session["run_status"] = Value::String("resuming".to_owned());
    session["fence_epoch"] = Value::from(new_epoch);
    session["updated_at_ms"] = Value::from(now);
    let switching_state = workjet_session_transfer_next_state(&state, "confirm_working_copy")
        .context("confirm_working_copy transition missing from transfer state table")?;
    let resuming_state = workjet_session_transfer_next_state(switching_state, "commit_complete")
        .context("commit_complete transition missing from transfer state table")?;
    transfer["state"] = Value::String(resuming_state.to_owned());
    transfer["fence_epoch"] = Value::from(new_epoch);
    transfer["target_working_copy_id"] = Value::String(working_copy_id.clone());
    transfer["switched_at_ms"] = Value::from(now);
    transfer["deadline_at_ms"] = Value::from(now.saturating_add(120_000));
    transfer["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(
        &tx,
        WORKING_COPIES_COLLECTION,
        &working_copy_id,
        now,
        target_working_copy,
    )?;
    upsert_business_record(
        &tx,
        WORKING_COPIES_COLLECTION,
        &source_working_copy_id,
        now,
        source_working_copy,
    )?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.switched",
        serde_json::json!({
            "event_type": "workjet.session.transfer.switched",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "old_working_copy_id": source_working_copy_id,
            "new_working_copy_id": working_copy_id,
            "new_fence_epoch": new_epoch,
            "idempotency_key": idempotency_key,
            "payload_sha256": payload_sha256,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload switched Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload switched Workjet transfer")?;
    let source_working_copy =
        outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &source_working_copy_id)?
            .context("failed to reload detached source working copy")?;
    let target_working_copy =
        outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &working_copy_id)?
            .context("failed to reload active target working copy")?;
    project_transfer_records(root, &session, &transfer)?;
    for working_copy in [&source_working_copy, &target_working_copy] {
        let id = working_copy
            .get("id")
            .and_then(Value::as_str)
            .context("working-copy projection has no id")?;
        upsert_rxdb_collection_record(
            root,
            WORKING_COPIES_COLLECTION,
            id,
            working_copy
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or(now),
            working_copy.clone(),
        )?;
    }
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_resume_ack_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferResumeAckPayload = match serde_json::from_value(command.payload.clone()) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                command.payload.get("transfer_id").and_then(Value::as_str),
                None,
                &format!("invalid transfer resume_ack payload: {error}"),
            ));
        }
    };
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let computer_id = bounded_required(&payload.computer_id, "computer_id", 256)?;
    let worker_instance_id =
        bounded_required(&payload.worker_instance_id, "worker_instance_id", 256)?;
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    if payload.fence_epoch < 0 {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            None,
            "resume_ack fence_epoch must be nonnegative",
        ));
    }
    let normalized_payload = serde_json::json!({
        "transfer_id": transfer_id,
        "computer_id": computer_id,
        "fence_epoch": payload.fence_epoch,
        "worker_instance_id": worker_instance_id,
        "idempotency_key": idempotency_key,
    });
    let payload_sha256 = normalized_payload_sha256(&normalized_payload)?;

    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    let state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            Some(&state),
            "Workjet session not found",
        ));
    };
    if let Some(previous_payload_sha256) = previous_transfer_step_payload_sha256(
        &conn,
        &transfer_id,
        "workjet.session.transfer.completed",
        &idempotency_key,
    )? {
        if previous_payload_sha256 != payload_sha256 {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&state),
                "resume_ack idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &transfer);
    }
    if state != "resuming" {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            Some(&state),
            "resume_ack is only valid in resuming",
        ));
    }
    let target_computer_id = transfer
        .get("target_computer_id")
        .and_then(Value::as_str)
        .context("Workjet transfer has no target_computer_id")?
        .to_owned();
    if computer_id != target_computer_id {
        return Ok(transfer_error(
            "computer_actor_mismatch",
            false,
            Some(&transfer_id),
            Some(&state),
            "resume_ack computer is not the transfer target",
        ));
    }
    if transfer.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch)
        || session.get("fence_epoch").and_then(Value::as_i64) != Some(payload.fence_epoch)
    {
        return Ok(transfer_error(
            "session_fenced",
            false,
            Some(&transfer_id),
            Some(&state),
            "resume_ack fence_epoch does not match the switched session",
        ));
    }

    let completed_state = workjet_session_transfer_next_state(&state, "resume_ack")
        .context("resume_ack transition missing from transfer state table")?;
    let now = super::store::now_ms() as i64;
    session["run_status"] = Value::String("running".to_owned());
    session
        .as_object_mut()
        .context("session must be an object")?
        .remove("active_transfer_id");
    session["updated_at_ms"] = Value::from(now);
    transfer["state"] = Value::String(completed_state.to_owned());
    transfer["completed_at_ms"] = Value::from(now);
    transfer["worker_instance_id"] = Value::String(worker_instance_id.clone());
    transfer["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.completed",
        serde_json::json!({
            "event_type": "workjet.session.transfer.completed",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "target_computer_id": target_computer_id,
            "worker_instance_id": worker_instance_id,
            "idempotency_key": idempotency_key,
            "payload_sha256": payload_sha256,
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload resumed Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload completed Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_abort_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferAbortPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.session.transfer.abort payload")?;
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let transfer_id = bounded_required(&payload.transfer_id, "transfer_id", 160)?;
    let reason = bounded_required(&payload.reason, "reason", 512)?;
    let audited_reason = redacted_transfer_reason(&reason);
    let reason_sha256 = format!("{:x}", Sha256::digest(reason.as_bytes()));
    let idempotency_key = bounded_required(&payload.idempotency_key, "idempotency_key", 160)?;
    let mut conn = open_store(root)?;
    let Some(mut transfer) = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            Some(&transfer_id),
            None,
            "Workjet transfer not found",
        ));
    };
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            Some(&transfer_id),
            transfer.get("state").and_then(Value::as_str),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = bounded_required(
        transfer
            .get("session_id")
            .and_then(Value::as_str)
            .context("Workjet transfer has no session_id")?,
        "session_id",
        160,
    )?;
    let Some(mut session) = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            Some(&transfer_id),
            transfer.get("state").and_then(Value::as_str),
            "Workjet session not found",
        ));
    };
    let old_state = transfer
        .get("state")
        .and_then(Value::as_str)
        .context("Workjet transfer has no state")?
        .to_owned();
    if let Some(previous_reason_sha256) =
        previous_abort_reason_sha256(&conn, &transfer_id, &idempotency_key)?
    {
        if previous_reason_sha256 != reason_sha256 {
            return Ok(transfer_error(
                "idempotency_conflict",
                false,
                Some(&transfer_id),
                Some(&old_state),
                "abort idempotency key was reused with different payload",
            ));
        }
        return transfer_success_response(&session, &transfer);
    }
    if is_terminal_transfer_state(Some(&old_state)) {
        return transfer_success_response(&session, &transfer);
    }

    let now = super::store::now_ms() as i64;
    let post_commit = matches!(old_state.as_str(), "switching" | "resuming");
    let final_state = if post_commit { "failed" } else { "rolled_back" };
    transfer["state"] = Value::String(final_state.to_owned());
    if post_commit {
        transfer["error_code"] = Value::String("manual_intervention".to_owned());
        transfer["error_detail"] = Value::String(audited_reason.clone());
    } else if let Some(object) = transfer.as_object_mut() {
        object.remove("error_code");
        object.remove("error_detail");
    }
    transfer["updated_at_ms"] = Value::from(now);
    if !post_commit {
        transfer["rolled_back_at_ms"] = Value::from(now);
        session["run_status"] = Value::String("running".to_owned());
    } else {
        session["run_status"] = Value::String("transfer_failed".to_owned());
    }
    session
        .as_object_mut()
        .context("session must be an object")?
        .remove("active_transfer_id");
    session["updated_at_ms"] = Value::from(now);

    let tx = conn.transaction()?;
    upsert_business_record(&tx, TRANSFERS_COLLECTION, &transfer_id, now, transfer)?;
    upsert_business_record(&tx, SESSIONS_COLLECTION, &session_id, now, session)?;
    insert_business_event(
        &tx,
        TRANSFERS_COLLECTION,
        &transfer_id,
        "workjet.session.transfer.aborted",
        serde_json::json!({
            "event_type": "workjet.session.transfer.aborted",
            "transfer_id": transfer_id,
            "session_id": session_id,
            "old_state": old_state,
            "state": final_state,
            "reason": audited_reason,
            "reason_sha256": reason_sha256,
            "idempotency_key": idempotency_key,
            "error_code": if post_commit { Value::String("manual_intervention".to_owned()) } else { Value::Null },
            "observed_at_ms": now,
        }),
        now,
    )?;
    tx.commit()?;

    let session = outbound_load_record(&conn, SESSIONS_COLLECTION, &session_id)?
        .context("failed to reload aborted Workjet session")?;
    let transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
        .context("failed to reload aborted Workjet transfer")?;
    project_transfer_records(root, &session, &transfer)?;
    transfer_success_response(&session, &transfer)
}

fn handle_workjet_session_transfer_status_command(
    root: &Path,
    command: &BusinessCommand,
    authorized_owner_user_id: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<Value> {
    let payload: TransferStatusPayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.session.transfer.status payload")?;
    anyhow::ensure!(
        payload.transfer_id.is_some() ^ payload.session_id.is_some(),
        "exactly one of transfer_id or session_id is required"
    );
    let actor_user_id = bounded_required(authorized_owner_user_id, "owner_user_id", 256)?;
    let conn = open_store(root)?;
    let transfer = if let Some(transfer_id) = payload.transfer_id.as_deref() {
        let transfer_id = bounded_required(transfer_id, "transfer_id", 160)?;
        outbound_load_record(&conn, TRANSFERS_COLLECTION, &transfer_id)?
    } else {
        let session_id = bounded_required(
            payload.session_id.as_deref().unwrap_or_default(),
            "session_id",
            160,
        )?;
        let mut transfers = outbound_load_records_by_string_field(
            &conn,
            TRANSFERS_COLLECTION,
            "session_id",
            &session_id,
        )?;
        transfers.sort_by(|left, right| {
            right
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .cmp(&left.get("updated_at_ms").and_then(Value::as_i64))
        });
        transfers.into_iter().next()
    };
    let Some(transfer) = transfer else {
        return Ok(transfer_error(
            "transfer_illegal_state",
            false,
            None,
            None,
            "Workjet transfer not found",
        ));
    };
    if !can_manage_all_records
        && transfer.get("owner_user_id").and_then(Value::as_str) != Some(actor_user_id.as_str())
    {
        return Ok(transfer_error(
            "session_not_owned",
            false,
            transfer.get("id").and_then(Value::as_str),
            transfer.get("state").and_then(Value::as_str),
            "Workjet transfer belongs to a different owner",
        ));
    }
    let session_id = transfer
        .get("session_id")
        .and_then(Value::as_str)
        .context("Workjet transfer has no session_id")?;
    let Some(session) = outbound_load_record(&conn, SESSIONS_COLLECTION, session_id)? else {
        return Ok(transfer_error(
            "session_not_found",
            false,
            transfer.get("id").and_then(Value::as_str),
            transfer.get("state").and_then(Value::as_str),
            "Workjet session not found",
        ));
    };
    transfer_success_response(&session, &transfer)
}

/// Free-text transfer reasons are operator input: they land in the audit
/// stream and the transfer record, so control characters are collapsed and
/// secret-shaped tokens are masked. The idempotency check hashes the raw
/// reason separately, so redaction never weakens conflict detection.
fn redacted_transfer_reason(reason: &str) -> String {
    const REDACTED: &str = "[REDACTED]";
    const SECRET_PREFIXES: [&str; 6] = ["sk-", "ghp_", "gho_", "xoxb-", "xoxp-", "AKIA"];
    let normalized = reason
        .split(|c: char| c.is_control() || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            let prefixed = SECRET_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(&prefix.to_ascii_lowercase()));
            let blob = part.len() >= 32
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'));
            if prefixed || lower == "bearer" || blob {
                REDACTED
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    normalized.chars().take(512).collect()
}

fn normalized_payload_sha256(payload: &Value) -> anyhow::Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(payload)?)
    ))
}

fn previous_transfer_step_payload_sha256(
    conn: &Connection,
    transfer_id: &str,
    event_type: &str,
    idempotency_key: &str,
) -> anyhow::Result<Option<String>> {
    let mut statement = conn.prepare(
        "SELECT payload_json FROM business_events
         WHERE collection=?1 AND record_id=?2 AND command_type=?3
         ORDER BY observed_at_ms ASC",
    )?;
    let rows = statement.query_map([TRANSFERS_COLLECTION, transfer_id, event_type], |row| {
        row.get::<_, String>(0)
    })?;
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        if payload.get("idempotency_key").and_then(Value::as_str) == Some(idempotency_key) {
            return Ok(Some(
                payload
                    .get("payload_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ));
        }
    }
    Ok(None)
}

fn previous_abort_reason_sha256(
    conn: &Connection,
    transfer_id: &str,
    idempotency_key: &str,
) -> anyhow::Result<Option<String>> {
    let mut statement = conn.prepare(
        "SELECT payload_json FROM business_events
         WHERE collection=?1 AND record_id=?2
           AND command_type='workjet.session.transfer.aborted'
         ORDER BY observed_at_ms ASC",
    )?;
    let rows = statement.query_map([TRANSFERS_COLLECTION, transfer_id], |row| {
        row.get::<_, String>(0)
    })?;
    for row in rows {
        let payload: Value = serde_json::from_str(&row?)?;
        if payload.get("idempotency_key").and_then(Value::as_str) == Some(idempotency_key) {
            return Ok(Some(
                payload
                    .get("reason_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ));
        }
    }
    Ok(None)
}

fn deterministic_working_copy_id(project_id: &str, computer_id: &str, path: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [project_id, computer_id, path] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("workjet_wc_{:x}", hasher.finalize())
}

fn deterministic_transfer_id(session_id: &str, idempotency_key: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [session_id, idempotency_key] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("workjet_xfer_{:x}", hasher.finalize())
}

fn is_terminal_transfer_state(state: Option<&str>) -> bool {
    matches!(state, Some("completed" | "rolled_back" | "failed"))
}

fn transfer_error(
    error_code: &str,
    retryable: bool,
    transfer_id: Option<&str>,
    state: Option<&str>,
    message: &str,
) -> Value {
    serde_json::json!({
        "ok": false,
        "error_code": error_code,
        "retryable": retryable,
        "transfer_id": transfer_id,
        "state": state,
        "error": message.chars().take(512).collect::<String>(),
    })
}

fn transfer_success_response(session: &Value, transfer: &Value) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "ok": true,
        "collection": TRANSFERS_COLLECTION,
        "transfer_id": transfer.get("id").cloned().unwrap_or(Value::Null),
        "state": transfer.get("state").cloned().unwrap_or(Value::Null),
        "fence_epoch": transfer.get("fence_epoch").cloned().unwrap_or(Value::Null),
        "session": session,
        "transfer": transfer,
    }))
}

fn project_transfer_records(root: &Path, session: &Value, transfer: &Value) -> anyhow::Result<()> {
    let session_id = session
        .get("id")
        .and_then(Value::as_str)
        .context("session projection has no id")?;
    let transfer_id = transfer
        .get("id")
        .and_then(Value::as_str)
        .context("transfer projection has no id")?;
    upsert_rxdb_collection_record(
        root,
        SESSIONS_COLLECTION,
        session_id,
        session
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        session.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        TRANSFERS_COLLECTION,
        transfer_id,
        transfer
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        transfer.clone(),
    )
}

fn parse_create_payload(command: &BusinessCommand) -> anyhow::Result<SessionCreatePayload> {
    serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.session.create payload")
}

fn session_id_for_delete(command: &BusinessCommand) -> anyhow::Result<String> {
    let payload: SessionDeletePayload = serde_json::from_value(command.payload.clone())
        .context("invalid ctox.workjet.session.delete payload")?;
    let payload_id = payload
        .session_id
        .as_deref()
        .map(|value| bounded_required(value, "session_id", 160))
        .transpose()?;
    let record_id = command
        .record_id
        .as_deref()
        .map(|value| bounded_required(value, "record_id", 160))
        .transpose()?;
    if let (Some(payload_id), Some(record_id)) = (&payload_id, &record_id) {
        anyhow::ensure!(
            payload_id == record_id,
            "payload session_id and record_id must match"
        );
    }
    payload_id
        .or(record_id)
        .context("session_id or record_id is required")
}

fn ensure_asserted_session_id(
    command: &BusinessCommand,
    payload_session_id: Option<&str>,
    expected: &str,
) -> anyhow::Result<()> {
    for (field, asserted) in [
        ("session_id", payload_session_id),
        ("record_id", command.record_id.as_deref()),
    ] {
        if let Some(asserted) = asserted {
            let asserted = bounded_required(asserted, field, 160)?;
            anyhow::ensure!(
                asserted == expected,
                "{field} does not match deterministic Workjet session id"
            );
        }
    }
    Ok(())
}

fn deterministic_session_id(project_id: &str, working_copy_id: &str, owner: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [project_id, working_copy_id, owner] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("workjet_session_{:x}", hasher.finalize())
}

fn persist_idempotently(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    now: i64,
    desired: Value,
) -> anyhow::Result<Value> {
    if let Some(existing) = outbound_load_record(conn, collection, record_id)? {
        if stable_record_content(&existing) == stable_record_content(&desired) {
            return Ok(existing);
        }
    }
    upsert_business_record(conn, collection, record_id, now, desired)?;
    outbound_load_record(conn, collection, record_id)?
        .with_context(|| format!("failed to reload {collection} record {record_id}"))
}

fn persist_and_project_idempotently(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
    now: i64,
    desired: Value,
) -> anyhow::Result<Value> {
    let record = persist_idempotently(conn, collection, record_id, now, desired)?;
    let updated_at_ms = record
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(now);
    upsert_rxdb_collection_record(root, collection, record_id, updated_at_ms, record.clone())?;
    Ok(record)
}

fn stable_record_content(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("_rev");
        object.remove("_deleted");
        object.remove("updated_at_ms");
    }
    value
}

fn ensure_owned(
    existing: Option<&Value>,
    owner_user_id: &str,
    kind: &str,
    can_manage_all_records: bool,
) -> anyhow::Result<()> {
    if let Some(existing) = existing {
        anyhow::ensure!(
            can_manage_all_records
                || existing.get("owner_user_id").and_then(Value::as_str) == Some(owner_user_id),
            "{kind} belongs to a different owner"
        );
    }
    Ok(())
}

fn exact_lower_hex(value: &str, field: &str, chars: usize) -> anyhow::Result<String> {
    let value = bounded_required(value, field, chars)?;
    anyhow::ensure!(
        value.len() == chars
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{field} must be exactly {chars} lowercase hexadecimal characters"
    );
    Ok(value)
}

fn bounded_required(value: &str, field: &str, max_chars: usize) -> anyhow::Result<String> {
    let value = value.trim();
    validate_bounded(value, field, max_chars, false)?;
    Ok(value.to_owned())
}

fn optional_bounded(
    value: Option<String>,
    field: &str,
    max_chars: usize,
) -> anyhow::Result<Option<String>> {
    value
        .map(|value| {
            let value = value.trim();
            validate_bounded(value, field, max_chars, true)?;
            Ok(value.to_owned())
        })
        .transpose()
}

fn validate_bounded(
    value: &str,
    field: &str,
    max_chars: usize,
    allow_empty: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        allow_empty || !value.is_empty(),
        "{field} must not be empty"
    );
    anyhow::ensure!(
        value.chars().count() <= max_chars,
        "{field} exceeds {max_chars} characters"
    );
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "{field} contains control characters"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::business_os::store::{load_rxdb_collection_record, rxdb_store_path, CommandOrigin};
    use crate::business_os::store_workjet_projects::tests::create_workjet_rxdb_projection_tables;
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn command(command_type: &str, record_id: Option<&str>, payload: Value) -> BusinessCommand {
        BusinessCommand {
            id: Some(format!("cmd-{command_type}")),
            module: "ctox".to_owned(),
            command_type: command_type.to_owned(),
            record_id: record_id.map(str::to_owned),
            payload,
            client_context: json!({}),
            origin: CommandOrigin::TrustedLocal,
        }
    }

    pub(crate) fn create_workjet_session_rxdb_projection_tables(root: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(root.join("runtime"))?;
        let conn = Connection::open(rxdb_store_path(root))?;
        for collection in [SESSIONS_COLLECTION, TRANSFERS_COLLECTION] {
            conn.execute(
                &format!(
                    "CREATE TABLE IF NOT EXISTS ctox_business_os__{collection}__v0 (
                        id TEXT PRIMARY KEY NOT NULL,
                        revision TEXT,
                        deleted INTEGER NOT NULL DEFAULT 0,
                        lastWriteTime REAL NOT NULL DEFAULT 0,
                        data TEXT NOT NULL
                    )"
                ),
                [],
            )?;
        }
        Ok(())
    }

    fn seed_project_and_copy(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<String> {
        super::super::store_workjet_projects::tests::handle_workjet_project_upsert_command(
            root,
            &command(
                "ctox.workjet.project.upsert",
                None,
                json!({
                    "project_id": format!("project-{suffix}"),
                    "name": format!("Project {suffix}")
                }),
            ),
            owner,
        )?;
        let copy =
            super::super::store_workjet_projects::handle_workjet_working_copy_upsert_command(
                root,
                &command(
                    "ctox.workjet.working_copy.upsert",
                    None,
                    json!({
                        "project_id": format!("project-{suffix}"),
                        "computer_id": format!("computer-{suffix}"),
                        "path": format!("opaque://computer-{suffix}/project-{suffix}"),
                        "active": true
                    }),
                ),
                owner,
            )?;
        Ok(copy["working_copy"]["id"]
            .as_str()
            .context("working copy id")?
            .to_owned())
    }

    fn create_session(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<Value> {
        let working_copy_id = seed_project_and_copy(root, owner, suffix)?;
        handle_workjet_session_create_command(
            root,
            &command(
                "ctox.workjet.session.create",
                None,
                json!({
                    "project_id": format!("project-{suffix}"),
                    "working_copy_id": working_copy_id,
                    "thread_id": format!("thread-{suffix}")
                }),
            ),
            owner,
        )
    }

    fn seed_online_computer(
        root: &Path,
        owner: &str,
        computer_id: &str,
        online: bool,
    ) -> anyhow::Result<()> {
        let conn = open_store(root)?;
        let now = super::super::store::now_ms() as i64;
        upsert_business_record(
            &conn,
            super::super::store_workjet_computers::COMPUTERS_COLLECTION,
            computer_id,
            now,
            json!({
                "id": computer_id,
                "display_name": computer_id,
                "hosting_mode": "workstation",
                "status": "assigned",
                "capabilities": [],
                "self_hosted_colocation": false,
                "device_binding_id": format!("binding-{computer_id}"),
                "actor_epoch": 1,
                "last_seen_at_ms": now,
                "replication_up": online,
                "owner_user_id": owner,
                "created_at_ms": now,
                "updated_at_ms": now,
                "is_deleted": false,
            }),
        )?;
        Ok(())
    }

    fn transfer_fixture_records(
        root: &Path,
        owner: &str,
        suffix: &str,
        target_online: bool,
    ) -> anyhow::Result<Value> {
        let created = create_session(root, owner, suffix)?;
        seed_online_computer(root, owner, &format!("computer-{suffix}"), true)?;
        seed_online_computer(root, owner, &format!("target-{suffix}"), target_online)?;
        Ok(created)
    }

    fn transfer_fixture(
        root: &Path,
        owner: &str,
        suffix: &str,
        target_online: bool,
    ) -> anyhow::Result<Value> {
        create_workjet_rxdb_projection_tables(root)?;
        create_workjet_session_rxdb_projection_tables(root)?;
        transfer_fixture_records(root, owner, suffix, target_online)
    }

    fn start_transfer(
        root: &Path,
        owner: &str,
        session_id: &str,
        suffix: &str,
        key: &str,
    ) -> anyhow::Result<Value> {
        handle_workjet_session_transfer_start_command(
            root,
            &command(
                "ctox.workjet.session.transfer.start",
                None,
                json!({
                    "session_id": session_id,
                    "target_computer_id": format!("target-{suffix}"),
                    "target_path": format!("opaque://target-{suffix}/project-{suffix}"),
                    "idempotency_key": key,
                }),
            ),
            owner,
            false,
        )
    }

    fn pause_ack_transfer_with_git(
        root: &Path,
        owner: &str,
        transfer_id: &str,
        computer_id: &str,
        fence_epoch: i64,
        git_repository: bool,
        key: &str,
    ) -> anyhow::Result<Value> {
        handle_workjet_session_transfer_pause_ack_command(
            root,
            &command(
                "ctox.workjet.session.transfer.pause_ack",
                None,
                json!({
                    "transfer_id": transfer_id,
                    "computer_id": computer_id,
                    "fence_epoch": fence_epoch,
                    "last_terminal_turn_id": "turn-terminal-1",
                    "git_repository": git_repository,
                    "idempotency_key": key,
                }),
            ),
            owner,
            false,
        )
    }

    #[test]
    fn workjet_session_transfer_pause_ack_accepts_missing_last_terminal_turn() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pause-ack-none", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(
            root.path(),
            "owner-1",
            session_id,
            "pause-ack-none",
            "start",
        )?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let result = handle_workjet_session_transfer_pause_ack_command(
            root.path(),
            &command(
                "ctox.workjet.session.transfer.pause_ack",
                None,
                json!({
                    "transfer_id": transfer_id,
                    "computer_id": "computer-pause-ack-none",
                    "fence_epoch": 1,
                    "git_repository": true,
                    "idempotency_key": "ack-none",
                }),
            ),
            "owner-1",
            false,
        )?;
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["state"], "packing");
        assert!(result["session"]["last_terminal_turn_id"].is_null());
        assert!(result["transfer"]["last_terminal_turn_id"].is_null());
        Ok(())
    }

    fn pause_ack_transfer(
        root: &Path,
        owner: &str,
        transfer_id: &str,
        computer_id: &str,
        fence_epoch: i64,
        key: &str,
    ) -> anyhow::Result<Value> {
        pause_ack_transfer_with_git(
            root,
            owner,
            transfer_id,
            computer_id,
            fence_epoch,
            true,
            key,
        )
    }

    fn git_pack_command(
        transfer_id: &str,
        computer_id: &str,
        generation: &str,
        key: &str,
    ) -> BusinessCommand {
        command(
            "ctox.workjet.session.transfer.pack_complete",
            None,
            json!({
                "transfer_id": transfer_id,
                "computer_id": computer_id,
                "fence_epoch": 1,
                "mode": "git",
                "manifest_file_id": "desktop-file-manifest",
                "artifact_file_ids": ["desktop-file-bundle", "desktop-file-patch", "desktop-file-untracked"],
                "artifact_generation_id": generation,
                "manifest_sha256": "0".repeat(64),
                "git": {
                    "head": "1".repeat(40),
                    "branch": "workjet/session-transfer",
                    "base_commit": "2".repeat(40),
                    "bundle_file_id": "desktop-file-bundle",
                    "patch_file_id": "desktop-file-patch",
                    "patch_sha256": "3".repeat(64),
                    "untracked_file_id": "desktop-file-untracked",
                    "untracked_sha256": "4".repeat(64),
                    "dirty": true
                },
                "idempotency_key": key,
            }),
        )
    }

    fn copy_pack_command(
        transfer_id: &str,
        computer_id: &str,
        generation: &str,
        key: &str,
    ) -> BusinessCommand {
        command(
            "ctox.workjet.session.transfer.pack_complete",
            None,
            json!({
                "transfer_id": transfer_id,
                "computer_id": computer_id,
                "fence_epoch": 1,
                "mode": "copy",
                "manifest_file_id": "desktop-file-copy-manifest",
                "artifact_file_ids": ["desktop-file-copy-archive"],
                "artifact_generation_id": generation,
                "manifest_sha256": "5".repeat(64),
                "tree_sha256": "6".repeat(64),
                "idempotency_key": key,
            }),
        )
    }

    fn packed_transfer_records(
        root: &Path,
        owner: &str,
        suffix: &str,
        mode: &str,
    ) -> anyhow::Result<Value> {
        let created = transfer_fixture_records(root, owner, suffix, true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root, owner, session_id, suffix, "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer_with_git(
            root,
            owner,
            transfer_id,
            &format!("computer-{suffix}"),
            1,
            mode == "git",
            "ack",
        )?;
        handle_workjet_session_transfer_pack_complete_command(
            root,
            &if mode == "git" {
                git_pack_command(
                    transfer_id,
                    &format!("computer-{suffix}"),
                    "generation",
                    "pack",
                )
            } else {
                copy_pack_command(
                    transfer_id,
                    &format!("computer-{suffix}"),
                    "generation",
                    "pack",
                )
            },
            owner,
            false,
        )
    }

    fn packed_transfer(
        root: &Path,
        owner: &str,
        suffix: &str,
        mode: &str,
    ) -> anyhow::Result<Value> {
        create_workjet_rxdb_projection_tables(root)?;
        create_workjet_session_rxdb_projection_tables(root)?;
        packed_transfer_records(root, owner, suffix, mode)
    }

    fn apply_command(
        transfer_id: &str,
        suffix: &str,
        mode: &str,
        matching: bool,
        key: &str,
    ) -> BusinessCommand {
        command(
            "ctox.workjet.session.transfer.apply_complete",
            None,
            if mode == "git" {
                json!({
                    "transfer_id": transfer_id,
                    "computer_id": format!("target-{suffix}"),
                    "fence_epoch": 1,
                    "observed_head": if matching { "1".repeat(40) } else { "9".repeat(40) },
                    "observed_manifest_sha256": "0".repeat(64),
                    "idempotency_key": key,
                })
            } else {
                json!({
                    "transfer_id": transfer_id,
                    "computer_id": format!("target-{suffix}"),
                    "fence_epoch": 1,
                    "observed_manifest_sha256": "5".repeat(64),
                    "observed_tree_sha256": if matching { "6".repeat(64) } else { "9".repeat(64) },
                    "idempotency_key": key,
                })
            },
        )
    }

    fn applied_transfer_records(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<Value> {
        let packed = packed_transfer_records(root, owner, suffix, "git")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        handle_workjet_session_transfer_apply_complete_command(
            root,
            &apply_command(transfer_id, suffix, "git", true, "apply"),
            owner,
            false,
        )
    }

    fn applied_transfer(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<Value> {
        create_workjet_rxdb_projection_tables(root)?;
        create_workjet_session_rxdb_projection_tables(root)?;
        applied_transfer_records(root, owner, suffix)
    }

    fn confirm_command(
        transfer_id: &str,
        suffix: &str,
        working_copy_id: &str,
        key: &str,
    ) -> BusinessCommand {
        command(
            "ctox.workjet.session.transfer.confirm_working_copy",
            None,
            json!({
                "transfer_id": transfer_id,
                "computer_id": format!("target-{suffix}"),
                "fence_epoch": 1,
                "working_copy_id": working_copy_id,
                "path": format!("opaque://target-{suffix}/project-{suffix}"),
                "idempotency_key": key,
            }),
        )
    }

    fn switched_transfer_records(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<Value> {
        let applied = applied_transfer_records(root, owner, suffix)?;
        let transfer_id = applied["transfer_id"].as_str().context("transfer id")?;
        let path = format!("opaque://target-{suffix}/project-{suffix}");
        let working_copy_id = deterministic_working_copy_id(
            &format!("project-{suffix}"),
            &format!("target-{suffix}"),
            &path,
        );
        handle_workjet_session_transfer_confirm_working_copy_command(
            root,
            &confirm_command(transfer_id, suffix, &working_copy_id, "confirm"),
            owner,
            false,
        )
    }

    fn switched_transfer(root: &Path, owner: &str, suffix: &str) -> anyhow::Result<Value> {
        create_workjet_rxdb_projection_tables(root)?;
        create_workjet_session_rxdb_projection_tables(root)?;
        switched_transfer_records(root, owner, suffix)
    }

    fn resume_ack_command(
        transfer_id: &str,
        computer_id: &str,
        fence_epoch: i64,
        key: &str,
    ) -> BusinessCommand {
        command(
            "ctox.workjet.session.transfer.resume_ack",
            None,
            json!({
                "transfer_id": transfer_id,
                "computer_id": computer_id,
                "fence_epoch": fence_epoch,
                "worker_instance_id": "worker-target-1",
                "idempotency_key": key,
            }),
        )
    }

    #[test]
    fn workjet_session_transfer_state_table_matches_rfc() {
        let states = WORKJET_SESSION_TRANSFER_STATES
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(states.len(), WORKJET_SESSION_TRANSFER_STATES.len());
        for transition in WORKJET_SESSION_TRANSFER_TRANSITIONS {
            assert!(states.contains(transition.from));
            assert!(states.contains(transition.to));
            assert_eq!(
                workjet_session_transfer_next_state(transition.from, transition.verb),
                Some(transition.to)
            );
        }

        assert_eq!(
            workjet_session_transfer_next_state("packed", "ship"),
            Some("shipping")
        );
        assert_eq!(
            workjet_session_transfer_next_state("shipping", "apply_start"),
            Some("applying")
        );

        for (state, verb) in [
            ("pause_requested", "pack_complete"),
            ("packing", "pause_ack"),
            ("packed", "apply_complete"),
            ("shipping", "confirm_working_copy"),
            ("applying", "resume_ack"),
            ("applied", "pack_complete"),
            ("switching", "pause_ack"),
            ("resuming", "confirm_working_copy"),
            ("aborting", "resume_ack"),
            ("completed", "abort"),
            ("rolled_back", "abort"),
            ("failed", "abort"),
        ] {
            assert_eq!(workjet_session_transfer_next_state(state, verb), None);
        }
    }

    #[test]
    fn workjet_session_transfer_start_projects_records_and_replays_without_second_epoch(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "start", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;

        let first = start_transfer(root.path(), "owner-1", session_id, "start", "move-once")?;
        let second = start_transfer(root.path(), "owner-1", session_id, "start", "move-once")?;
        assert_eq!(first["ok"], true);
        assert_eq!(first["state"], "pause_requested");
        assert_eq!(first["fence_epoch"], 1);
        assert_eq!(first["session"]["run_status"], "pausing");
        assert_eq!(first["session"]["active_transfer_id"], first["transfer_id"]);
        assert_eq!(first["session"]["_rev"], second["session"]["_rev"]);
        assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
        assert_eq!(second["fence_epoch"], 1);

        let transfer_id = first["transfer_id"].as_str().context("transfer id")?;
        assert_eq!(
            load_rxdb_collection_record(root.path(), SESSIONS_COLLECTION, session_id)?
                .context("session projection")?["fence_epoch"],
            1
        );
        assert_eq!(
            load_rxdb_collection_record(root.path(), TRANSFERS_COLLECTION, transfer_id)?
                .context("transfer projection")?["state"],
            "pause_requested"
        );
        let conn = open_store(root.path())?;
        let audit_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_events
             WHERE record_id=?1 AND command_type='workjet.session.transfer.started'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1, "start replay must not duplicate audit");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_start_returns_stable_precondition_errors() -> anyhow::Result<()> {
        let missing = tempdir()?;
        let missing_result = start_transfer(
            missing.path(),
            "owner-1",
            "workjet_session_missing",
            "missing",
            "missing",
        )?;
        assert_eq!(missing_result["error_code"], "session_not_found");

        let offline = tempdir()?;
        let offline_session = transfer_fixture(offline.path(), "owner-1", "offline", false)?;
        let offline_id = offline_session["session"]["id"]
            .as_str()
            .context("offline session id")?;
        assert_eq!(
            start_transfer(offline.path(), "owner-1", offline_id, "offline", "offline")?
                ["error_code"],
            "target_computer_offline"
        );

        let active = tempdir()?;
        let active_session = transfer_fixture(active.path(), "owner-1", "active", true)?;
        let active_id = active_session["session"]["id"]
            .as_str()
            .context("active session id")?;
        assert_eq!(
            start_transfer(active.path(), "owner-1", active_id, "active", "first")?["ok"],
            true
        );
        assert_eq!(
            start_transfer(active.path(), "owner-1", active_id, "active", "second")?["error_code"],
            "session_already_transferring"
        );
        assert_eq!(
            outbound_load_record(&open_store(active.path())?, SESSIONS_COLLECTION, active_id)?
                .context("active session")?["fence_epoch"],
            1
        );

        let foreign = tempdir()?;
        let foreign_session = transfer_fixture(foreign.path(), "owner-1", "foreign-start", true)?;
        let foreign_id = foreign_session["session"]["id"]
            .as_str()
            .context("foreign session id")?;
        assert_eq!(
            start_transfer(
                foreign.path(),
                "owner-2",
                foreign_id,
                "foreign-start",
                "foreign"
            )?["error_code"],
            "session_not_owned"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pause_ack_moves_to_packing_and_pauses_session() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pause-ack", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pause-ack", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let result = pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pause-ack",
            1,
            "ack-once",
        )?;
        assert_eq!(result["state"], "packing");
        assert_eq!(result["session"]["run_status"], "paused");
        assert_eq!(
            result["session"]["last_terminal_turn_id"],
            "turn-terminal-1"
        );
        assert_eq!(result["transfer"]["source_git_repository"], true);
        assert!(result["transfer"]["pause_acked_at_ms"].as_i64().is_some());
        assert!(
            result["transfer"]["deadline_at_ms"]
                .as_i64()
                .unwrap_or_default()
                > result["transfer"]["pause_acked_at_ms"]
                    .as_i64()
                    .unwrap_or_default()
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pause_ack_rejects_wrong_computer() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "ack-computer", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "ack-computer", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let result = pause_ack_transfer(root.path(), "owner-1", transfer_id, "other", 1, "ack")?;
        assert_eq!(result["error_code"], "computer_actor_mismatch");
        assert_eq!(result["state"], "pause_requested");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pause_ack_rejects_epoch_mismatch() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "ack-epoch", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "ack-epoch", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let result = pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-ack-epoch",
            0,
            "ack",
        )?;
        assert_eq!(result["error_code"], "session_fenced");
        assert_eq!(result["state"], "pause_requested");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pause_ack_rejects_illegal_state() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "ack-state", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "ack-state", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let conn = open_store(root.path())?;
        let now = super::super::store::now_ms() as i64;
        let mut transfer =
            outbound_load_record(&conn, TRANSFERS_COLLECTION, transfer_id)?.context("transfer")?;
        transfer["state"] = Value::String("packing".to_owned());
        upsert_business_record(&conn, TRANSFERS_COLLECTION, transfer_id, now, transfer)?;
        let result = pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-ack-state",
            1,
            "ack",
        )?;
        assert_eq!(result["error_code"], "transfer_illegal_state");
        assert_eq!(result["state"], "packing");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pause_ack_replays_without_second_effect() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "ack-replay", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "ack-replay", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let first = pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-ack-replay",
            1,
            "ack-once",
        )?;
        let second = pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-ack-replay",
            1,
            "ack-once",
        )?;
        assert_eq!(first, second);
        let audit_count: i64 = open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_events
             WHERE record_id=?1 AND command_type='workjet.session.transfer.fenced'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1, "pause_ack replay must not duplicate audit");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_git_reaches_shipping_with_manifest(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-git", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pack-git", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pack-git",
            1,
            "ack",
        )?;
        let packed = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &git_pack_command(transfer_id, "computer-pack-git", "generation-1", "pack"),
            "owner-1",
            false,
        )?;
        assert_eq!(packed["state"], "shipping");
        assert_eq!(packed["session"]["run_status"], "transferring");
        assert_eq!(packed["transfer"]["mode"], "git");
        assert_eq!(packed["transfer"]["manifest_sha256"], "0".repeat(64));
        assert_eq!(packed["transfer"]["artifact_generation_id"], "generation-1");
        assert_eq!(packed["transfer"]["git"]["head"], "1".repeat(40));
        assert!(
            packed["transfer"]["deadline_at_ms"]
                .as_i64()
                .unwrap_or_default()
                > packed["transfer"]["packed_at_ms"]
                    .as_i64()
                    .unwrap_or_default()
        );
        let audit_count: i64 = open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_events
             WHERE record_id=?1 AND command_type='workjet.session.transfer.packed'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1);
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_copy_reaches_shipping() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-copy", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pack-copy", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer_with_git(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pack-copy",
            1,
            false,
            "ack",
        )?;
        let packed = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &copy_pack_command(transfer_id, "computer-pack-copy", "generation-copy", "pack"),
            "owner-1",
            false,
        )?;
        assert_eq!(packed["state"], "shipping");
        assert_eq!(packed["transfer"]["mode"], "copy");
        assert_eq!(packed["transfer"]["tree_sha256"], "6".repeat(64));
        assert_eq!(
            packed["transfer"]["artifact_file_ids"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(packed["transfer"].get("git").is_none());
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_rejects_copy_for_git_source() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "copy-git", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "copy-git", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-copy-git",
            1,
            "ack",
        )?;
        let result = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &copy_pack_command(transfer_id, "computer-copy-git", "generation-copy", "pack"),
            "owner-1",
            false,
        )?;
        assert_eq!(result["error_code"], "copy_not_allowed_for_git_repo");
        assert_eq!(result["state"], "packing");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_rejects_wrong_sha_length_without_panic(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-sha", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pack-sha", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pack-sha",
            1,
            "ack",
        )?;
        let mut malformed =
            git_pack_command(transfer_id, "computer-pack-sha", "generation-1", "pack");
        malformed.payload["manifest_sha256"] = Value::String("abc".to_owned());
        let error = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &malformed,
            "owner-1",
            false,
        )
        .expect_err("short SHA must be rejected");
        assert!(error.to_string().contains("manifest_sha256"));
        assert_eq!(
            outbound_load_record(&open_store(root.path())?, TRANSFERS_COLLECTION, transfer_id)?
                .context("transfer")?["state"],
            "packing"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_rejects_second_generation() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-generation", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(
            root.path(),
            "owner-1",
            session_id,
            "pack-generation",
            "start",
        )?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pack-generation",
            1,
            "ack",
        )?;
        assert_eq!(
            handle_workjet_session_transfer_pack_complete_command(
                root.path(),
                &git_pack_command(
                    transfer_id,
                    "computer-pack-generation",
                    "generation-1",
                    "pack-1"
                ),
                "owner-1",
                false,
            )?["state"],
            "shipping"
        );
        let conflict = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &git_pack_command(
                transfer_id,
                "computer-pack-generation",
                "generation-2",
                "pack-2",
            ),
            "owner-1",
            false,
        )?;
        assert_eq!(conflict["error_code"], "idempotency_conflict");
        assert_eq!(conflict["state"], "shipping");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_replays_identically() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-replay", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pack-replay", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        pause_ack_transfer(
            root.path(),
            "owner-1",
            transfer_id,
            "computer-pack-replay",
            1,
            "ack",
        )?;
        let pack = git_pack_command(
            transfer_id,
            "computer-pack-replay",
            "generation-1",
            "pack-once",
        );
        let first = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &pack,
            "owner-1",
            false,
        )?;
        let second = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &pack,
            "owner-1",
            false,
        )?;
        assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
        assert_eq!(first["session"]["_rev"], second["session"]["_rev"]);
        let audit_count: i64 = open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_events
             WHERE record_id=?1 AND command_type='workjet.session.transfer.packed'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1, "pack replay must not duplicate audit");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_pack_complete_requires_pause_ack() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "pack-no-ack", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "pack-no-ack", "start")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let result = handle_workjet_session_transfer_pack_complete_command(
            root.path(),
            &git_pack_command(transfer_id, "computer-pack-no-ack", "generation-1", "pack"),
            "owner-1",
            false,
        )?;
        assert_eq!(result["error_code"], "transfer_illegal_state");
        assert_eq!(result["state"], "pause_requested");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_apply_complete_accepts_git_and_replays() -> anyhow::Result<()> {
        let root = tempdir()?;
        let packed = packed_transfer(root.path(), "owner-1", "apply-git", "git")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        let apply = apply_command(transfer_id, "apply-git", "git", true, "apply-once");
        let first = handle_workjet_session_transfer_apply_complete_command(
            root.path(),
            &apply,
            "owner-1",
            false,
        )?;
        let second = handle_workjet_session_transfer_apply_complete_command(
            root.path(),
            &apply,
            "owner-1",
            false,
        )?;
        assert_eq!(first["state"], "applied");
        assert_eq!(first["transfer"]["observed_head"], "1".repeat(40));
        assert_eq!(
            first["transfer"]["observed_manifest_sha256"],
            "0".repeat(64)
        );
        assert!(first["transfer"]["applied_at_ms"].as_i64().is_some());
        assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
        let audit_count: i64 = open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_events WHERE record_id=?1 AND command_type='workjet.session.transfer.applied'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1);
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_apply_complete_accepts_copy() -> anyhow::Result<()> {
        let root = tempdir()?;
        let packed = packed_transfer(root.path(), "owner-1", "apply-copy", "copy")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        let applied = handle_workjet_session_transfer_apply_complete_command(
            root.path(),
            &apply_command(transfer_id, "apply-copy", "copy", true, "apply"),
            "owner-1",
            false,
        )?;
        assert_eq!(applied["state"], "applied");
        assert_eq!(applied["transfer"]["observed_tree_sha256"], "6".repeat(64));
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_apply_complete_fails_third_identical_mismatch() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        let packed = packed_transfer(root.path(), "owner-1", "apply-mismatch", "git")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        for attempt in 1..=3 {
            let result = handle_workjet_session_transfer_apply_complete_command(
                root.path(),
                &apply_command(
                    transfer_id,
                    "apply-mismatch",
                    "git",
                    false,
                    &format!("apply-{attempt}"),
                ),
                "owner-1",
                false,
            )?;
            assert_eq!(result["error_code"], "apply_hash_mismatch");
            assert_eq!(result["retryable"], true);
            assert_eq!(
                result["state"],
                if attempt < 3 { "applying" } else { "failed" }
            );
        }
        let conn = open_store(root.path())?;
        let transfer =
            outbound_load_record(&conn, TRANSFERS_COLLECTION, transfer_id)?.context("transfer")?;
        assert_eq!(transfer["apply_mismatch_count"], 3);
        assert_eq!(transfer["error_code"], "apply_hash_mismatch");
        let session_id = transfer["session_id"].as_str().context("session id")?;
        let session =
            outbound_load_record(&conn, SESSIONS_COLLECTION, session_id)?.context("session")?;
        assert_eq!(session["computer_id"], "computer-apply-mismatch");
        assert_eq!(session["run_status"], "transferring");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_apply_complete_rejects_wrong_computer_and_epoch(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let packed = packed_transfer(root.path(), "owner-1", "apply-actor", "git")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        let mut wrong_computer = apply_command(transfer_id, "apply-actor", "git", true, "computer");
        wrong_computer.payload["computer_id"] = Value::String("computer-apply-actor".to_owned());
        assert_eq!(
            handle_workjet_session_transfer_apply_complete_command(
                root.path(),
                &wrong_computer,
                "owner-1",
                false,
            )?["error_code"],
            "computer_actor_mismatch"
        );
        let mut wrong_epoch = apply_command(transfer_id, "apply-actor", "git", true, "epoch");
        wrong_epoch.payload["fence_epoch"] = Value::from(0);
        assert_eq!(
            handle_workjet_session_transfer_apply_complete_command(
                root.path(),
                &wrong_epoch,
                "owner-1",
                false,
            )?["error_code"],
            "session_fenced"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_confirm_working_copy_commits_all_switch_effects(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let applied = applied_transfer(root.path(), "owner-1", "confirm")?;
        let transfer_id = applied["transfer_id"].as_str().context("transfer id")?;
        let session_id = applied["session"]["id"].as_str().context("session id")?;
        let source_working_copy_id = applied["transfer"]["source_working_copy_id"]
            .as_str()
            .context("source working copy id")?;
        let target_path = "opaque://target-confirm/project-confirm";
        let target_working_copy_id =
            deterministic_working_copy_id("project-confirm", "target-confirm", target_path);
        let result = handle_workjet_session_transfer_confirm_working_copy_command(
            root.path(),
            &confirm_command(transfer_id, "confirm", &target_working_copy_id, "confirm"),
            "owner-1",
            false,
        )?;
        assert_eq!(result["state"], "resuming");
        assert_eq!(result["fence_epoch"], 2);
        assert_eq!(result["session"]["working_copy_id"], target_working_copy_id);
        assert_eq!(result["session"]["computer_id"], "target-confirm");
        assert_eq!(result["session"]["run_status"], "resuming");
        assert_eq!(result["session"]["fence_epoch"], 2);
        assert_eq!(result["transfer"]["fence_epoch"], 2);
        assert!(result["transfer"]["switched_at_ms"].as_i64().is_some());
        assert_eq!(
            result["transfer"]["deadline_at_ms"]
                .as_i64()
                .unwrap_or_default()
                - result["transfer"]["switched_at_ms"]
                    .as_i64()
                    .unwrap_or_default(),
            120_000
        );
        let conn = open_store(root.path())?;
        let source =
            outbound_load_record(&conn, WORKING_COPIES_COLLECTION, source_working_copy_id)?
                .context("source working copy")?;
        let target =
            outbound_load_record(&conn, WORKING_COPIES_COLLECTION, &target_working_copy_id)?
                .context("target working copy")?;
        assert_eq!(source["status"], "detached");
        assert_eq!(target["status"], "active");
        assert!(target["verified_at_ms"].as_i64().is_some());
        assert_eq!(target["path"], target_path);
        assert_eq!(
            load_rxdb_collection_record(
                root.path(),
                WORKING_COPIES_COLLECTION,
                &target_working_copy_id,
            )?
            .context("target projection")?["status"],
            "active"
        );
        assert_eq!(
            load_rxdb_collection_record(root.path(), SESSIONS_COLLECTION, session_id)?
                .context("session projection")?["fence_epoch"],
            2
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_confirm_working_copy_rejects_id_and_actor_mismatch(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let applied = applied_transfer(root.path(), "owner-1", "confirm-reject")?;
        let transfer_id = applied["transfer_id"].as_str().context("transfer id")?;
        let mismatch = handle_workjet_session_transfer_confirm_working_copy_command(
            root.path(),
            &confirm_command(transfer_id, "confirm-reject", "workjet_wc_wrong", "bad-id"),
            "owner-1",
            false,
        )?;
        assert_eq!(mismatch["error_code"], "working_copy_id_mismatch");
        let expected = deterministic_working_copy_id(
            "project-confirm-reject",
            "target-confirm-reject",
            "opaque://target-confirm-reject/project-confirm-reject",
        );
        let mut wrong_actor = confirm_command(transfer_id, "confirm-reject", &expected, "actor");
        wrong_actor.payload["computer_id"] = Value::String("computer-confirm-reject".to_owned());
        assert_eq!(
            handle_workjet_session_transfer_confirm_working_copy_command(
                root.path(),
                &wrong_actor,
                "owner-1",
                false,
            )?["error_code"],
            "computer_actor_mismatch"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_confirm_working_copy_replays_without_second_epoch(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let applied = applied_transfer(root.path(), "owner-1", "confirm-replay")?;
        let transfer_id = applied["transfer_id"].as_str().context("transfer id")?;
        let expected = deterministic_working_copy_id(
            "project-confirm-replay",
            "target-confirm-replay",
            "opaque://target-confirm-replay/project-confirm-replay",
        );
        let confirm = confirm_command(transfer_id, "confirm-replay", &expected, "once");
        let first = handle_workjet_session_transfer_confirm_working_copy_command(
            root.path(),
            &confirm,
            "owner-1",
            false,
        )?;
        let second = handle_workjet_session_transfer_confirm_working_copy_command(
            root.path(),
            &confirm,
            "owner-1",
            false,
        )?;
        assert_eq!(first["fence_epoch"], 2);
        assert_eq!(second["fence_epoch"], 2);
        assert_eq!(first["session"]["_rev"], second["session"]["_rev"]);
        assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_confirm_working_copy_requires_applied_state() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        let packed = packed_transfer(root.path(), "owner-1", "confirm-state", "git")?;
        let transfer_id = packed["transfer_id"].as_str().context("transfer id")?;
        let expected = deterministic_working_copy_id(
            "project-confirm-state",
            "target-confirm-state",
            "opaque://target-confirm-state/project-confirm-state",
        );
        let result = handle_workjet_session_transfer_confirm_working_copy_command(
            root.path(),
            &confirm_command(transfer_id, "confirm-state", &expected, "confirm"),
            "owner-1",
            false,
        )?;
        assert_eq!(result["error_code"], "transfer_illegal_state");
        assert_eq!(result["state"], "shipping");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_resume_ack_completes_and_replays() -> anyhow::Result<()> {
        let root = tempdir()?;
        let switched = switched_transfer(root.path(), "owner-1", "resume")?;
        let transfer_id = switched["transfer_id"].as_str().context("transfer id")?;
        let ack = resume_ack_command(transfer_id, "target-resume", 2, "resume-once");
        let first = handle_workjet_session_transfer_resume_ack_command(
            root.path(),
            &ack,
            "owner-1",
            false,
        )?;
        let second = handle_workjet_session_transfer_resume_ack_command(
            root.path(),
            &ack,
            "owner-1",
            false,
        )?;
        assert_eq!(first["state"], "completed");
        assert_eq!(first["session"]["run_status"], "running");
        assert!(first["session"].get("active_transfer_id").is_none());
        assert_eq!(first["transfer"]["worker_instance_id"], "worker-target-1");
        assert!(first["transfer"]["completed_at_ms"].as_i64().is_some());
        assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
        let audit_count: i64 = open_store(root.path())?.query_row(
            "SELECT COUNT(*) FROM business_events WHERE record_id=?1 AND command_type='workjet.session.transfer.completed'",
            [transfer_id],
            |row| row.get(0),
        )?;
        assert_eq!(audit_count, 1);
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_resume_ack_rejects_source_and_old_epoch() -> anyhow::Result<()> {
        let root = tempdir()?;
        let switched = switched_transfer(root.path(), "owner-1", "resume-reject")?;
        let transfer_id = switched["transfer_id"].as_str().context("transfer id")?;
        assert_eq!(
            handle_workjet_session_transfer_resume_ack_command(
                root.path(),
                &resume_ack_command(transfer_id, "computer-resume-reject", 2, "source"),
                "owner-1",
                false,
            )?["error_code"],
            "computer_actor_mismatch"
        );
        assert_eq!(
            handle_workjet_session_transfer_resume_ack_command(
                root.path(),
                &resume_ack_command(transfer_id, "target-resume-reject", 1, "old-epoch"),
                "owner-1",
                false,
            )?["error_code"],
            "session_fenced"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_abort_in_resuming_requires_manual_intervention(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let switched = switched_transfer(root.path(), "owner-1", "resume-abort")?;
        let transfer_id = switched["transfer_id"].as_str().context("transfer id")?;
        let aborted = handle_workjet_session_transfer_abort_command(
            root.path(),
            &command(
                "ctox.workjet.session.transfer.abort",
                None,
                json!({
                    "transfer_id": transfer_id,
                    "reason": "operator_cancel_after_switch",
                    "idempotency_key": "abort-after-switch",
                }),
            ),
            "owner-1",
            false,
        )?;
        assert_eq!(aborted["state"], "failed");
        assert_eq!(aborted["transfer"]["error_code"], "manual_intervention");
        assert_eq!(aborted["session"]["run_status"], "transfer_failed");
        assert_eq!(aborted["session"]["computer_id"], "target-resume-abort");
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_abort_compensates_precommit_states_and_is_idempotent(
    ) -> anyhow::Result<()> {
        for (suffix, transfer_state, session_status) in [
            ("abort-pausing", "pause_requested", "pausing"),
            ("abort-transferring", "applying", "transferring"),
        ] {
            let root = tempdir()?;
            let created = transfer_fixture(root.path(), "owner-1", suffix, true)?;
            let session_id = created["session"]["id"].as_str().context("session id")?;
            let started = start_transfer(root.path(), "owner-1", session_id, suffix, "start")?;
            let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
            if transfer_state != "pause_requested" {
                let conn = open_store(root.path())?;
                let now = super::super::store::now_ms() as i64;
                let mut transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, transfer_id)?
                    .context("transfer")?;
                transfer["state"] = Value::String(transfer_state.to_owned());
                upsert_business_record(&conn, TRANSFERS_COLLECTION, transfer_id, now, transfer)?;
                let mut session = outbound_load_record(&conn, SESSIONS_COLLECTION, session_id)?
                    .context("session")?;
                session["run_status"] = Value::String(session_status.to_owned());
                upsert_business_record(&conn, SESSIONS_COLLECTION, session_id, now, session)?;
            }

            let abort = command(
                "ctox.workjet.session.transfer.abort",
                None,
                json!({
                    "transfer_id": transfer_id,
                    "reason": "operator_cancel",
                    "idempotency_key": "abort-once",
                }),
            );
            let first = handle_workjet_session_transfer_abort_command(
                root.path(),
                &abort,
                "owner-1",
                false,
            )?;
            let second = handle_workjet_session_transfer_abort_command(
                root.path(),
                &abort,
                "owner-1",
                false,
            )?;
            assert_eq!(first["state"], "rolled_back");
            assert_eq!(first["session"]["run_status"], "running");
            assert!(first["session"].get("active_transfer_id").is_none());
            assert!(first["transfer"]["rolled_back_at_ms"].as_i64().is_some());
            assert_eq!(first["transfer"]["_rev"], second["transfer"]["_rev"]);
            assert_eq!(first["session"]["_rev"], second["session"]["_rev"]);
            let audit_count: i64 = open_store(root.path())?.query_row(
                "SELECT COUNT(*) FROM business_events
                 WHERE record_id=?1 AND command_type='workjet.session.transfer.aborted'",
                [transfer_id],
                |row| row.get(0),
            )?;
            assert_eq!(audit_count, 1, "abort replay must not duplicate audit");
        }
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_recovery_handles_expired_sides_and_is_idempotent(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let now = super::super::store::now_ms() as i64;

        let pre_created = transfer_fixture(root.path(), "owner-1", "recovery-pre", true)?;
        let pre_session_id = pre_created["session"]["id"]
            .as_str()
            .context("pre session id")?;
        let pre = start_transfer(
            root.path(),
            "owner-1",
            pre_session_id,
            "recovery-pre",
            "start",
        )?;
        let pre_transfer_id = pre["transfer_id"].as_str().context("pre transfer id")?;

        let post = switched_transfer_records(root.path(), "owner-1", "recovery-post")?;
        let post_transfer_id = post["transfer_id"].as_str().context("post transfer id")?;
        let post_session_id = post["session"]["id"].as_str().context("post session id")?;

        let fresh_created =
            transfer_fixture_records(root.path(), "owner-1", "recovery-fresh", true)?;
        let fresh_session_id = fresh_created["session"]["id"]
            .as_str()
            .context("fresh session id")?;
        let fresh = start_transfer(
            root.path(),
            "owner-1",
            fresh_session_id,
            "recovery-fresh",
            "start",
        )?;
        let fresh_transfer_id = fresh["transfer_id"].as_str().context("fresh transfer id")?;

        let conn = open_store(root.path())?;
        for (transfer_id, deadline) in [
            (pre_transfer_id, now - 1),
            (post_transfer_id, now - 1),
            (fresh_transfer_id, now + 60_000),
        ] {
            let mut transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, transfer_id)?
                .context("transfer")?;
            transfer["deadline_at_ms"] = Value::from(deadline);
            upsert_business_record(&conn, TRANSFERS_COLLECTION, transfer_id, now, transfer)?;
        }
        drop(conn);

        let mut outcomes = run_workjet_session_transfer_recovery(root.path(), now);
        outcomes.sort_by(|left, right| left.transfer_id.cmp(&right.transfer_id));
        assert_eq!(outcomes.len(), 2);
        let pre_outcome = outcomes
            .iter()
            .find(|outcome| outcome.transfer_id == pre_transfer_id)
            .context("pre outcome")?;
        assert_eq!(pre_outcome.old_state, "pause_requested");
        assert_eq!(pre_outcome.new_state, "rolled_back");
        assert_eq!(pre_outcome.error_code, "pause_timeout");
        let post_outcome = outcomes
            .iter()
            .find(|outcome| outcome.transfer_id == post_transfer_id)
            .context("post outcome")?;
        assert_eq!(post_outcome.old_state, "resuming");
        assert_eq!(post_outcome.new_state, "failed");
        assert_eq!(post_outcome.error_code, "resume_timeout");

        let conn = open_store(root.path())?;
        let pre_transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, pre_transfer_id)?
            .context("pre transfer")?;
        let pre_session = outbound_load_record(&conn, SESSIONS_COLLECTION, pre_session_id)?
            .context("pre session")?;
        assert_eq!(pre_transfer["state"], "rolled_back");
        assert_eq!(pre_transfer["target_partial_cleanup_required"], true);
        assert_eq!(pre_session["run_status"], "running");
        assert!(pre_session.get("active_transfer_id").is_none());

        let post_transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, post_transfer_id)?
            .context("post transfer")?;
        let post_session = outbound_load_record(&conn, SESSIONS_COLLECTION, post_session_id)?
            .context("post session")?;
        assert_eq!(post_transfer["state"], "failed");
        assert_eq!(post_transfer["error_code"], "resume_timeout");
        assert_eq!(post_session["run_status"], "transfer_failed");
        assert_eq!(post_session["computer_id"], "target-recovery-post");

        let fresh_transfer = outbound_load_record(&conn, TRANSFERS_COLLECTION, fresh_transfer_id)?
            .context("fresh transfer")?;
        assert_eq!(fresh_transfer["state"], "pause_requested");
        drop(conn);
        assert!(run_workjet_session_transfer_recovery(root.path(), now).is_empty());
        Ok(())
    }

    #[test]
    fn workjet_session_transfer_status_is_read_only_and_owner_scoped() -> anyhow::Result<()> {
        let root = tempdir()?;
        let created = transfer_fixture(root.path(), "owner-1", "status", true)?;
        let session_id = created["session"]["id"].as_str().context("session id")?;
        let started = start_transfer(root.path(), "owner-1", session_id, "status", "status")?;
        let transfer_id = started["transfer_id"].as_str().context("transfer id")?;
        let before_session_rev = started["session"]["_rev"].clone();
        let before_transfer_rev = started["transfer"]["_rev"].clone();

        for payload in [
            json!({ "transfer_id": transfer_id }),
            json!({ "session_id": session_id }),
        ] {
            let status = handle_workjet_session_transfer_status_command(
                root.path(),
                &command("ctox.workjet.session.transfer.status", None, payload),
                "owner-1",
                false,
            )?;
            assert_eq!(status["ok"], true);
            assert_eq!(status["session"]["_rev"], before_session_rev);
            assert_eq!(status["transfer"]["_rev"], before_transfer_rev);
        }
        assert_eq!(
            handle_workjet_session_transfer_status_command(
                root.path(),
                &command(
                    "ctox.workjet.session.transfer.status",
                    None,
                    json!({ "transfer_id": transfer_id })
                ),
                "owner-2",
                false,
            )?["error_code"],
            "session_not_owned"
        );
        Ok(())
    }

    #[test]
    fn workjet_session_create_is_idempotent_and_stamps_invariants() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_rxdb_projection_tables(root.path())?;
        create_workjet_session_rxdb_projection_tables(root.path())?;
        let working_copy_id = seed_project_and_copy(root.path(), "owner-1", "one")?;
        let create = command(
            "ctox.workjet.session.create",
            None,
            json!({
                "project_id": "project-one",
                "working_copy_id": working_copy_id,
                "thread_id": "thread-1",
                "coding_session_id": "coding-1"
            }),
        );
        let first = handle_workjet_session_create_command(root.path(), &create, "owner-1")?;
        let second = handle_workjet_session_create_command(root.path(), &create, "owner-1")?;
        let session = &first["session"];
        assert!(session["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("workjet_session_")));
        assert_eq!(session["working_copy_id"], working_copy_id);
        assert_eq!(session["computer_id"], "computer-one");
        assert_eq!(session["run_status"], "running");
        assert_eq!(session["fence_epoch"], 0);
        assert!(session.get("active_transfer_id").is_none());
        assert_eq!(session["_rev"], second["session"]["_rev"]);
        assert_eq!(session["updated_at_ms"], second["session"]["updated_at_ms"]);
        Ok(())
    }

    #[test]
    fn workjet_session_create_list_delete_are_owner_scoped_and_projected() -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_rxdb_projection_tables(root.path())?;
        create_workjet_session_rxdb_projection_tables(root.path())?;
        let owned = create_session(root.path(), "owner-1", "owned")?;
        create_session(root.path(), "owner-2", "foreign")?;
        let session_id = owned["session"]["id"]
            .as_str()
            .context("session id")?
            .to_owned();

        let listed = handle_workjet_session_list_command(
            root.path(),
            &command("ctox.workjet.session.list", None, json!({ "limit": 100 })),
            "owner-1",
        )?;
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["sessions"][0]["id"], session_id);
        assert!(handle_workjet_session_delete_command(
            root.path(),
            &command("ctox.workjet.session.delete", Some(&session_id), json!({})),
            "owner-2",
            false,
        )
        .is_err());

        let delete = command("ctox.workjet.session.delete", Some(&session_id), json!({}));
        let first = handle_workjet_session_delete_command(root.path(), &delete, "owner-1", false)?;
        let second = handle_workjet_session_delete_command(root.path(), &delete, "owner-1", false)?;
        assert_eq!(first["session"]["is_deleted"], true);
        assert_eq!(first["session"]["_rev"], second["session"]["_rev"]);
        assert_eq!(
            load_rxdb_collection_record(root.path(), SESSIONS_COLLECTION, &session_id)?
                .context("session projection")?["is_deleted"],
            true
        );
        assert_eq!(
            handle_workjet_session_list_command(
                root.path(),
                &command("ctox.workjet.session.list", None, json!({})),
                "owner-1",
            )?["count"],
            0
        );
        Ok(())
    }

    #[test]
    fn workjet_session_rejects_spoofed_ids_unknown_fields_and_invalid_copy_binding(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        let working_copy_id = seed_project_and_copy(root.path(), "owner-1", "strict")?;
        for payload in [
            json!({
                "project_id": "project-strict",
                "working_copy_id": working_copy_id,
                "owner_user_id": "attacker"
            }),
            json!({
                "project_id": "project-strict",
                "working_copy_id": working_copy_id,
                "session_id": "browser-chosen"
            }),
        ] {
            assert!(handle_workjet_session_create_command(
                root.path(),
                &command("ctox.workjet.session.create", None, payload),
                "owner-1",
            )
            .is_err());
        }
        assert!(handle_workjet_session_create_command(
            root.path(),
            &command(
                "ctox.workjet.session.create",
                None,
                json!({
                    "project_id": "project-strict",
                    "working_copy_id": working_copy_id
                })
            ),
            "owner-2",
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn workjet_session_owner_migration_updates_session_and_transfer_projections(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        create_workjet_session_rxdb_projection_tables(root.path())?;
        let email = "owner@example.test";
        let stable = "stable-owner-id";
        let conn = open_store(root.path())?;
        let now = 10;
        let session = json!({
            "id": "workjet_session_email",
            "project_id": "project-email",
            "working_copy_id": "workjet_wc_email",
            "computer_id": "computer-email",
            "run_status": "running",
            "fence_epoch": 0,
            "owner_user_id": email,
            "created_at_ms": now,
            "updated_at_ms": now,
            "is_deleted": false
        });
        let transfer = json!({
            "id": "workjet_xfer_email",
            "session_id": "workjet_session_email",
            "project_id": "project-email",
            "source_working_copy_id": "workjet_wc_email",
            "source_computer_id": "computer-email",
            "target_computer_id": "computer-target",
            "target_path": "opaque://target/project-email",
            "state": "pause_requested",
            "fence_epoch": 1,
            "artifact_file_ids": [],
            "deadline_at_ms": 100,
            "created_at_ms": now,
            "updated_at_ms": now,
            "owner_user_id": email,
            "is_deleted": false
        });
        upsert_business_record(
            &conn,
            SESSIONS_COLLECTION,
            "workjet_session_email",
            now,
            session,
        )?;
        upsert_business_record(
            &conn,
            TRANSFERS_COLLECTION,
            "workjet_xfer_email",
            now,
            transfer,
        )?;
        drop(conn);

        migrate_signed_owner_alias(root.path(), stable, Some(email))?;
        for (collection, id) in [
            (SESSIONS_COLLECTION, "workjet_session_email"),
            (TRANSFERS_COLLECTION, "workjet_xfer_email"),
        ] {
            assert_eq!(
                outbound_load_record(&open_store(root.path())?, collection, id)?
                    .context("migrated core record")?["owner_user_id"],
                stable
            );
            assert_eq!(
                load_rxdb_collection_record(root.path(), collection, id)?
                    .context("migrated projected record")?["owner_user_id"],
                stable
            );
        }
        Ok(())
    }
}
