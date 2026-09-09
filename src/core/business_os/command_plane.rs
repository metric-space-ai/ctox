// Origin: CTOX
// License: Apache-2.0

use super::app_runtime;
use super::control_command_types::ActiveExternalSqlControlCommand;
use super::domain_effect::{self, DomainEffectAdmission};
use super::policy::{
    policy_actor_from_session, BusinessOsPermission, BusinessOsRole, BusinessOsScope,
    BusinessOsScopeType,
};
use super::session::{session_user_id, BusinessOsSession};
use super::store::{
    appsec_business_command_requires_data_write, authorize_recoverable_background_control_command,
    business_command_core_claim_with_authorization, command_inbound_channel,
    external_sql_command_in_flight_outcome, first_string_field, handle_app_lifecycle_command,
    handle_business_os_command, handle_mailserver_command, handle_module_command,
    handle_secret_command, handle_source_command, handle_workspace_control_command,
    is_appsec_business_command, is_ats_active_command, is_ats_mutating_command,
    is_customers_active_command, is_iot_active_command, is_outbound_active_command,
    is_recoverable_background_control_command_type, is_rxdb_control_command_type, now_ms,
    open_store, persist_business_command_lifecycle_projection,
    project_iot_business_command_outcome, pull_collection_record,
    record_business_module_lifecycle_event, record_command, record_report_command,
    recoverable_background_control_claim_authorization, run_channel_command,
    rxdb_authenticated_session, rxdb_command_session, rxdb_verified_identity_email,
    stored_rxdb_business_command_outcome, write_rxdb_failed_control_command_outcome,
    BusinessCommand, BusinessOsReportMutation, ChannelCommandRequest, CommandOrigin,
    RxdbProjectionWriterCache, APPSEC_MODULE_ID,
};
use super::store_appsec_commands::handle_appsec_business_command;
use super::store_ats_commands::{handle_ats_active_command, handle_ats_mutating_command};
use super::store_customer_commands::handle_customers_active_command;
use super::store_office_commands::handle_office_control_command;
use super::store_outbound_commands::handle_outbound_active_command;
use super::store_policy::{enforce_command_policy, CommandPolicyRequirement};
use super::store_policy_audit::app_build_command_policy_target;
use super::store_projections::{
    is_business_chat_command, materialize_control_business_chat_state, upsert_business_record,
};
use crate::mission::channels;
use anyhow::Context;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;
use std::path::Path;

#[path = "command_plane_domain_effect.rs"]
mod domain_effect_recovery;
use domain_effect_recovery::recover_applied_domain_effect;

#[cfg(test)]
#[path = "command_plane_domain_effect_tests.rs"]
mod domain_effect_tests;

const BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT: &str = "ctox-business-command-replay-receipt-v1";
const COMMAND_TIMING_PROBE_FIELD: &str = "command_timing_probe";
const COMMAND_TIMING_RESULT_FIELD: &str = "command_timing";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BusinessCommandReplayState {
    Terminal,
    Running,
    Blocked,
    Uncertain,
    Known,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BusinessCommandReplayForm {
    StoredOutcomeWithLifecycle {
        existing_status: String,
        lifecycle_projection: Value,
        stored_outcome: Value,
    },
    StoredOutcome {
        existing_status: String,
        stored_outcome: Value,
    },
    ExistingCommand {
        existing_status: String,
    },
    TerminalControlClaim {
        terminal_status: String,
        result: Value,
    },
    UncertainControlClaim {
        claim_disposition: String,
        presented_execution_phase: String,
        presented_task_status: String,
        claim_result: Value,
    },
}

#[derive(Debug, Clone, Serialize)]
struct BusinessCommandReplayReceipt {
    contract: &'static str,
    command_id: String,
    already_accepted: bool,
    state: BusinessCommandReplayState,
    control_claim_disposition: Option<String>,
    form: BusinessCommandReplayForm,
}

impl BusinessCommandReplayReceipt {
    fn stored_outcome_with_lifecycle(
        command_id: &str,
        existing_status: &str,
        lifecycle_projection: &Value,
        stored_outcome: &Value,
        control_claim_disposition: Option<&str>,
    ) -> Self {
        Self {
            contract: BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT,
            command_id: command_id.to_string(),
            already_accepted: true,
            state: replay_state_with_control_claim(
                lifecycle_projection,
                existing_status,
                control_claim_disposition,
            ),
            control_claim_disposition: control_claim_disposition.map(str::to_string),
            form: BusinessCommandReplayForm::StoredOutcomeWithLifecycle {
                existing_status: existing_status.to_string(),
                lifecycle_projection: lifecycle_projection.clone(),
                stored_outcome: stored_outcome.clone(),
            },
        }
    }

    fn stored_outcome(
        command_id: &str,
        existing_status: &str,
        stored_outcome: &Value,
        control_claim_disposition: Option<&str>,
    ) -> Self {
        Self {
            contract: BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT,
            command_id: command_id.to_string(),
            already_accepted: true,
            state: replay_state_with_control_claim(
                stored_outcome,
                existing_status,
                control_claim_disposition,
            ),
            control_claim_disposition: control_claim_disposition.map(str::to_string),
            form: BusinessCommandReplayForm::StoredOutcome {
                existing_status: existing_status.to_string(),
                stored_outcome: stored_outcome.clone(),
            },
        }
    }

    fn existing_command(
        command_id: &str,
        existing_status: &str,
        control_claim_disposition: Option<&str>,
    ) -> Self {
        let status_document = serde_json::json!({ "status": existing_status });
        Self {
            contract: BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT,
            command_id: command_id.to_string(),
            already_accepted: true,
            state: replay_state_with_control_claim(
                &status_document,
                existing_status,
                control_claim_disposition,
            ),
            control_claim_disposition: control_claim_disposition.map(str::to_string),
            form: BusinessCommandReplayForm::ExistingCommand {
                existing_status: existing_status.to_string(),
            },
        }
    }

    fn terminal_control_claim(command_id: &str, terminal_status: &str, result: &Value) -> Self {
        Self {
            contract: BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT,
            command_id: command_id.to_string(),
            already_accepted: true,
            state: BusinessCommandReplayState::Terminal,
            control_claim_disposition: Some("terminal".to_string()),
            form: BusinessCommandReplayForm::TerminalControlClaim {
                terminal_status: terminal_status.to_string(),
                result: result.clone(),
            },
        }
    }

    fn uncertain_control_claim(command_id: &str, claim_result: Option<&Value>) -> Self {
        Self {
            contract: BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT,
            command_id: command_id.to_string(),
            already_accepted: true,
            state: BusinessCommandReplayState::Uncertain,
            control_claim_disposition: Some("uncertain".to_string()),
            form: BusinessCommandReplayForm::UncertainControlClaim {
                claim_disposition: "uncertain".to_string(),
                presented_execution_phase: "blocked".to_string(),
                presented_task_status: "blocked".to_string(),
                claim_result: claim_result.cloned().unwrap_or(Value::Null),
            },
        }
    }
}

fn replay_state_with_control_claim(
    document: &Value,
    fallback_status: &str,
    control_claim_disposition: Option<&str>,
) -> BusinessCommandReplayState {
    match control_claim_disposition {
        Some("terminal") => BusinessCommandReplayState::Terminal,
        Some("uncertain") => BusinessCommandReplayState::Uncertain,
        _ => replay_state_from_document(document, fallback_status),
    }
}

fn replay_state_from_document(
    document: &Value,
    fallback_status: &str,
) -> BusinessCommandReplayState {
    if document.get("execution_phase").and_then(Value::as_str) == Some("terminal")
        || document
            .get("terminal_status")
            .and_then(Value::as_str)
            .is_some_and(|status| status != "none")
    {
        return BusinessCommandReplayState::Terminal;
    }
    if let Some(phase) = document.get("execution_phase").and_then(Value::as_str) {
        return match phase {
            "blocked" | "waiting_dependencies" => BusinessCommandReplayState::Blocked,
            "accepted" | "leased" | "running" | "awaiting_review" | "validating" => {
                BusinessCommandReplayState::Running
            }
            _ => replay_state_from_status(
                document
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback_status),
            ),
        };
    }
    replay_state_from_status(
        document
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(fallback_status),
    )
}

fn replay_state_from_status(status: &str) -> BusinessCommandReplayState {
    match status {
        "completed" | "failed" | "cancelled" => BusinessCommandReplayState::Terminal,
        "blocked" | "waiting_dependencies" => BusinessCommandReplayState::Blocked,
        "accepted" | "running" | "queued" | "pending" | "pending_sync" => {
            BusinessCommandReplayState::Running
        }
        _ => BusinessCommandReplayState::Known,
    }
}

fn with_business_command_replay_receipt(
    mut response: Value,
    receipt: BusinessCommandReplayReceipt,
) -> anyhow::Result<Value> {
    let object = response
        .as_object_mut()
        .context("business command replay response must be an object")?;
    object.insert("already_accepted".to_string(), Value::Bool(true));
    object.insert("replay_receipt".to_string(), serde_json::to_value(receipt)?);
    Ok(response)
}

#[cfg(test)]
#[path = "crew_cockpit_command_tests.rs"]
mod crew_cockpit_tests;
#[cfg(test)]
#[path = "crew_identity_command_tests.rs"]
mod crew_identity_tests;

pub(super) const EXACT_CONTROL_TYPES: [&str; 94] = [
    "ctox.crew.member.create",
    "ctox.crew.memory.update",
    "ctox.crew.member.update",
    "ctox.crew.learning.confirm",
    "ctox.crew.learning.update",
    "ctox.crew.learning.delete",
    "ctox.crew.assign",
    "ctox.queue.release",
    "ctox.queue.block",
    "ctox.queue.retry",
    "ctox.queue.capacity",
    "ctox.queue.pause",
    "ctox.queue.abort_turn",
    "ctox.app.access.grant",
    "ctox.app.access.revoke",
    "ctox.app.action.run",
    "ctox.app_store.install",
    "ctox.app_store.uninstall",
    "ctox.business_os.audit.list",
    "ctox.business_os.audit.retention",
    "ctox.business_os.audit.retention_policy.set",
    "ctox.business_os.backup.restore_drill",
    "ctox.business_os.branding.update",
    "ctox.business_os.support.export_diagnostics",
    "ctox.business_os.user.upsert",
    "ctox.business_os.why",
    "ctox.coding.models",
    "ctox.coding.turn",
    "ctox.command.cancel",
    "ctox.file.export",
    "ctox.file.materialize",
    "ctox.mailserver.delete_domain",
    "ctox.mailserver.delete_user",
    "ctox.mailserver.get_config",
    "ctox.mailserver.save_domain",
    "ctox.mailserver.save_runtime",
    "ctox.mailserver.save_user",
    "ctox.maintenance.client_ready",
    "ctox.module.assign_founder",
    "ctox.module.check_updates",
    "ctox.module.delete",
    "ctox.module.install_template",
    "ctox.module.list_versions",
    "ctox.module.release",
    "ctox.module.repair_lifecycle_projection",
    "ctox.module.rollback",
    "ctox.module.rollback_version",
    "ctox.module.save",
    "ctox.module.set_visible",
    "ctox.module.update",
    "ctox.office.settings.save",
    "ctox.provider_subscription.disconnect",
    "ctox.provider_subscription.rotate",
    "ctox.provider_subscription.status",
    "ctox.runtime_settings.save",
    "ctox.secret.delete",
    "ctox.secret.list",
    "ctox.secret.put",
    "ctox.source.commit",
    "ctox.source.diff",
    "ctox.source.list_snapshots",
    "ctox.source.load",
    "ctox.source.log",
    "ctox.source.rollback_snapshot",
    "ctox.source.save",
    "ctox.subscription_auth.start",
    "ctox.task.delete",
    "ctox.task.update",
    "ctox.workjet.computer.assign",
    "ctox.workjet.computer.list",
    "ctox.workjet.computer.unassign",
    "ctox.workjet.project.list",
    "ctox.workjet.project.chat.ensure",
    "ctox.workjet.project.chat.create",
    "ctox.workjet.project.worker.add",
    "ctox.workjet.project.worker.remove",
    "ctox.workjet.worker_profile.bind",
    "ctox.workjet.worker_profile.unbind",
    "ctox.workjet.project.upsert",
    "ctox.workjet.session.create",
    "ctox.workjet.session.delete",
    "ctox.workjet.session.list",
    "ctox.workjet.session.transfer.abort",
    "ctox.workjet.session.transfer.apply_complete",
    "ctox.workjet.session.transfer.confirm_working_copy",
    "ctox.workjet.session.transfer.pack_complete",
    "ctox.workjet.session.transfer.pause_ack",
    "ctox.workjet.session.transfer.resume_ack",
    "ctox.workjet.session.transfer.start",
    "ctox.workjet.session.transfer.status",
    "ctox.workjet.working_copy.upsert",
    "knowledge.command",
    "outbound.lead.research_writeback",
    "web_stack.person_research",
];

thread_local! {
    static COMMAND_TIMING_PROBE: RefCell<Option<CommandTimingProbe>> = RefCell::new(None);
}

struct CommandTimingProbe {
    native_dispatch_entered: i64,
    native_handler_completed: Option<i64>,
    native_rxdb_projection_committed: Option<i64>,
}

struct CommandTimingProbeGuard;

impl Drop for CommandTimingProbeGuard {
    fn drop(&mut self) {
        COMMAND_TIMING_PROBE.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

fn command_timing_probe_requested(command: &BusinessCommand) -> bool {
    command
        .client_context
        .get(COMMAND_TIMING_PROBE_FIELD)
        .and_then(Value::as_bool)
        == Some(true)
}

fn install_command_timing_probe(command: &BusinessCommand) -> Option<CommandTimingProbeGuard> {
    if !command_timing_probe_requested(command) {
        return None;
    }
    COMMAND_TIMING_PROBE.with(|slot| {
        *slot.borrow_mut() = Some(CommandTimingProbe {
            native_dispatch_entered: now_ms() as i64,
            native_handler_completed: None,
            native_rxdb_projection_committed: None,
        });
    });
    Some(CommandTimingProbeGuard)
}

fn mark_command_timing_handler_completed() {
    COMMAND_TIMING_PROBE.with(|slot| {
        if let Some(probe) = slot.borrow_mut().as_mut() {
            if probe.native_handler_completed.is_none() {
                probe.native_handler_completed = Some(now_ms() as i64);
            }
        }
    });
}

fn mark_command_timing_projection_committed() {
    COMMAND_TIMING_PROBE.with(|slot| {
        if let Some(probe) = slot.borrow_mut().as_mut() {
            if probe.native_rxdb_projection_committed.is_none() {
                probe.native_rxdb_projection_committed = Some(now_ms() as i64);
            }
        }
    });
}

fn attach_command_timing_to_result(result: &mut Value) {
    COMMAND_TIMING_PROBE.with(|slot| {
        // Den Borrow an eine Bindung heften: `slot.borrow()` waere sonst ein
        // Temporary, das am Ende des let-else-Statements faellt, waehrend
        // `probe` noch daraus leiht.
        let borrowed = slot.borrow();
        let Some(probe) = borrowed.as_ref() else {
            return;
        };
        let Some(object) = result.as_object_mut() else {
            return;
        };
        let handler_completed = probe
            .native_handler_completed
            .unwrap_or(probe.native_dispatch_entered);
        let projection_committed = probe
            .native_rxdb_projection_committed
            .unwrap_or(handler_completed);
        object.insert(
            COMMAND_TIMING_RESULT_FIELD.to_string(),
            serde_json::json!({
                "native_dispatch_entered": probe.native_dispatch_entered,
                "native_handler_completed": handler_completed,
                "native_rxdb_projection_committed": projection_committed,
            }),
        );
    });
}

/// Accept a command that originated in trusted, in-process code (operator CLI,
/// server-side handlers, internal projections, tests). The claimed actor in
/// `client_context` is trusted. Network-originated commands MUST use
/// [`accept_rxdb_business_command_with_origin`] with [`CommandOrigin::ReplicatedPeer`].
pub fn accept_rxdb_business_command(root: &Path, document: Value) -> anyhow::Result<Value> {
    accept_rxdb_business_command_with_origin(root, document, CommandOrigin::TrustedLocal)
}

fn client_context_has_user_identity(client_context: &Value) -> bool {
    let client_context = if let Value::String(value) = client_context {
        serde_json::from_str(value).unwrap_or_else(|_| client_context.clone())
    } else {
        client_context.clone()
    };
    ["/actor/id", "/owner_user_id", "/user_id"]
        .into_iter()
        .any(|pointer| {
            client_context
                .pointer(pointer)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn stamp_verified_session_identity(
    root: &Path,
    command: &mut BusinessCommand,
    session: &BusinessOsSession,
) {
    let Some(owner_user_id) = session_user_id(session)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let verified_email = rxdb_verified_identity_email(root, command, owner_user_id);
    let display_name = verified_email
        .as_deref()
        .or_else(|| {
            session
                .user
                .as_ref()
                .map(|user| user.display_name.trim())
                .filter(|value| !value.is_empty() && *value != "rxdb-command")
        })
        .unwrap_or(owner_user_id)
        .to_string();
    let mut client_context = if let Value::String(value) = &command.client_context {
        serde_json::from_str(value).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        command.client_context.clone()
    };
    if !client_context.is_object() {
        client_context = serde_json::json!({});
    }
    let had_client_identity = client_context_has_user_identity(&client_context);
    let claimed_actor_id = client_context
        .pointer("/actor/id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let claimed_owner_user_id = client_context
        .get("owner_user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let claimed_user_id = client_context
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let identity_was_overridden = [
        claimed_actor_id.as_deref(),
        claimed_owner_user_id.as_deref(),
        claimed_user_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|claimed| claimed != owner_user_id);
    let first_claimed_id = [
        claimed_actor_id.as_deref(),
        claimed_owner_user_id.as_deref(),
        claimed_user_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .find(|claimed| *claimed != owner_user_id)
    .unwrap_or("<missing>")
    .to_string();

    let object = client_context
        .as_object_mut()
        .expect("client context was normalized to an object");
    object.remove("claimed_actor");
    object.insert(
        "owner_user_id".to_string(),
        Value::String(owner_user_id.to_string()),
    );
    if object.contains_key("user_id") {
        object.insert(
            "user_id".to_string(),
            Value::String(owner_user_id.to_string()),
        );
    }
    if claimed_actor_id.as_deref() == Some(owner_user_id) {
        // Preserve matching actor metadata exactly as supplied.
    } else {
        object.insert(
            "actor".to_string(),
            serde_json::json!({
                "id": owner_user_id,
                "display_name": display_name,
            }),
        );
    }
    if identity_was_overridden && had_client_identity {
        object.insert(
            "claimed_actor".to_string(),
            serde_json::json!({
                "actor": { "id": claimed_actor_id },
                "owner_user_id": claimed_owner_user_id,
                "user_id": claimed_user_id,
            }),
        );
        eprintln!(
            "[business-os] intake overrode client-claimed actor command_id={} claimed_id={} verified_id={}",
            command.id.as_deref().unwrap_or("<missing>"),
            first_claimed_id,
            owner_user_id
        );
    }
    command.client_context = client_context;
}

/// Accept a Business OS command, tagging it with its trust [`CommandOrigin`].
/// `ReplicatedPeer` commands (arriving over the WebRTC/RxDB data plane) cannot
/// authorize a privileged role from the browser-asserted actor; identity/role
/// for those comes only from a verified capability token.
pub fn accept_rxdb_business_command_with_origin(
    root: &Path,
    document: Value,
    origin: CommandOrigin,
) -> anyhow::Result<Value> {
    let command_id = document
        .get("command_id")
        .or_else(|| document.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("business command id is required")?
        .to_string();
    let mut command = BusinessCommand {
        origin,
        id: Some(command_id.clone()),
        module: document
            .get("module")
            .and_then(Value::as_str)
            .unwrap_or("ctox")
            .to_string(),
        command_type: document
            .get("command_type")
            .or_else(|| document.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("business_os.command")
            .to_string(),
        record_id: document
            .get("record_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        payload: document.get("payload").cloned().unwrap_or(Value::Null),
        client_context: document
            .get("client_context")
            .cloned()
            .unwrap_or(Value::Null),
    };
    if matches!(command.origin, CommandOrigin::ReplicatedPeer) {
        let intake_started = command_timing_probe_requested(&command).then(std::time::Instant::now);
        let session = rxdb_authenticated_session(root, &command)?;
        let authentication_ms =
            intake_started.map(|started| started.elapsed().as_secs_f64() * 1_000.0);
        stamp_verified_session_identity(root, &mut command, &session);
        if let Some((started, authentication_ms)) = intake_started.zip(authentication_ms) {
            // This work precedes native_dispatch_entered in the existing
            // roundtrip marks. Keep the budget unchanged and expose the two
            // measured phases only for explicitly requested timing probes.
            eprintln!(
                "command_intake_sample={}",
                serde_json::json!({
                    "command_id": command_id,
                    "authentication_ms": authentication_ms,
                    "identity_stamping_ms":
                        started.elapsed().as_secs_f64() * 1_000.0 - authentication_ms,
                })
            );
        }
    }
    let _command_timing_probe = install_command_timing_probe(&command);
    let native_authorization = recoverable_background_control_claim_authorization(root, &command);
    let control_intent = if is_rxdb_control_command_type(&command.command_type) {
        Some(business_command_core_claim_with_authorization(
            &command_id,
            &command,
            native_authorization.as_ref(),
        )?)
    } else {
        None
    };
    let domain_effect_hash = domain_effect::supports_command(&command.command_type)
        .then(|| {
            control_intent
                .as_ref()
                .map(|claim| claim.payload_hash.clone())
        })
        .flatten();
    let control_claim = control_intent
        .map(|claim| channels::claim_business_control_command(root, claim))
        .transpose()?;
    let _external_sql_execution_guard =
        if super::external_sql_sync::is_external_sql_command(&command.command_type) {
            match ActiveExternalSqlControlCommand::try_acquire(&command_id) {
                Some(guard) => Some(guard),
                None => return external_sql_command_in_flight_outcome(root, &command_id),
            }
        } else {
            None
        };
    let owns_new_control_claim = control_claim
        .as_ref()
        .is_some_and(|claim| claim.disposition == "new");
    let conn = open_store(root)?;
    // Only proof-bearing domain commands bypass the uncertain replay shortcut.
    // Authentication and central policy still run before reading their result.
    let resumes_domain_effect =
        domain_effect_hash.is_some() && domain_effect::contains(&conn, &command_id)?;
    let existing_status: Option<String> = conn
        .query_row(
            "SELECT status FROM business_commands WHERE command_id = ?1",
            params![command_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    let resumes_recoverable_accepted_claim = control_claim
        .as_ref()
        .is_some_and(|claim| claim.disposition == "uncertain")
        && existing_status.as_deref() == Some("accepted")
        && is_recoverable_background_control_command_type(&command.command_type);
    if !owns_new_control_claim
        && !resumes_domain_effect
        && !resumes_recoverable_accepted_claim
        && existing_status.as_deref() != Some("waiting_dependencies")
        && existing_status.is_some()
    {
        if let Some(stored_outcome) = stored_rxdb_business_command_outcome(&conn, &command_id)? {
            if let Ok(mut lifecycle_outcome) =
                channels::business_command_projection(root, &command_id)
            {
                if let Some(object) = lifecycle_outcome.as_object_mut() {
                    let stored_chat_id = stored_outcome
                        .get("chat_id")
                        .or_else(|| {
                            stored_outcome
                                .get("result")
                                .and_then(|result| result.get("chat_id"))
                        })
                        .cloned();
                    if let Some(chat_id) = stored_chat_id {
                        object.insert("chat_id".to_string(), chat_id);
                    }
                    for field in ["outbound_text", "response", "answer", "summary"] {
                        if !object.contains_key(field) {
                            if let Some(value) = stored_outcome.get(field) {
                                object.insert(field.to_string(), value.clone());
                            }
                        }
                    }
                    object.insert("ok".to_string(), Value::Bool(true));
                    object.insert("already_accepted".to_string(), Value::Bool(true));
                }
                persist_business_command_lifecycle_projection(root, &lifecycle_outcome)?;
                let receipt = BusinessCommandReplayReceipt::stored_outcome_with_lifecycle(
                    &command_id,
                    existing_status.as_deref().unwrap_or("known"),
                    &lifecycle_outcome,
                    &stored_outcome,
                    control_claim.as_ref().map(|claim| claim.disposition),
                );
                return with_business_command_replay_receipt(lifecycle_outcome, receipt);
            }
            let receipt = BusinessCommandReplayReceipt::stored_outcome(
                &command_id,
                existing_status.as_deref().unwrap_or("known"),
                &stored_outcome,
                control_claim.as_ref().map(|claim| claim.disposition),
            );
            return with_business_command_replay_receipt(stored_outcome, receipt);
        }
        let receipt = BusinessCommandReplayReceipt::existing_command(
            &command_id,
            existing_status.as_deref().unwrap_or("known"),
            control_claim.as_ref().map(|claim| claim.disposition),
        );
        return with_business_command_replay_receipt(
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "status": "already_accepted"
            }),
            receipt,
        );
    }
    drop(conn);
    let command = if resumes_recoverable_accepted_claim {
        match authorize_recoverable_background_control_command(root, &command) {
            Ok(command) => command,
            Err(error) => {
                return write_rxdb_failed_control_command_outcome(
                    root,
                    &command,
                    "recoverable_control_authorization",
                    error,
                );
            }
        }
    } else {
        command
    };
    if is_rxdb_control_command_type(&command.command_type) {
        let claim = control_claim.context("business control command claim is missing")?;
        match claim.disposition {
            "new" => {}
            _ if resumes_domain_effect => {}
            "terminal" => {
                let terminal_status = claim.terminal_status.as_deref().unwrap_or("completed");
                let result = claim.result.unwrap_or(Value::Null);
                let receipt = BusinessCommandReplayReceipt::terminal_control_claim(
                    &command_id,
                    terminal_status,
                    &result,
                );
                return with_business_command_replay_receipt(
                    serde_json::json!({
                        "ok": terminal_status == "completed",
                        "id": command_id,
                        "command_id": command_id,
                        "status": terminal_status,
                        "execution_mode": "control",
                        "execution_task_id": "",
                        "target_task_id": "",
                        "target_record_id": command.record_id.clone().unwrap_or_default(),
                        "task_id": "",
                        "task_status": terminal_status,
                        "result": result,
                        "already_accepted": true,
                    }),
                    receipt,
                );
            }
            _ if is_recoverable_background_control_command_type(&command.command_type) => {}
            _ => {
                if command.command_type == app_runtime::APP_ACTION_COMMAND_TYPE {
                    if let Some(snapshot) = app_runtime::admitted_snapshot(root, &command_id)? {
                        return Ok(serde_json::json!({
                            "ok": true,
                            "id": command_id,
                            "command_id": command_id,
                            "status": "accepted",
                            "execution_mode": "control",
                            "execution_phase": "accepted",
                            "terminal_status": "none",
                            "task_status": "accepted",
                            "_app_action_snapshot": snapshot,
                            "already_accepted": false,
                            "resumed": true,
                        }));
                    }
                } else {
                    let receipt = BusinessCommandReplayReceipt::uncertain_control_claim(
                        &command_id,
                        claim.result.as_ref(),
                    );
                    return with_business_command_replay_receipt(
                        serde_json::json!({
                            "ok": false,
                            "id": command_id,
                            "command_id": command_id,
                            "status": "accepted",
                            "execution_mode": "control",
                            "execution_task_id": "",
                            "target_task_id": "",
                            "target_record_id": command.record_id.clone().unwrap_or_default(),
                            "task_id": "",
                            "task_status": "blocked",
                            "execution_phase": "blocked",
                            "terminal_status": "none",
                            "error_code": "dependency_missing",
                            "error_message": "control effect was durably claimed but has no terminal outcome; automatic replay is suppressed to prevent a duplicate side effect",
                            "retryable": false,
                            "already_accepted": true,
                        }),
                        receipt,
                    );
                }
            }
        }
    }
    if command.command_type == "web_stack.person_research" && command.module.trim().is_empty() {
        return write_rxdb_failed_control_command_outcome(
            root,
            &command,
            "person_research_authorization",
            anyhow::anyhow!("web_stack.person_research requires a calling module"),
        );
    }

    let mut prepared = PreparedBusinessCommand::from_command(&command)?;
    prepared.domain_effect_hash = domain_effect_hash;
    prepared.owns_new_control_claim = owns_new_control_claim;
    match CommandAuthorizationStage::for_command(&command) {
        CommandAuthorizationStage::Policy(requirement) => {
            match enforce_command_policy(
                root,
                &command,
                |session| requirement.resolve(root, &command, session),
                |session| {
                    dispatch_business_command_with_outcome(
                        root,
                        &command_id,
                        &command,
                        prepared,
                        Some(session),
                    )
                },
            ) {
                Ok(enforced) => enforced.into_outcome(),
                Err(error) if command.command_type == "web_stack.person_research" => {
                    write_rxdb_failed_control_command_outcome(
                        root,
                        &command,
                        "person_research_authorization",
                        error,
                    )
                }
                Err(error) => Err(error),
            }
        }
        CommandAuthorizationStage::AuthenticatedSession => {
            let session = rxdb_authenticated_session(root, &command)?;
            dispatch_business_command_with_outcome(
                root,
                &command_id,
                &command,
                prepared,
                Some(&session),
            )
        }
        CommandAuthorizationStage::CommandSession => {
            let session = rxdb_command_session(root, &command)?;
            dispatch_business_command_with_outcome(
                root,
                &command_id,
                &command,
                prepared,
                Some(&session),
            )
        }
        CommandAuthorizationStage::InDispatch => {
            dispatch_business_command_with_outcome(root, &command_id, &command, prepared, None)
        }
    }
}

enum CommandAuthorizationStage {
    Policy(CentralCommandPolicyRequirement),
    AuthenticatedSession,
    CommandSession,
    InDispatch,
}

impl CommandAuthorizationStage {
    fn for_command(command: &BusinessCommand) -> Self {
        if let Some(requirement) = CentralCommandPolicyRequirement::for_command(command) {
            return Self::Policy(requirement);
        }
        if is_ats_active_command(&command.command_type)
            || super::threads::is_threads_command(&command.command_type)
            || command.command_type.starts_with("ctox.report.")
        {
            return Self::AuthenticatedSession;
        }
        if is_iot_active_command(&command.command_type)
            || is_ats_mutating_command(&command.command_type)
        {
            return Self::CommandSession;
        }
        Self::InDispatch
    }
}

enum CentralCommandPolicyRequirement {
    Fixed(CommandPolicyRequirement),
    ThreadExternalApproval,
    WorkjetSessionDataRead,
    WorkjetSessionDataWrite,
}

impl CentralCommandPolicyRequirement {
    fn for_command(command: &BusinessCommand) -> Option<Self> {
        let command_type = command.command_type.as_str();
        let fixed = if command_type.starts_with("ctox.crew.") {
            Some(CommandPolicyRequirement::scoped(
                BusinessOsPermission::CrewManage,
                super::policy::BusinessOsScope::record(command_type),
            ))
        } else if matches!(command_type, "ctox.queue.capacity" | "ctox.queue.pause") {
            Some(CommandPolicyRequirement::workspace(
                BusinessOsPermission::CtoxTaskManage,
            ))
        } else if matches!(
            command_type,
            "ctox.queue.release"
                | "ctox.queue.block"
                | "ctox.queue.retry"
                | "ctox.queue.abort_turn"
        ) {
            Some(CommandPolicyRequirement::scoped(
                BusinessOsPermission::CtoxTaskManage,
                super::policy::BusinessOsScope::task(
                    command
                        .payload
                        .get("task_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim(),
                    false,
                    false,
                ),
            ))
        } else if command_type == "web_stack.person_research" {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::DataRead,
                &command.module,
            ))
        } else if command_type.starts_with("office.document.") {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::DataWrite,
                "documents",
            ))
        } else if command_type.starts_with("office.spreadsheet.") {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::DataWrite,
                "spreadsheets",
            ))
        } else if is_customers_active_command(command_type) {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::DataWrite,
                "customers",
            ))
        } else if super::external_sql_sync::is_external_sql_command(command_type) {
            Some(CommandPolicyRequirement::module(
                super::external_sql_sync::data_write_permission(),
                &command.module,
            ))
        } else if is_outbound_active_command(command_type) {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::DataWrite,
                "outbound",
            ))
        } else if command_type.starts_with("ctox.channel.") {
            Some(CommandPolicyRequirement::workspace(
                BusinessOsPermission::IntegrationsManage,
            ))
        } else if matches!(
            command_type,
            "ctox.workjet.project.list"
                | "ctox.workjet.computer.list"
                | "ctox.workjet.session.list"
        ) {
            Some(CommandPolicyRequirement::workspace(
                BusinessOsPermission::DataRead,
            ))
        } else if super::project_chats::is_command(command_type) {
            Some(CommandPolicyRequirement::workspace(
                BusinessOsPermission::DataWrite,
            ))
        } else if matches!(
            command_type,
            "ctox.workjet.project.upsert"
                | "ctox.workjet.working_copy.upsert"
                | "ctox.workjet.computer.assign"
                | "ctox.workjet.computer.unassign"
        ) {
            Some(CommandPolicyRequirement::workspace(
                BusinessOsPermission::DataWrite,
            ))
        } else if matches!(
            command_type,
            "ctox.workjet.session.create"
                | "ctox.workjet.session.delete"
                | "ctox.workjet.session.transfer.start"
                | "ctox.workjet.session.transfer.pause_ack"
                | "ctox.workjet.session.transfer.pack_complete"
                | "ctox.workjet.session.transfer.apply_complete"
                | "ctox.workjet.session.transfer.confirm_working_copy"
                | "ctox.workjet.session.transfer.resume_ack"
                | "ctox.workjet.session.transfer.abort"
        ) {
            return Some(Self::WorkjetSessionDataWrite);
        } else if command_type == "ctox.workjet.session.transfer.status" {
            return Some(Self::WorkjetSessionDataRead);
        } else if is_appsec_business_command(command_type) {
            let permission = if appsec_business_command_requires_data_write(command_type) {
                BusinessOsPermission::DataWrite
            } else {
                BusinessOsPermission::DataRead
            };
            Some(CommandPolicyRequirement::module(
                permission,
                APPSEC_MODULE_ID,
            ))
        } else if command_type.starts_with("ctox.ticket.") {
            Some(CommandPolicyRequirement::module(
                BusinessOsPermission::SupportTriage,
                "support",
            ))
        } else if super::support::is_support_command(command_type) {
            Some(CommandPolicyRequirement::module(
                super::support::command_permission(command_type),
                "support",
            ))
        } else {
            None
        };
        if let Some(requirement) = fixed {
            return Some(Self::Fixed(requirement));
        }
        if super::threads::is_threads_command(command_type)
            && super::threads::requires_external_approval(command_type)
        {
            return Some(Self::ThreadExternalApproval);
        }
        if matches!(command.origin, CommandOrigin::ReplicatedPeer)
            && !is_rxdb_control_command_type(command_type)
            && !command_type.starts_with("ctox.report.")
            && app_build_command_policy_target(command).is_none()
        {
            let permission = if command_type == "business_os.context.ask" {
                BusinessOsPermission::DataRead
            } else {
                BusinessOsPermission::DataWrite
            };
            return Some(Self::Fixed(CommandPolicyRequirement::module(
                permission,
                &command.module,
            )));
        }
        None
    }

    fn resolve(
        self,
        root: &Path,
        command: &BusinessCommand,
        session: &BusinessOsSession,
    ) -> anyhow::Result<CommandPolicyRequirement> {
        match self {
            Self::Fixed(requirement) => Ok(requirement),
            Self::WorkjetSessionDataRead => {
                let owner_user_id = session_user_id(session)
                    .context("authorized Workjet session command is missing a user identity")?;
                let owner_email = rxdb_verified_identity_email(root, command, owner_user_id);
                let scope = super::store_workjet_sessions::workjet_session_record_policy_scope(
                    root,
                    command,
                    owner_user_id,
                    owner_email.as_deref(),
                )?;
                Ok(CommandPolicyRequirement::scoped(
                    BusinessOsPermission::DataRead,
                    scope,
                ))
            }
            Self::WorkjetSessionDataWrite => {
                let owner_user_id = session_user_id(session)
                    .context("authorized Workjet session command is missing a user identity")?;
                let owner_email = rxdb_verified_identity_email(root, command, owner_user_id);
                let scope = super::store_workjet_sessions::workjet_session_record_policy_scope(
                    root,
                    command,
                    owner_user_id,
                    owner_email.as_deref(),
                )?;
                Ok(CommandPolicyRequirement::scoped(
                    BusinessOsPermission::DataWrite,
                    scope,
                ))
            }
            Self::ThreadExternalApproval => {
                let approval_id = command
                    .payload
                    .get("approval_request_id")
                    .or_else(|| command.payload.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or(command.record_id.as_deref())
                    .unwrap_or_default()
                    .to_owned();
                let assigned_to_actor = if approval_id.is_empty() {
                    false
                } else {
                    let actor_id = session_user_id(session).unwrap_or_default();
                    pull_collection_record(root, "ctox_task_approval_requests", &approval_id)?
                        .and_then(|approval| first_string_field(&approval, &["reviewer_user_id"]))
                        .map(|reviewer| reviewer == actor_id)
                        .unwrap_or(false)
                };
                Ok(CommandPolicyRequirement::scoped(
                    BusinessOsPermission::ExternalApprove,
                    BusinessOsScope {
                        scope_type: BusinessOsScopeType::Approval,
                        scope_id: if approval_id.is_empty() {
                            None
                        } else {
                            Some(approval_id)
                        },
                        assigned_to_actor,
                        owned_by_actor: false,
                    },
                ))
            }
        }
    }
}

#[derive(Default)]
struct PreparedBusinessCommand {
    channel_mutation: Option<ChannelCommandRequest>,
    report_mutation: Option<BusinessOsReportMutation>,
    domain_effect_hash: Option<String>,
    owns_new_control_claim: bool,
    domain_effect_admission: Option<DomainEffectAdmission>,
}

impl PreparedBusinessCommand {
    fn from_command(command: &BusinessCommand) -> anyhow::Result<Self> {
        let mut prepared = Self::default();
        if command.command_type.starts_with("ctox.channel.") {
            prepared.channel_mutation = Some(
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.channel payload")?,
            );
        }
        if command.command_type.starts_with("ctox.report.") {
            let mut mutation: BusinessOsReportMutation =
                serde_json::from_value(command.payload.clone())
                    .context("invalid ctox.report payload")?;
            if mutation.kind.trim().is_empty() {
                mutation.kind = command
                    .command_type
                    .strip_prefix("ctox.report.")
                    .unwrap_or("bug")
                    .to_string();
            }
            mutation.client_context = command.client_context.clone();
            prepared.report_mutation = Some(mutation);
        }
        Ok(prepared)
    }
}

enum BusinessCommandDispatchOutcome {
    Returned(Value),
    Control {
        status: String,
        task_id: Option<String>,
        task_status: Option<String>,
        result: Value,
    },
    FailedControl {
        task_id: Option<String>,
        result: Value,
        error: anyhow::Error,
    },
}

impl BusinessCommandDispatchOutcome {
    fn completed(result: Value, task_id: Option<String>) -> Self {
        Self::Control {
            status: "completed".to_string(),
            task_id,
            task_status: Some("completed".to_string()),
            result,
        }
    }

    fn failed(task_id: Option<String>, result: Value, error: anyhow::Error) -> Self {
        Self::FailedControl {
            task_id,
            result,
            error,
        }
    }
}

fn dispatch_business_command_with_outcome(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
    mut prepared: PreparedBusinessCommand,
    authorized_session: Option<&BusinessOsSession>,
) -> anyhow::Result<Value> {
    let domain_identity = if let Some(hash) = prepared.domain_effect_hash.as_ref() {
        let actor = authorized_session
            .and_then(session_user_id)
            .context("domain effect recovery requires central authenticated identity")?;
        if let Some(outcome) = recover_applied_domain_effect(root, command, hash, actor)? {
            return Ok(outcome);
        }
        anyhow::ensure!(
            prepared.owns_new_control_claim,
            "uncertain domain command has no applied-effect receipt"
        );
        prepared.domain_effect_admission = Some(DomainEffectAdmission::newly_claimed(
            command_id, hash, actor,
        )?);
        Some((hash.clone(), actor.to_owned()))
    } else {
        None
    };
    let dispatched =
        dispatch_business_command(root, command_id, command, prepared, authorized_session);
    // A handler error after COMMIT must never become terminal failure. Recover
    // the durable result first; publication errors leave the claim recoverable.
    if let Some((hash, actor)) = domain_identity {
        if let Some(outcome) = recover_applied_domain_effect(root, command, &hash, &actor)? {
            return Ok(outcome);
        }
    }
    mark_command_timing_handler_completed();
    write_business_command_dispatch_outcome(root, command, dispatched?)
}

fn authorized_dispatch_session<'a>(
    authorized_session: Option<&'a BusinessOsSession>,
    command_type: &str,
) -> anyhow::Result<&'a BusinessOsSession> {
    authorized_session.with_context(|| {
        format!("central authorization did not provide a session for {command_type}")
    })
}

fn dispatch_business_command(
    root: &Path,
    command_id: &str,
    command: &BusinessCommand,
    mut prepared: PreparedBusinessCommand,
    authorized_session: Option<&BusinessOsSession>,
) -> anyhow::Result<BusinessCommandDispatchOutcome> {
    match command.command_type.as_str() {
        "ctox.crew.member.create"
        | "ctox.crew.member.update"
        | "ctox.crew.memory.update"
        | "ctox.crew.learning.confirm"
        | "ctox.crew.learning.update"
        | "ctox.crew.learning.delete"
        | "ctox.crew.assign" => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            Ok(
                match super::crew_commands::control(root, command, session) {
                    Ok(result) => BusinessCommandDispatchOutcome::completed(result, None),
                    Err(error) => BusinessCommandDispatchOutcome::failed(
                        None,
                        serde_json::json!({"ok":false,"error":error.to_string()}),
                        error,
                    ),
                },
            )
        }
        "ctox.queue.release"
        | "ctox.queue.block"
        | "ctox.queue.retry"
        | "ctox.queue.capacity"
        | "ctox.queue.pause"
        | "ctox.queue.abort_turn" => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            // Finish the durable control claim and replicate its outcome. The
            // target queue task is not an execution task of this control command.
            Ok(
                match super::harness_cockpit::control(root, command, session) {
                    Ok(result)
                        if result.get("status").and_then(Value::as_str) == Some("unsupported") =>
                    {
                        BusinessCommandDispatchOutcome::Control {
                            status: "failed".into(),
                            task_id: None,
                            task_status: Some("failed".into()),
                            result,
                        }
                    }
                    Ok(result) => BusinessCommandDispatchOutcome::completed(result, None),
                    Err(error) => BusinessCommandDispatchOutcome::failed(
                        None,
                        serde_json::json!({"ok":false,"error":error.to_string()}),
                        error,
                    ),
                },
            )
        }
        "ctox.maintenance.client_ready"
        | "ctox.task.update"
        | "ctox.task.delete"
        | "ctox.command.cancel"
        | "ctox.runtime_settings.save"
        | "ctox.office.settings.save"
        | "ctox.coding.models"
        | "ctox.coding.turn"
        | "ctox.file.materialize"
        | "ctox.file.export" => handle_workspace_control_command(root, command)
            .map(BusinessCommandDispatchOutcome::Returned),
        "ctox.app.action.run"
        | "ctox.app.access.grant"
        | "ctox.app.access.revoke"
        | "ctox.app_store.install"
        | "ctox.app_store.uninstall" => handle_app_lifecycle_command(root, command_id, command)
            .map(BusinessCommandDispatchOutcome::Returned),
        "web_stack.person_research" => super::person_research_command::start(root, command.clone())
            .map(BusinessCommandDispatchOutcome::Returned),
        "outbound.lead.research_writeback" => {
            match super::person_research_gap_closure::handle_research_writeback(root, command) {
                Ok(result) => Ok(BusinessCommandDispatchOutcome::completed(
                    result,
                    command
                        .payload
                        .get("gap_task_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                )),
                Err(error) => {
                    let message = error.to_string();
                    Ok(BusinessCommandDispatchOutcome::failed(
                        command
                            .payload
                            .get("gap_task_id")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        serde_json::json!({
                            "ok": false,
                            "error_code": "person_research_writeback_contract",
                            "error": message,
                        }),
                        error,
                    ))
                }
            }
        }
        "knowledge.command" => {
            let args = command
                .payload
                .get("args")
                .and_then(Value::as_array)
                .context("knowledge.command payload.args array is required")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .context("knowledge.command args must be strings")
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let outcome = crate::knowledge::dispatch_capturing(root, &args)?;
            Ok(BusinessCommandDispatchOutcome::completed(outcome, None))
        }
        "ctox.business_os.user.upsert"
        | "ctox.business_os.branding.update"
        | "ctox.business_os.audit.list"
        | "ctox.business_os.audit.retention"
        | "ctox.business_os.audit.retention_policy.set"
        | "ctox.business_os.backup.restore_drill"
        | "ctox.business_os.support.export_diagnostics"
        | "ctox.business_os.why" => {
            handle_business_os_command(root, command).map(BusinessCommandDispatchOutcome::Returned)
        }
        kind if super::project_chats::is_command(kind) => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            let owner = session_user_id(session)
                .context("authorized Workjet chat command is missing a user identity")?;
            match super::project_chats::handle_command(
                root,
                command,
                owner,
                prepared
                    .domain_effect_admission
                    .as_ref()
                    .context("new Workjet chat mutation requires domain admission")?,
            ) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({"ok": false, "error": error.to_string()}),
                    error,
                )),
            }
        }
        "ctox.workjet.project.list"
        | "ctox.workjet.project.upsert"
        | "ctox.workjet.working_copy.upsert" => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            let owner_user_id = session_user_id(session)
                .context("authorized Workjet project command is missing a user identity")?;
            let owner_email = rxdb_verified_identity_email(root, command, owner_user_id);
            match super::store_workjet_projects::handle_workjet_project_store_command(
                root,
                command,
                owner_user_id,
                owner_email.as_deref(),
                prepared.domain_effect_admission.as_ref(),
            ) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        "ctox.workjet.computer.assign"
        | "ctox.workjet.computer.list"
        | "ctox.workjet.computer.unassign" => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            let owner_user_id = session_user_id(session)
                .context("authorized Workjet computer command is missing a user identity")?;
            let owner_email = rxdb_verified_identity_email(root, command, owner_user_id);
            match super::store_workjet_computers::handle_workjet_computer_store_command(
                root,
                command,
                owner_user_id,
                owner_email.as_deref(),
            ) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        "ctox.workjet.session.create"
        | "ctox.workjet.session.list"
        | "ctox.workjet.session.delete"
        | "ctox.workjet.session.transfer.start"
        | "ctox.workjet.session.transfer.pause_ack"
        | "ctox.workjet.session.transfer.pack_complete"
        | "ctox.workjet.session.transfer.apply_complete"
        | "ctox.workjet.session.transfer.confirm_working_copy"
        | "ctox.workjet.session.transfer.resume_ack"
        | "ctox.workjet.session.transfer.abort"
        | "ctox.workjet.session.transfer.status" => {
            let session = authorized_dispatch_session(authorized_session, &command.command_type)?;
            let owner_user_id = session_user_id(session)
                .context("authorized Workjet session command is missing a user identity")?;
            let owner_email = rxdb_verified_identity_email(root, command, owner_user_id);
            let can_manage_all_records = matches!(
                policy_actor_from_session(session).role,
                BusinessOsRole::Chef | BusinessOsRole::Admin
            );
            match super::store_workjet_sessions::handle_workjet_session_store_command(
                root,
                command,
                owner_user_id,
                owner_email.as_deref(),
                can_manage_all_records,
            ) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        "ctox.secret.list"
        | "ctox.secret.put"
        | "ctox.secret.delete"
        | "ctox.provider_subscription.disconnect"
        | "ctox.provider_subscription.rotate"
        | "ctox.provider_subscription.status"
        | "ctox.subscription_auth.start" => {
            handle_secret_command(root, command).map(BusinessCommandDispatchOutcome::Returned)
        }
        "ctox.module.repair_lifecycle_projection"
        | "ctox.module.release"
        | "ctox.module.assign_founder"
        | "ctox.module.save"
        | "ctox.module.delete"
        | "ctox.module.install_template"
        | "ctox.module.update"
        | "ctox.module.set_visible"
        | "ctox.module.check_updates"
        | "ctox.module.rollback"
        | "ctox.module.list_versions"
        | "ctox.module.rollback_version" => {
            handle_module_command(root, command).map(BusinessCommandDispatchOutcome::Returned)
        }
        command_type
            if command_type.starts_with("office.document.")
                || command_type.starts_with("office.spreadsheet.") =>
        {
            match handle_office_control_command(root, command) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(
                    outcome,
                    command.record_id.clone(),
                )),
                Err(error) => {
                    let error_code = if error.to_string().contains("version_conflict") {
                        "version_conflict"
                    } else if error.to_string().contains("feature_dependency_pending") {
                        "feature_dependency_pending"
                    } else {
                        "office_engine_failed"
                    };
                    Ok(BusinessCommandDispatchOutcome::failed(
                        command.record_id.clone(),
                        serde_json::json!({
                            "ok": false,
                            "error_code": error_code,
                            "error": error.to_string(),
                        }),
                        error,
                    ))
                }
            }
        }
        command_type if is_customers_active_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            match handle_customers_active_command(root, session, command) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if super::external_sql_sync::is_external_sql_command(command_type) => {
            match super::external_sql_sync::handle_business_command(root, command) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(
                    outcome,
                    command.record_id.clone(),
                )),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    command.record_id.clone(),
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if is_outbound_active_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let outcome = handle_outbound_active_command(root, session, command_id, command)?;
            Ok(BusinessCommandDispatchOutcome::completed(
                outcome,
                command.record_id.clone(),
            ))
        }
        command_type if is_iot_active_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let outcome = crate::iot::commands::handle_business_command(
                root,
                command_type,
                &command.payload,
                session,
            );
            match outcome {
                Ok(outcome) => {
                    project_iot_business_command_outcome(root, &outcome)
                        .context("project iot business command outcome")?;
                    Ok(BusinessCommandDispatchOutcome::completed(
                        outcome,
                        command.record_id.clone(),
                    ))
                }
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    command.record_id.clone(),
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if is_ats_active_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let outcome = handle_ats_active_command(root, session, command)?;
            Ok(BusinessCommandDispatchOutcome::completed(
                outcome,
                command.record_id.clone(),
            ))
        }
        command_type if is_ats_mutating_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let outcome = handle_ats_mutating_command(root, session, command)?;
            Ok(BusinessCommandDispatchOutcome::completed(
                outcome,
                command.record_id.clone(),
            ))
        }
        command_type if command_type.starts_with("invoices.") => {
            let outcome = (|| -> anyhow::Result<Value> {
                anyhow::ensure!(
                    super::invoices::is_invoices_active_command(command_type),
                    "unsupported invoices command type: {command_type}"
                );
                let session = rxdb_command_session(root, command)?;
                super::invoices::handle_invoices_active_command(root, &session, command)
            })();
            match outcome {
                Ok(value) => Ok(BusinessCommandDispatchOutcome::completed(
                    value,
                    command.record_id.clone(),
                )),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    command.record_id.clone(),
                    serde_json::json!({ "ok": false, "error": error.to_string() }),
                    error,
                )),
            }
        }
        command_type if command_type.starts_with("ctox.channel.") => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let mutation = prepared
                .channel_mutation
                .take()
                .context("ctox.channel mutation was not prepared before authorization")?;
            let outcome = run_channel_command(root, session, command_type, mutation)?;
            Ok(BusinessCommandDispatchOutcome::completed(outcome, None))
        }
        command_type if is_appsec_business_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            match handle_appsec_business_command(root, session, command) {
                Ok(outcome) => {
                    let status = if outcome.get("ok").and_then(Value::as_bool) == Some(false) {
                        "failed"
                    } else {
                        "completed"
                    };
                    Ok(BusinessCommandDispatchOutcome::Control {
                        status: status.to_string(),
                        task_id: command.record_id.clone(),
                        task_status: Some(status.to_string()),
                        result: outcome,
                    })
                }
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    command.record_id.clone(),
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if command_type.starts_with("ctox.ticket.") => {
            let outcome = crate::mission::tickets::run_business_os_ticket_command(
                root,
                command_type,
                &command.payload,
            )?;
            let task_id = outcome
                .get("case_id")
                .or_else(|| outcome.get("ticket_key"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(BusinessCommandDispatchOutcome::completed(outcome, task_id))
        }
        command_type if super::support::is_support_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            match super::support::handle_business_command(root, session, command) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(
                    outcome,
                    command.record_id.clone(),
                )),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    command.record_id.clone(),
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if super::threads::is_threads_command(command_type) => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            match super::threads::handle_business_command(root, session, command) {
                Ok(outcome) => Ok(BusinessCommandDispatchOutcome::completed(outcome, None)),
                Err(error) => Ok(BusinessCommandDispatchOutcome::failed(
                    None,
                    serde_json::json!({
                        "ok": false,
                        "error": error.to_string(),
                    }),
                    error,
                )),
            }
        }
        command_type if command_type.starts_with("ctox.report.") => {
            let session = authorized_dispatch_session(authorized_session, command_type)?;
            let mutation = prepared
                .report_mutation
                .take()
                .context("ctox.report mutation was not prepared before authorization")?;
            let accepted = record_report_command(
                root,
                session,
                mutation,
                Some(command_id.to_string()),
                command.record_id.clone(),
            )?;
            Ok(BusinessCommandDispatchOutcome::Returned(
                serde_json::json!({
                    "ok": true,
                    "id": accepted.command_id,
                    "command_id": accepted.command_id,
                    "status": "accepted",
                    "task_id": accepted.task_id.unwrap_or_default(),
                    "task_status": accepted.task_status.unwrap_or_else(|| "accepted".to_string()),
                    "report_id": accepted.report_id,
                    "report_status": "open"
                }),
            ))
        }
        "ctox.source.load"
        | "ctox.source.save"
        | "ctox.source.list_snapshots"
        | "ctox.source.rollback_snapshot"
        | "ctox.source.commit"
        | "ctox.source.log"
        | "ctox.source.diff" => {
            handle_source_command(root, command).map(BusinessCommandDispatchOutcome::Returned)
        }
        "ctox.mailserver.get_config"
        | "ctox.mailserver.save_domain"
        | "ctox.mailserver.save_runtime"
        | "ctox.mailserver.delete_domain"
        | "ctox.mailserver.save_user"
        | "ctox.mailserver.delete_user" => {
            handle_mailserver_command(root, command).map(BusinessCommandDispatchOutcome::Returned)
        }
        _ => {
            // Legacy native dispatch: these types are outside EXACT_CONTROL_TYPES
            // but still execute immediately and return before record_command.
            // Their inventory classification does not imply durable queue admission.
            if matches!(
                command.command_type.as_str(),
                "kundenpipeline.triage.write"
                    | "kundenpipeline.decision.request"
                    | "kundenpipeline.decision.resolve"
                    | "kundenpipeline.decision.answer"
                    | "kundenpipeline.mail.send"
                    | "kundenpipeline.delegate"
            ) {
                return super::decision_hub::handle_command(root, command_id, command)
                    .map(BusinessCommandDispatchOutcome::Returned);
            }
            let accepted = record_command(root, command.clone())?;
            Ok(BusinessCommandDispatchOutcome::Returned(
                serde_json::to_value(accepted)?,
            ))
        }
    }
}

fn write_business_command_dispatch_outcome(
    root: &Path,
    command: &BusinessCommand,
    dispatched: BusinessCommandDispatchOutcome,
) -> anyhow::Result<Value> {
    match dispatched {
        BusinessCommandDispatchOutcome::Returned(value) => Ok(value),
        BusinessCommandDispatchOutcome::Control {
            status,
            task_id,
            task_status,
            result,
        } => write_rxdb_control_command_outcome(
            root,
            command,
            &status,
            task_id.as_deref(),
            task_status.as_deref(),
            result,
        ),
        BusinessCommandDispatchOutcome::FailedControl {
            task_id,
            result,
            error,
        } => Err(write_failed_control_command_outcome_observably(
            root,
            command,
            task_id.as_deref(),
            result,
            error,
        )),
    }
}

pub(super) fn write_rxdb_control_command_outcome(
    root: &Path,
    command: &BusinessCommand,
    status: &str,
    task_id: Option<&str>,
    task_status: Option<&str>,
    result: Value,
) -> anyhow::Result<Value> {
    write_rxdb_control_command_state(root, command, status, task_id, task_status, result, true)
}

fn write_failed_control_command_outcome_observably(
    root: &Path,
    command: &BusinessCommand,
    task_id: Option<&str>,
    result: Value,
    command_error: anyhow::Error,
) -> anyhow::Error {
    match write_rxdb_control_command_outcome(
        root,
        command,
        "failed",
        task_id,
        Some("failed"),
        result,
    ) {
        Ok(_) => command_error,
        Err(outcome_error) => {
            let command_id = command.id.as_deref().unwrap_or("<missing>");
            let command_type = command.command_type.as_str();
            eprintln!(
                "[business-os] failed to write failed control command outcome: \
                 command_id={command_id} command_type={command_type} \
                 outcome_error={outcome_error:#}; command_error={command_error:#}"
            );
            command_error.context(format!(
                "failed to write failed control command outcome for \
                 command_id={command_id} command_type={command_type}: {outcome_error:#}"
            ))
        }
    }
}

pub(super) fn write_rxdb_control_command_progress(
    root: &Path,
    command: &BusinessCommand,
    status: &str,
    result: Value,
) -> anyhow::Result<Value> {
    write_rxdb_control_command_state(root, command, status, None, Some(status), result, false)
}

fn write_rxdb_control_command_state(
    root: &Path,
    command: &BusinessCommand,
    status: &str,
    task_id: Option<&str>,
    task_status: Option<&str>,
    mut result: Value,
    terminal: bool,
) -> anyhow::Result<Value> {
    let command_id = command.id.as_deref().context("command id is required")?;
    if terminal
        && status != "completed"
        && domain_effect::supports_command(&command.command_type)
        && domain_effect::contains(&open_store(root)?, command_id)?
    {
        anyhow::bail!("applied domain effect cannot be terminalized as a failed mutation");
    }
    mark_command_timing_handler_completed();
    let now = now_ms() as i64;
    let target_task_id = if command.command_type.starts_with("ctox.task.") {
        task_id.unwrap_or_default()
    } else {
        ""
    };
    let target_record_id = command
        .record_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            (!command.command_type.starts_with("ctox.task."))
                .then_some(task_id)
                .flatten()
        })
        .unwrap_or_default();
    if let Some(object) = result.as_object_mut() {
        object
            .entry("status".to_string())
            .or_insert_with(|| Value::String(status.to_string()));
        object.insert(
            "task_status".to_string(),
            Value::String(task_status.unwrap_or(status).to_string()),
        );
    }
    if !terminal && is_rxdb_control_command_type(&command.command_type) {
        channels::progress_business_control_command(root, command_id, status, &result)?;
    }
    let canonical_terminal_status =
        (terminal && is_rxdb_control_command_type(&command.command_type)).then(|| match status {
            "completed" => "completed",
            "cancelled" => "cancelled",
            _ => "failed",
        });
    let conn = open_store(root)?;
    conn.execute(
        "INSERT INTO business_commands
            (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(command_id) DO UPDATE SET
            module = excluded.module,
            command_type = excluded.command_type,
            record_id = excluded.record_id,
            status = excluded.status,
            payload_json = excluded.payload_json,
            client_context_json = excluded.client_context_json,
            observed_at_ms = excluded.observed_at_ms",
        params![
            command_id,
            command.module,
            command.command_type,
            command.record_id.clone().unwrap_or_default(),
            status,
            serde_json::to_string(&command.payload)?,
            serde_json::to_string(&command.client_context)?,
            now
        ],
    )?;
    let chat_id = if is_business_chat_command(command) {
        Some(materialize_control_business_chat_state(
            root, &conn, command_id, command, status, &result, terminal, now,
        )?)
    } else {
        None
    };
    let mut projection = serde_json::json!({
        "id": command_id,
        "command_id": command_id,
        "module": command.module.clone(),
        "command_type": command.command_type.clone(),
        "record_id": command.record_id.clone().unwrap_or_default(),
        "status": status,
        "execution_mode": "control",
        "execution_task_id": "",
        "target_task_id": target_task_id,
        "target_record_id": target_record_id,
        "inbound_channel": command_inbound_channel(command),
        "task_id": task_id.unwrap_or_default(),
        "task_status": task_status.unwrap_or(status),
        "payload": command.payload.clone(),
        "client_context": command.client_context.clone(),
        "result": result.clone(),
        "error_code": result.get("error_code").cloned().unwrap_or(Value::Null),
        "error_message": result.get("error").cloned().unwrap_or(Value::Null),
        "updated_at_ms": now
    });
    if let Some(chat_id) = chat_id.as_deref() {
        projection["chat_id"] = Value::String(chat_id.to_string());
    }
    upsert_business_record(
        &conn,
        "business_commands",
        command_id,
        now,
        projection.clone(),
    )?;
    record_business_module_lifecycle_event(root, command, status, &result)?;
    // Publish terminal state and its native timing marks in one RxDB write.  A
    // second timing-only write races terminal observers and makes the sample
    // nondeterministic.  The mark is taken immediately before the durable
    // projection call, so its sub-write skew is bounded by that single call.
    mark_command_timing_projection_committed();
    if command_timing_probe_requested(command) {
        attach_command_timing_to_result(&mut result);
        projection["result"] = result.clone();
    }
    // Both writes belong to this command. Retain its writer until canonical
    // completion instead of reopening and inspecting the entire RxDB schema.
    let mut projection_writers = RxdbProjectionWriterCache::new(root);
    let projection_started = command_timing_probe_requested(command).then(std::time::Instant::now);
    projection_writers.upsert("business_commands", command_id, now, projection)?;
    if let Some(started) = projection_started {
        let sample = serde_json::json!({
            "command_id": command_id,
            "initial_rxdb_projection_ms": started.elapsed().as_secs_f64() * 1_000.0,
        })
        .to_string();
        eprintln!("command_initial_projection_sample={sample}");
    }
    // Readers treat the canonical terminal transition as a completion barrier.
    // Publish it only after the chat and all local/RxDB projections are durable.
    if let Some(terminal_status) = canonical_terminal_status {
        complete_and_project_business_control_command(
            root,
            command_id,
            terminal_status,
            &result,
            (terminal_status == "failed")
                .then(|| {
                    result
                        .get("error")
                        .or_else(|| result.pointer("/outcome/stderr"))
                        .and_then(Value::as_str)
                })
                .flatten(),
            &mut projection_writers,
        )?;
    }
    Ok(serde_json::json!({
        "ok": true,
        "id": command_id,
        "command_id": command_id,
        "status": status,
        "execution_mode": "control",
        "execution_task_id": "",
        "target_task_id": target_task_id,
        "target_record_id": target_record_id,
        "task_id": task_id.unwrap_or_default(),
        "task_status": task_status.unwrap_or(status),
        "error_code": result.get("error_code").cloned().unwrap_or(Value::Null),
        "error_message": result.get("error").cloned().unwrap_or(Value::Null),
        "chat_id": chat_id,
        "result": result
    }))
}

pub(super) fn complete_and_project_business_control_command(
    root: &Path,
    command_id: &str,
    terminal_status: &str,
    result: &Value,
    error_message: Option<&str>,
    projection_writers: &mut RxdbProjectionWriterCache,
) -> anyhow::Result<()> {
    let completion_started = COMMAND_TIMING_PROBE
        .with(|slot| slot.borrow().is_some())
        .then(std::time::Instant::now);
    let mut delay_ms = 10_u64;
    for attempt in 0..6 {
        match channels::complete_business_control_command(
            root,
            command_id,
            terminal_status,
            result,
            error_message,
        ) {
            Ok(()) => break,
            Err(error)
                if attempt < 5
                    && super::rxdb_peer_intake_state::is_transient_business_command_store_error(
                        &error,
                    ) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                delay_ms = delay_ms.saturating_mul(2);
            }
            Err(error) => return Err(error),
        }
    }

    // The core transition is the completion barrier. Mirror its canonical
    // lifecycle document synchronously afterwards so a transient outbox lag
    // cannot leave readers with status=completed but terminal_status=none.
    let core_completed_ms =
        completion_started.map(|started| started.elapsed().as_secs_f64() * 1_000.0);
    let canonical = channels::business_command_projection(root, command_id)?;
    let canonical_read_ms =
        completion_started.map(|started| started.elapsed().as_secs_f64() * 1_000.0);
    persist_business_command_lifecycle_projection(root, &canonical)?;
    let local_projected_ms =
        completion_started.map(|started| started.elapsed().as_secs_f64() * 1_000.0);
    let updated_at_ms = canonical
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| now_ms() as i64);
    projection_writers.upsert("business_commands", command_id, updated_at_ms, canonical)?;
    if let Some((((started, core_ms), read_ms), local_ms)) = completion_started
        .zip(core_completed_ms)
        .zip(canonical_read_ms)
        .zip(local_projected_ms)
    {
        // A browser can observe the first RxDB projection while this serial
        // intake is still completing the canonical claim. Measure that tail
        // separately; never subtract it from the end-to-end command budget.
        let sample = serde_json::json!({
            "command_id": command_id,
            "core_completion_ms": core_ms,
            "canonical_read_ms": read_ms - core_ms,
            "local_projection_ms": local_ms - read_ms,
            "rxdb_projection_ms": started.elapsed().as_secs_f64() * 1_000.0 - local_ms,
        })
        .to_string();
        eprintln!("command_terminal_sample={sample}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::store::{
        business_command_core_claim, issue_business_os_capability_token_for_managed_user,
        load_rxdb_collection_record, rxdb_store_path,
    };
    use super::super::store_projections::tests::create_repair_rxdb_tables;
    use super::super::store_workjet_projects::tests::create_workjet_rxdb_projection_tables;
    use super::super::store_workjet_sessions::tests::create_workjet_session_rxdb_projection_tables;
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn intake_stamps_verified_identity_and_preserves_claimed_actor_evidence() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        let owner_user_id = "michael.welsch@metric-space.ai";
        let (capability_token, _) =
            super::super::store::issue_business_os_capability_token_for_managed_user_with_email(
                root.path(),
                owner_user_id,
                Some(owner_user_id),
                "Michael Welsch",
                "admin",
                now_ms() as i64,
            )?;
        let accept = |command_id: &str, client_context: Value| {
            accept_rxdb_business_command_with_origin(
                root.path(),
                serde_json::json!({
                    "id": command_id,
                    "command_id": command_id,
                    "module": "research",
                    "command_type": "business_os.chat.task",
                    "payload": {
                        "title": "Identity intake test",
                        "instruction": "Verify the persisted command identity."
                    },
                    "client_context": client_context,
                }),
                CommandOrigin::ReplicatedPeer,
            )
        };
        let load_context = |command_id: &str| -> anyhow::Result<Value> {
            let conn = open_store(root.path())?;
            let raw = conn.query_row(
                "SELECT client_context_json FROM business_commands WHERE command_id = ?1",
                params![command_id],
                |row| row.get::<_, String>(0),
            )?;
            Ok(serde_json::from_str(&raw)?)
        };

        let missing_id = "cmd_missing_identity";
        accept(
            missing_id,
            serde_json::json!({
                "source": "business-os-chat",
                "capability_token": capability_token,
            }),
        )?;
        let stamped = load_context(missing_id)?;
        assert_eq!(
            stamped.get("owner_user_id").and_then(Value::as_str),
            Some(owner_user_id)
        );
        assert_eq!(
            stamped.pointer("/actor/id").and_then(Value::as_str),
            Some(owner_user_id)
        );
        assert_eq!(
            stamped
                .pointer("/actor/display_name")
                .and_then(Value::as_str),
            Some(owner_user_id)
        );

        let foreign_id = "cmd_foreign_identity";
        accept(
            foreign_id,
            serde_json::json!({
                "source": "business-os-chat",
                "capability_token": capability_token,
                "actor": { "id": "attacker", "display_name": "Claimed User" },
                "owner_user_id": "foreign-owner",
                "user_id": "foreign-user"
            }),
        )?;
        let overridden = load_context(foreign_id)?;
        assert_eq!(
            overridden.pointer("/actor/id").and_then(Value::as_str),
            Some(owner_user_id)
        );
        assert_eq!(
            overridden.get("owner_user_id").and_then(Value::as_str),
            Some(owner_user_id)
        );
        assert_eq!(
            overridden.get("user_id").and_then(Value::as_str),
            Some(owner_user_id)
        );
        assert_eq!(
            overridden
                .pointer("/claimed_actor/actor/id")
                .and_then(Value::as_str),
            Some("attacker")
        );
        assert_eq!(
            overridden
                .pointer("/claimed_actor/owner_user_id")
                .and_then(Value::as_str),
            Some("foreign-owner")
        );
        assert_eq!(
            overridden
                .pointer("/claimed_actor/user_id")
                .and_then(Value::as_str),
            Some("foreign-user")
        );

        let matching_id = "cmd_matching_identity";
        accept(
            matching_id,
            serde_json::json!({
                "source": "business-os-chat",
                "capability_token": capability_token,
                "actor": { "id": owner_user_id, "display_name": "Existing User" },
                "owner_user_id": owner_user_id,
                "user_id": owner_user_id
            }),
        )?;
        let matching = load_context(matching_id)?;
        assert_eq!(
            matching
                .pointer("/actor/display_name")
                .and_then(Value::as_str),
            Some("Existing User")
        );
        assert!(matching.get("claimed_actor").is_none());
        Ok(())
    }

    fn rust_brace_depth_after(mut depth: usize, line: &str) -> usize {
        let mut quoted = false;
        let mut escaped = false;
        let mut chars = line.chars().peekable();
        while let Some(character) = chars.next() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quoted = false;
                }
                continue;
            }
            if character == '/' && chars.peek() == Some(&'/') {
                break;
            }
            match character {
                '"' => quoted = true,
                '{' => depth += 1,
                '}' => depth = depth.checked_sub(1).expect("unbalanced Rust braces"),
                _ => {}
            }
        }
        depth
    }

    fn dispatcher_exact_control_types(source: &str) -> BTreeSet<String> {
        let function_start = source
            .find("fn dispatch_business_command(")
            .expect("dispatch_business_command must exist");
        let match_start = source[function_start..]
            .find("match command.command_type.as_str()")
            .map(|offset| function_start + offset)
            .expect("command dispatcher match must exist");
        let fallback = source[match_start..]
            .find("        _ => {")
            .map(|offset| match_start + offset)
            .expect("command dispatcher fallback must exist");
        let classifier = &source[match_start..fallback];
        let mut lines = classifier.lines();
        let match_line = lines
            .next()
            .expect("command dispatcher match must have a body");
        let mut brace_depth = rust_brace_depth_after(0, match_line);
        let mut exact_types = BTreeSet::new();
        let mut arm_pattern = String::new();

        for line in lines {
            let trimmed = line.trim();
            if brace_depth == 1 && (trimmed.starts_with('"') || !arm_pattern.is_empty()) {
                if !arm_pattern.is_empty() {
                    arm_pattern.push(' ');
                }
                arm_pattern.push_str(trimmed);
                if arm_pattern.contains("=>") {
                    let pattern = arm_pattern
                        .split_once("=>")
                        .expect("literal dispatcher arm must contain =>")
                        .0;
                    exact_types.extend(pattern.split('"').skip(1).step_by(2).map(str::to_string));
                    arm_pattern.clear();
                }
            }
            brace_depth = rust_brace_depth_after(brace_depth, line);
        }

        assert!(
            arm_pattern.is_empty(),
            "unterminated string-literal dispatcher arm: {arm_pattern}"
        );
        exact_types
    }

    #[test]
    fn workjet_command_documents_project_into_native_rxdb_before_return() -> anyhow::Result<()> {
        let root = tempdir()?;
        drop(create_repair_rxdb_tables(root.path())?);
        create_workjet_rxdb_projection_tables(root.path())?;

        let project_id = "project-command-plane-e2e";
        let project_command_id = "cmd-workjet-project-command-plane-e2e";
        let working_copy_command_id = "cmd-workjet-working-copy-command-plane-e2e";
        let actor = json!({
            "id": "owner-1",
            "role": "admin",
            "is_admin": true,
            "display_name": "owner-1",
            "email": "",
            "login": ""
        });
        let project_document = json!({
            "id": project_command_id,
            "command_id": project_command_id,
            "inbound_channel": "ctox",
            "contract_version": 2,
            "idempotency_key": project_command_id,
            "status": "pending_sync",
            "module": "ctox",
            "command_type": "ctox.workjet.project.upsert",
            "record_id": project_id,
            "payload": {
                "project_id": project_id,
                "name": "Command Plane Project",
                "inbound_channel": "ctox"
            },
            "client_context": {
                "source": "workjet-project-control",
                "inbound_channel": "ctox",
                "dispatch_transport": "rxdb-command-bus",
                "actor": actor.clone()
            }
        });
        let working_copy_document = json!({
            "id": working_copy_command_id,
            "command_id": working_copy_command_id,
            "inbound_channel": "ctox",
            "contract_version": 2,
            "idempotency_key": working_copy_command_id,
            "status": "pending_sync",
            "module": "ctox",
            "command_type": "ctox.workjet.working_copy.upsert",
            "record_id": project_id,
            "payload": {
                "project_id": project_id,
                "computer_id": "computer-command-plane-e2e",
                "path": "/workspace/command-plane-e2e",
                "active": true,
                "inbound_channel": "ctox"
            },
            "client_context": {
                "source": "workjet-project-control",
                "inbound_channel": "ctox",
                "dispatch_transport": "rxdb-command-bus",
                "actor": actor
            }
        });

        let started = Instant::now();
        let project_outcome = accept_rxdb_business_command(root.path(), project_document.clone())?;
        let working_copy_outcome =
            accept_rxdb_business_command(root.path(), working_copy_document.clone())?;
        let elapsed = started.elapsed();
        assert!(
            elapsed <= Duration::from_secs(5),
            "Workjet command-plane projection took {elapsed:?}, expected at most 5s"
        );
        assert_eq!(project_outcome["result"]["project"]["id"], project_id);

        let project = load_rxdb_collection_record(root.path(), "workjet_projects", project_id)?
            .context("missing native RxDB workjet_projects row after command return")?;
        assert_eq!(project["name"], "Command Plane Project");
        assert_eq!(project["status"], "active");
        assert_eq!(project["owner_user_id"], "owner-1");
        assert_eq!(project["is_deleted"], false);

        let working_copy_id = working_copy_outcome["result"]["working_copy"]["id"]
            .as_str()
            .context("working-copy command outcome has no result.working_copy.id")?;
        assert!(working_copy_id.starts_with("workjet_wc_"));
        let working_copy =
            load_rxdb_collection_record(root.path(), "workjet_working_copies", working_copy_id)?
                .context("missing native RxDB workjet_working_copies row after command return")?;
        assert_eq!(working_copy["project_id"], project_id);
        assert_eq!(working_copy["computer_id"], "computer-command-plane-e2e");
        assert_eq!(working_copy["path"], "/workspace/command-plane-e2e");
        assert_eq!(working_copy["status"], "active");
        assert_eq!(working_copy["owner_user_id"], "owner-1");

        for command_id in [project_command_id, working_copy_command_id] {
            let command =
                load_rxdb_collection_record(root.path(), "business_commands", command_id)?
                    .with_context(|| {
                        format!("missing business_commands projection for {command_id}")
                    })?;
            assert_eq!(command["inbound_channel"], "ctox");
        }

        accept_rxdb_business_command(root.path(), project_document)?;
        accept_rxdb_business_command(root.path(), working_copy_document)?;

        let replayed_project =
            load_rxdb_collection_record(root.path(), "workjet_projects", project_id)?
                .context("project projection disappeared after identical command replay")?;
        assert_eq!(replayed_project["id"], project["id"]);
        assert_eq!(replayed_project["updated_at_ms"], project["updated_at_ms"]);
        let replayed_working_copy =
            load_rxdb_collection_record(root.path(), "workjet_working_copies", working_copy_id)?
                .context("working-copy projection disappeared after identical command replay")?;
        assert_eq!(replayed_working_copy["id"], working_copy["id"]);
        assert_eq!(
            replayed_working_copy["updated_at_ms"],
            working_copy["updated_at_ms"]
        );
        Ok(())
    }

    #[test]
    fn record_owned_data_write_workjet_session_owner_allowed_foreign_denied_and_admin_allowed(
    ) -> anyhow::Result<()> {
        let root = tempdir()?;
        drop(create_repair_rxdb_tables(root.path())?);
        create_workjet_rxdb_projection_tables(root.path())?;
        create_workjet_session_rxdb_projection_tables(root.path())?;
        let issued_at_ms = now_ms() as i64;
        issue_business_os_capability_token_for_managed_user(
            root.path(),
            "session-owner",
            "Session Owner",
            "admin",
            issued_at_ms,
        )?;
        issue_business_os_capability_token_for_managed_user(
            root.path(),
            "foreign-user",
            "Foreign User",
            "user",
            issued_at_ms,
        )?;
        issue_business_os_capability_token_for_managed_user(
            root.path(),
            "admin-operator",
            "Admin Operator",
            "admin",
            issued_at_ms,
        )?;
        let owner_actor = json!({
            "id": "session-owner",
            "role": "admin",
            "is_admin": true,
            "display_name": "Session Owner",
            "email": "",
            "login": ""
        });
        let project = json!({
            "id": "cmd-record-owned-project",
            "command_id": "cmd-record-owned-project",
            "module": "ctox",
            "command_type": "ctox.workjet.project.upsert",
            "record_id": "record-owned-project",
            "payload": {
                "project_id": "record-owned-project",
                "name": "Record-owned Project"
            },
            "client_context": { "actor": owner_actor.clone() }
        });
        accept_rxdb_business_command(root.path(), project)?;
        let copy = accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd-record-owned-copy",
                "command_id": "cmd-record-owned-copy",
                "module": "ctox",
                "command_type": "ctox.workjet.working_copy.upsert",
                "payload": {
                    "project_id": "record-owned-project",
                    "computer_id": "record-owned-computer",
                    "path": "opaque://record-owned/project",
                    "active": true
                },
                "client_context": { "actor": owner_actor }
            }),
        )?;
        let working_copy_id = copy["result"]["working_copy"]["id"]
            .as_str()
            .context("working copy id")?;
        issue_business_os_capability_token_for_managed_user(
            root.path(),
            "session-owner",
            "Session Owner",
            "user",
            issued_at_ms,
        )?;
        let owner_create = accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd-record-owned-session-create",
                "command_id": "cmd-record-owned-session-create",
                "module": "ctox",
                "command_type": "ctox.workjet.session.create",
                "payload": {
                    "project_id": "record-owned-project",
                    "working_copy_id": working_copy_id
                },
                "client_context": {
                    "actor": {
                        "id": "session-owner",
                        "role": "user",
                        "is_admin": false,
                        "display_name": "Session Owner",
                        "email": "",
                        "login": ""
                    }
                }
            }),
        )?;
        assert_eq!(owner_create["ok"], true);
        assert_eq!(
            owner_create["result"]["session"]["owner_user_id"],
            "session-owner"
        );
        let session_id = owner_create["result"]["session"]["id"]
            .as_str()
            .context("session id")?;
        let projected = load_rxdb_collection_record(root.path(), "workjet_sessions", session_id)?
            .context("session projection must exist before command return")?;
        assert_eq!(projected["id"], session_id);
        assert_eq!(projected["owner_user_id"], "session-owner");

        let foreign_delete = accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd-record-owned-session-foreign-delete",
                "command_id": "cmd-record-owned-session-foreign-delete",
                "module": "ctox",
                "command_type": "ctox.workjet.session.delete",
                "record_id": session_id,
                "payload": {},
                "client_context": {
                    "actor": {
                        "id": "foreign-user",
                        "role": "user",
                        "is_admin": false,
                        "display_name": "Foreign User",
                        "email": "",
                        "login": ""
                    }
                }
            }),
        )?;
        assert_eq!(foreign_delete["ok"], false);
        assert_eq!(
            foreign_delete["result"]["policy_decision"]["reason_code"],
            "role_or_scope_denied"
        );

        let admin_delete = accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd-record-owned-session-admin-delete",
                "command_id": "cmd-record-owned-session-admin-delete",
                "module": "ctox",
                "command_type": "ctox.workjet.session.delete",
                "record_id": session_id,
                "payload": {},
                "client_context": {
                    "actor": {
                        "id": "admin-operator",
                        "role": "admin",
                        "is_admin": true,
                        "display_name": "Admin Operator",
                        "email": "",
                        "login": ""
                    }
                }
            }),
        )?;
        assert_eq!(admin_delete["ok"], true);
        assert_eq!(admin_delete["result"]["session"]["is_deleted"], true);
        Ok(())
    }

    #[test]
    fn already_accepted_replay_receipt_distinguishes_all_five_forms() -> anyhow::Result<()> {
        let command_id = "cmd_replay_receipt_forms";
        let stored_outcome = serde_json::json!({
            "id": command_id,
            "command_id": command_id,
            "status": "running",
            "result": { "progress": 3 },
            "stored_only": "preserved"
        });
        let lifecycle_projection = serde_json::json!({
            "id": command_id,
            "command_id": command_id,
            "status": "accepted",
            "execution_phase": "running",
            "terminal_status": "none",
            "projection_only": "preserved"
        });
        let terminal_result = serde_json::json!({ "ok": true, "value": 42 });
        let uncertain_result = serde_json::json!({ "status": "running", "progress": 1 });

        let receipts = [
            serde_json::to_value(BusinessCommandReplayReceipt::stored_outcome_with_lifecycle(
                command_id,
                "running",
                &lifecycle_projection,
                &stored_outcome,
                Some("uncertain"),
            ))?,
            serde_json::to_value(BusinessCommandReplayReceipt::stored_outcome(
                command_id,
                "running",
                &stored_outcome,
                None,
            ))?,
            serde_json::to_value(BusinessCommandReplayReceipt::existing_command(
                command_id, "accepted", None,
            ))?,
            serde_json::to_value(BusinessCommandReplayReceipt::terminal_control_claim(
                command_id,
                "completed",
                &terminal_result,
            ))?,
            serde_json::to_value(BusinessCommandReplayReceipt::uncertain_control_claim(
                command_id,
                Some(&uncertain_result),
            ))?,
        ];

        let forms = receipts
            .iter()
            .map(|receipt| receipt["form"]["kind"].as_str().unwrap_or_default())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            forms,
            BTreeSet::from([
                "existing_command",
                "stored_outcome",
                "stored_outcome_with_lifecycle",
                "terminal_control_claim",
                "uncertain_control_claim",
            ]),
            "every former replay response shape needs a distinct typed form"
        );
        assert_eq!(receipts[0]["state"], "uncertain");
        assert_eq!(receipts[0]["control_claim_disposition"], "uncertain");
        assert_eq!(receipts[1]["state"], "running");
        assert_eq!(receipts[2]["state"], "running");
        assert_eq!(receipts[3]["state"], "terminal");
        assert_eq!(receipts[4]["state"], "uncertain");
        assert_ne!(
            receipts[3]["state"], receipts[4]["state"],
            "terminal and uncertain are load-bearing distinct replay states"
        );
        assert_eq!(
            receipts[0]["form"]["lifecycle_projection"]["projection_only"],
            "preserved"
        );
        assert_eq!(
            receipts[0]["form"]["stored_outcome"]["stored_only"],
            "preserved"
        );
        assert_eq!(
            receipts[1]["form"]["stored_outcome"]["stored_only"],
            "preserved"
        );
        assert_eq!(receipts[2]["form"]["existing_status"], "accepted");
        assert_eq!(receipts[3]["form"]["result"], terminal_result);
        assert_eq!(receipts[4]["form"]["claim_result"], uncertain_result);
        assert_eq!(receipts[4]["form"]["presented_execution_phase"], "blocked");

        let wrapped = with_business_command_replay_receipt(
            serde_json::json!({
                "id": command_id,
                "command_id": command_id,
                "status": "already_accepted",
                "legacy_only": "preserved"
            }),
            BusinessCommandReplayReceipt::existing_command(command_id, "accepted", None),
        )?;
        assert_eq!(wrapped["legacy_only"], "preserved");
        assert_eq!(wrapped["status"], "already_accepted");
        assert_eq!(wrapped["already_accepted"], true);
        assert_eq!(
            wrapped["replay_receipt"]["contract"],
            BUSINESS_COMMAND_REPLAY_RECEIPT_CONTRACT
        );
        Ok(())
    }

    #[test]
    fn business_command_inventory_matches_exact_control_types() {
        let dispatcher_types = dispatcher_exact_control_types(include_str!("command_plane.rs"));
        let declared_types = EXACT_CONTROL_TYPES
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_types.len(),
            EXACT_CONTROL_TYPES.len(),
            "EXACT_CONTROL_TYPES contains duplicates"
        );

        let dispatcher_only = dispatcher_types
            .difference(&declared_types)
            .cloned()
            .collect::<Vec<_>>();
        let constant_only = declared_types
            .difference(&dispatcher_types)
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            dispatcher_only.is_empty() && constant_only.is_empty(),
            "EXACT_CONTROL_TYPES and the dispatcher's string-literal arms differ; \
             dispatcher_only={dispatcher_only:?}, constant_only={constant_only:?}"
        );
    }

    #[test]
    fn control_command_outcome_updates_outbox_intake_projection() -> anyhow::Result<()> {
        let root = tempdir()?;
        let command_id = "cmd_appsec_outbox_race";
        drop(create_repair_rxdb_tables(root.path())?);
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(command_id.to_string()),
            module: APPSEC_MODULE_ID.to_string(),
            command_type: "ctox.appsec.tools.doctor".to_string(),
            record_id: Some("runtime/appsec/test".to_string()),
            payload: serde_json::json!({ "profile": "full" }),
            client_context: serde_json::json!({ "actor": { "id": "local-dev" } }),
        };

        let claim = channels::claim_business_control_command(
            root.path(),
            business_command_core_claim(command_id, &command)?,
        )?;
        assert_eq!(claim.disposition, "new");

        let conn = open_store(root.path())?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, 1)",
            params![
                command_id,
                command.module,
                command.command_type,
                command.record_id,
                serde_json::to_string(&command.payload)?,
                serde_json::to_string(&command.client_context)?,
            ],
        )?;
        drop(conn);

        write_rxdb_control_command_progress(
            root.path(),
            &command,
            "running",
            serde_json::json!({ "ok": true, "status": "running" }),
        )?;

        let outcome = write_rxdb_control_command_outcome(
            root.path(),
            &command,
            "completed",
            command.record_id.as_deref(),
            Some("completed"),
            serde_json::json!({ "ok": true }),
        )?;
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );

        let conn = open_store(root.path())?;
        let (count, status): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(status) FROM business_commands WHERE command_id = ?1",
            params![command_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(count, 1);
        assert_eq!(status, "completed");
        let rxdb_conn = Connection::open(rxdb_store_path(root.path()))?;
        let projected: String = rxdb_conn.query_row(
            "SELECT data FROM ctox_business_os__business_commands__v1 WHERE id = ?1",
            params![command_id],
            |row| row.get(0),
        )?;
        let projected: Value = serde_json::from_str(&projected)?;
        assert_eq!(projected["status"], "completed");
        assert_eq!(projected["task_status"], "completed");
        assert_eq!(projected["result"]["status"], "completed");
        assert_eq!(projected["result"]["task_status"], "completed");
        Ok(())
    }

    #[test]
    fn terminal_control_command_settles_linked_leased_queue_task_without_repair(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let business_store = open_store(root)?;
        let rxdb = create_repair_rxdb_tables(root)?;

        for (suffix, terminal_status, expected_route, error_message, reject_projection) in [
            ("success", "completed", "handled", None, false),
            (
                "failure",
                "failed",
                "failed",
                Some("synthetic terminal failure"),
                false,
            ),
            ("projection-rollback", "completed", "leased", None, true),
        ] {
            let command_id = format!("cmd_terminal_linked_queue_{suffix}");
            let command = BusinessCommand {
                origin: CommandOrigin::TrustedLocal,
                id: Some(command_id.clone()),
                module: "research".to_string(),
                command_type: "business_os.chat.task".to_string(),
                record_id: Some("research".to_string()),
                payload: serde_json::json!({ "prompt": "prove atomic terminal carry" }),
                client_context: serde_json::json!({ "source": "test" }),
            };
            let claimed = channels::claim_business_command_with_queue(
                root,
                business_command_core_claim(&command_id, &command)?,
                channels::QueueTaskCreateRequest {
                    title: format!("Terminal carry {suffix}"),
                    prompt: "prove atomic terminal carry".to_string(),
                    thread_key: format!("business-os/tests/terminal-carry/{suffix}"),
                    workspace_root: Some(root.display().to_string()),
                    priority: "normal".to_string(),
                    suggested_skill: None,
                    parent_message_key: None,
                    extra_metadata: Some(serde_json::json!({
                        "command_id": command_id,
                    })),
                },
            )?;
            let task_id = claimed.task.message_key;
            channels::lease_queue_task(root, &task_id, "ctox-test")?;
            channels::transition_business_command_for_task(
                root,
                &task_id,
                "leased",
                None,
                None,
                None,
                "test worker leased linked task",
            )?;

            let initial_projection = serde_json::json!({
                "id": task_id,
                "command_id": command_id,
                "status": "running",
                "route_status": "leased",
            });
            upsert_business_record(
                &business_store,
                "ctox_queue_tasks",
                &task_id,
                1,
                initial_projection.clone(),
            )?;
            rxdb.execute(
                "INSERT INTO ctox_business_os__ctox_queue_tasks__v0(id, data) VALUES(?1, ?2)",
                params![task_id, serde_json::to_string(&initial_projection)?],
            )?;
            if reject_projection {
                rxdb.execute_batch(
                    "CREATE TRIGGER reject_linked_queue_projection
                     BEFORE UPDATE ON ctox_business_os__ctox_queue_tasks__v0
                     BEGIN SELECT RAISE(ABORT, 'injected linked queue projection failure'); END;",
                )?;
            }
            let completion = channels::complete_business_control_command(
                root,
                &command_id,
                terminal_status,
                &serde_json::json!({
                    "ok": terminal_status == "completed",
                    "status": terminal_status,
                }),
                error_message,
            );
            if reject_projection {
                let error = completion.expect_err("a failed projection must abort completion");
                assert!(format!("{error:#}").contains("injected linked queue projection failure"));
                let canonical = channels::business_command_projection(root, &command_id)?;
                assert_eq!(canonical["execution_phase"], "leased");
                assert_eq!(canonical["terminal_status"], "none");
            } else {
                completion?;
            }

            let task = channels::load_queue_task(root, &task_id)?
                .context("linked queue task must remain loadable after terminalization")?;
            assert_eq!(
                task.route_status, expected_route,
                "terminal command must settle its canonical queue task without repair"
            );
            if reject_projection {
                assert_eq!(task.lease_owner.as_deref(), Some("ctox-test"));
                assert!(task.leased_at.is_some());
            } else {
                assert!(task.lease_owner.is_none());
                assert!(task.leased_at.is_none());
            }
            let local_projection: String = business_store.query_row(
                "SELECT payload_json FROM business_records WHERE collection = 'ctox_queue_tasks' AND record_id = ?1",
                [&task_id], |row| row.get(0),
            )?;
            let replicated_projection: String = rxdb.query_row(
                "SELECT data FROM ctox_business_os__ctox_queue_tasks__v0 WHERE id = ?1",
                [&task_id],
                |row| row.get(0),
            )?;
            for raw in [local_projection, replicated_projection] {
                let projection: Value = serde_json::from_str(&raw)?;
                assert_eq!(projection["route_status"], expected_route);
                if reject_projection {
                    assert_eq!(projection["status"], "running");
                }
            }
        }

        Ok(())
    }

    #[test]
    fn failed_outcome_write_is_observable() {
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: None,
            module: "invoices".to_string(),
            command_type: "invoices.invoice.create".to_string(),
            record_id: Some("invoice-observability-test".to_string()),
            payload: serde_json::json!({}),
            client_context: serde_json::json!({}),
        };

        let error = write_failed_control_command_outcome_observably(
            Path::new("unused-because-the-command-id-is-missing"),
            &command,
            command.record_id.as_deref(),
            serde_json::json!({ "ok": false, "error": "handler failed after mutation" }),
            anyhow::anyhow!("handler failed after mutation"),
        );
        let observable = format!("{error:#}");
        assert!(observable.contains("failed to write failed control command outcome"));
        assert!(observable.contains("command_id=<missing>"));
        assert!(observable.contains("command_type=invoices.invoice.create"));
        assert!(observable.contains("command id is required"));
        assert!(observable.contains("handler failed after mutation"));

        let source = include_str!("command_plane.rs");
        let ignored_write = ["let _ = ", "write_rxdb_control_command_outcome("].concat();
        assert!(
            !source.contains(&ignored_write),
            "failed outcome writes must reach the observable helper"
        );
    }

    #[test]
    fn control_command_outcome_updates_an_existing_intake_projection() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let command_id = "cmd_long_running_control_outcome";
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(command_id.to_string()),
            module: "inventory".to_string(),
            command_type: "external_sql.sync.refresh".to_string(),
            record_id: None,
            payload: serde_json::json!({ "input": { "mode": "full" } }),
            client_context: serde_json::json!({ "actor": { "id": "mcp-admin" } }),
        };

        let claim = channels::claim_business_control_command(
            root,
            business_command_core_claim(command_id, &command)?,
        )?;
        assert_eq!(claim.disposition, "new");

        let conn = open_store(root)?;
        conn.execute(
            "INSERT INTO business_commands
                (command_id, module, command_type, record_id, status, payload_json, client_context_json, observed_at_ms)
             VALUES (?1, 'inventory', 'external_sql.sync.refresh', '', 'accepted', '{}', '{}', 1)",
            params![command_id],
        )?;
        drop(conn);

        let outcome = write_rxdb_control_command_outcome(
            root,
            &command,
            "completed",
            None,
            Some("completed"),
            serde_json::json!({ "ok": true, "synced": 1 }),
        )?;
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );

        let conn = open_store(root)?;
        let (count, status): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(status) FROM business_commands WHERE command_id = ?1",
            params![command_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(count, 1);
        assert_eq!(status, "completed");
        Ok(())
    }

    #[test]
    fn command_timing_probe_is_absent_without_explicit_marker() -> anyhow::Result<()> {
        let root = tempdir()?;
        let command_id = "cmd_timing_probe_off";
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(command_id.to_string()),
            module: "ctox".to_string(),
            command_type: "ctox.provider_subscription.status".to_string(),
            record_id: None,
            payload: serde_json::json!({}),
            client_context: serde_json::json!({ "actor": { "id": "local-dev" } }),
        };
        channels::claim_business_control_command(
            root.path(),
            business_command_core_claim(command_id, &command)?,
        )?;
        let outcome = write_rxdb_control_command_outcome(
            root.path(),
            &command,
            "completed",
            None,
            Some("completed"),
            serde_json::json!({ "ok": true }),
        )?;
        assert!(outcome.get(COMMAND_TIMING_RESULT_FIELD).is_none());
        assert!(outcome["result"].get(COMMAND_TIMING_RESULT_FIELD).is_none());
        Ok(())
    }

    #[test]
    fn command_timing_probe_writes_ordered_native_marks() -> anyhow::Result<()> {
        let root = tempdir()?;
        let rxdb = create_repair_rxdb_tables(root.path())?;
        super::super::store::reset_rxdb_collection_writer_open_count(
            root.path(),
            "business_commands",
        );
        let command_id = "cmd_timing_probe_on";
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(command_id.to_string()),
            module: "ctox".to_string(),
            command_type: "ctox.provider_subscription.status".to_string(),
            record_id: None,
            payload: serde_json::json!({}),
            client_context: serde_json::json!({
                "actor": { "id": "local-dev" },
                "command_timing_probe": true
            }),
        };
        channels::claim_business_control_command(
            root.path(),
            business_command_core_claim(command_id, &command)?,
        )?;
        let _guard = install_command_timing_probe(&command);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let outcome = write_rxdb_control_command_outcome(
            root.path(),
            &command,
            "completed",
            None,
            Some("completed"),
            serde_json::json!({ "ok": true }),
        )?;
        let timing = outcome
            .get("result")
            .and_then(|value| value.get(COMMAND_TIMING_RESULT_FIELD))
            .cloned()
            .unwrap_or(Value::Null);
        let entered = timing["native_dispatch_entered"].as_i64().unwrap();
        let completed = timing["native_handler_completed"].as_i64().unwrap();
        let committed = timing["native_rxdb_projection_committed"].as_i64().unwrap();
        assert!(entered > 0);
        assert!(completed >= entered);
        assert!(committed >= completed);
        assert!(timing.get("capability_token").is_none());
        assert!(timing.get("payload").is_none());
        assert_eq!(
            super::super::store::rxdb_collection_writer_open_count(
                root.path(),
                "business_commands"
            ),
            1,
            "one command must retain its projection writer through canonical completion",
        );
        let persisted: String = rxdb.query_row(
            "SELECT data FROM ctox_business_os__business_commands__v1 WHERE id = ?1",
            [command_id],
            |row| row.get(0),
        )?;
        let persisted: Value = serde_json::from_str(&persisted)?;
        assert_eq!(persisted["execution_phase"], "terminal");
        assert_eq!(persisted["terminal_status"], "completed");
        Ok(())
    }

    #[test]
    fn control_completion_without_queue_link_does_not_open_queue_projections() -> anyhow::Result<()>
    {
        let root = tempdir()?;
        drop(open_store(root.path())?); // Register the real projection hooks.
        let command_id = "cmd_core_only_completion";
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(command_id.into()),
            module: "ctox".into(),
            command_type: "ctox.provider_subscription.status".into(),
            record_id: None,
            payload: serde_json::json!({}),
            client_context: serde_json::json!({}),
        };
        channels::claim_business_control_command(
            root.path(),
            business_command_core_claim(command_id, &command)?,
        )?;
        let projection_path = rxdb_store_path(root.path());
        std::fs::create_dir_all(projection_path.parent().unwrap())?;
        std::fs::write(&projection_path, b"unavailable queue projection database")?;
        channels::complete_business_control_command(
            root.path(),
            command_id,
            "completed",
            &serde_json::json!({"ok": true}),
            None,
        )?;
        let canonical = channels::business_command_projection(root.path(), command_id)?;
        assert_eq!(canonical["terminal_status"], "completed");
        // Completion stays durable/idempotent even when unrelated queue
        // projection storage cannot be opened. Delivery still has its outbox.
        let replay = channels::claim_business_control_command(
            root.path(),
            business_command_core_claim(command_id, &command)?,
        )?;
        assert_eq!(replay.disposition, "terminal");
        Ok(())
    }
}
