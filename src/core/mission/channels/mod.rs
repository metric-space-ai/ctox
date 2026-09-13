mod account_helpers;
mod outbound_review;
use crate::communication_store::parse_string_json_array;
pub(crate) use crate::communication_store::{
    now_iso_string, open_channel_db, preview_text, refresh_thread, refresh_thread_tx,
    upsert_communication_message, upsert_communication_message_tx, UpsertMessage,
};
use account_helpers::*;
pub(crate) use account_helpers::{
    record_communication_sync_run, stable_digest, upsert_communication_account,
};
use outbound_review::{
    cached_queue_task_count, cached_queue_task_list, channel_projection_tables_exist,
    empty_business_os_projection, enforce_external_chat_send_is_reviewed,
    enforce_external_work_ack_has_pipeline_backing,
    enforce_reviewed_communication_send_core_transition_if_approved,
    external_chat_action_from_send_request, founder_reply_sent_after_review,
    guard_founder_handled_ack, mark_founder_reply_review_sent, open_channel_db_read_only,
    parse_send_request, parse_tui_ingest_request, queue_task_count_cache_key,
    queue_task_list_cache_key, queue_task_list_cache_stamp,
    require_any_unconsumed_external_chat_review, resolve_outbound_subject, send_email_message,
    send_reviewed_email_communication_request, send_reviewed_founder_outbound_request, sha256_hex,
    store_queue_task_count_cache, store_queue_task_list_cache, test_channel,
    thread_prefers_voice_reply, validate_founder_outbound_email,
};
#[cfg(test)]
pub(crate) use outbound_review::{
    channel_db_open_count_for_tests, record_channel_db_open_for_tests,
    reset_channel_db_open_count_for_tests,
};
#[cfg(test)]
use outbound_review::{
    channel_open_routing_ensure_count_for_tests, channel_schema_ensure_count_for_tests,
    derive_founder_reply_recipients, detect_required_founder_deliverables,
    emit_reviewed_founder_send_failed_transition, emit_reviewed_founder_send_succeeded_transition,
    enforce_reviewed_founder_send_core_transition, ensure_founder_outbound_body_clean,
    founder_outbound_review_digest, load_message_addressing_from_conn, load_message_from_conn,
    mark_outbound_send_accepted, mark_outbound_send_attempt_started, mark_outbound_send_failed,
    message_has_terminal_no_send_in_conn, message_metadata_marks_auto_submitted,
    pending_send_message_key, queue_task_count_cache_miss_count_for_tests,
    queue_task_list_cache_miss_count_for_tests, record_outbound_pending_send,
    record_queue_task_count_cache_miss_for_tests, record_queue_task_list_cache_miss_for_tests,
    require_any_unconsumed_founder_outbound_review, require_unconsumed_founder_reply_review,
    reviewed_outbound_evidence, stranded_outbound_send_attempt, update_pending_send_to_accepted,
    update_pending_send_to_failed,
};
pub(crate) use outbound_review::{
    default_email_account_key, ensure_founder_outbound_body_text_clean,
    ensure_founder_reply_deliverables_present, is_reviewed_external_chat_channel,
    prepare_reviewed_external_chat_reply, prepare_reviewed_founder_reply,
    record_and_send_external_chat_escalation_reply, record_and_send_founder_escalation_reply,
    record_external_chat_review_approval, record_founder_outbound_review_approval,
    record_founder_reply_review_approval, required_founder_reply_deliverables,
    reviewed_send_result_has_durable_outbound_artifact, send_reviewed_external_chat_action,
    send_reviewed_founder_outbound_action, terminal_founder_outbound_artifact_count,
};
pub(crate) use outbound_review::{ensure_open_routing_rows_once, ensure_schema_once};
pub use outbound_review::{
    inbound_message_has_terminal_no_send, inbound_message_is_auto_submitted,
    record_terminal_no_send_verdict, send_reviewed_founder_reply,
};

mod auth_assist;
pub(crate) use auth_assist::recover_auth_assist_requests;
mod command_saga;
use command_saga::transition_business_command_for_task_in_transaction;
mod route_status;
pub(crate) use command_saga::{
    audit_and_migrate_business_command_storage, business_command_core_diagnostics,
    business_command_projection, business_command_projection_from_conn,
    business_command_retention_maintenance, business_command_saga_pending_compensation_steps,
    business_command_saga_status, business_command_saga_step_evidence,
    claim_business_command_saga_step, claim_business_command_waiting_dependencies,
    claim_business_command_with_queue, claim_business_control_command,
    complete_business_command_saga_step, complete_business_control_command,
    fail_business_command_saga_step, inspect_business_command, inspect_business_command_for_task,
    inspect_business_command_for_task_from_conn, mark_business_command_outbox_delivered,
    mark_business_command_outbox_failed, pending_business_command_outbox,
    persist_business_command_worker_result, progress_business_control_command,
    reconcile_business_command_invariants, record_business_command_applied_effect_delivery_failure,
    record_business_command_intake_failure, record_business_command_review,
    record_business_command_saga_step_evidence, resolve_business_command_intake_failures,
    retry_failed_app_create_business_command, runtime_business_command_action_snapshot,
    start_business_command_saga, start_runtime_business_command_saga,
    transition_business_command_for_task,
};
pub(crate) use route_status::QueueRouteStatus;

mod business_os_projection;
pub(crate) use business_os_projection::communication_intake_source_stamp;
use business_os_projection::non_negative_i64_to_usize;
pub use business_os_projection::{
    disconnect_communication_account_for_business_os, export_jami_archive_for_business_os,
    list_communication_accounts_for_business_os, pull_communication_accounts_for_business_os,
    pull_communication_accounts_for_business_os_after, pull_communication_messages_for_business_os,
    pull_communication_messages_for_business_os_after, pull_communication_record_for_business_os,
    pull_communication_threads_for_business_os, pull_communication_threads_for_business_os_after,
    read_pairing_state_for_business_os, save_channel_settings_for_business_os,
    start_pairing_for_business_os, sync_channel_for_business_os, test_channel_for_business_os,
};

use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use qrcode::types::Color as QrColor;
use qrcode::QrCode;
use rusqlite::params;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::communication::adapters as communication_adapters;
use crate::communication::adapters::CommunicationTransportAdapter;
use crate::communication::gateway as communication_gateway;
use crate::core_state::guard::{
    enforce_core_spawn, enforce_core_spawn_in_transaction, enforce_core_transition,
    ensure_core_transition_guard_schema, evaluate_core_spawn, CoreSpawnProof, CoreSpawnRequest,
};
use crate::core_state::{
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
};
use crate::mission::review::HoldReason;
use crate::secrets;
use crate::service::harness_flow::{
    record_harness_flow_event_lossy, RecordHarnessFlowEventRequest,
};

const DEFAULT_TAKE_LIMIT: usize = 10;
const QUEUE_CHANNEL_NAME: &str = "queue";
const QUEUE_ACCOUNT_KEY: &str = "queue:system";
const QUEUE_ACCOUNT_ADDRESS: &str = "ctox queue";
const QUEUE_PROVIDER: &str = "system";
const QUEUE_SENDER_DISPLAY: &str = "CTOX queue";
const QUEUE_SENDER_ADDRESS: &str = "queue:system";
static REVIEWED_FOUNDER_SEND_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

type QueueProjectionAttachHook = fn(&Path, &Connection) -> Result<()>;
type QueueProjectionRefreshHook = fn(&Path, &Connection, &[QueueTaskView]) -> Result<()>;

#[derive(Clone, Copy)]
struct QueueProjectionHooks {
    attach: QueueProjectionAttachHook,
    refresh: QueueProjectionRefreshHook,
}

static QUEUE_PROJECTION_HOOKS: OnceLock<QueueProjectionHooks> = OnceLock::new();
static CHANNEL_SCHEMA_READY: OnceLock<Mutex<HashSet<ChannelSchemaCacheKey>>> = OnceLock::new();
static CHANNEL_OPEN_ROUTING_READY: OnceLock<
    Mutex<BTreeMap<ChannelSchemaCacheKey, ChannelRoutingCacheStamp>>,
> = OnceLock::new();
static QUEUE_TASK_LIST_CACHE: OnceLock<
    Mutex<BTreeMap<QueueTaskListCacheKey, QueueTaskListCacheEntry>>,
> = OnceLock::new();
static QUEUE_TASK_COUNT_CACHE: OnceLock<
    Mutex<BTreeMap<QueueTaskCountCacheKey, QueueTaskCountCacheEntry>>,
> = OnceLock::new();

const QUEUE_TASK_LIST_CACHE_MAX_ENTRIES: usize = 256;
const QUEUE_TASK_COUNT_CACHE_MAX_ENTRIES: usize = 256;

type ChannelFileChangeStamp = (u64, u128);
type ChannelRoutingCacheStamp = (u64, u64, u64);

#[derive(Debug, Clone, PartialEq, Eq)]
enum QueueTaskListCacheStamp {
    ProjectionClock {
        database_exists: bool,
        clock_exists: bool,
        version: i64,
        message_count: usize,
        routing_count: usize,
        updated_at: String,
    },
    File {
        main: ChannelFileChangeStamp,
        wal: ChannelFileChangeStamp,
        journal: ChannelFileChangeStamp,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct QueueTaskListCacheKey {
    database: ChannelSchemaCacheKey,
    statuses: Vec<String>,
    limit: usize,
}

#[derive(Debug, Clone)]
struct QueueTaskListCacheEntry {
    stamp: QueueTaskListCacheStamp,
    tasks: Vec<QueueTaskView>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct QueueTaskCountCacheKey {
    database: ChannelSchemaCacheKey,
    statuses: Vec<String>,
}

#[derive(Debug, Clone)]
struct QueueTaskCountCacheEntry {
    stamp: QueueTaskListCacheStamp,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommunicationIntakeSourceStamp {
    database_exists: bool,
    accounts_table_exists: bool,
    threads_table_exists: bool,
    messages_table_exists: bool,
    routing_table_exists: bool,
    projection_version: i64,
    account_count: usize,
    latest_account_updated_at_ms: i64,
    thread_count: usize,
    latest_thread_updated_at_ms: i64,
    message_count: usize,
    latest_message_updated_at_ms: i64,
    routing_count: usize,
    clock_updated_at: String,
    content_hash: String,
    /// Earliest `retry_not_before` among durable queue routes still in
    /// `pending`, RFC 3339, or empty. The idle router gates compare this
    /// against the clock: a retry hold that expires changes nothing else in
    /// the stamp, and without it a task held after a transient API failure
    /// waited for the 3600 s safety poll instead of its 300 s backoff
    /// (skf.ctox.dev, 2026-09-02).
    earliest_pending_retry_not_before: String,
}

impl CommunicationIntakeSourceStamp {
    pub(crate) fn earliest_pending_retry_not_before(&self) -> &str {
        &self.earliest_pending_retry_not_before
    }
}

const COMMUNICATION_INTAKE_SOURCE_STAMP_SQL: &str = r#"
    SELECT
        version,
        account_count,
        thread_count,
        message_count,
        routing_count,
        updated_at
    FROM communication_projection_clock
    WHERE id = 1
"#;

#[cfg(test)]
static CHANNEL_SCHEMA_ENSURE_COUNTS: OnceLock<Mutex<BTreeMap<ChannelSchemaCacheKey, usize>>> =
    OnceLock::new();

#[cfg(unix)]
type ChannelSchemaCacheKey = (PathBuf, u64, u64);
#[cfg(not(unix))]
type ChannelSchemaCacheKey = PathBuf;

#[cfg(test)]
static CHANNEL_OPEN_ROUTING_ENSURE_COUNTS: OnceLock<Mutex<BTreeMap<ChannelSchemaCacheKey, usize>>> =
    OnceLock::new();

#[cfg(test)]
static QUEUE_TASK_LIST_CACHE_MISS_COUNTS: OnceLock<Mutex<BTreeMap<QueueTaskListCacheKey, usize>>> =
    OnceLock::new();

#[cfg(test)]
static QUEUE_TASK_COUNT_CACHE_MISS_COUNTS: OnceLock<
    Mutex<BTreeMap<QueueTaskCountCacheKey, usize>>,
> = OnceLock::new();
#[cfg(test)]
static CHANNEL_DB_OPEN_CALL_COUNTS: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct QueueTaskView {
    pub message_key: String,
    pub thread_key: String,
    pub title: String,
    pub prompt: String,
    pub workspace_root: Option<String>,
    pub ticket_self_work_id: Option<String>,
    pub priority: String,
    pub suggested_skill: Option<String>,
    pub parent_message_key: Option<String>,
    pub metadata: Value,
    pub route_status: String,
    pub status_note: Option<String>,
    pub lease_owner: Option<String>,
    pub leased_at: Option<String>,
    pub acked_at: Option<String>,
    pub created_at: String,
    pub sort_at: String,
    pub updated_at: String,
    pub lease_expires_at: Option<String>,
    pub lease_worker_id: Option<String>,
    pub first_pending_at: Option<String>,
    pub failure_class: Option<String>,
    pub failure_attempt_count: i64,
    pub retry_not_before: Option<String>,
    pub hold_reason: Option<String>,
    pub wait_entity_type: Option<String>,
    pub wait_entity_id: Option<String>,
    pub priority_time_credit_hours: i64,
    pub attempt: i64,
    pub crew_member_id: Option<String>,
    pub crew_assigned_member_id: Option<String>,
}

pub(crate) fn register_queue_projection_hooks(
    attach: QueueProjectionAttachHook,
    refresh: QueueProjectionRefreshHook,
) {
    let _ = QUEUE_PROJECTION_HOOKS.set(QueueProjectionHooks { attach, refresh });
}

fn attach_queue_projection_store(root: &Path, conn: &Connection) -> Result<bool> {
    let Some(hooks) = QUEUE_PROJECTION_HOOKS.get().copied() else {
        return Ok(false);
    };
    (hooks.attach)(root, conn)?;
    Ok(true)
}

fn refresh_queue_projection_tasks(
    root: &Path,
    conn: &Connection,
    tasks: &[QueueTaskView],
) -> Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }
    if let Some(hooks) = QUEUE_PROJECTION_HOOKS.get().copied() {
        (hooks.refresh)(root, conn, tasks)?;
    }
    crate::business_os::harness_cockpit::schedule_refresh(root);
    Ok(())
}

fn load_queue_projection_tasks(
    conn: &Connection,
    message_keys: &[String],
) -> Result<Vec<QueueTaskView>> {
    let mut tasks = Vec::new();
    for message_key in message_keys {
        if let Some(task) = load_queue_task_from_conn(conn, message_key)? {
            tasks.push(task);
        }
    }
    Ok(tasks)
}

/// Control-plane assignment updates the existing queue projection through the
/// same hooks as queue transitions; it never changes queue status or admission.
pub(crate) fn refresh_queue_assignment_projection(root: &Path, task: &str) -> Result<()> {
    let conn = open_channel_db(&crate::paths::core_db(root))?;
    attach_queue_projection_store(root, &conn)?;
    let tasks = load_queue_projection_tasks(&conn, &[task.to_string()])?;
    refresh_queue_projection_tasks(root, &conn, &tasks)
}

#[derive(Debug, Clone)]
pub(crate) struct BusinessCommandClaimRequest {
    pub command_id: String,
    pub idempotency_key: String,
    pub payload_hash: String,
    pub module: String,
    pub command_type: String,
    pub record_id: String,
    pub intent: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct BusinessCommandQueueClaim {
    pub task: QueueTaskView,
    pub already_claimed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct BusinessCommandControlClaim {
    pub disposition: &'static str,
    pub result: Option<Value>,
    pub terminal_status: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct BusinessCommandOutboxEvent {
    pub event_id: String,
    pub command_id: String,
    pub projection_version: i64,
    pub destination: String,
    pub event_type: String,
    pub attempts: u32,
}

#[derive(Debug, Clone)]
pub struct QueueTaskCreateRequest {
    pub title: String,
    pub prompt: String,
    pub thread_key: String,
    pub workspace_root: Option<String>,
    pub priority: String,
    pub suggested_skill: Option<String>,
    pub parent_message_key: Option<String>,
    pub extra_metadata: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalPolicyGrantKind {
    BusinessCommandReviewedTerminalSuccess,
    BusinessOsAppValidationPassed,
    AppSecPipelineStageCompleted,
    MeetingScheduled,
    MeetingPassiveMention,
    HistoricalAutoSubmittedInbound,
    SystemProbeInbound,
    RoutingBackfillNonWork,
}

/// Capability minted only by code paths that have established the matching
/// terminal policy condition. Actor and reason remain audit data and cannot
/// create this grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalPolicyGrant(TerminalPolicyGrantKind);

impl TerminalPolicyGrant {
    fn business_command_reviewed_terminal_success() -> Self {
        Self(TerminalPolicyGrantKind::BusinessCommandReviewedTerminalSuccess)
    }

    pub(crate) fn business_os_app_validation_passed() -> Self {
        Self(TerminalPolicyGrantKind::BusinessOsAppValidationPassed)
    }

    pub(crate) fn appsec_pipeline_stage_completed() -> Self {
        Self(TerminalPolicyGrantKind::AppSecPipelineStageCompleted)
    }

    pub(crate) fn meeting_scheduled() -> Self {
        Self(TerminalPolicyGrantKind::MeetingScheduled)
    }

    pub(crate) fn meeting_passive_mention() -> Self {
        Self(TerminalPolicyGrantKind::MeetingPassiveMention)
    }

    fn historical_auto_submitted_inbound() -> Self {
        Self(TerminalPolicyGrantKind::HistoricalAutoSubmittedInbound)
    }

    fn system_probe_inbound() -> Self {
        Self(TerminalPolicyGrantKind::SystemProbeInbound)
    }

    fn routing_backfill_non_work() -> Self {
        Self(TerminalPolicyGrantKind::RoutingBackfillNonWork)
    }

    fn proof(self) -> &'static str {
        match self.0 {
            TerminalPolicyGrantKind::BusinessCommandReviewedTerminalSuccess => {
                "policy:business-command-reviewed-terminal-success"
            }
            TerminalPolicyGrantKind::BusinessOsAppValidationPassed => {
                "policy:business-os-app-validation-terminal-success"
            }
            TerminalPolicyGrantKind::AppSecPipelineStageCompleted => {
                "policy:appsec-pipeline-stage-terminal-success"
            }
            TerminalPolicyGrantKind::MeetingScheduled => {
                "policy:meeting-scheduled-terminal-no-send"
            }
            TerminalPolicyGrantKind::MeetingPassiveMention => {
                "policy:meeting-passive-inbound-terminal-no-send"
            }
            TerminalPolicyGrantKind::HistoricalAutoSubmittedInbound => {
                "policy:auto-submitted-inbound-terminal-no-send"
            }
            TerminalPolicyGrantKind::SystemProbeInbound => {
                "policy:system-probe-inbound-terminal-no-send"
            }
            TerminalPolicyGrantKind::RoutingBackfillNonWork => {
                "policy:routing-backfill-non-work-terminal-no-send"
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueTaskUpdateRequest {
    pub message_key: String,
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub thread_key: Option<String>,
    pub workspace_root: Option<String>,
    pub clear_workspace_root: bool,
    pub priority: Option<String>,
    pub suggested_skill: Option<String>,
    pub clear_skill: bool,
    pub route_status: Option<String>,
    pub status_note: Option<String>,
    pub clear_note: bool,
}

pub struct OwnerPromptContext {
    pub owner_name: String,
    pub owner_email_address: Option<String>,
    pub founder_email_addresses: Vec<String>,
    pub founder_email_roles: Vec<String>,
    pub allowed_email_domain: Option<String>,
    pub admin_email_policies: Vec<String>,
    pub channels: Vec<String>,
    pub preferred_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EmailSenderPolicy {
    pub normalized_email: String,
    pub role: String,
    pub allowed: bool,
    pub allow_admin_actions: bool,
    pub allow_sudo_actions: bool,
    pub secrets_via_email_allowed: bool,
    pub allowed_email_domain: Option<String>,
    pub block_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdminEmailPolicy {
    email: String,
    can_sudo: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunicationFeedItem {
    pub message_key: String,
    pub channel: String,
    pub direction: String,
    pub sender_display: String,
    pub sender_address: String,
    pub subject: String,
    pub preview: String,
    pub thread_key: String,
    pub route_status: String,
    pub external_created_at: String,
}

pub fn sync_prompt_identity(root: &Path, settings: &BTreeMap<String, String>) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    if let Some(owner_name) = settings
        .get("CTOX_OWNER_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        upsert_owner_profile(&mut conn, owner_name)?;
    }
    sync_identity_profiles(&mut conn, settings)?;

    if let Some(email_address) = settings
        .get("CTO_EMAIL_ADDRESS")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let provider = settings
            .get("CTO_EMAIL_PROVIDER")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("imap");
        let profile_json = json!({
            "imapHost": settings.get("CTO_EMAIL_IMAP_HOST").map(|value| value.trim()).unwrap_or(""),
            "imapPort": settings.get("CTO_EMAIL_IMAP_PORT").map(|value| value.trim()).unwrap_or(""),
            "smtpHost": settings.get("CTO_EMAIL_SMTP_HOST").map(|value| value.trim()).unwrap_or(""),
            "smtpPort": settings.get("CTO_EMAIL_SMTP_PORT").map(|value| value.trim()).unwrap_or(""),
            "graphUser": settings.get("CTO_EMAIL_GRAPH_USER").map(|value| value.trim()).unwrap_or(""),
            "ewsUrl": settings.get("CTO_EMAIL_EWS_URL").map(|value| value.trim()).unwrap_or(""),
            "owaUrl": settings.get("CTO_EMAIL_OWA_URL").map(|value| value.trim()).unwrap_or(""),
            "ewsAuthType": settings.get("CTO_EMAIL_EWS_AUTH_TYPE").map(|value| value.trim()).unwrap_or(""),
            "ewsUsername": settings.get("CTO_EMAIL_EWS_USERNAME").map(|value| value.trim()).unwrap_or(""),
            "ewsVersion": settings.get("CTO_EMAIL_EWS_VERSION").map(|value| value.trim()).unwrap_or(""),
            "activeSyncServer": settings.get("CTO_EMAIL_ACTIVESYNC_SERVER").map(|value| value.trim()).unwrap_or(""),
            "activeSyncUsername": settings.get("CTO_EMAIL_ACTIVESYNC_USERNAME").map(|value| value.trim()).unwrap_or(""),
            "activeSyncPath": settings.get("CTO_EMAIL_ACTIVESYNC_PATH").map(|value| value.trim()).unwrap_or(""),
            "activeSyncDeviceId": settings.get("CTO_EMAIL_ACTIVESYNC_DEVICE_ID").map(|value| value.trim()).unwrap_or(""),
            "activeSyncDeviceType": settings.get("CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE").map(|value| value.trim()).unwrap_or(""),
            "activeSyncProtocolVersion": settings.get("CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION").map(|value| value.trim()).unwrap_or(""),
            "activeSyncPolicyKey": settings.get("CTO_EMAIL_ACTIVESYNC_POLICY_KEY").map(|value| value.trim()).unwrap_or(""),
        });
        ensure_account(
            &mut conn,
            &format!("email:{email_address}"),
            "email",
            email_address,
            provider,
            profile_json,
        )?;
    }

    if let Some(jami_account_id) = settings
        .get("CTO_JAMI_ACCOUNT_ID")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let profile_name = settings
            .get("CTO_JAMI_PROFILE_NAME")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(jami_account_id);
        let profile_json = json!({
            "accountId": jami_account_id,
            "profileName": profile_name,
            "inboxDir": settings.get("CTO_JAMI_INBOX_DIR").map(|value| value.trim()).unwrap_or(""),
            "outboxDir": settings.get("CTO_JAMI_OUTBOX_DIR").map(|value| value.trim()).unwrap_or(""),
            "archiveDir": settings.get("CTO_JAMI_ARCHIVE_DIR").map(|value| value.trim()).unwrap_or(""),
            "dbusEnvFile": settings.get("CTO_JAMI_DBUS_ENV_FILE").map(|value| value.trim()).unwrap_or(""),
        });
        ensure_account(
            &mut conn,
            &format!("jami:{jami_account_id}"),
            "jami",
            profile_name,
            "jami",
            profile_json,
        )?;
    }

    let setting = |key: &str| -> String {
        settings
            .get(key)
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };
    let first_setting = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|key| {
                settings
                    .get(*key)
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_default()
    };
    let split_list = |raw: &str| -> Vec<String> {
        raw.split(|ch| matches!(ch, ',' | ';' | '\n'))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()
    };
    let first_list = |keys: &[&str]| -> Vec<String> {
        keys.iter()
            .find_map(|key| {
                settings
                    .get(*key)
                    .map(|value| split_list(value))
                    .filter(|values| !values.is_empty())
            })
            .unwrap_or_default()
    };
    let mut ensure_chat_account = |channel: &str,
                                   provider: &str,
                                   id: String,
                                   address: String,
                                   profile_json: Value|
     -> Result<()> {
        if id.trim().is_empty() && address.trim().is_empty() {
            return Ok(());
        }
        let suffix = if id.trim().is_empty() {
            stable_digest(&address)
        } else {
            id.trim().to_string()
        };
        ensure_account(
            &mut conn,
            &format!("{channel}:{suffix}"),
            channel,
            if address.trim().is_empty() {
                &suffix
            } else {
                address.trim()
            },
            provider,
            profile_json,
        )
    };

    if !first_setting(&[
        "CTO_SLACK_BOT_TOKEN",
        "CTO_SLACK_WORKSPACE_ID",
        "CTO_SLACK_CHANNEL_ID",
    ])
    .is_empty()
    {
        let workspace_id = setting("CTO_SLACK_WORKSPACE_ID");
        let bot_user_id = setting("CTO_SLACK_BOT_USER_ID");
        ensure_chat_account(
            "slack",
            "slack-web-api",
            first_setting(&["CTO_SLACK_BOT_USER_ID", "CTO_SLACK_WORKSPACE_ID"]),
            if bot_user_id.is_empty() {
                workspace_id.clone()
            } else {
                bot_user_id.clone()
            },
            json!({
                "workspaceId": workspace_id,
                "botUserId": bot_user_id,
                "channelIds": first_list(&["CTO_SLACK_CHANNEL_IDS", "CTO_SLACK_CHANNEL_ID"]),
                "apiBaseUrl": setting("CTO_SLACK_API_BASE_URL"),
            }),
        )?;
    }

    if !first_setting(&[
        "CTO_DISCORD_BOT_TOKEN",
        "CTO_DISCORD_APPLICATION_ID",
        "CTO_DISCORD_CHANNEL_ID",
    ])
    .is_empty()
    {
        let application_id = setting("CTO_DISCORD_APPLICATION_ID");
        ensure_chat_account(
            "discord",
            "discord-rest",
            first_setting(&["CTO_DISCORD_APPLICATION_ID", "CTO_DISCORD_BOT_USER_ID"]),
            if application_id.is_empty() {
                setting("CTO_DISCORD_BOT_USER_ID")
            } else {
                application_id.clone()
            },
            json!({
                "applicationId": application_id,
                "botUserId": setting("CTO_DISCORD_BOT_USER_ID"),
                "guildIds": first_list(&["CTO_DISCORD_GUILD_IDS", "CTO_DISCORD_GUILD_ID"]),
                "channelIds": first_list(&["CTO_DISCORD_CHANNEL_IDS", "CTO_DISCORD_CHANNEL_ID"]),
                "apiBaseUrl": setting("CTO_DISCORD_API_BASE_URL"),
            }),
        )?;
    }

    if !first_setting(&["CTO_TELEGRAM_BOT_TOKEN", "CTO_TELEGRAM_BOT_USERNAME"]).is_empty() {
        let bot_username = setting("CTO_TELEGRAM_BOT_USERNAME");
        ensure_chat_account(
            "telegram",
            "telegram-bot-api",
            bot_username.clone(),
            bot_username.clone(),
            json!({
                "botUsername": bot_username,
                "chatIds": first_list(&["CTO_TELEGRAM_CHAT_IDS", "CTO_TELEGRAM_CHAT_ID"]),
                "apiBaseUrl": setting("CTO_TELEGRAM_API_BASE_URL"),
            }),
        )?;
    }

    if !first_setting(&["CTO_MATRIX_HOMESERVER_URL", "CTO_MATRIX_USER_ID"]).is_empty() {
        let user_id = setting("CTO_MATRIX_USER_ID");
        ensure_chat_account(
            "matrix",
            "matrix-client-server",
            user_id.clone(),
            user_id.clone(),
            json!({
                "homeserverUrl": setting("CTO_MATRIX_HOMESERVER_URL"),
                "userId": user_id,
                "roomIds": first_list(&["CTO_MATRIX_ROOM_IDS", "CTO_MATRIX_ROOM_ID"]),
            }),
        )?;
    }

    if !first_setting(&[
        "CTO_MATTERMOST_SERVER_URL",
        "CTO_MATTERMOST_BOT_USER_ID",
        "CTO_MATTERMOST_CHANNEL_ID",
    ])
    .is_empty()
    {
        let bot_user_id = setting("CTO_MATTERMOST_BOT_USER_ID");
        ensure_chat_account(
            "mattermost",
            "mattermost-api-v4",
            first_setting(&["CTO_MATTERMOST_BOT_USER_ID", "CTO_MATTERMOST_SERVER_URL"]),
            if bot_user_id.is_empty() {
                setting("CTO_MATTERMOST_SERVER_URL")
            } else {
                bot_user_id.clone()
            },
            json!({
                "serverUrl": setting("CTO_MATTERMOST_SERVER_URL"),
                "botUserId": bot_user_id,
                "teamId": setting("CTO_MATTERMOST_TEAM_ID"),
                "channelIds": first_list(&["CTO_MATTERMOST_CHANNEL_IDS", "CTO_MATTERMOST_CHANNEL_ID"]),
            }),
        )?;
    }

    if !first_setting(&[
        "CTO_ZULIP_REALM_URL",
        "CTO_ZULIP_BOT_EMAIL",
        "CTO_ZULIP_EMAIL",
    ])
    .is_empty()
    {
        let bot_email = first_setting(&["CTO_ZULIP_BOT_EMAIL", "CTO_ZULIP_EMAIL"]);
        ensure_chat_account(
            "zulip",
            "zulip-rest-api",
            bot_email.clone(),
            bot_email.clone(),
            json!({
                "realmUrl": setting("CTO_ZULIP_REALM_URL"),
                "botEmail": bot_email,
                "streams": first_list(&["CTO_ZULIP_STREAMS", "CTO_ZULIP_STREAM"]),
                "topic": setting("CTO_ZULIP_TOPIC"),
            }),
        )?;
    }

    if !first_setting(&[
        "CTO_GOOGLE_CHAT_USER",
        "CTO_GOOGLE_CHAT_APP_ID",
        "CTO_GOOGLE_CHAT_SPACE_NAME",
    ])
    .is_empty()
    {
        let account_id = first_setting(&["CTO_GOOGLE_CHAT_USER", "CTO_GOOGLE_CHAT_APP_ID"]);
        ensure_chat_account(
            "google_chat",
            "google-chat-api",
            account_id.clone(),
            account_id.clone(),
            json!({
                "user": setting("CTO_GOOGLE_CHAT_USER"),
                "appId": setting("CTO_GOOGLE_CHAT_APP_ID"),
                "spaceNames": first_list(&["CTO_GOOGLE_CHAT_SPACE_NAMES", "CTO_GOOGLE_CHAT_SPACE_NAME"]),
                "apiBaseUrl": setting("CTO_GOOGLE_CHAT_API_BASE_URL"),
            }),
        )?;
    }

    Ok(())
}

pub fn merge_owner_profile_settings(
    root: &Path,
    settings: &mut BTreeMap<String, String>,
) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT owner_key, metadata_json
        FROM owner_profiles
        ORDER BY owner_key ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut founder_emails = parse_founder_email_addresses(settings)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut founder_roles = parse_founder_email_roles(settings);
    let mut admin_policies = parse_admin_email_policies(settings)
        .into_iter()
        .map(|entry| (entry.email, entry.can_sudo))
        .collect::<BTreeMap<_, _>>();

    for row in rows {
        let (owner_key, metadata_json) = row?;
        let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
        let email = metadata
            .get("email")
            .and_then(Value::as_str)
            .map(normalize_email_address)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                let normalized = normalize_email_address(&owner_key);
                normalized.contains('@').then_some(normalized)
            });
        let Some(email) = email else {
            continue;
        };
        let role = metadata
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match role.as_str() {
            "owner" => {
                settings
                    .entry("CTOX_OWNER_EMAIL_ADDRESS".to_string())
                    .or_insert(email);
            }
            "founder" => {
                founder_emails.insert(email.clone());
                let role_title = metadata
                    .get("role_title")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Founder");
                founder_roles
                    .entry(email)
                    .or_insert_with(|| role_title.to_string());
            }
            "admin" => {
                let can_sudo = metadata
                    .get("allow_sudo_actions")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                admin_policies.entry(email).or_insert(can_sudo);
            }
            _ => {}
        }
    }

    if !founder_emails.is_empty() {
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ADDRESSES".to_string(),
            founder_emails.into_iter().collect::<Vec<_>>().join(","),
        );
    }
    if !founder_roles.is_empty() {
        settings.insert(
            "CTOX_FOUNDER_EMAIL_ROLES".to_string(),
            founder_roles
                .into_iter()
                .map(|(email, role)| format!("{email}={role}"))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if !admin_policies.is_empty() {
        settings.insert(
            "CTOX_EMAIL_ADMIN_POLICIES".to_string(),
            admin_policies
                .into_iter()
                .map(|(email, can_sudo)| {
                    if can_sudo {
                        format!("{email}=sudo")
                    } else {
                        email
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    Ok(())
}

fn runtime_settings_with_owner_profiles(
    root: &Path,
    kind: communication_gateway::CommunicationAdapterKind,
) -> BTreeMap<String, String> {
    let mut settings = communication_gateway::runtime_settings_from_root(root, kind);
    let _ = merge_owner_profile_settings(root, &mut settings);
    settings
}

pub fn ensure_store(root: &Path) -> Result<()> {
    let db_path = resolve_db_path(root, None);
    let _conn = open_channel_db(&db_path)?;
    Ok(())
}

pub fn load_prompt_identity(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<OwnerPromptContext> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let owner_name = load_owner_name(&conn)?
        .or_else(|| {
            settings
                .get("CTOX_OWNER_NAME")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "the owner".to_string());

    let mut channels = BTreeSet::new();
    channels.insert("- tui: direct local CTOX session".to_string());

    let mut stmt = conn.prepare(
        r#"
        SELECT channel, address, provider, profile_json
        FROM communication_accounts
        ORDER BY channel ASC, account_key ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (channel, address, provider, profile_json) = row?;
        match channel.as_str() {
            "email" => {
                if !address.trim().is_empty() {
                    channels.insert(format!(
                        "- email: {} (provider: {})",
                        address.trim(),
                        provider.trim()
                    ));
                }
            }
            "jami" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let profile_name = parsed
                    .get("profileName")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !profile_name.is_empty() {
                    channels.insert(format!("- jami: {}", profile_name));
                }
            }
            "teams" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let bot_id = parsed
                    .get("botId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !bot_id.is_empty() {
                    channels.insert(format!("- teams: {}", bot_id));
                }
            }
            "whatsapp" => {
                let parsed =
                    serde_json::from_str::<Value>(&profile_json).unwrap_or_else(|_| json!({}));
                let jid = parsed
                    .get("jid")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(address.trim());
                if !jid.is_empty() {
                    channels.insert(format!("- whatsapp: {}", jid));
                }
            }
            "cron" | "plan" | "queue" => {}
            other => {
                if !address.trim().is_empty() {
                    channels.insert(format!("- {}: {}", other, address.trim()));
                } else {
                    channels.insert(format!("- {}", other));
                }
            }
        }
    }

    Ok(OwnerPromptContext {
        owner_name,
        owner_email_address: settings
            .get("CTOX_OWNER_EMAIL_ADDRESS")
            .map(|value| normalize_email_address(value))
            .filter(|value| !value.is_empty()),
        founder_email_addresses: parse_founder_email_addresses(settings),
        founder_email_roles: founder_email_role_summaries(settings),
        allowed_email_domain: normalized_allowed_email_domain(settings),
        admin_email_policies: admin_email_policy_summaries(settings),
        channels: channels.into_iter().collect(),
        preferred_channel: settings
            .get("CTOX_OWNER_PREFERRED_CHANNEL")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    })
}

/// Whether the inbound message metadata carries the structured
/// "auto-submitted" marker we extract from RFC 3834 / Outlook headers
/// at IMAP/Graph ingestion time.
///
/// We deliberately do NOT inspect subject lines or body text here:
/// language- and template-specific scraping belongs in skills, not in
/// the core. This check looks only at JSON fields written by the
/// inbound parser.
pub fn metadata_marks_auto_submitted(metadata: &Value) -> bool {
    let direct = metadata
        .get("autoSubmitted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if direct {
        return true;
    }
    let suppress = metadata
        .get("autoResponseSuppress")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if suppress {
        return true;
    }
    // Defense-in-depth: when the inbound parser captured the raw
    // header value but failed to populate the boolean (older row
    // shape), still honour an `auto-replied`/`auto-generated`/
    // `auto-notified` token. We compare structured tokens, not
    // free-form strings.
    if let Some(value) = metadata.get("autoSubmittedValue").and_then(Value::as_str) {
        let token = value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !token.is_empty() && token != "no" {
            return true;
        }
    }
    false
}

pub fn reclassify_historical_auto_submitted_inbounds(root: &Path) -> Result<usize> {
    #[derive(Debug)]
    struct Candidate {
        message_key: String,
        subject: String,
        sender_address: String,
        body_text: String,
        metadata: Value,
    }

    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.subject,
            m.sender_address,
            m.body_text,
            m.metadata_json
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.status = 'received'
          AND m.channel = 'email'
          AND COALESCE(r.route_status, 'pending') IN ('pending','leased','failed','review_rework','handled')
          AND NOT EXISTS (
              SELECT 1
              FROM communication_founder_reply_reviews review
              WHERE review.inbound_message_key = m.message_key
                AND review.terminal_no_send = 1
          )
        "#,
    )?;
    let candidates = stmt
        .query_map([], |row| {
            let metadata_raw: String = row.get(4)?;
            Ok(Candidate {
                message_key: row.get(0)?,
                subject: row.get(1)?,
                sender_address: row.get(2)?,
                body_text: row.get(3)?,
                metadata: serde_json::from_str(&metadata_raw).unwrap_or_else(|_| json!({})),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut reclassified = 0usize;
    for mut candidate in candidates {
        if metadata_marks_auto_submitted(&candidate.metadata) {
            continue;
        }
        let Some(reason) = historical_auto_submitted_reason(
            &candidate.subject,
            &candidate.sender_address,
            &candidate.body_text,
        ) else {
            continue;
        };
        let now = now_iso_string();
        if let Some(object) = candidate.metadata.as_object_mut() {
            object.insert("autoSubmitted".to_string(), Value::Bool(true));
            object.insert(
                "autoSubmittedValue".to_string(),
                Value::String("historical-reclassifier".to_string()),
            );
            object.insert("terminalNoSend".to_string(), Value::Bool(true));
            object.insert(
                "terminalNoSendReason".to_string(),
                Value::String(reason.clone()),
            );
            object.insert("reclassifiedAt".to_string(), Value::String(now.clone()));
        }
        conn.execute(
            r#"
            UPDATE communication_messages
            SET metadata_json = ?2
            WHERE message_key = ?1
            "#,
            params![
                candidate.message_key,
                serde_json::to_string(&candidate.metadata)?
            ],
        )?;
        record_terminal_no_send_verdict(
            root,
            &candidate.message_key,
            "boot-reclassifier",
            &reason,
        )?;
        let route_status = QueueRouteStatus::Handled;
        let previous_route_status = current_queue_route_status(&conn, &candidate.message_key)?;
        enforce_queue_route_status_transition_with_grant(
            &conn,
            &candidate.message_key,
            &previous_route_status,
            route_status.as_str(),
            "ctox-boot-reclassifier",
            "mark_historical_auto_submitted_inbound_handled",
            Some(TerminalPolicyGrant::historical_auto_submitted_inbound()),
        )?;
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            VALUES (?1, ?2, NULL, NULL, ?3, NULL, ?3)
            ON CONFLICT(message_key) DO UPDATE SET
                route_status=excluded.route_status,
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=?3,
                last_error=NULL,
                updated_at=?3
            "#,
            params![candidate.message_key, route_status.as_str(), now],
        )?;
        reclassified += 1;
    }
    Ok(reclassified)
}

fn historical_auto_submitted_reason(
    subject: &str,
    sender_address: &str,
    body_text: &str,
) -> Option<String> {
    let subject = subject.trim().to_ascii_lowercase();
    if [
        "automatische antwort:",
        "auto-reply:",
        "out of office:",
        "automatic reply:",
    ]
    .iter()
    .any(|prefix| subject.starts_with(prefix))
    {
        return Some("historical auto-reply subject: terminal NO-SEND".to_string());
    }

    let sender = normalize_email_address(sender_address);
    let local_part = sender.split('@').next().unwrap_or("");
    if matches!(
        local_part,
        "noreply" | "no-reply" | "donotreply" | "do-not-reply" | "notification" | "notifications"
    ) {
        return Some("historical notification sender: terminal NO-SEND".to_string());
    }

    if body_is_only_teams_meeting_link(body_text) {
        return Some(
            "historical Teams meeting-link notification without human content: terminal NO-SEND"
                .to_string(),
        );
    }
    None
}

fn body_is_only_teams_meeting_link(body_text: &str) -> bool {
    let lowered = body_text.to_ascii_lowercase();
    if !(lowered.contains("teams.microsoft.com/l/meetup-join")
        || lowered.contains("teams.live.com/meet")
        || lowered.contains("join.microsoft.com/meet"))
    {
        return false;
    }
    let mut remainder = lowered.as_str();
    let mut cleaned = String::new();
    while let Some(start) = remainder.find("http") {
        cleaned.push_str(&remainder[..start]);
        let after_start = &remainder[start..];
        let end = after_start
            .find(char::is_whitespace)
            .unwrap_or(after_start.len());
        remainder = &after_start[end..];
    }
    cleaned.push_str(remainder);
    for phrase in [
        "microsoft teams",
        "join the meeting",
        "meeting id",
        "passcode",
        "dial in",
        "privacy and security",
        "learn more",
        "need help",
        "besprechungs-id",
        "kenncode",
        "an besprechung teilnehmen",
        "teilnehmen",
    ] {
        cleaned = cleaned.replace(phrase, " ");
    }
    let meaningful = cleaned
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect::<String>();
    meaningful.len() <= 40
}

/// Terminal route states that are sticky against further re-routing.
/// Once an inbound message is acked into one of these, the service
/// loop must NOT pull it back into `review_rework` or any other
/// non-terminal state. New work for the same thread must arrive via a
/// fresh inbound message (with its own message_key).
pub fn route_status_is_terminal(route_status: &str) -> bool {
    QueueRouteStatus::parse(route_status).is_some_and(QueueRouteStatus::is_terminal)
}

pub fn classify_email_sender(
    settings: &BTreeMap<String, String>,
    sender_address: &str,
) -> EmailSenderPolicy {
    let normalized_email = normalize_email_address(sender_address);
    let owner_email = settings
        .get("CTOX_OWNER_EMAIL_ADDRESS")
        .map(|value| normalize_email_address(value))
        .filter(|value| !value.is_empty());
    let founder_emails = parse_founder_email_addresses(settings);
    let allowed_email_domain = normalized_allowed_email_domain(settings);
    let admin_policies = parse_admin_email_policies(settings);

    if normalized_email.is_empty() {
        return EmailSenderPolicy {
            normalized_email,
            role: "external".to_string(),
            allowed: false,
            allow_admin_actions: false,
            allow_sudo_actions: false,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: Some("sender email address is empty".to_string()),
        };
    }

    if owner_email.as_deref() == Some(normalized_email.as_str()) {
        return EmailSenderPolicy {
            normalized_email,
            role: "owner".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: true,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if founder_emails
        .iter()
        .any(|email| email == &normalized_email)
    {
        return EmailSenderPolicy {
            normalized_email,
            role: "founder".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: false,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if let Some(admin) = admin_policies
        .iter()
        .find(|entry| entry.email == normalized_email)
    {
        return EmailSenderPolicy {
            normalized_email,
            role: "admin".to_string(),
            allowed: true,
            allow_admin_actions: true,
            allow_sudo_actions: admin.can_sudo,
            secrets_via_email_allowed: false,
            allowed_email_domain,
            block_reason: None,
        };
    }

    if let Some(domain) = allowed_email_domain.clone() {
        if email_matches_domain(&normalized_email, &domain) {
            return EmailSenderPolicy {
                normalized_email,
                role: "domain_user".to_string(),
                allowed: true,
                allow_admin_actions: false,
                allow_sudo_actions: false,
                secrets_via_email_allowed: false,
                allowed_email_domain: Some(domain),
                block_reason: None,
            };
        }
    }

    EmailSenderPolicy {
        normalized_email,
        role: "external".to_string(),
        allowed: false,
        allow_admin_actions: false,
        allow_sudo_actions: false,
        secrets_via_email_allowed: false,
        allowed_email_domain,
        block_reason: Some(
            "sender is outside the configured founder/owner/admin list and allowed employee email domain"
                .to_string(),
        ),
    }
}

/// F4: snapshot of the founder/owner outbound pipeline for a single thread.
/// Joins:
/// - `mission_states`        — current mission status, agent_failure_count
/// - `messages`              — agent attempts and their structured outcomes
/// - `communication_founder_reply_reviews` — review and approval records
/// - `communication_messages` (outbound rows, plus their routing state) —
///   actual send attempts and their delivery state
///
/// Output is flat JSON shaped for operator consumption, intentionally
/// avoiding internal-only field names where they would leak past CTOX.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineStatusReport {
    pub thread_key: Option<String>,
    pub founder_outbound_intent: bool,
    pub agent_attempts: Vec<PipelineAgentAttempt>,
    pub review_runs: Vec<PipelineReviewRun>,
    pub approval_records: Vec<PipelineApprovalRecord>,
    pub send_attempts: Vec<PipelineSendAttempt>,
    pub current_mission_status: String,
    pub agent_failure_count: i64,
    /// Iteration counter for the lightweight rewrite-only review path
    /// (per-mission, reset on approval). Surfaced so operators can see
    /// when a thread is bouncing in the body-fix loop versus the heavy
    /// rework path.
    pub rewrite_iteration_count: i64,
    /// Iteration counter for the heavy rework path. Derived from the
    /// stored `agent_failure_count` because rework continuations inherit
    /// the agent-failure backoff machinery; this duplication keeps the
    /// pipeline-status surface self-describing without changing the
    /// underlying schema.
    pub rework_iteration_count: i64,
    /// Most recent disposition the dispatcher chose. One of `Approved`,
    /// `RewriteOnly`, `RequeueInternalWork`, `None`. Computed from the latest
    /// review run / mission status, so it stays accurate without an
    /// extra column.
    pub current_disposition: String,
    pub last_error: Option<String>,
    /// Recent governance events from the strategic-directive owner-authority
    /// gate that touched this thread. Surfaces both permitted and blocked
    /// inbound-mail-driven mutations so operators can see whether the
    /// authority gate fired (and how) for the conversation. Filtered by
    /// `details.thread_key` or `details.conversation_id` so unrelated
    /// global authority events do not leak into the per-thread surface.
    pub strategic_directive_authority_events: Vec<StrategicDirectiveAuthorityEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategicDirectiveAuthorityEvent {
    pub event_id: String,
    pub mechanism_id: String,
    pub severity: String,
    pub created_at: String,
    pub sender_role: Option<String>,
    pub sender_address: Option<String>,
    pub directive_kind: Option<String>,
    pub attempted_status: Option<String>,
    pub action: Option<String>,
    pub triggered_by_message_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineAgentAttempt {
    pub turn_id: String,
    pub outcome: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineReviewRun {
    pub approval_key: String,
    pub inbound_message_key: String,
    pub reviewer: String,
    pub review_summary: String,
    pub approved_at: String,
    pub sent_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineApprovalRecord {
    pub approval_key: String,
    pub action_digest: String,
    pub body_sha256: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineSendAttempt {
    pub message_key: String,
    pub direction: String,
    pub subject: String,
    pub external_created_at: String,
    pub route_status: Option<String>,
    pub last_error: Option<String>,
}

pub(crate) fn pipeline_status(
    root: &Path,
    thread_key: Option<&str>,
    limit: usize,
) -> Result<PipelineStatusReport> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;

    // Agent attempts and reviews are scoped per-conversation_id derived
    // from the thread_key. If the operator did not supply one, we report
    // global state without per-thread review/send rows.
    let conversation_id = thread_key
        .map(|key| crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(key)));

    // Mission state for the conversation that owns this thread.
    let mission_state = if let Some(conv_id) = conversation_id {
        crate::lcm::LcmEngine::open(&db_path, crate::lcm::LcmConfig::default())
            .ok()
            .and_then(|engine| engine.stored_mission_state(conv_id).ok().flatten())
    } else {
        None
    };
    let (
        current_mission_status,
        agent_failure_count,
        rewrite_iteration_count,
        rework_iteration_count,
        last_error,
    ) = match &mission_state {
        Some(record) => (
            record.mission_status.clone(),
            record.agent_failure_count,
            record.rewrite_failure_count,
            record.agent_failure_count,
            record.deferred_reason.clone(),
        ),
        None => ("unknown".to_string(), 0, 0, 0, None),
    };

    // Agent attempts: most recent assistant rows for the conversation, in
    // reverse-chronological order, along with their structured outcome.
    let agent_attempts = if let Some(conv_id) = conversation_id {
        let mut stmt = conn.prepare(
            "SELECT message_id, agent_outcome, created_at
             FROM messages
             WHERE conversation_id = ?1 AND role = 'assistant'
             ORDER BY seq DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![conv_id, limit as i64], |row| {
            let id: i64 = row.get(0)?;
            let outcome: Option<String> = row.get(1)?;
            let ended: Option<String> = row.get(2)?;
            Ok(PipelineAgentAttempt {
                turn_id: format!("msg:{id}"),
                outcome,
                started_at: None,
                ended_at: ended,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    // Review and approval records keyed on the inbound message belonging
    // to this thread. If thread_key is None, return all recent reviews.
    let mut review_runs = Vec::new();
    let mut approval_records = Vec::new();
    if let Some(thread) = thread_key {
        let mut stmt = conn.prepare(
            "SELECT r.approval_key, r.inbound_message_key, r.action_digest, r.body_sha256,
                    r.reviewer, r.review_summary, r.approved_at, r.sent_at
             FROM communication_founder_reply_reviews r
             JOIN communication_messages m ON m.message_key = r.inbound_message_key
             WHERE m.thread_key = ?1
             ORDER BY r.approved_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread, limit as i64], |row| {
            let approval_key: String = row.get(0)?;
            let inbound_message_key: String = row.get(1)?;
            let action_digest: String = row.get(2)?;
            let body_sha256: String = row.get(3)?;
            let reviewer: String = row.get(4)?;
            let review_summary: String = row.get(5)?;
            let approved_at: String = row.get(6)?;
            let sent_at: Option<String> = row.get(7)?;
            Ok((
                PipelineReviewRun {
                    approval_key: approval_key.clone(),
                    inbound_message_key,
                    reviewer,
                    review_summary,
                    approved_at: approved_at.clone(),
                    sent_at,
                },
                PipelineApprovalRecord {
                    approval_key,
                    action_digest,
                    body_sha256,
                    approved_at,
                },
            ))
        })?;
        for row in rows {
            let (review, approval) = row?;
            review_runs.push(review);
            approval_records.push(approval);
        }
    }

    // Send attempts: outbound communication_messages rows for this thread.
    let send_attempts = if let Some(thread) = thread_key {
        let mut stmt = conn.prepare(
            "SELECT m.message_key, m.direction, m.subject, m.external_created_at,
                    r.route_status, r.last_error
             FROM communication_messages m
             LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
             WHERE m.thread_key = ?1 AND m.direction = 'outbound'
             ORDER BY m.external_created_at DESC, m.observed_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread, limit as i64], |row| {
            Ok(PipelineSendAttempt {
                message_key: row.get(0)?,
                direction: row.get(1)?,
                subject: row.get(2)?,
                external_created_at: row.get(3)?,
                route_status: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    // founder_outbound_intent is true if there's at least one approval
    // record for this thread (a reviewed founder send was prepared).
    let founder_outbound_intent = !approval_records.is_empty();

    // The dispatcher disposition is structural (no string scraping). We
    // derive it from the persisted state: an approval row implies the most
    // recent disposition was `Approved`; a non-zero rewrite_failure_count
    // implies the loop is in the lightweight rewrite path; a non-zero
    // agent_failure_count implies the heavy rework path. Otherwise the
    // pipeline has never produced a reviewed slice — `None`.
    let current_disposition = if !approval_records.is_empty() {
        "Approved".to_string()
    } else if rewrite_iteration_count > 0 {
        "RewriteOnly".to_string()
    } else if rework_iteration_count > 0 {
        "RequeueInternalWork".to_string()
    } else {
        "None".to_string()
    };

    // E (PR): per-thread strategic-directive authority audit trail. We
    // pull both the `_owner_authorised` and `_blocked_non_owner_sender`
    // events the strategy-mutation gate emits, and filter to those whose
    // structured details reference this thread (`thread_key` or
    // `conversation_id`). The default surface is the last `limit` such
    // events; if no thread was supplied we leave the list empty rather
    // than reporting global state, which matches how the surrounding
    // pipeline fields treat an absent thread_key.
    let strategic_directive_authority_events =
        load_strategic_directive_authority_events(&db_path, thread_key, conversation_id, limit)?;

    Ok(PipelineStatusReport {
        thread_key: thread_key.map(ToOwned::to_owned),
        founder_outbound_intent,
        agent_attempts,
        review_runs,
        approval_records,
        send_attempts,
        current_mission_status,
        agent_failure_count,
        rewrite_iteration_count,
        rework_iteration_count,
        current_disposition,
        last_error,
        strategic_directive_authority_events,
    })
}

fn load_strategic_directive_authority_events(
    db_path: &Path,
    thread_key: Option<&str>,
    conversation_id: Option<i64>,
    limit: usize,
) -> Result<Vec<StrategicDirectiveAuthorityEvent>> {
    if thread_key.is_none() && conversation_id.is_none() {
        return Ok(Vec::new());
    }
    // The governance schema is created lazily by the governance module; if
    // it does not exist yet, return an empty vec rather than erroring.
    let conn = Connection::open(db_path).with_context(|| {
        format!(
            "failed to open db {} for strategic-directive authority events",
            db_path.display()
        )
    })?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='governance_events'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT event_id, mechanism_id, severity, details_json, created_at
         FROM governance_events
         WHERE mechanism_id IN (
             'strategic_directive_mutation_owner_authorised',
             'strategic_directive_mutation_blocked_non_owner_sender'
         )
         ORDER BY CAST(created_at AS INTEGER) DESC
         LIMIT ?1",
    )?;
    // We pull a generous slice and filter in Rust because the structured
    // thread/conversation match lives inside `details_json`. Clamp to a
    // sane upper bound so this stays cheap even if the audit trail is busy.
    let scan_limit = (limit.max(1) * 8).min(512) as i64;
    let rows = stmt.query_map(params![scan_limit], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out: Vec<StrategicDirectiveAuthorityEvent> = Vec::new();
    for row in rows {
        let (event_id, mechanism_id, severity, details_json, created_at) = row?;
        let details: serde_json::Value =
            serde_json::from_str(&details_json).unwrap_or(serde_json::Value::Null);
        let detail_thread = details
            .get("thread_key")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let detail_conversation = details
            .get("conversation_id")
            .and_then(|value| value.as_i64());
        let matches_thread = match thread_key {
            Some(key) => detail_thread.as_deref() == Some(key),
            None => false,
        };
        let matches_conversation = match (conversation_id, detail_conversation) {
            (Some(want), Some(got)) => want == got,
            _ => false,
        };
        if !matches_thread && !matches_conversation {
            continue;
        }
        out.push(StrategicDirectiveAuthorityEvent {
            event_id,
            mechanism_id,
            severity,
            created_at,
            sender_role: details
                .get("sender_role")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            sender_address: details
                .get("sender_address")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            directive_kind: details
                .get("directive_kind")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            attempted_status: details
                .get("attempted_status")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            action: details
                .get("action")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            triggered_by_message_key: details
                .get("triggered_by_message_key")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// JSON-friendly wrappers for the Business OS HTTP routes. These wrap the
/// existing internal channel functions without duplicating logic — the routes
/// in src/core/business_os/server.rs call these directly.

pub fn handle_channel_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let conn = open_channel_db(&db_path)?;
            let result = json!({
                "ok": true,
                "db_path": db_path,
                "initialized": schema_state(&conn)?,
            });
            print_json(&result)
        }
        "sync" => {
            let channel = required_flag_value(args, "--channel")?;
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let result = sync_channel(root, &db_path, channel, args)?;
            print_json(&result)
        }
        "take" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let lease_owner = find_flag_value(args, "--lease-owner")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "codex".to_string());
            let channel = find_flag_value(args, "--channel").map(ToOwned::to_owned);
            let mut conn = open_channel_db(&db_path)?;
            let taken = take_messages_with_projection(
                find_flag_value(args, "--db").is_none().then_some(root),
                &mut conn,
                channel.as_deref(),
                limit,
                &lease_owner,
            )?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "lease_owner": lease_owner,
                "count": taken.len(),
                "messages": taken,
            }))
        }
        "ack" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let status = canonical_queue_route_status(
                find_flag_value(args, "--status").unwrap_or("handled"),
            )?;
            let failure_reason = find_flag_value(args, "--reason");
            let message_keys = positional_after_flags(&args[1..]);
            if message_keys.is_empty() {
                anyhow::bail!(
                    "usage: ctox channel ack [--db <path>] [--status <status>] [--reason <text>] <message-key>..."
                );
            }
            let mut conn = open_channel_db(&db_path)?;
            let (failure_note, ack_reason) = if status == QueueRouteStatus::Failed {
                (failure_reason, None)
            } else {
                (None, failure_reason)
            };
            let updated = ack_messages(
                find_flag_value(args, "--db").is_none().then_some(root),
                &mut conn,
                &message_keys,
                status.as_str(),
                failure_note,
                ack_reason,
                None,
            )?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "updated": updated,
                "status": status.as_str(),
                "message_keys": message_keys,
            }))
        }
        "send" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let request = parse_send_request(args)?;
            let result = send_message(root, &db_path, request)?;
            print_json(&result)
        }
        "founder-reply" => {
            anyhow::bail!(
                "direct founder-reply is disabled; founder/owner outbound email must be sent only through the reviewed service path"
            )
        }
        "test" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let channel = required_flag_value(args, "--channel")?;
            let account_key = find_flag_value(args, "--account-key").map(ToOwned::to_owned);
            let result = test_channel(root, &db_path, channel, account_key.as_deref())?;
            print_json(&result)
        }
        "ingest-tui" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let request = parse_tui_ingest_request(args)?;
            let mut conn = open_channel_db(&db_path)?;
            let stored = ingest_tui_message(root, &mut conn, request)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "stored": stored,
            }))
        }
        "list" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let channel = find_flag_value(args, "--channel");
            let conn = open_channel_db(&db_path)?;
            let messages = list_messages(&conn, channel, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "history" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = required_flag_value(args, "--thread-key")?;
            let conn = open_channel_db(&db_path)?;
            let messages = list_thread_messages(&conn, thread_key, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "thread_key": thread_key,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "search" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let query = required_flag_value(args, "--query")?;
            let channel = find_flag_value(args, "--channel");
            let sender = find_flag_value(args, "--sender");
            let conn = open_channel_db(&db_path)?;
            let messages = search_messages(&conn, query, channel, sender, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "query": query,
                "channel": channel,
                "sender": sender,
                "count": messages.len(),
                "messages": messages,
            }))
        }
        "context" => {
            let db_path = resolve_db_path(root, find_flag_value(args, "--db"));
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = required_flag_value(args, "--thread-key")?;
            let query = find_flag_value(args, "--query");
            let sender = find_flag_value(args, "--sender");
            let conn = open_channel_db(&db_path)?;
            let context = build_communication_context(&conn, thread_key, query, sender, limit)?;
            print_json(&json!({
                "ok": true,
                "db_path": db_path,
                "context": context,
            }))
        }
        "pipeline-status" => {
            let limit = find_flag_value(args, "--limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(DEFAULT_TAKE_LIMIT);
            let thread_key = find_flag_value(args, "--thread-key");
            let report = pipeline_status(root, thread_key, limit)?;
            print_json(&json!({
                "ok": true,
                "report": report,
            }))
        }
        _ => {
            anyhow::bail!(
                "usage:\n  ctox channel init [--db <path>]\n  ctox channel sync --channel <email|jami|teams|meeting|whatsapp|slack|discord|telegram|matrix|mattermost|zulip|google_chat> [--db <path>] [adapter flags]\n  ctox channel take [--db <path>] [--channel <name>] [--limit <n>] [--lease-owner <owner>]\n  ctox channel ack [--db <path>] [--status <status>] <message-key>...\n  ctox channel send --channel <tui|email|jami|teams|meeting|whatsapp|slack|discord|telegram|matrix|mattermost|zulip|google_chat> --account-key <key> --thread-key <key> --body <text> [--subject <text>] [--to <addr>]... [--cc <addr>]... [--attach-file <path>]... [--send-voice] [--reviewed-founder-send] [--reviewed-communication-send]\n  ctox channel founder-reply --message-key <inbound-email-key> --body <text>\n  ctox channel test --channel <tui|email|jami|teams|whatsapp|slack|discord|telegram|matrix|mattermost|zulip|google_chat> [--db <path>] [--account-key <key>]\n  ctox channel ingest-tui --account-key <key> --thread-key <key> --body <text> [--sender-display <name>] [--sender-address <addr>] [--subject <text>]\n  ctox channel list [--db <path>] [--channel <name>] [--limit <n>]\n  ctox channel history --thread-key <key> [--db <path>] [--limit <n>]\n  ctox channel search --query <text> [--db <path>] [--channel <name>] [--sender <addr>] [--limit <n>]\n  ctox channel context --thread-key <key> [--db <path>] [--query <text>] [--sender <addr>] [--limit <n>]\n  ctox channel pipeline-status [--thread-key <key>] [--limit <n>]"
            )
        }
    }
}

pub fn load_recent_communication_feed(
    root: &Path,
    limit: usize,
) -> Result<Vec<CommunicationFeedItem>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.channel,
            m.direction,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.thread_key,
            COALESCE(r.route_status, 'pending'),
            m.external_created_at
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(CommunicationFeedItem {
            message_key: String::new(),
            channel: row.get(0)?,
            direction: row.get(1)?,
            sender_display: row.get(2)?,
            sender_address: row.get(3)?,
            subject: row.get(4)?,
            preview: row.get(5)?,
            thread_key: row.get(6)?,
            route_status: row.get(7)?,
            external_created_at: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub fn load_thread_communication_feed(
    root: &Path,
    thread_key: &str,
    limit: usize,
) -> Result<Vec<CommunicationFeedItem>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.direction,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.thread_key,
            COALESCE(r.route_status, 'pending'),
            m.external_created_at
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.thread_key = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![thread_key, limit as i64], |row| {
        Ok(CommunicationFeedItem {
            message_key: row.get(0)?,
            channel: row.get(1)?,
            direction: row.get(2)?,
            sender_display: row.get(3)?,
            sender_address: row.get(4)?,
            subject: row.get(5)?,
            preview: row.get(6)?,
            thread_key: row.get(7)?,
            route_status: row.get(8)?,
            external_created_at: row.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub fn lease_pending_inbound_messages(
    root: &Path,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<RoutedInboundMessage>> {
    if crate::business_os::harness_cockpit::queue_is_paused(root) {
        return Ok(Vec::new());
    }
    refresh_inbound_priority_credits(root)?;
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let leased = take_messages_with_projection(Some(root), &mut conn, None, limit, lease_owner)?;
    Ok(leased
        .into_iter()
        .map(|item| {
            let preferred_reply_modality = item
                .metadata
                .get("preferredReplyModality")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let workspace_root =
                workspace_root_from_queue_metadata_or_prompt(&item.metadata, &item.body_text);
            RoutedInboundMessage {
                message_key: item.message_key,
                channel: item.channel,
                account_key: item.account_key,
                thread_key: item.thread_key,
                sender_display: item.sender_display,
                sender_address: item.sender_address,
                subject: item.subject,
                preview: item.preview,
                body_text: item.body_text,
                external_created_at: item.external_created_at,
                workspace_root,
                metadata: item.metadata,
                preferred_reply_modality,
            }
        })
        .collect())
}

/// router-4: read-only, NON-leasing peek at the inbound messages the serial
/// router would lease this tick. Mirrors `take_messages` (no channel filter) — the
/// same eligibility `lease_pending_inbound_messages` uses (direction='inbound',
/// route_status pending, `not_before` elapsed, one row
/// per thread) — but performs NO lease UPDATE. Lets the router consult
/// `source_label_dispatch_rank` at the durable-queue-vs-inbound boundary without
/// consuming the message it is only inspecting.
pub fn peek_leasable_inbound_messages(
    root: &Path,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<RoutedInboundMessage>> {
    if crate::business_os::harness_cockpit::queue_is_paused(root) {
        return Ok(Vec::new());
    }
    refresh_inbound_priority_credits(root)?;
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        WITH eligible AS (
            SELECT
                m.message_key, m.channel, m.account_key, m.thread_key, m.remote_id,
                m.direction, m.folder_hint, m.sender_display, m.sender_address,
                m.subject, m.preview, m.body_text, m.status, m.seen,
                m.external_created_at, m.observed_at, m.metadata_json,
                r.route_status, r.lease_owner, r.leased_at, r.acked_at, r.last_error, r.updated_at,
                MIN(COALESCE(r.first_pending_at, m.external_created_at)) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_pending_since,
                MIN(r.priority_time_credit_hours) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_priority_credit_hours,
                ROW_NUMBER() OVER (
                    PARTITION BY m.thread_key
                    ORDER BY
                        CASE WHEN m.channel = 'queue' THEN m.external_created_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.external_created_at END DESC,
                        CASE WHEN m.channel = 'queue' THEN m.observed_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.observed_at END DESC,
                        m.message_key DESC
                ) AS thread_rank
            FROM communication_messages m
            JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.direction = 'inbound'
              AND r.route_status = 'pending'
              AND (r.retry_not_before IS NULL OR datetime(r.retry_not_before) <= datetime('now'))
              AND (
                    json_extract(m.metadata_json, '$.not_before') IS NULL
                 OR json_extract(m.metadata_json, '$.not_before') = ''
                 OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
              )
        )
        SELECT
            message_key, channel, account_key, thread_key, remote_id, direction,
            folder_hint, sender_display, sender_address, subject, preview, body_text,
            status, seen, external_created_at, observed_at, metadata_json,
            route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        FROM eligible
        WHERE thread_rank = 1
        ORDER BY
            CASE
                WHEN channel = 'tui' THEN datetime(thread_pending_since, '-24 hours')
                WHEN channel = 'queue' THEN datetime(thread_pending_since, '+1 hour')
                ELSE datetime(thread_pending_since, printf('%+d hours', thread_priority_credit_hours))
            END ASC,
            CASE WHEN channel = 'queue' THEN external_created_at END ASC,
            CASE WHEN channel <> 'queue' THEN external_created_at END DESC,
            CASE WHEN channel = 'queue' THEN observed_at END ASC,
            CASE WHEN channel <> 'queue' THEN observed_at END DESC,
            message_key DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![lease_owner, limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .map(|items| {
            items
                .into_iter()
                .map(routed_inbound_message_from_view)
                .collect()
        })
}

fn refresh_inbound_priority_credits(root: &Path) -> Result<()> {
    let settings = runtime_settings_with_owner_profiles(
        root,
        communication_gateway::CommunicationAdapterKind::Email,
    );
    let conn = open_channel_db(&resolve_db_path(root, None))?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key, m.sender_address
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key=m.message_key
        WHERE m.direction='inbound' AND m.channel='email' AND r.route_status='pending'
        "#,
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (message_key, sender_address) in rows {
        let policy = classify_email_sender(&settings, &sender_address);
        let credit =
            if policy.allowed && matches!(policy.role.as_str(), "owner" | "founder" | "admin") {
                -24
            } else {
                0
            };
        conn.execute(
            "UPDATE communication_routing_state SET priority_time_credit_hours=?2 WHERE message_key=?1 AND priority_time_credit_hours!=?2",
            params![message_key, credit],
        )?;
    }
    Ok(())
}

pub fn list_stalled_inbound_messages(
    root: &Path,
    limit: usize,
) -> Result<Vec<RoutedInboundMessage>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.channel IN (
                'email', 'jami', 'teams', 'whatsapp', 'meeting', 'slack',
                'discord', 'telegram', 'matrix', 'mattermost', 'zulip',
                'google_chat'
          )
          AND r.route_status IN ('failed', 'review_rework')
          AND (
                r.acked_at IS NULL
             OR r.route_status IN ('failed', 'review_rework')
          )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .map(|items| {
            items
                .into_iter()
                .map(routed_inbound_message_from_view)
                .collect()
        })
}

pub fn list_unreviewed_handled_inbound_messages(
    root: &Path,
    limit: usize,
) -> Result<Vec<RoutedInboundMessage>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND m.channel IN ('email', 'jami')
          AND r.route_status = 'handled'
          AND NOT EXISTS (
              SELECT 1
              FROM communication_founder_reply_reviews review
              WHERE review.inbound_message_key = m.message_key
                AND review.sent_at IS NOT NULL
          )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
        .map(|items| {
            items
                .into_iter()
                .map(routed_inbound_message_from_view)
                .collect()
        })
}

pub fn founder_reply_sent_after_review_for_message(
    root: &Path,
    inbound_message_key: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    founder_reply_sent_after_review(&conn, inbound_message_key)
}

/// Whether any inbound communication message is still pending or leased
/// (i.e. not acked as handled/blocked). Used by the mission watchdog to
/// avoid queuing redundant continuation tasks when real work is already
/// waiting in the channel queue.
pub fn has_runnable_inbound_message(root: &Path) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_routing_state
        WHERE route_status IN ('pending', 'leased')
        "#,
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn ack_leased_messages(root: &Path, message_keys: &[String], status: &str) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status.as_str())?;
    ack_messages(
        Some(root),
        &mut conn,
        message_keys,
        status.as_str(),
        None,
        None,
        None,
    )
}

/// Ack with an explicit routing reason. The reason is audit data only and does
/// not authorize a terminal-success transition.
/// I-071: apply a queue acknowledgement at most once for a durable worker
/// attempt. The queue transition and the attempt effect marker share one SQLite
/// transaction, so a crash cannot leave an unmarked acknowledgement that is
/// replayed with a second `acked_at` timestamp.
pub fn ack_leased_messages_for_attempt(
    root: &Path,
    attempt_id: &str,
    message_keys: &[String],
    status: &str,
    failure_reason: Option<&str>,
) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    if status == QueueRouteStatus::Failed {
        anyhow::ensure!(
            failure_reason
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            "failed queue acknowledgement requires a failure reason"
        );
    }
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status.as_str())?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.unchecked_transaction()?;
    let already_applied: Option<Option<String>> = tx
        .query_row(
            "SELECT queue_effects_applied_at FROM worker_attempt_finalizations WHERE attempt_id = ?1",
            [attempt_id],
            |row| row.get(0),
        )
        .optional()?;
    let already_applied = already_applied
        .with_context(|| format!("worker attempt {attempt_id} does not exist for queue ack"))?;
    if already_applied.is_some() {
        tx.commit()?;
        return Ok(0);
    }
    let updated = ack_messages_in_transaction(
        &tx,
        message_keys,
        status.as_str(),
        failure_reason,
        None,
        None,
    )?;
    tx.execute(
        "UPDATE worker_attempt_finalizations
         SET queue_effects_applied_at = ?2, updated_at = ?2
         WHERE attempt_id = ?1 AND queue_effects_applied_at IS NULL",
        params![attempt_id, now_iso_string()],
    )?;
    let tasks = load_queue_projection_tasks(&tx, message_keys)?;
    refresh_queue_projection_tasks(root, &tx, &tasks)?;
    tx.commit()?;
    Ok(updated)
}

/// Bind an already-durable queue outcome to its worker attempt without
/// rewriting the queue row. This closes the recovery window for effects (such
/// as Business OS command writeback) that terminalize the queue in their own
/// transaction before the service can persist `effects_completed`.
pub fn mark_worker_attempt_queue_effects_applied_if_status(
    root: &Path,
    attempt_id: &str,
    message_keys: &[String],
    expected_status: &str,
) -> Result<bool> {
    if message_keys.is_empty() {
        return Ok(false);
    }
    let expected_status = canonical_queue_route_status(expected_status)?;
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let tx = conn.unchecked_transaction()?;
    let already_applied: Option<Option<String>> = tx
        .query_row(
            "SELECT queue_effects_applied_at FROM worker_attempt_finalizations WHERE attempt_id = ?1",
            [attempt_id],
            |row| row.get(0),
        )
        .optional()?;
    let already_applied = already_applied.with_context(|| {
        format!("worker attempt {attempt_id} does not exist for queue effect adoption")
    })?;
    if already_applied.is_some() {
        tx.commit()?;
        return Ok(true);
    }
    for message_key in message_keys {
        let route_status = current_queue_route_status(&tx, message_key)?;
        if route_status != expected_status.as_str() {
            tx.commit()?;
            return Ok(false);
        }
    }
    let now = now_iso_string();
    let updated = tx.execute(
        "UPDATE worker_attempt_finalizations
         SET queue_effects_applied_at = ?2, updated_at = ?2
         WHERE attempt_id = ?1 AND queue_effects_applied_at IS NULL",
        params![attempt_id, now],
    )?;
    anyhow::ensure!(
        updated == 1,
        "worker attempt {attempt_id} queue effect adoption was not persisted"
    );
    tx.commit()?;
    Ok(true)
}

pub fn ack_leased_messages_with_reason(
    root: &Path,
    message_keys: &[String],
    status: &str,
    reason: &str,
) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status.as_str())?;
    ack_messages(
        Some(root),
        &mut conn,
        message_keys,
        status.as_str(),
        None,
        Some(reason),
        None,
    )
}

pub(crate) fn ack_leased_messages_with_reason_and_grant(
    root: &Path,
    message_keys: &[String],
    status: &str,
    reason: &str,
    terminal_policy_grant: TerminalPolicyGrant,
) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status.as_str())?;
    ack_messages(
        Some(root),
        &mut conn,
        message_keys,
        status.as_str(),
        None,
        Some(reason),
        Some(terminal_policy_grant),
    )
}

pub fn ack_leased_messages_with_failure_reason(
    root: &Path,
    message_keys: &[String],
    status: &str,
    failure_reason: &str,
) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    anyhow::ensure!(
        status == QueueRouteStatus::Failed,
        "ack_leased_messages_with_failure_reason only accepts status='failed'"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    guard_founder_handled_ack(root, &conn, message_keys, status.as_str())?;
    ack_messages(
        Some(root),
        &mut conn,
        message_keys,
        status.as_str(),
        Some(failure_reason),
        None,
        None,
    )
}

/// Persist a typed completion hold without allowing an unbounded
/// pending→leased→pending loop. External waits become dormant `blocked` rows;
/// technical/evidence/artifact holds consume the existing five-attempt review
/// budget with exponential backoff and terminalize when exhausted.
pub fn hold_leased_messages(
    root: &Path,
    message_keys: &[String],
    reason: &HoldReason,
    summary: &str,
) -> Result<usize> {
    hold_leased_messages_impl(root, None, message_keys, reason, summary)
}

/// I-072: apply a typed hold at most once for a durable worker attempt. The
/// hold transition, retry-budget timestamp, projection refresh, and attempt
/// marker commit in the same SQLite transaction.
pub fn hold_leased_messages_for_attempt(
    root: &Path,
    attempt_id: &str,
    message_keys: &[String],
    reason: &HoldReason,
    summary: &str,
) -> Result<usize> {
    hold_leased_messages_impl(root, Some(attempt_id), message_keys, reason, summary)
}

fn hold_leased_messages_impl(
    root: &Path,
    attempt_id: Option<&str>,
    message_keys: &[String],
    reason: &HoldReason,
    summary: &str,
) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    // Immediate: the transaction reads, then writes across the attached queue
    // projection store the RxDB peer writes constantly. A deferred read cannot be
    // promoted once the peer committed (SQLite 517, "database is locked" at once,
    // without the busy timeout): 15 of 43 worker starts failed so on 11.09.2026.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(attempt_id) = attempt_id {
        let already_applied: Option<Option<String>> = tx
            .query_row(
                "SELECT queue_effects_applied_at FROM worker_attempt_finalizations WHERE attempt_id = ?1",
                [attempt_id],
                |row| row.get(0),
            )
            .optional()?;
        let already_applied = already_applied.with_context(|| {
            format!("worker attempt {attempt_id} does not exist for queue hold")
        })?;
        if already_applied.is_some() {
            tx.commit()?;
            return Ok(0);
        }
    }
    let now = now_iso_string();
    let mut updated = 0usize;
    for message_key in message_keys {
        match reason {
            HoldReason::WaitingExternal(wait_ref) => {
                let route_status = QueueRouteStatus::Blocked;
                let command_transitioned = transition_business_command_for_task_in_transaction(
                    &tx,
                    message_key,
                    route_status.as_str(),
                    None,
                    None,
                    Some(summary.trim()),
                    "waiting_external",
                )?;
                if !command_transitioned {
                    updated += ack_messages_in_transaction(
                        &tx,
                        std::slice::from_ref(message_key),
                        route_status.as_str(),
                        None,
                        Some("waiting_external"),
                        None,
                    )?;
                } else {
                    updated += 1;
                }
                anyhow::ensure!(
                    current_queue_route_status(&tx, message_key)? == route_status.as_str(),
                    "waiting-external hold did not persist the linked queue route"
                );
                tx.execute(
                    r#"
                    UPDATE communication_routing_state
                    SET hold_reason='waiting_external', wait_entity_type=?2,
                        wait_entity_id=?3, retry_not_before=NULL,
                        lease_expires_at=NULL, lease_worker_id=NULL, last_error=?4, updated_at=?5
                    WHERE message_key=?1
                    "#,
                    params![
                        message_key,
                        wait_ref.entity_type,
                        wait_ref.entity_id,
                        summary.trim(),
                        now,
                    ],
                )?;
            }
            HoldReason::Technical { .. }
            | HoldReason::MissingReviewEvidence
            | HoldReason::MissingArtifact => {
                let previous_attempts: i64 = tx
                    .query_row(
                        "SELECT failure_attempt_count FROM communication_routing_state WHERE message_key=?1",
                        params![message_key],
                        |row| row.get(0),
                    )
                    .optional()?
                    .unwrap_or(0);
                let attempts = previous_attempts.saturating_add(1);
                let exhausted = attempts >= 5;
                let failure_class = match reason {
                    HoldReason::Technical { .. } => "technical",
                    HoldReason::MissingReviewEvidence => "missing_review_evidence",
                    HoldReason::MissingArtifact => "missing_artifact",
                    HoldReason::WaitingExternal(_) => unreachable!(),
                };
                let hold_reason = match reason {
                    HoldReason::Technical { policy_id } => format!("technical:{policy_id}"),
                    HoldReason::MissingReviewEvidence => "missing_review_evidence".to_string(),
                    HoldReason::MissingArtifact => "missing_artifact".to_string(),
                    HoldReason::WaitingExternal(_) => unreachable!(),
                };
                let next_status = if exhausted {
                    QueueRouteStatus::Failed
                } else {
                    QueueRouteStatus::Pending
                };
                let retry_not_before = (!exhausted).then(|| {
                    let exponent = u32::try_from(attempts.saturating_sub(1))
                        .unwrap_or(16)
                        .min(16);
                    let seconds = 300_i64
                        .saturating_mul(2_i64.saturating_pow(exponent))
                        .min(3_600);
                    (Utc::now() + Duration::seconds(seconds)).to_rfc3339()
                });
                let command_transitioned = transition_business_command_for_task_in_transaction(
                    &tx,
                    message_key,
                    next_status.as_str(),
                    None,
                    None,
                    Some(summary.trim()),
                    "budgeted_completion_hold",
                )?;
                if !command_transitioned {
                    updated += ack_messages_in_transaction(
                        &tx,
                        std::slice::from_ref(message_key),
                        next_status.as_str(),
                        exhausted.then_some(summary.trim()),
                        (!exhausted).then_some("budgeted_completion_hold"),
                        None,
                    )?;
                } else {
                    updated += 1;
                }
                anyhow::ensure!(
                    current_queue_route_status(&tx, message_key)? == next_status.as_str(),
                    "budgeted completion hold did not persist the linked queue route"
                );
                tx.execute(
                    r#"
                    UPDATE communication_routing_state
                    SET failure_class=?2, failure_attempt_count=?3,
                        retry_not_before=?4, hold_reason=?5,
                        wait_entity_type=NULL, wait_entity_id=NULL,
                        lease_expires_at=NULL, lease_worker_id=NULL, last_error=?6, updated_at=?7
                    WHERE message_key=?1
                    "#,
                    params![
                        message_key,
                        failure_class,
                        attempts,
                        retry_not_before,
                        hold_reason,
                        summary.trim(),
                        now,
                    ],
                )?;
            }
        }
    }
    if let Some(attempt_id) = attempt_id {
        tx.execute(
            "UPDATE worker_attempt_finalizations
             SET queue_effects_applied_at = ?2, updated_at = ?2
             WHERE attempt_id = ?1 AND queue_effects_applied_at IS NULL",
            params![attempt_id, now],
        )?;
    }
    let tasks = load_queue_projection_tasks(&tx, message_keys)?;
    refresh_queue_projection_tasks(root, &tx, &tasks)?;
    tx.commit()?;
    Ok(updated)
}

pub fn wake_messages_waiting_for(root: &Path, entity_type: &str, entity_id: &str) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    attach_queue_projection_store(root, &conn)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let message_keys = {
        let mut statement = tx.prepare(
            r#"
            SELECT message_key
            FROM communication_routing_state
            WHERE route_status='blocked'
              AND hold_reason='waiting_external'
              AND LOWER(wait_entity_type)=LOWER(?1)
              AND wait_entity_id=?2
            "#,
        )?;
        let rows = statement
            .query_map(params![entity_type.trim(), entity_id.trim()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let from_route_status = QueueRouteStatus::Blocked;
    let to_route_status = QueueRouteStatus::Pending;
    let mut updated = 0usize;
    for message_key in &message_keys {
        let changed = tx.execute(
            r#"UPDATE communication_routing_state
               SET route_status=?2, hold_reason=NULL, wait_entity_type=NULL,
                   wait_entity_id=NULL, retry_not_before=NULL, first_pending_at=?3, updated_at=?3
               WHERE message_key=?1 AND route_status='blocked' AND hold_reason='waiting_external'"#,
            params![message_key, to_route_status.as_str(), now],
        )?;
        if changed != 0 {
            enforce_queue_route_status_transition(
                &tx,
                &message_key,
                from_route_status.as_str(),
                to_route_status.as_str(),
                "ctox-wait-wakeup",
                "wake_messages_waiting_for",
            )?;
            updated += changed;
        }
    }
    let tasks = load_queue_projection_tasks(&tx, &message_keys)?;
    refresh_queue_projection_tasks(root, &tx, &tasks)?;
    tx.commit()?;
    Ok(updated)
}

pub fn set_queue_task_route_status(
    root: &Path,
    message_key: &str,
    route_status: &str,
) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    if load_queue_message_from_conn(&conn, message_key)?.is_none() {
        return Ok(false);
    }
    let route_status = canonical_queue_route_status(route_status)?;
    update_queue_task(
        root,
        QueueTaskUpdateRequest {
            message_key: message_key.to_string(),
            route_status: Some(route_status.as_str().to_string()),
            ..Default::default()
        },
    )?;
    Ok(true)
}

mod repair_queue;
pub use repair_queue::create_scrape_repair_queue_task;

pub fn create_queue_task(root: &Path, request: QueueTaskCreateRequest) -> Result<QueueTaskView> {
    create_queue_task_with_metadata(root, request)
}

pub fn create_queue_task_with_metadata(
    root: &Path,
    request: QueueTaskCreateRequest,
) -> Result<QueueTaskView> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    attach_queue_projection_store(root, &conn)?;
    // Immediate: the transaction reads, then writes across the attached queue
    // projection store the RxDB peer writes constantly. A deferred read cannot be
    // promoted once the peer committed (SQLite 517, "database is locked" at once,
    // without the busy timeout): 15 of 43 worker starts failed so on 11.09.2026.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let task = create_queue_task_with_metadata_tx(&tx, request)?;
    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
    tx.commit()?;
    Ok(task)
}

fn create_queue_task_with_metadata_tx(
    tx: &Transaction<'_>,
    request: QueueTaskCreateRequest,
) -> Result<QueueTaskView> {
    let title = request.title.trim();
    let prompt = request.prompt.trim();
    if title.is_empty() {
        anyhow::bail!("queue task title must not be empty");
    }
    if prompt.is_empty() {
        anyhow::bail!("queue task prompt must not be empty");
    }
    let priority = canonical_queue_priority(&request.priority)?;
    let now = now_iso_string();
    let sort_at = queue_sort_at(&priority, &now)?;
    // ref: tickets.rs:4603-4617 — honor a caller-supplied idempotency key so a
    // crash-retried create folds to the same message_key (stable edge_id +
    // ON CONFLICT(message_key) DO UPDATE); default to now-salt when absent so
    // distinct callers are unaffected.
    let idempotency_key = request
        .extra_metadata
        .as_ref()
        .and_then(|extra| metadata_string_value(extra, "idempotency_key"));
    let digest = stable_digest(&format!(
        "{}:{}:{}:{}",
        title,
        prompt,
        request.thread_key.trim(),
        idempotency_key.as_deref().unwrap_or(now.as_str())
    ));
    let message_key = format!("{QUEUE_ACCOUNT_KEY}::{digest}");
    let remote_id = format!("queue-{digest}");
    let mut metadata = json!({
        "source": "ctox-queue",
        "priority": priority,
        "skill": request.suggested_skill.as_deref(),
        "parent_message_key": request.parent_message_key.as_deref(),
        "workspace_root": normalize_workspace_root(request.workspace_root.as_deref())
            .or_else(|| legacy_workspace_root_from_prompt(prompt)),
        "created_at": now,
        "sort_at": sort_at,
    });
    if let Some(extra) = request.extra_metadata {
        merge_object_metadata(&mut metadata, extra);
    }
    enforce_queue_task_spawn(
        tx,
        &metadata,
        request.parent_message_key.as_deref(),
        request.thread_key.trim(),
        &message_key,
        title,
    )?;
    upsert_communication_message_tx(
        tx,
        UpsertMessage {
            message_key: &message_key,
            channel: QUEUE_CHANNEL_NAME,
            account_key: QUEUE_ACCOUNT_KEY,
            thread_key: request.thread_key.trim(),
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "queue",
            sender_display: QUEUE_SENDER_DISPLAY,
            sender_address: QUEUE_SENDER_ADDRESS,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: title,
            preview: &preview_text(prompt, title),
            body_text: prompt,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &sort_at,
            observed_at: &now,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread_tx(tx, request.thread_key.trim())?;
    ensure_routing_rows_for_inbound(tx)?;
    let conversation_id = crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(
        request.thread_key.trim(),
    ));
    crate::lcm::seed_mission_state_for_queue_with(tx, conversation_id, title)?;
    load_queue_task_from_conn(tx, &message_key)?.context("failed to load created queue task")
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn enforce_queue_task_spawn(
    conn: &Connection,
    metadata: &Value,
    parent_message_key: Option<&str>,
    thread_key: &str,
    message_key: &str,
    title: &str,
) -> Result<()> {
    let ticket_self_work_id = metadata_string_value(metadata, "ticket_self_work_id");
    let ticket_self_work_kind = metadata_string_value(metadata, "ticket_self_work_kind");
    let (parent_entity_type, parent_entity_id, spawn_kind, spawn_reason, budget_key, max_attempts) =
        if let Some(work_id) = ticket_self_work_id.clone() {
            (
                "WorkItem".to_string(),
                work_id.clone(),
                "self-work-queue-task".to_string(),
                "publish_self_work_for_execution".to_string(),
                format!("self-work-queue:{work_id}"),
                64,
            )
        } else if let Some(parent_message_key) = parent_message_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            (
                "Message".to_string(),
                parent_message_key.to_string(),
                "queue-task".to_string(),
                "create_queue_task".to_string(),
                format!("queue-task:message:{parent_message_key}"),
                64,
            )
        } else if let Some(command_id) = metadata_string_value(metadata, "business_os_command_id") {
            (
                "ControlPlane".to_string(),
                format!("business-os-command:{command_id}"),
                "queue-task".to_string(),
                "create_business_os_command_queue_task".to_string(),
                format!("queue-task:business-os-command:{command_id}"),
                1,
            )
        } else {
            (
                "Thread".to_string(),
                thread_key.to_string(),
                "queue-task".to_string(),
                "create_queue_task".to_string(),
                format!("queue-task:thread:{thread_key}"),
                64,
            )
        };
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("queue_title".to_string(), title.to_string());
    if let Some(skill) = metadata_string_value(metadata, "skill") {
        edge_metadata.insert("suggested_skill".to_string(), skill);
    }
    if let Some(workspace_root) = metadata_string_value(metadata, "workspace_root") {
        edge_metadata.insert("workspace_root".to_string(), workspace_root);
    }
    if let Some(run_class) = metadata_string_value(metadata, "core_run_class") {
        edge_metadata.insert("core_run_class".to_string(), run_class);
    }
    if let Some(kind) = ticket_self_work_kind {
        edge_metadata.insert("self_work_kind".to_string(), kind);
    }

    enforce_core_spawn_in_transaction(
        conn,
        &CoreSpawnRequest {
            parent_entity_type,
            parent_entity_id,
            child_entity_type: "QueueTask".to_string(),
            child_entity_id: message_key.to_string(),
            spawn_kind,
            spawn_reason,
            actor: "ctox-queue".to_string(),
            checkpoint_key: Some(message_key.to_string()),
            budget_key: Some(budget_key),
            max_attempts: Some(max_attempts),
            metadata: edge_metadata,
        },
    )?;
    Ok(())
}

fn metadata_string_value(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn merge_object_metadata(target: &mut Value, extra: Value) {
    let Some(target_map) = target.as_object_mut() else {
        return;
    };
    let Some(extra_map) = extra.as_object() else {
        return;
    };
    for (key, value) in extra_map {
        target_map.insert(key.clone(), value.clone());
    }
}

pub fn list_queue_tasks(
    root: &Path,
    statuses: &[String],
    limit: usize,
) -> Result<Vec<QueueTaskView>> {
    let db_path = resolve_db_path(root, None);
    let allowed = statuses
        .iter()
        .map(|status| status.trim().to_lowercase())
        .filter(|status| !status.is_empty())
        .collect::<Vec<_>>();
    if !statuses.is_empty() && allowed.is_empty() {
        return Ok(Vec::new());
    }
    let cache_key = queue_task_list_cache_key(&db_path, &allowed, limit);
    let stamp = queue_task_list_cache_stamp(&db_path);
    if let Some(tasks) = cached_queue_task_list(&cache_key, &stamp) {
        return Ok(tasks);
    }

    let conn = open_channel_db(&db_path)?;
    let tasks = if allowed.is_empty() {
        list_queue_tasks_from_conn(&conn, limit)?
    } else {
        list_queue_tasks_from_conn_with_statuses(&conn, &allowed, limit)?
    };
    drop(conn);
    let cache_key = queue_task_list_cache_key(&db_path, &allowed, limit);
    let stamp = queue_task_list_cache_stamp(&db_path);
    #[cfg(test)]
    record_queue_task_list_cache_miss_for_tests(&cache_key);
    store_queue_task_list_cache(cache_key, stamp, tasks.clone());
    Ok(tasks)
}

pub fn load_queue_task_for_business_os_command(
    root: &Path,
    command_id: &str,
) -> Result<Option<QueueTaskView>> {
    let command_id = command_id.trim();
    if command_id.is_empty() {
        return Ok(None);
    }
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    load_queue_task_for_business_os_command_from_conn(&conn, command_id)
}

pub fn count_queue_tasks(root: &Path, statuses: &[String]) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let allowed = statuses
        .iter()
        .map(|status| status.trim().to_lowercase())
        .filter(|status| !status.is_empty())
        .collect::<Vec<_>>();
    if !statuses.is_empty() && allowed.is_empty() {
        return Ok(0);
    }
    let cache_key = queue_task_count_cache_key(&db_path, &allowed);
    let stamp = queue_task_list_cache_stamp(&db_path);
    if let Some(count) = cached_queue_task_count(&cache_key, &stamp) {
        return Ok(count);
    }

    let conn = open_channel_db(&db_path)?;
    let count = if allowed.is_empty() {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM communication_messages
             WHERE channel = ?1
               AND direction = 'inbound'",
            params![QUEUE_CHANNEL_NAME],
            |row| row.get(0),
        )?;
        count.max(0) as usize
    } else {
        count_queue_tasks_from_conn_with_statuses(&conn, &allowed)?
    };
    drop(conn);
    let cache_key = queue_task_count_cache_key(&db_path, &allowed);
    let stamp = queue_task_list_cache_stamp(&db_path);
    #[cfg(test)]
    record_queue_task_count_cache_miss_for_tests(&cache_key);
    store_queue_task_count_cache(cache_key, stamp, count);
    Ok(count)
}

pub(crate) fn pending_queue_task_count_uncached(root: &Path) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let Some(conn) = open_channel_db_read_only(&db_path)? else {
        return Ok(0);
    };
    if !channel_projection_tables_exist(
        &conn,
        &["communication_messages", "communication_routing_state"],
    )? {
        return Ok(0);
    }
    let count: i64 = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND lower(COALESCE(r.route_status, 'pending')) = 'pending'
          AND (
                json_extract(m.metadata_json, '$.not_before') IS NULL
             OR json_extract(m.metadata_json, '$.not_before') = ''
             OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
          )
        "#,
        params![QUEUE_CHANNEL_NAME],
        |row| row.get(0),
    )?;
    Ok(non_negative_i64_to_usize(count))
}

pub(crate) fn queue_task_deferred_until(root: &Path, message_key: &str) -> Result<Option<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    queue_task_deferred_until_from_conn(&conn, message_key)
}

fn queue_task_deferred_until_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        SELECT CASE
            WHEN COALESCE(json_extract(m.metadata_json, '$.not_before'), '') = ''
                THEN r.retry_not_before
            WHEN COALESCE(r.retry_not_before, '') = ''
                THEN json_extract(m.metadata_json, '$.not_before')
            WHEN datetime(r.retry_not_before) > datetime(json_extract(m.metadata_json, '$.not_before'))
                THEN r.retry_not_before
            ELSE json_extract(m.metadata_json, '$.not_before')
        END
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key=m.message_key
        WHERE m.message_key = ?1
          AND m.channel = ?2
          AND m.direction = 'inbound'
          AND (
                datetime(json_extract(m.metadata_json, '$.not_before')) > datetime('now')
             OR datetime(r.retry_not_before) > datetime('now')
          )
        LIMIT 1
        "#,
        params![message_key, QUEUE_CHANNEL_NAME],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub fn load_queue_task(root: &Path, message_key: &str) -> Result<Option<QueueTaskView>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    load_queue_task_from_conn(&conn, message_key)
}

pub(crate) fn load_queue_tasks(root: &Path, message_keys: &[String]) -> Result<Vec<QueueTaskView>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    load_queue_projection_tasks(&conn, message_keys)
}

pub fn load_queue_task_last_error(root: &Path, message_key: &str) -> Result<Option<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    conn.query_row(
        "SELECT NULLIF(TRIM(last_error), '')
         FROM communication_routing_state
         WHERE message_key = ?1
         LIMIT 1",
        params![message_key],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|value| value.flatten())
    .map_err(anyhow::Error::from)
}

pub fn update_queue_task(root: &Path, request: QueueTaskUpdateRequest) -> Result<QueueTaskView> {
    update_queue_task_with_optional_terminal_policy_grant(root, request, None, false, false)
}

pub(crate) fn update_queue_task_with_terminal_policy_grant(
    root: &Path,
    request: QueueTaskUpdateRequest,
    terminal_policy_grant: TerminalPolicyGrant,
) -> Result<QueueTaskView> {
    update_queue_task_with_optional_terminal_policy_grant(
        root,
        request,
        Some(terminal_policy_grant),
        false,
        false,
    )
}

pub(crate) fn control_queue_task(
    root: &Path,
    request: QueueTaskUpdateRequest,
    reset_failure: bool,
) -> Result<QueueTaskView> {
    update_queue_task_with_optional_terminal_policy_grant(root, request, None, reset_failure, true)
}

/// Every linked command must permit release, regardless of link insertion order.
fn ensure_linked_commands_allow_queue_release(conn: &Connection, task_id: &str) -> Result<()> {
    let (validating, terminal_status): (bool, Option<String>) = conn.query_row(
        "SELECT COALESCE(MAX(aggregate.execution_phase = 'validating'), 0),
                MAX(CASE WHEN aggregate.execution_phase = 'terminal'
                         THEN aggregate.terminal_status END)
         FROM business_command_task_links link
         JOIN business_command_aggregates aggregate
           ON aggregate.command_id = link.command_id
         WHERE link.task_id = ?1",
        [task_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    anyhow::ensure!(
        !validating,
        "cannot release queue task `{task_id}` while a linked Business OS command is validating; terminalization or an atomic completion hold must resolve it"
    );
    anyhow::ensure!(
        terminal_status.is_none(),
        "cannot release queue task `{task_id}` for terminal Business OS command status `{}`; submit a new command to retry",
        terminal_status.as_deref().unwrap_or_default()
    );
    Ok(())
}

fn update_queue_task_with_optional_terminal_policy_grant(
    root: &Path,
    request: QueueTaskUpdateRequest,
    terminal_policy_grant: Option<TerminalPolicyGrant>,
    reset_failure: bool,
    cockpit_control: bool,
) -> Result<QueueTaskView> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let current = load_queue_message_from_conn(&conn, &request.message_key)?
        .context("queue task not found")?;
    let requested_route_status = request
        .route_status
        .as_deref()
        .map(canonical_queue_route_status)
        .transpose()?;
    if requested_route_status.is_some_and(QueueRouteStatus::is_pending) {
        ensure_linked_commands_allow_queue_release(&conn, &request.message_key)?;
    }
    let current_metadata = queue_metadata_object(&current.metadata);
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.subject.trim())
        .to_string();
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.body_text.trim())
        .to_string();
    let thread_key = request
        .thread_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(current.thread_key.trim())
        .to_string();
    let priority = if let Some(priority) = request.priority.as_deref() {
        canonical_queue_priority(priority)?
    } else {
        current_queue_priority(&current)
    };
    let now = now_iso_string();
    let sort_at = queue_sort_at(&priority, &now)?;
    let mut metadata = current_metadata;
    metadata.insert(
        "source".to_string(),
        Value::String("ctox-queue".to_string()),
    );
    metadata.insert("priority".to_string(), Value::String(priority.clone()));
    metadata.insert("sort_at".to_string(), Value::String(sort_at.clone()));
    if metadata.get("created_at").is_none() {
        metadata.insert(
            "created_at".to_string(),
            Value::String(current.observed_at.clone()),
        );
    }
    if let Some(skill) = request
        .suggested_skill
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("skill".to_string(), Value::String(skill.to_string()));
    } else if request.clear_skill {
        metadata.remove("skill");
    }
    if let Some(workspace_root) = normalize_workspace_root(request.workspace_root.as_deref()) {
        metadata.insert("workspace_root".to_string(), Value::String(workspace_root));
    } else if request.clear_workspace_root {
        metadata.remove("workspace_root");
    } else if metadata
        .get("workspace_root")
        .and_then(Value::as_str)
        .and_then(|value| normalize_workspace_root(Some(value)))
        .is_none()
    {
        if let Some(workspace_root) = legacy_workspace_root_from_prompt(&prompt) {
            metadata.insert("workspace_root".to_string(), Value::String(workspace_root));
        }
    }
    if let Some(note) = request
        .status_note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert("status_note".to_string(), Value::String(note.to_string()));
    } else if request.clear_note {
        metadata.remove("status_note");
    }
    let releases_deferred_work = requested_route_status.is_some_and(QueueRouteStatus::is_pending);
    if releases_deferred_work {
        metadata.remove("not_before");
        metadata.remove("defer_reason");
    }
    attach_queue_projection_store(root, &conn)?;
    // Immediate: the transaction reads, then writes across the attached queue
    // projection store the RxDB peer writes constantly. A deferred read cannot be
    // promoted once the peer committed (SQLite 517, "database is locked" at once,
    // without the busy timeout): 15 of 43 worker starts failed so on 11.09.2026.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if cockpit_control {
        let status = current_queue_route_status(&tx, &current.message_key)?;
        anyhow::ensure!(
            status != "leased",
            "active leases must finish before a cockpit queue control"
        );
        if reset_failure {
            anyhow::ensure!(
                matches!(status.as_str(), "failed" | "blocked"),
                "retry requires a failed or blocked task"
            );
        }
        if releases_deferred_work {
            ensure_linked_commands_allow_queue_release(&tx, &current.message_key)?;
        }
    }
    upsert_communication_message_tx(
        &tx,
        UpsertMessage {
            message_key: &current.message_key,
            channel: QUEUE_CHANNEL_NAME,
            account_key: QUEUE_ACCOUNT_KEY,
            thread_key: &thread_key,
            remote_id: &current.remote_id,
            direction: "inbound",
            folder_hint: "queue",
            sender_display: QUEUE_SENDER_DISPLAY,
            sender_address: QUEUE_SENDER_ADDRESS,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &title,
            preview: &preview_text(&prompt, &title),
            body_text: &prompt,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: current.seen,
            has_attachments: false,
            external_created_at: &sort_at,
            observed_at: &now,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    if let Some(route_status) = requested_route_status {
        let status_note = request
            .status_note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let command_transitioned = (route_status.is_pending()
            || route_status == QueueRouteStatus::Cancelled)
            && transition_business_command_for_task_in_transaction(
                &tx,
                &current.message_key,
                route_status.as_str(),
                None,
                None,
                status_note,
                status_note.unwrap_or("queue task released for retry"),
            )?;
        if !command_transitioned {
            set_routing_status(
                &tx,
                &current.message_key,
                route_status.as_str(),
                &now,
                "ctox-queue-update",
                "update_queue_task",
                status_note,
                terminal_policy_grant,
            )?;
        }
    }
    if reset_failure {
        anyhow::ensure!(
            requested_route_status.is_some_and(QueueRouteStatus::is_pending),
            "retry must release to pending"
        );
        tx.execute("UPDATE communication_routing_state SET failure_class=NULL,failure_attempt_count=0,retry_not_before=NULL,hold_reason=NULL,wait_entity_type=NULL,wait_entity_id=NULL WHERE message_key=?1", [&current.message_key])?;
    }
    if cockpit_control && requested_route_status == Some(QueueRouteStatus::Blocked) {
        tx.execute(
            "UPDATE communication_routing_state SET hold_reason=?2 WHERE message_key=?1",
            params![current.message_key, request.status_note],
        )?;
    }
    refresh_thread_tx(&tx, &thread_key)?;
    let updated = load_queue_task_from_conn(&tx, &current.message_key)?
        .context("failed to load updated queue task")?;
    refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&updated))?;
    tx.commit()?;
    Ok(updated)
}

pub fn set_queue_task_metadata_value(
    root: &Path,
    message_key: &str,
    key: &str,
    value: Value,
) -> Result<()> {
    let key = key.trim();
    anyhow::ensure!(!key.is_empty(), "queue metadata key must not be empty");
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let current =
        load_queue_message_from_conn(&conn, message_key)?.context("queue task not found")?;
    anyhow::ensure!(
        current.channel == QUEUE_CHANNEL_NAME && current.direction == "inbound",
        "message `{message_key}` is not a queue task"
    );
    let mut metadata = queue_metadata_object(&current.metadata);
    metadata.insert(key.to_string(), value);
    conn.execute(
        "UPDATE communication_messages
         SET metadata_json = ?2
         WHERE message_key = ?1
           AND channel = 'queue'
           AND direction = 'inbound'",
        params![message_key, serde_json::to_string(&metadata)?],
    )?;
    Ok(())
}

pub fn queue_task_metadata_value(
    root: &Path,
    message_key: &str,
    key: &str,
) -> Result<Option<Value>> {
    let key = key.trim();
    anyhow::ensure!(!key.is_empty(), "queue metadata key must not be empty");
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let Some(current) = load_queue_message_from_conn(&conn, message_key)? else {
        return Ok(None);
    };
    if current.channel != QUEUE_CHANNEL_NAME || current.direction != "inbound" {
        return Ok(None);
    }
    Ok(current.metadata.get(key).cloned())
}

pub fn lease_queue_task(
    root: &Path,
    message_key: &str,
    lease_owner: &str,
) -> Result<QueueTaskView> {
    anyhow::ensure!(
        !crate::business_os::harness_cockpit::queue_is_paused(root),
        "queue admission is paused"
    );
    let normalized_owner = lease_owner.trim();
    anyhow::ensure!(
        !normalized_owner.is_empty(),
        "lease owner must not be empty"
    );
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_queue_account(&mut conn)?;
    let current =
        load_queue_message_from_conn(&conn, message_key)?.context("queue task not found")?;
    // Thread refresh is ancillary routing maintenance. Run it before the lease
    // transaction so a transient write-lock error cannot be reported after the
    // lease has already committed and strand work between `pending` and worker
    // activation.
    refresh_thread(&mut conn, &current.thread_key)?;
    let now = now_iso_string();
    let lease_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    // Hold a write lock across the read-modify-write so a concurrent leaser on
    // a separate connection cannot overwrite our lease_owner (lost-update).
    attach_queue_projection_store(root, &conn)?;
    let leased = {
        let tx =
            rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Immediate)?;
        if let Some(not_before) = queue_task_deferred_until_from_conn(&tx, message_key)? {
            anyhow::bail!("queue task {message_key} is deferred until {not_before}");
        }
        let previous_route_status = current_queue_route_status(&tx, message_key)?;
        // Check-and-set: only lease a row that is free, ours already, or
        // pending. A losing racer flips 0 rows and must NOT load a task it
        // does not own.
        // Record the core-transition proof only after the CAS actually flips
        // the row, so a losing racer never writes a phantom proof.
        let updated = tx.execute(
            r#"INSERT INTO communication_routing_state (message_key, route_status, lease_owner, leased_at, first_pending_at, lease_expires_at, lease_worker_id, acked_at, last_error, updated_at, attempt)
               VALUES (?1, ?5, ?2, ?3, ?3, ?4, NULL, NULL, NULL, ?3, 1)
               ON CONFLICT(message_key) DO UPDATE SET route_status=excluded.route_status, lease_owner=excluded.lease_owner, leased_at=excluded.leased_at, first_pending_at=COALESCE(communication_routing_state.first_pending_at, excluded.first_pending_at), lease_expires_at=excluded.lease_expires_at, lease_worker_id=NULL, retry_not_before=NULL, hold_reason=NULL, acked_at=NULL, attempt=communication_routing_state.attempt+1, updated_at=excluded.updated_at
               WHERE communication_routing_state.route_status = 'pending'"#,
            params![
                message_key,
                normalized_owner,
                now,
                lease_expires_at,
                QueueRouteStatus::Leased.as_str()
            ],
        )?;
        if updated != 0 {
            enforce_queue_route_status_transition(
                &tx,
                message_key,
                &previous_route_status,
                "leased",
                "ctox-queue-lease",
                "lease_queue_task",
            )?;
        } else {
            anyhow::bail!(
                "queue task {} lease lost: already leased by another owner",
                message_key
            );
        }
        let leased = load_queue_task_from_conn(&tx, message_key)?
            .context("failed to load leased queue task")?;
        refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&leased))?;
        tx.commit()?;
        leased
    };
    Ok(leased)
}

/// router-3: the age past which a still-leased, unacked queue task is "stuck".
/// Kept in lockstep with the process-mining `stuck_queue_items` diagnostic
/// (process_mining.rs) so the durable escalation and the advisory finding never
/// drift apart.
pub const STALE_QUEUE_LEASE_AGE_MINUTES: i64 = 15;

/// router-3: read-only — `message_key`s of queue-task leases held by `lease_owner`
/// that are older than [`STALE_QUEUE_LEASE_AGE_MINUTES`] and still unacked (the
/// exact condition process-mining flags as `stuck_queue_items`). This does NOT
/// release anything; it lets the reconciler escalate-as-evidence the in-active-key
/// case (a worker wedged mid-slice) that `release_stale_queue_task_leases`
/// intentionally protects from auto-release. The `leased_at < cutoff` comparison
/// mirrors the process-mining query (RFC3339-millis cutoff) so the two surfaces
/// agree on which leases are stuck.
pub fn list_stale_queue_task_leases(root: &Path, lease_owner: &str) -> Result<Vec<String>> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let cutoff = (Utc::now() - Duration::minutes(STALE_QUEUE_LEASE_AGE_MINUTES))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = 'queue'
          AND m.direction = 'inbound'
          AND r.route_status IN ('leased', 'running')
          AND r.lease_owner = ?1
          AND r.leased_at IS NOT NULL
          AND r.leased_at < ?2
          AND (r.acked_at IS NULL OR r.acked_at = '')
        ORDER BY r.leased_at ASC
        LIMIT 128
        "#,
    )?;
    let rows = statement.query_map(params![lease_owner, cutoff], |row| row.get::<_, String>(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

#[derive(Debug, Default)]
pub struct QueueLeaseSweepResult {
    pub released: Vec<String>,
    pub failures: Vec<String>,
}

pub fn release_stale_queue_task_leases(
    root: &Path,
    _lease_owner: &str,
    active_message_keys: &HashSet<String>,
) -> Result<QueueLeaseSweepResult> {
    #[derive(Debug)]
    struct Candidate {
        message_key: String,
        lease_owner: Option<String>,
        leased_at: Option<String>,
        lease_expires_at: Option<String>,
        lease_worker_id: Option<String>,
    }

    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT m.message_key, r.lease_owner, r.leased_at,
               r.lease_expires_at, r.lease_worker_id
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.direction = 'inbound'
          AND r.route_status = 'leased'
          AND (
                r.lease_owner IS NULL
             OR trim(r.lease_owner) = ''
             OR r.leased_at IS NULL
             OR trim(r.leased_at) = ''
             OR r.lease_expires_at IS NULL
             OR trim(r.lease_expires_at) = ''
             OR datetime(r.lease_expires_at) <= datetime('now')
          )
        ORDER BY r.leased_at ASC, r.updated_at ASC
        LIMIT 128
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(Candidate {
            message_key: row.get(0)?,
            lease_owner: row.get(1)?,
            leased_at: row.get(2)?,
            lease_expires_at: row.get(3)?,
            lease_worker_id: row.get(4)?,
        })
    })?;
    let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    attach_queue_projection_store(root, &conn)?;

    let now = now_iso_string();
    let mut result = QueueLeaseSweepResult::default();
    for candidate in candidates {
        if active_message_keys.contains(&candidate.message_key) {
            continue;
        }
        // lease-2 (F-002): one bad candidate must not abort the whole sweep —
        // otherwise every orphaned lease queued behind a deterministically
        // failing row survives its worker forever. Each failing candidate is
        // retried on the next sweep pass; released rows stay released
        // (idempotent), so a re-run never duplicates the linked command.
        let outcome = (|| -> Result<bool> {
            let tx = conn.transaction()?;
            // Acquire SQLite's write lock while proving that every lease
            // identity field still matches the stale candidate. A renewed or
            // re-leased row changes at least one field and is left untouched.
            let claimed = tx.execute(
                r#"
                UPDATE communication_routing_state
                SET updated_at=updated_at
                WHERE message_key=?1
                  AND route_status='leased'
                  AND lease_owner IS ?2
                  AND leased_at IS ?3
                  AND lease_expires_at IS ?4
                  AND lease_worker_id IS ?5
                "#,
                params![
                    candidate.message_key,
                    candidate.lease_owner,
                    candidate.leased_at,
                    candidate.lease_expires_at,
                    candidate.lease_worker_id,
                ],
            )?;
            if claimed != 1 {
                tx.rollback()?;
                return Ok(false);
            }
            if transition_business_command_for_task_in_transaction(
                &tx,
                &candidate.message_key,
                "pending",
                None,
                None,
                Some("stale or ownerless queue lease recovered"),
                "stale or ownerless queue lease recovered",
            )? {
                let task = load_queue_task_from_conn(&tx, &candidate.message_key)?
                    .context("failed to load recovered queue task")?;
                refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
                tx.commit()?;
                return Ok(true);
            }
            let from_route_status = QueueRouteStatus::Leased;
            let to_route_status = QueueRouteStatus::Pending;
            enforce_queue_route_status_transition(
                &tx,
                &candidate.message_key,
                from_route_status.as_str(),
                to_route_status.as_str(),
                "ctox-communication-lease-repair",
                "release_stale_queue_task_leases",
            )?;
            let updated = tx.execute(
                r#"
            UPDATE communication_routing_state
            SET route_status=?2,
                lease_owner=NULL,
                leased_at=NULL,
                lease_expires_at=NULL,
                lease_worker_id=NULL,
                acked_at=NULL,
                last_error=NULL,
                updated_at=?3
            WHERE message_key = ?1
              AND route_status = 'leased'
              AND lease_owner IS ?4
              AND leased_at IS ?5
              AND lease_expires_at IS ?6
              AND lease_worker_id IS ?7
            "#,
                params![
                    candidate.message_key,
                    to_route_status.as_str(),
                    now,
                    candidate.lease_owner,
                    candidate.leased_at,
                    candidate.lease_expires_at,
                    candidate.lease_worker_id,
                ],
            )?;
            anyhow::ensure!(
                updated == 1,
                "stale lease identity changed before recovery write"
            );
            let task = load_queue_task_from_conn(&tx, &candidate.message_key)?
                .context("failed to load recovered queue task")?;
            refresh_queue_projection_tasks(root, &tx, std::slice::from_ref(&task))?;
            tx.commit()?;
            Ok(true)
        })();
        match outcome {
            Ok(true) => result.released.push(candidate.message_key),
            Ok(false) => {}
            Err(err) => result
                .failures
                .push(format!("{}: {err}", candidate.message_key)),
        }
    }
    Ok(result)
}

pub fn renew_message_leases(
    root: &Path,
    lease_owner: &str,
    message_keys: &[String],
) -> Result<usize> {
    if message_keys.is_empty() {
        return Ok(0);
    }
    let conn = open_channel_db(&resolve_db_path(root, None))?;
    let now = now_iso_string();
    let lease_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    let mut renewed = 0usize;
    for message_key in message_keys {
        renewed += conn.execute(
            r#"
            UPDATE communication_routing_state
            SET lease_expires_at=?3, updated_at=?4
            WHERE message_key=?1 AND route_status='leased' AND lease_owner=?2
            "#,
            params![message_key, lease_owner, lease_expires_at, now],
        )?;
    }
    Ok(renewed)
}

/// lease-3 (F-002): durable worker identity for queue-task leases. The worker
/// that actually starts a leased slice persists its instance-unique id on the
/// lease row so a recovery sweep (or operator) can tell a lease held by a
/// live worker apart from one left behind by a worker that disappeared. This
/// write is deliberately constrained to rows still leased by the expected
/// owner, so a stale worker can never stamp a lease that was already
/// reclaimed and re-leased by someone else.
pub fn record_queue_lease_worker(
    root: &Path,
    message_keys: &[String],
    lease_owner: &str,
    worker_id: &str,
) -> Result<usize> {
    let normalized_worker = worker_id.trim();
    if message_keys.is_empty() || normalized_worker.is_empty() {
        return Ok(0);
    }
    let conn = open_channel_db(&resolve_db_path(root, None))?;
    let now = now_iso_string();
    let mut updated = 0usize;
    for message_key in message_keys {
        updated += conn.execute(
            r#"
            UPDATE communication_routing_state
            SET lease_worker_id=?3, updated_at=?4
            WHERE message_key=?1 AND route_status='leased' AND lease_owner=?2
            "#,
            params![message_key, lease_owner, normalized_worker, now],
        )?;
    }
    Ok(updated)
}

/// lease-3 (F-002): read-only lease health probe for recovery sweeps and
/// status projections. A leased route is stalled when its lease is incomplete
/// (missing owner or timestamps) or its expiry has passed without a worker
/// heartbeat renewal — i.e. no live worker can still own it. Mirrors the
/// candidate condition in `release_stale_queue_task_leases`.
pub fn queue_task_lease_stalled(root: &Path, message_key: &str) -> Result<bool> {
    let conn = open_channel_db(&resolve_db_path(root, None))?;
    let stalled = conn.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM communication_routing_state
            WHERE message_key = ?1
              AND route_status = 'leased'
              AND (
                    lease_owner IS NULL
                 OR trim(lease_owner) = ''
                 OR leased_at IS NULL
                 OR trim(leased_at) = ''
                 OR lease_expires_at IS NULL
                 OR trim(lease_expires_at) = ''
                 OR datetime(lease_expires_at) <= datetime('now')
              )
        )
        "#,
        params![message_key],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(stalled)
}

pub fn ingest_cron_message(
    root: &Path,
    run_id: &str,
    thread_key: &str,
    task_name: &str,
    body: &str,
    skill: Option<&str>,
    scheduled_for: &str,
) -> Result<String> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_account(
        &mut conn,
        "cron:system",
        "cron",
        "ctox scheduler",
        "system",
        json!({"source": "cron"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!("cron-{run_id}");
    let message_key = format!("cron:system::{remote_id}");
    let metadata = json!({
        "source": "ctox-schedule",
        "task_name": task_name,
        "skill": skill,
        "scheduled_for": scheduled_for,
        "run_id": run_id,
    });
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("task_name".to_string(), task_name.to_string());
    edge_metadata.insert("scheduled_for".to_string(), scheduled_for.to_string());
    enforce_core_spawn(
        &conn,
        &CoreSpawnRequest {
            parent_entity_type: "ScheduleTask".to_string(),
            parent_entity_id: task_name.to_string(),
            child_entity_type: "Message".to_string(),
            child_entity_id: message_key.clone(),
            spawn_kind: "schedule-run-message".to_string(),
            spawn_reason: "emit_due_schedule".to_string(),
            actor: "ctox-schedule".to_string(),
            checkpoint_key: Some(run_id.to_string()),
            budget_key: Some(format!("schedule-run:{task_name}:{scheduled_for}")),
            max_attempts: Some(64),
            metadata: edge_metadata,
        },
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "cron",
            account_key: "cron:system",
            thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "schedule",
            sender_display: "CTOX scheduler",
            sender_address: "cron:system",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: task_name,
            preview: &preview_text(body, task_name),
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: scheduled_for,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, thread_key)?;
    ensure_routing_rows_for_inbound(&conn)?;
    Ok(message_key)
}

pub fn ingest_plan_message(
    root: &Path,
    goal_id: &str,
    step_id: &str,
    thread_key: &str,
    goal_title: &str,
    step_title: &str,
    body: &str,
    skill: Option<&str>,
    step_order: i64,
    total_steps: i64,
) -> Result<String> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_account(
        &mut conn,
        "plan:system",
        "plan",
        "ctox planner",
        "system",
        json!({"source": "plan"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!("plan-{goal_id}-{step_id}");
    let message_key = format!("plan:system::{goal_id}::{step_id}");
    let metadata = json!({
        "source": "ctox-plan",
        "goal_id": goal_id,
        "step_id": step_id,
        "goal_title": goal_title,
        "step_title": step_title,
        "skill": skill,
        "step_order": step_order,
        "total_steps": total_steps,
    });
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key.to_string());
    edge_metadata.insert("goal_id".to_string(), goal_id.to_string());
    edge_metadata.insert("goal_title".to_string(), goal_title.to_string());
    edge_metadata.insert("step_title".to_string(), step_title.to_string());
    enforce_core_spawn(
        &conn,
        &CoreSpawnRequest {
            parent_entity_type: "PlanStep".to_string(),
            parent_entity_id: step_id.to_string(),
            child_entity_type: "Message".to_string(),
            child_entity_id: message_key.clone(),
            spawn_kind: "plan-step-message".to_string(),
            spawn_reason: "emit_plan_step".to_string(),
            actor: "ctox-plan".to_string(),
            checkpoint_key: Some(step_id.to_string()),
            budget_key: Some(format!("plan-step:{step_id}")),
            max_attempts: Some(8),
            metadata: edge_metadata,
        },
    )?;
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "plan",
            account_key: "plan:system",
            thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "plan",
            sender_display: "CTOX planner",
            sender_address: "plan:system",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: step_title,
            preview: &preview_text(body, step_title),
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, thread_key)?;
    ensure_routing_rows_for_inbound(&conn)?;
    Ok(message_key)
}

/// Outcome of an IoT-event durable-work emission: which durable message key was
/// upserted (if any) and the spawn proof recorded in `ctox_core_spawn_edges`.
#[derive(Debug)]
pub struct IotEventEmitOutcome {
    /// The durable `communication_messages` key, present only when the spawn was
    /// accepted by the budget. Idempotent: the same `dedup_key` always maps to
    /// the same message key, so repeated matches collapse to ONE durable task.
    pub message_key: Option<String>,
    /// The recorded spawn-edge proof (accepted or budget-exhausted/rejected).
    pub spawn: CoreSpawnProof,
}

/// Emit ONE durable queue task for a matched IoT condition (§4A surface 3 /
/// §2A.15). This is the IoT analogue of `ingest_cron_message` /
/// `ingest_plan_message`: it is CTOX's mission brain doing the firing — there is
/// NO second automation engine in `iot::conditions`.
///
/// Boundedness comes from TWO complementary CTOX-native mechanisms (the plan's
/// "thin condition layer, not a second engine" decision):
///   * the durable `message_key` is derived from `dedup_key` and is the
///     `communication_messages` PRIMARY KEY, so re-firing the same condition
///     UPSERTs the same row — EXACTLY ONE durable queue task per dedup key
///     (§2A.15 re-trigger suppression, delegated to `queue.rs`/dedup), and
///   * every emission is a budget-bounded spawn edge under `budget_key`
///     (parent `IotAlarm` → child `QueueTask`, the registered
///     `iot-event-queue-task` contract family), so the *number of re-fires* is
///     provably bounded by the spawn budget recorded in `ctox_core_spawn_edges`
///     (§2A.20), not by a ported 100-trigger cap.
///
/// When the spawn budget is exhausted the durable message is NOT (re)written and
/// `message_key` is `None`; the caller treats that as suppressed re-firing.
#[allow(clippy::too_many_arguments)]
pub fn ingest_iot_event_message(
    root: &Path,
    alarm_id: &str,
    ruleset_id: &str,
    asset_id: &str,
    dedup_key: &str,
    budget_key: &str,
    max_attempts: i64,
    rule_name: &str,
    body: &str,
    skill: Option<&str>,
    observed_at: &str,
) -> Result<IotEventEmitOutcome> {
    let db_path = resolve_db_path(root, None);
    let mut conn = open_channel_db(&db_path)?;
    ensure_core_transition_guard_schema(&conn)?;
    ensure_account(
        &mut conn,
        "iot:system",
        "iot",
        "ctox IoT engine",
        "system",
        json!({"source": "iot"}),
    )?;

    // The durable task key IS the dedup key: the same matched condition for the
    // same asset/ruleset always resolves to the same `communication_messages`
    // PRIMARY KEY, so repeated matches UPSERT one row (one durable queue task).
    let message_key = format!("iot:system::{dedup_key}");
    let thread_key = format!("iot:{ruleset_id}:{asset_id}");
    let remote_id = format!("iot-{dedup_key}");

    // Budget-bounded spawn edge (parent IotAlarm → child QueueTask). The child
    // entity id is the per-fire alarm-scoped key so each accepted re-fire
    // consumes one unit of the finite budget recorded in ctox_core_spawn_edges.
    let child_id = format!("iot-qt:{alarm_id}");
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("iot_alarm_id".to_string(), alarm_id.to_string());
    edge_metadata.insert("iot_ruleset_id".to_string(), ruleset_id.to_string());
    edge_metadata.insert("iot_asset_id".to_string(), asset_id.to_string());
    edge_metadata.insert("dedup_key".to_string(), dedup_key.to_string());
    edge_metadata.insert("message_key".to_string(), message_key.clone());
    let spawn = evaluate_core_spawn(
        &conn,
        &CoreSpawnRequest {
            parent_entity_type: "IotAlarm".to_string(),
            parent_entity_id: alarm_id.to_string(),
            child_entity_type: "QueueTask".to_string(),
            child_entity_id: child_id,
            spawn_kind: "iot-event-queue-task".to_string(),
            spawn_reason: "iot_condition_match".to_string(),
            actor: "iot-conditions".to_string(),
            checkpoint_key: Some(alarm_id.to_string()),
            budget_key: Some(budget_key.to_string()),
            max_attempts: Some(max_attempts),
            metadata: edge_metadata,
        },
    )?;
    if !spawn.accepted {
        // Budget exhausted (or otherwise rejected): re-firing is bounded, so we
        // do NOT (re)write the durable task. One brain, finite budget.
        return Ok(IotEventEmitOutcome {
            message_key: None,
            spawn,
        });
    }

    let metadata = json!({
        "source": "ctox-iot",
        "iot_alarm_id": alarm_id,
        "iot_ruleset_id": ruleset_id,
        "iot_asset_id": asset_id,
        "dedup_key": dedup_key,
        "skill": skill,
    });
    upsert_communication_message(
        &mut conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "iot",
            account_key: "iot:system",
            thread_key: &thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "iot",
            sender_display: "CTOX IoT engine",
            sender_address: "iot:system",
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: rule_name,
            preview: &preview_text(body, rule_name),
            body_text: body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: observed_at,
            observed_at,
            metadata_json: &serde_json::to_string(&metadata)?,
        },
    )?;
    refresh_thread(&mut conn, &thread_key)?;
    ensure_routing_rows_for_inbound(&conn)?;
    Ok(IotEventEmitOutcome {
        message_key: Some(message_key),
        spawn,
    })
}

#[derive(Debug, Serialize)]
struct ChannelMessageView {
    message_key: String,
    channel: String,
    account_key: String,
    thread_key: String,
    remote_id: String,
    direction: String,
    folder_hint: String,
    sender_display: String,
    sender_address: String,
    subject: String,
    preview: String,
    body_text: String,
    status: String,
    seen: bool,
    external_created_at: String,
    observed_at: String,
    metadata: Value,
    routing: RoutingView,
}

#[derive(Debug)]
struct MessageAddressing {
    recipient_addresses: Vec<String>,
    cc_addresses: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CommunicationStateCandidate {
    kind: String,
    message_key: String,
    channel: String,
    thread_key: String,
    created_at: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct CommunicationContextView {
    thread_key: String,
    latest_subject: Option<String>,
    latest_inbound: Option<CommunicationStateCandidate>,
    latest_outbound: Option<CommunicationStateCandidate>,
    thread_messages: Vec<ChannelMessageView>,
    related_messages: Vec<ChannelMessageView>,
    candidate_blockers: Vec<CommunicationStateCandidate>,
    candidate_promises: Vec<CommunicationStateCandidate>,
    open_owner_questions: Vec<CommunicationStateCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutedInboundMessage {
    pub message_key: String,
    pub channel: String,
    pub account_key: String,
    pub thread_key: String,
    pub sender_display: String,
    pub sender_address: String,
    pub subject: String,
    pub preview: String,
    pub body_text: String,
    pub external_created_at: String,
    pub workspace_root: Option<String>,
    pub metadata: Value,
    pub preferred_reply_modality: Option<String>,
}

#[derive(Debug, Serialize)]
struct RoutingView {
    route_status: String,
    lease_owner: Option<String>,
    leased_at: Option<String>,
    acked_at: Option<String>,
    last_error: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct TuiIngestRequest {
    account_key: String,
    thread_key: String,
    body: String,
    subject: String,
    sender_display: String,
    sender_address: String,
    metadata: Value,
}

#[derive(Debug)]
struct ChannelSendRequest {
    channel: String,
    account_key: String,
    thread_key: String,
    body: String,
    subject: String,
    to: Vec<String>,
    cc: Vec<String>,
    attachments: Vec<String>,
    sender_display: Option<String>,
    sender_address: Option<String>,
    send_voice: bool,
    reviewed_founder_send: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FounderReplyAction {
    pub account_key: String,
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FounderOutboundAction {
    pub account_key: String,
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ExternalChatAction {
    pub channel: String,
    pub account_key: String,
    pub thread_key: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub attachments: Vec<String>,
}

fn sync_channel(root: &Path, db_path: &Path, channel: &str, args: &[String]) -> Result<Value> {
    let conn = open_channel_db(db_path)?;
    match communication_adapters::external_adapter_for_channel(channel) {
        Some(communication_adapters::ExternalCommunicationAdapter::Discord(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Email(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::GoogleChat(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Jami(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Matrix(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Mattermost(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Meeting(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Slack(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Teams(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Telegram(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Whatsapp(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        Some(communication_adapters::ExternalCommunicationAdapter::Zulip(adapter)) => {
            let adapter_json = adapter.sync_cli(
                root,
                &communication_adapters::AdapterSyncCommandRequest {
                    db_path,
                    passthrough_args: args,
                    skip_flags: &["--db", "--channel"],
                },
            )?;
            ensure_routing_rows_for_inbound(&conn)?;
            Ok(json!({
                "ok": true,
                "channel": adapter.channel_name(),
                "db_path": db_path,
                "adapter_result": adapter_json,
            }))
        }
        None => anyhow::bail!("unsupported channel sync target: {channel}"),
    }
}

fn send_message(root: &Path, db_path: &Path, request: ChannelSendRequest) -> Result<Value> {
    let mut conn = open_channel_db(db_path)?;
    let request = resolve_outbound_subject(&conn, request)?;
    enforce_external_chat_send_is_reviewed(&request)?;
    enforce_external_work_ack_has_pipeline_backing(&conn, &request)?;
    enforce_channel_attachment_support(&request)?;
    let reviewed_external_chat_approval =
        if request.reviewed_founder_send && is_reviewed_external_chat_channel(&request.channel) {
            let action = external_chat_action_from_send_request(&request);
            Some(require_any_unconsumed_external_chat_review(
                &conn,
                &action,
                &request.body,
            )?)
        } else {
            None
        };
    macro_rules! send_chat_adapter {
        ($factory:ident, $channel:literal) => {{
            let _core_send_proof = enforce_reviewed_communication_send_core_transition_if_approved(
                &conn,
                &request,
                reviewed_external_chat_approval.as_ref(),
            )?;
            let adapter = communication_adapters::$factory();
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::ChatSendCommandRequest {
                    db_path,
                    account_key: &request.account_key,
                    thread_key: &request.thread_key,
                    to: &request.to,
                    cc: &request.cc,
                    sender_display: request.sender_display.as_deref(),
                    subject: &request.subject,
                    body: &request.body,
                    attachments: &request.attachments,
                },
            )?;
            if let Some((approval_key, _anchor_key)) = reviewed_external_chat_approval.as_ref() {
                mark_founder_reply_review_sent(&conn, approval_key, &adapter_json)?;
            }
            Ok(json!({
                "ok": true,
                "channel": $channel,
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }};
    }
    match request.channel.as_str() {
        "tui" => {
            let message_key = store_tui_outbound_message(&mut conn, &request)?;
            Ok(json!({
                "ok": true,
                "channel": "tui",
                "db_path": db_path,
                "message_key": message_key,
                "status": "sent",
            }))
        }
        "email" => {
            let settings = runtime_settings_with_owner_profiles(
                root,
                communication_gateway::CommunicationAdapterKind::Email,
            );
            if request.reviewed_founder_send {
                let action = external_chat_action_from_send_request(&request);
                if let Ok((approval_key, _anchor_key)) =
                    require_any_unconsumed_external_chat_review(&conn, &action, &request.body)
                {
                    return send_reviewed_email_communication_request(
                        root,
                        &conn,
                        db_path,
                        &request,
                        &approval_key,
                    );
                }
                return send_reviewed_founder_outbound_request(root, &conn, db_path, &request);
            }
            validate_founder_outbound_email(&settings, &request)?;
            send_email_message(root, &conn, db_path, &request, None)
        }
        "discord" => send_chat_adapter!(discord, "discord"),
        "google_chat" => send_chat_adapter!(google_chat, "google_chat"),
        "jami" => {
            let _core_send_proof = enforce_reviewed_communication_send_core_transition_if_approved(
                &conn,
                &request,
                reviewed_external_chat_approval.as_ref(),
            )?;
            let adapter = communication_adapters::jami();
            let sender = request
                .sender_address
                .clone()
                .unwrap_or_else(|| jami_address_from_account_key(&request.account_key));
            let send_voice =
                request.send_voice || thread_prefers_voice_reply(&conn, &request.thread_key)?;
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::JamiSendCommandRequest {
                    db_path,
                    account_id: &sender,
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    subject: &request.subject,
                    body: &request.body,
                    send_voice,
                    attachments: &request.attachments,
                },
            )?;
            if let Some((approval_key, _anchor_key)) = reviewed_external_chat_approval.as_ref() {
                mark_founder_reply_review_sent(&conn, approval_key, &adapter_json)?;
            }
            Ok(json!({
                "ok": true,
                "channel": "jami",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("queued"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "matrix" => send_chat_adapter!(matrix, "matrix"),
        "mattermost" => send_chat_adapter!(mattermost, "mattermost"),
        "teams" => {
            let _core_send_proof = enforce_reviewed_communication_send_core_transition_if_approved(
                &conn,
                &request,
                reviewed_external_chat_approval.as_ref(),
            )?;
            let adapter = communication_adapters::teams();
            let account_config = load_account_config(&conn, &request.account_key)?;
            let tenant_id = teams_tenant_from_account_config(account_config.as_ref());
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::TeamsSendCommandRequest {
                    db_path,
                    tenant_id: tenant_id.as_deref().unwrap_or_default(),
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    subject: &request.subject,
                    body: &request.body,
                    attachments: &request.attachments,
                },
            )?;
            if let Some((approval_key, _anchor_key)) = reviewed_external_chat_approval.as_ref() {
                mark_founder_reply_review_sent(&conn, approval_key, &adapter_json)?;
            }
            Ok(json!({
                "ok": true,
                "channel": "teams",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "slack" => send_chat_adapter!(slack, "slack"),
        "telegram" => send_chat_adapter!(telegram, "telegram"),
        "whatsapp" => {
            let _core_send_proof = enforce_reviewed_communication_send_core_transition_if_approved(
                &conn,
                &request,
                reviewed_external_chat_approval.as_ref(),
            )?;
            let adapter = communication_adapters::whatsapp();
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::WhatsappSendCommandRequest {
                    db_path,
                    account_key: &request.account_key,
                    thread_key: &request.thread_key,
                    to: &request.to,
                    sender_display: request.sender_display.as_deref(),
                    body: &request.body,
                    attachments: &request.attachments,
                },
            )?;
            if let Some((approval_key, _anchor_key)) = reviewed_external_chat_approval.as_ref() {
                mark_founder_reply_review_sent(&conn, approval_key, &adapter_json)?;
            }
            Ok(json!({
                "ok": true,
                "channel": "whatsapp",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "delivery_confirmed": adapter_json
                    .get("delivery")
                    .and_then(|value| value.get("confirmed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "adapter_result": adapter_json,
            }))
        }
        "zulip" => send_chat_adapter!(zulip, "zulip"),
        "meeting" => {
            let _core_send_proof = enforce_reviewed_communication_send_core_transition_if_approved(
                &conn,
                &request,
                reviewed_external_chat_approval.as_ref(),
            )?;
            let adapter = communication_adapters::meeting();
            let session_id = &request.thread_key;
            let adapter_json = adapter.send_cli(
                root,
                &communication_adapters::MeetingSendCommandRequest {
                    db_path,
                    session_id,
                    body: &request.body,
                },
            )?;
            if let Some((approval_key, _anchor_key)) = reviewed_external_chat_approval.as_ref() {
                mark_founder_reply_review_sent(&conn, approval_key, &adapter_json)?;
            }
            Ok(json!({
                "ok": true,
                "channel": "meeting",
                "db_path": db_path,
                "status": adapter_json
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("sent"),
                "adapter_result": adapter_json,
            }))
        }
        other => anyhow::bail!("unsupported channel send target: {other}"),
    }
}

fn enforce_channel_attachment_support(request: &ChannelSendRequest) -> Result<()> {
    if request.attachments.is_empty() {
        return Ok(());
    }
    if chat_channel_is_text_only_v1(&request.channel) {
        anyhow::bail!(
            "{} attachments are not supported by the native chat adapter v1. Send a text-only message or upload/share the file through a supported channel after attachment handling has a provider-specific security review.",
            request.channel
        );
    }
    Ok(())
}

fn chat_channel_is_text_only_v1(channel: &str) -> bool {
    matches!(
        channel,
        "slack" | "discord" | "telegram" | "matrix" | "mattermost" | "zulip" | "google_chat"
    )
}

fn ensure_routing_state_hardening_columns(conn: &Connection) -> Result<()> {
    for (column, definition) in [
        ("first_pending_at", "TEXT"),
        ("lease_expires_at", "TEXT"),
        ("lease_worker_id", "TEXT"),
        ("failure_class", "TEXT"),
        ("failure_attempt_count", "INTEGER NOT NULL DEFAULT 0"),
        ("attempt", "INTEGER NOT NULL DEFAULT 0"),
        ("retry_not_before", "TEXT"),
        ("priority_time_credit_hours", "INTEGER NOT NULL DEFAULT 0"),
        ("hold_reason", "TEXT"),
        ("wait_entity_type", "TEXT"),
        ("wait_entity_id", "TEXT"),
    ] {
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('communication_routing_state') WHERE name=?1)",
            params![column],
            |row| row.get(0),
        )?;
        if exists == 0 {
            if let Err(err) = conn.execute_batch(&format!(
                "ALTER TABLE communication_routing_state ADD COLUMN {column} {definition};"
            )) {
                if !is_duplicate_column_error(&err) {
                    return Err(err).with_context(|| {
                        format!("failed to add communication_routing_state.{column}")
                    });
                }
            }
        }
    }
    conn.execute(
        r#"
        UPDATE communication_routing_state
        SET first_pending_at=COALESCE(
                first_pending_at,
                (SELECT COALESCE(m.external_created_at, m.observed_at)
                 FROM communication_messages m
                 WHERE m.message_key=communication_routing_state.message_key),
                updated_at
            ),
            lease_expires_at=CASE
                WHEN route_status='leased' AND lease_expires_at IS NULL
                THEN datetime(leased_at, '+15 minutes')
                ELSE lease_expires_at
            END
        WHERE first_pending_at IS NULL
           OR (route_status='leased' AND lease_expires_at IS NULL)
        "#,
        [],
    )?;
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_cockpit_queue_status_time ON communication_routing_state(route_status,updated_at DESC,message_key DESC);
        CREATE INDEX IF NOT EXISTS idx_cockpit_queue_terminal_time ON communication_routing_state(updated_at DESC,message_key DESC) WHERE route_status IN ('handled','failed','cancelled');")?;
    Ok(())
}

/// Add the `terminal_no_send` column to
/// `communication_founder_reply_reviews` on existing databases that
/// were created before the NO-SEND verdict was a structured field.
/// New databases pick the column up from the CREATE TABLE statement
/// in this same migration block. This is idempotent: we probe via
/// `pragma_table_info` and only ALTER when the column is missing.
/// True when a rusqlite error is SQLite's "duplicate column name" error, which
/// is raised when an `ALTER TABLE ... ADD COLUMN` targets a column that already
/// exists. A probe-then-ALTER migration can hit this when a concurrent process
/// added the column between the probe and the ALTER, in which case the desired
/// end state already holds and the error is benign.
fn is_duplicate_column_error(err: &rusqlite::Error) -> bool {
    err.to_string()
        .to_ascii_lowercase()
        .contains("duplicate column name")
}

fn ensure_terminal_no_send_column(conn: &Connection) -> Result<()> {
    let column_exists: bool = conn
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM pragma_table_info('communication_founder_reply_reviews')
                WHERE name = 'terminal_no_send'
            )
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .unwrap_or(false);
    if !column_exists {
        if let Err(err) = conn.execute(
            "ALTER TABLE communication_founder_reply_reviews ADD COLUMN terminal_no_send INTEGER NOT NULL DEFAULT 0",
            [],
        ) {
            // Cross-process race: another writer may have added the column
            // between the probe above and this ALTER. A duplicate-column error
            // means the column now exists, which is exactly the desired end
            // state, so tolerate it instead of failing open_channel_db.
            if !is_duplicate_column_error(&err) {
                return Err(err).context(
                    "failed to add terminal_no_send column to communication_founder_reply_reviews",
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn ensure_routing_rows_for_inbound(conn: &Connection) -> Result<()> {
    // Historical auto-handle rule: inbound messages whose external timestamp
    // predates the communication account's creation are marked as already
    // handled so we don't re-process mailbox history at first boot. The
    // synthetic `queue` and `tui` channels are programmatic — work items are
    // created after the account exists and must stay `pending` until leased —
    // so they are excluded from the pre-account auto-handle.
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            CASE
                WHEN m.direction = 'outbound' THEN 'handled'
                WHEN m.trust_level = 'system_probe' THEN 'handled'
                WHEN m.channel IN ('queue', 'tui') THEN 'pending'
                WHEN m.channel = 'teams'
                     AND m.direction = 'inbound'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at
                     AND datetime(m.external_created_at) < datetime('now', '-24 hours') THEN 'handled'
                WHEN m.direction = 'inbound'
                     AND m.channel <> 'teams'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at THEN 'handled'
	                ELSE 'pending'
            END,
            CASE
                WHEN m.direction = 'outbound' OR m.trust_level = 'system_probe' THEN m.observed_at
                WHEN m.channel IN ('queue', 'tui') THEN NULL
                WHEN m.channel = 'teams'
                     AND m.direction = 'inbound'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at
                     AND datetime(m.external_created_at) < datetime('now', '-24 hours') THEN m.observed_at
                WHEN m.direction = 'inbound'
                     AND m.channel <> 'teams'
                     AND a.created_at IS NOT NULL
                     AND m.external_created_at <= a.created_at THEN m.observed_at
	                ELSE NULL
            END,
            m.observed_at
        FROM communication_messages m
        LEFT JOIN communication_accounts a ON a.account_key = m.account_key
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE r.message_key IS NULL
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let missing = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (message_key, raw_route_status, acked_at, updated_at) in missing {
        let route_status = canonical_queue_route_status(&raw_route_status)?;
        let previous_route_status = current_queue_route_status(conn, &message_key)?;
        enforce_queue_route_status_transition_with_grant(
            conn,
            &message_key,
            &previous_route_status,
            route_status.as_str(),
            "ctox-routing-backfill",
            "ensure_routing_rows_for_inbound",
            (route_status == QueueRouteStatus::Handled)
                .then_some(TerminalPolicyGrant::routing_backfill_non_work()),
        )?;
        conn.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            VALUES (?1, ?2, NULL, NULL, ?3, NULL, ?4)
            ON CONFLICT(message_key) DO NOTHING
            "#,
            params![message_key, route_status.as_str(), acked_at, updated_at],
        )?;
    }
    let mut statement = conn.prepare(
        r#"
        SELECT r.message_key, r.route_status
        FROM communication_routing_state r
        JOIN communication_messages m ON m.message_key = r.message_key
        WHERE m.direction = 'inbound'
          AND m.trust_level = 'system_probe'
          AND r.route_status <> 'handled'
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let probe_updates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let route_status = QueueRouteStatus::Handled;
    for (message_key, previous_route_status) in probe_updates {
        enforce_queue_route_status_transition_with_grant(
            conn,
            &message_key,
            &previous_route_status,
            route_status.as_str(),
            "ctox-routing-backfill",
            "normalize_system_probe_messages",
            Some(TerminalPolicyGrant::system_probe_inbound()),
        )?;
    }
    conn.execute(
        r#"
        UPDATE communication_routing_state
        SET route_status = ?1,
            lease_owner = NULL,
            leased_at = NULL,
            acked_at = COALESCE(acked_at, (
                SELECT observed_at
                FROM communication_messages m
                WHERE m.message_key = communication_routing_state.message_key
            )),
            last_error = NULL,
            updated_at = COALESCE((
                SELECT observed_at
                FROM communication_messages m
                WHERE m.message_key = communication_routing_state.message_key
            ), updated_at)
        WHERE message_key IN (
            SELECT message_key
            FROM communication_messages
            WHERE direction = 'inbound'
              AND trust_level = 'system_probe'
        )
          AND route_status <> 'handled'
        "#,
        params![route_status.as_str()],
    )
    .context("failed to normalize routing for system probe messages")?;
    Ok(())
}

fn schema_state(conn: &Connection) -> Result<Value> {
    let inbound_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM communication_messages WHERE direction = 'inbound'",
        [],
        |row| row.get(0),
    )?;
    let thread_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM communication_threads", [], |row| {
            row.get(0)
        })?;
    Ok(json!({
        "inbound_messages": inbound_count,
        "threads": thread_count,
    }))
}

fn ingest_tui_message(
    root: &Path,
    conn: &mut Connection,
    mut request: TuiIngestRequest,
) -> Result<Value> {
    let sanitized = secrets::auto_intake_prompt_secrets(root, &request.body)
        .context("failed to sanitize TUI secret-bearing input")?;
    if sanitized.auto_ingested_secrets > 0 {
        request.body = sanitized.sanitized_prompt;
        request.metadata = json!({
            "source": "ctox-channel-ingest-tui",
            "secret_sanitized": true,
            "auto_ingested_secrets": sanitized.auto_ingested_secrets,
            "suggested_skill": "secret-hygiene",
        });
    }
    ensure_account(
        conn,
        &request.account_key,
        "tui",
        &request.sender_address,
        "local",
        json!({"source": "tui"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!(
        "tui-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.thread_key, request.sender_address, request.body
        ))
    );
    let message_key = format!("{}::{remote_id}", request.account_key);
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "tui",
            account_key: &request.account_key,
            thread_key: &request.thread_key,
            remote_id: &remote_id,
            direction: "inbound",
            folder_hint: "tui",
            sender_display: &request.sender_display,
            sender_address: &request.sender_address,
            recipient_addresses_json: "[]",
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &request.subject,
            preview: &preview_text(&request.body, &request.subject),
            body_text: &request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "medium",
            status: "received",
            seen: false,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: &serde_json::to_string(&request.metadata)?,
        },
    )?;
    refresh_thread(conn, &request.thread_key)?;
    ensure_routing_rows_for_inbound(conn)?;
    Ok(json!({
        "message_key": message_key,
        "thread_key": request.thread_key,
        "channel": "tui",
    }))
}

fn store_tui_outbound_message(
    conn: &mut Connection,
    request: &ChannelSendRequest,
) -> Result<String> {
    ensure_account(
        conn,
        &request.account_key,
        "tui",
        request.sender_address.as_deref().unwrap_or("tui:local"),
        "local",
        json!({"source": "tui"}),
    )?;
    let observed_at = now_iso_string();
    let remote_id = format!(
        "tui-out-{}",
        stable_digest(&format!(
            "{}:{}:{}",
            request.thread_key, request.account_key, observed_at
        ))
    );
    let message_key = format!("{}::{remote_id}", request.account_key);
    let sender_display = request
        .sender_display
        .clone()
        .unwrap_or_else(|| "Local TUI".to_string());
    let sender_address = request
        .sender_address
        .clone()
        .unwrap_or_else(|| "tui:local".to_string());
    upsert_communication_message(
        conn,
        UpsertMessage {
            message_key: &message_key,
            channel: "tui",
            account_key: &request.account_key,
            thread_key: &request.thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "tui",
            sender_display: &sender_display,
            sender_address: &sender_address,
            recipient_addresses_json: &serde_json::to_string(&request.to)?,
            cc_addresses_json: &serde_json::to_string(&request.cc)?,
            bcc_addresses_json: "[]",
            subject: &request.subject,
            preview: &preview_text(&request.body, &request.subject),
            body_text: &request.body,
            body_html: "",
            raw_payload_ref: "",
            trust_level: "high",
            status: "sent",
            seen: true,
            has_attachments: false,
            external_created_at: &observed_at,
            observed_at: &observed_at,
            metadata_json: r#"{"source":"ctox-tui-send"}"#,
        },
    )?;
    refresh_thread(conn, &request.thread_key)?;
    ensure_routing_rows_for_inbound(conn)?;
    Ok(message_key)
}

fn take_messages(
    conn: &mut Connection,
    channel: Option<&str>,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<ChannelMessageView>> {
    take_messages_with_projection(None, conn, channel, limit, lease_owner)
}

fn take_messages_with_projection(
    projection_root: Option<&Path>,
    conn: &mut Connection,
    channel: Option<&str>,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<ChannelMessageView>> {
    let sql = if channel.is_some() {
        r#"
        WITH eligible AS (
            SELECT
                m.message_key,
                m.channel,
                m.account_key,
                m.thread_key,
                m.remote_id,
                m.direction,
                m.folder_hint,
                m.sender_display,
                m.sender_address,
                m.subject,
                m.preview,
                m.body_text,
                m.status,
                m.seen,
                m.external_created_at,
                m.observed_at,
                m.metadata_json,
                r.route_status,
                r.lease_owner,
                r.leased_at,
                r.acked_at,
                r.last_error,
                r.updated_at,
                MIN(COALESCE(r.first_pending_at, m.external_created_at)) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_pending_since,
                MIN(r.priority_time_credit_hours) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_priority_credit_hours,
                ROW_NUMBER() OVER (
                    PARTITION BY m.thread_key
                    ORDER BY
                        CASE WHEN m.channel = 'queue' THEN m.external_created_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.external_created_at END DESC,
                        CASE WHEN m.channel = 'queue' THEN m.observed_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.observed_at END DESC,
                        m.message_key DESC
                ) AS thread_rank
            FROM communication_messages m
            JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.direction = 'inbound'
              AND m.channel = ?1
              AND r.route_status = 'pending'
              AND (r.retry_not_before IS NULL OR datetime(r.retry_not_before) <= datetime('now'))
              AND (
                    json_extract(m.metadata_json, '$.not_before') IS NULL
                 OR json_extract(m.metadata_json, '$.not_before') = ''
                 OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
              )
        )
        SELECT
            message_key,
            channel,
            account_key,
            thread_key,
            remote_id,
            direction,
            folder_hint,
            sender_display,
            sender_address,
            subject,
            preview,
            body_text,
            status,
            seen,
            external_created_at,
            observed_at,
            metadata_json,
            route_status,
            lease_owner,
            leased_at,
            acked_at,
            last_error,
            updated_at
        FROM eligible
        WHERE thread_rank = 1
        ORDER BY
            CASE
                WHEN channel = 'tui' THEN datetime(thread_pending_since, '-24 hours')
                WHEN channel = 'queue' THEN datetime(thread_pending_since, '+1 hour')
                ELSE datetime(thread_pending_since, printf('%+d hours', thread_priority_credit_hours))
            END ASC,
            CASE WHEN channel = 'queue' THEN external_created_at END ASC,
            CASE WHEN channel <> 'queue' THEN external_created_at END DESC,
            CASE WHEN channel = 'queue' THEN observed_at END ASC,
            CASE WHEN channel <> 'queue' THEN observed_at END DESC,
            message_key DESC
        LIMIT ?3
        "#
    } else {
        r#"
        WITH eligible AS (
            SELECT
                m.message_key,
                m.channel,
                m.account_key,
                m.thread_key,
                m.remote_id,
                m.direction,
                m.folder_hint,
                m.sender_display,
                m.sender_address,
                m.subject,
                m.preview,
                m.body_text,
                m.status,
                m.seen,
                m.external_created_at,
                m.observed_at,
                m.metadata_json,
                r.route_status,
                r.lease_owner,
                r.leased_at,
                r.acked_at,
                r.last_error,
                r.updated_at,
                MIN(COALESCE(r.first_pending_at, m.external_created_at)) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_pending_since,
                MIN(r.priority_time_credit_hours) OVER (
                    PARTITION BY m.thread_key
                ) AS thread_priority_credit_hours,
                ROW_NUMBER() OVER (
                    PARTITION BY m.thread_key
                    ORDER BY
                        CASE WHEN m.channel = 'queue' THEN m.external_created_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.external_created_at END DESC,
                        CASE WHEN m.channel = 'queue' THEN m.observed_at END ASC,
                        CASE WHEN m.channel <> 'queue' THEN m.observed_at END DESC,
                        m.message_key DESC
                ) AS thread_rank
            FROM communication_messages m
            JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.direction = 'inbound'
              AND r.route_status = 'pending'
              AND (r.retry_not_before IS NULL OR datetime(r.retry_not_before) <= datetime('now'))
              AND (
                    json_extract(m.metadata_json, '$.not_before') IS NULL
                 OR json_extract(m.metadata_json, '$.not_before') = ''
                 OR json_extract(m.metadata_json, '$.not_before') <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
              )
        )
        SELECT
            message_key,
            channel,
            account_key,
            thread_key,
            remote_id,
            direction,
            folder_hint,
            sender_display,
            sender_address,
            subject,
            preview,
            body_text,
            status,
            seen,
            external_created_at,
            observed_at,
            metadata_json,
            route_status,
            lease_owner,
            leased_at,
            acked_at,
            last_error,
            updated_at
        FROM eligible
        WHERE thread_rank = 1
        ORDER BY
            CASE
                WHEN channel = 'tui' THEN datetime(thread_pending_since, '-24 hours')
                WHEN channel = 'queue' THEN datetime(thread_pending_since, '+1 hour')
                ELSE datetime(thread_pending_since, printf('%+d hours', thread_priority_credit_hours))
            END ASC,
            CASE WHEN channel = 'queue' THEN external_created_at END ASC,
            CASE WHEN channel <> 'queue' THEN external_created_at END DESC,
            CASE WHEN channel = 'queue' THEN observed_at END ASC,
            CASE WHEN channel <> 'queue' THEN observed_at END DESC,
            message_key DESC
        LIMIT ?2
        "#
    };

    // Hold a write lock for the whole check-then-act window so a concurrent
    // leaser on a different connection cannot steal a lease between our
    // eligibility SELECT and our UPDATE (lost-update). The lease UPDATE is a
    // check-and-set: its WHERE mirrors the eligibility predicate above, so a
    // losing racer flips 0 rows and we record neither the row nor a
    // core-transition proof for it.
    if let Some(root) = projection_root {
        attach_queue_projection_store(root, conn)?;
    }
    let tx =
        rusqlite::Transaction::new_unchecked(&*conn, rusqlite::TransactionBehavior::Immediate)?;
    let rows = {
        let mut statement = tx.prepare(sql)?;
        let mapped = if let Some(channel) = channel {
            statement.query_map(
                params![channel, lease_owner, limit as i64],
                map_channel_message_row,
            )?
        } else {
            statement.query_map(params![lease_owner, limit as i64], map_channel_message_row)?
        };
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let leased_at = now_iso_string();
    let lease_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    let mut taken = Vec::new();
    for mut item in rows {
        let updated = tx.execute(
            r#"INSERT INTO communication_routing_state (message_key, route_status, lease_owner, leased_at, first_pending_at, lease_expires_at, lease_worker_id, acked_at, last_error, updated_at, attempt)
               VALUES (?1, ?5, ?2, ?3, ?3, ?4, NULL, NULL, NULL, ?3, 1)
               ON CONFLICT(message_key) DO UPDATE SET route_status=excluded.route_status, lease_owner=excluded.lease_owner, leased_at=excluded.leased_at, first_pending_at=COALESCE(communication_routing_state.first_pending_at, excluded.first_pending_at), lease_expires_at=excluded.lease_expires_at, lease_worker_id=NULL, retry_not_before=NULL, hold_reason=NULL, acked_at=NULL, attempt=communication_routing_state.attempt+1, updated_at=excluded.updated_at
               WHERE communication_routing_state.route_status = 'pending'"#,
            params![
                item.message_key,
                lease_owner,
                leased_at,
                lease_expires_at,
                QueueRouteStatus::Leased.as_str()
            ],
        )?;
        if updated != 0 {
            enforce_queue_route_status_transition(
                &tx,
                &item.message_key,
                &item.routing.route_status,
                "leased",
                lease_owner,
                "lease_messages",
            )?;
        }
        if updated == 0 {
            // Lost the race: another owner leased this key between our SELECT
            // and UPDATE. Skip the core-transition proof and the push so we
            // never return a message we did not actually lease.
            continue;
        }
        item.routing.route_status = QueueRouteStatus::Leased.as_str().to_string();
        item.routing.lease_owner = Some(lease_owner.to_string());
        item.routing.leased_at = Some(leased_at.clone());
        item.routing.updated_at = leased_at.clone();
        taken.push(item);
    }
    if let Some(root) = projection_root {
        let message_keys = taken
            .iter()
            .map(|item| item.message_key.clone())
            .collect::<Vec<_>>();
        let tasks = load_queue_projection_tasks(&tx, &message_keys)?;
        refresh_queue_projection_tasks(root, &tx, &tasks)?;
    }
    tx.commit()?;
    Ok(taken)
}

pub fn defer_messages_until(
    root: &Path,
    message_keys: &[String],
    not_before: &str,
    reason: &str,
) -> Result<usize> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    let mut updated = 0usize;
    for message_key in message_keys {
        let message_updated = conn.execute(
            r#"
            UPDATE communication_messages
            SET metadata_json = json_set(
                json_set(metadata_json, '$.not_before', ?2),
                '$.defer_reason',
                ?3
            )
            WHERE message_key = ?1
            "#,
            params![message_key, not_before, reason],
        )?;
        if message_updated != 0 {
            conn.execute(
                r#"
                UPDATE communication_routing_state
                SET retry_not_before = ?2,
                    hold_reason = ?3,
                    updated_at = ?4
                WHERE message_key = ?1
                "#,
                params![message_key, not_before, reason, now_iso_string()],
            )?;
        }
        updated += message_updated;
    }
    Ok(updated)
}

fn ack_messages(
    projection_root: Option<&Path>,
    conn: &mut Connection,
    message_keys: &[String],
    status: &str,
    failure_note: Option<&str>,
    ack_reason: Option<&str>,
    terminal_policy_grant: Option<TerminalPolicyGrant>,
) -> Result<usize> {
    if let Some(root) = projection_root {
        attach_queue_projection_store(root, conn)?;
    }
    let tx = conn.unchecked_transaction()?;
    let updated = ack_messages_in_transaction(
        &tx,
        message_keys,
        status,
        failure_note,
        ack_reason,
        terminal_policy_grant,
    )?;
    if let Some(root) = projection_root {
        let tasks = load_queue_projection_tasks(&tx, message_keys)?;
        refresh_queue_projection_tasks(root, &tx, &tasks)?;
    }
    tx.commit()?;
    Ok(updated)
}

fn ack_messages_in_transaction(
    tx: &Transaction<'_>,
    message_keys: &[String],
    status: &str,
    failure_note: Option<&str>,
    ack_reason: Option<&str>,
    terminal_policy_grant: Option<TerminalPolicyGrant>,
) -> Result<usize> {
    let status = canonical_queue_route_status(status)?;
    let now = now_iso_string();
    let acked_at = if matches!(
        status,
        QueueRouteStatus::Handled | QueueRouteStatus::Cancelled
    ) {
        Some(now.as_str())
    } else {
        None
    };
    let failure_note = if status == QueueRouteStatus::Failed {
        Some(
            failure_note
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .context("failed queue ack requires a non-empty failure reason")?,
        )
    } else {
        None
    };
    let mut updated = 0usize;
    for message_key in message_keys {
        let transition_reason = ack_reason.or(failure_note).unwrap_or("ack_messages");
        if matches!(
            status,
            QueueRouteStatus::Cancelled | QueueRouteStatus::Failed
        ) && transition_business_command_for_task_in_transaction(
            tx,
            message_key,
            status.as_str(),
            None,
            (status == QueueRouteStatus::Failed).then_some("queue_terminal_failure"),
            failure_note,
            transition_reason,
        )? {
            updated = updated.saturating_add(1);
            tx.execute(
                "UPDATE communication_messages SET seen = 1 WHERE message_key = ?1",
                params![message_key],
            )?;
            continue;
        }
        let previous_route_status = current_queue_route_status(&tx, message_key)?;
        enforce_queue_route_status_transition_with_grant(
            &tx,
            message_key,
            &previous_route_status,
            status.as_str(),
            "ctox-queue-ack",
            ack_reason.or(failure_note).unwrap_or("ack_messages"),
            terminal_policy_grant,
        )?;
        let routing_updates = tx.execute(
            r#"
            INSERT INTO communication_routing_state (
                message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
            )
            SELECT ?1, ?2, NULL, NULL, ?3, ?4, ?5
            FROM communication_messages
            WHERE message_key = ?1
            ON CONFLICT(message_key) DO UPDATE SET
                route_status=excluded.route_status,
                lease_owner=NULL,
                leased_at=NULL,
                lease_worker_id=NULL,
                acked_at=excluded.acked_at,
                last_error=excluded.last_error,
                updated_at=excluded.updated_at
            "#,
            params![message_key, status.as_str(), acked_at, failure_note, now],
        )?;
        if routing_updates == 0 {
            continue;
        }
        updated += routing_updates;
        tx.execute(
            "UPDATE communication_messages SET seen = 1 WHERE message_key = ?1",
            params![message_key],
        )?;
    }
    Ok(updated)
}

fn list_messages(
    conn: &Connection,
    channel: Option<&str>,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let sql = if channel.is_some() {
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#
    } else {
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?1
        "#
    };
    let mut statement = conn.prepare(sql)?;
    let rows = if let Some(channel) = channel {
        statement.query_map(params![channel, limit as i64], map_channel_message_row)?
    } else {
        statement.query_map(params![limit as i64], map_channel_message_row)?
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_thread_messages(
    conn: &Connection,
    thread_key: &str,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.thread_key = ?1
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![thread_key, limit as i64], map_channel_message_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_messages(
    conn: &Connection,
    query: &str,
    channel: Option<&str>,
    sender: Option<&str>,
    limit: usize,
) -> Result<Vec<ChannelMessageView>> {
    let normalized_query = format!("%{}%", search_query_seed(query));
    let normalized_sender = sender
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE
            (?1 IS NULL OR m.channel = ?1)
            AND (?2 IS NULL OR LOWER(m.sender_address) = ?2)
            AND (
                LOWER(m.subject) LIKE ?3
                OR LOWER(m.preview) LIKE ?3
                OR LOWER(m.body_text) LIKE ?3
                OR LOWER(m.sender_display) LIKE ?3
                OR LOWER(m.sender_address) LIKE ?3
                OR LOWER(m.thread_key) LIKE ?3
            )
        ORDER BY m.external_created_at DESC, m.observed_at DESC
        LIMIT ?4
        "#,
    )?;
    let rows = statement.query_map(
        params![channel, normalized_sender, normalized_query, limit as i64],
        map_channel_message_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn search_query_seed(query: &str) -> String {
    let compact = query.trim().to_ascii_lowercase();
    compact
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .find(|token| token.len() >= 3)
        .unwrap_or(compact.as_str())
        .to_string()
}

fn build_communication_context(
    conn: &Connection,
    thread_key: &str,
    query: Option<&str>,
    sender: Option<&str>,
    limit: usize,
) -> Result<CommunicationContextView> {
    let thread_messages = list_thread_messages(conn, thread_key, limit)?;
    let latest_subject = thread_messages
        .iter()
        .find(|item| !item.subject.trim().is_empty())
        .map(|item| item.subject.clone());
    let mut related_messages = Vec::new();
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let mut seen = std::collections::BTreeSet::new();
        for message in search_messages(conn, query, None, None, limit)?
            .into_iter()
            .chain(
                sender
                    .map(|value| search_messages(conn, query, None, Some(value), limit))
                    .transpose()?
                    .unwrap_or_default()
                    .into_iter(),
            )
        {
            if seen.insert(message.message_key.clone()) {
                related_messages.push(message);
            }
        }
    }
    related_messages.retain(|item| item.thread_key != thread_key);
    let latest_inbound = thread_messages
        .iter()
        .find(|item| item.direction == "inbound")
        .map(|item| candidate_from_message("latest_inbound", item));
    let latest_outbound = thread_messages
        .iter()
        .find(|item| item.direction == "outbound")
        .map(|item| candidate_from_message("latest_outbound", item));
    let mut candidate_blockers = collect_candidates(&thread_messages, &related_messages, "blocker");
    let mut candidate_promises = collect_candidates(&thread_messages, &related_messages, "promise");
    let open_owner_questions = collect_open_owner_questions(&thread_messages);
    candidate_blockers.truncate(limit.min(8));
    candidate_promises.truncate(limit.min(8));
    Ok(CommunicationContextView {
        thread_key: thread_key.to_string(),
        latest_subject,
        latest_inbound,
        latest_outbound,
        thread_messages,
        related_messages,
        candidate_blockers,
        candidate_promises,
        open_owner_questions,
    })
}

fn collect_candidates(
    thread_messages: &[ChannelMessageView],
    related_messages: &[ChannelMessageView],
    candidate_kind: &str,
) -> Vec<CommunicationStateCandidate> {
    let mut out = Vec::new();
    for message in thread_messages.iter().chain(related_messages.iter()) {
        if message.direction != "outbound" {
            continue;
        }
        let body = format!(
            "{}\n{}\n{}",
            message.subject.to_ascii_lowercase(),
            message.preview.to_ascii_lowercase(),
            message.body_text.to_ascii_lowercase()
        );
        let is_match = match candidate_kind {
            "blocker" => {
                body.contains("blocked")
                    || body.contains("blocker")
                    || body.contains("need ")
                    || body.contains("missing ")
                    || body.contains("requires ")
                    || body.contains("cannot ")
            }
            "promise" => {
                body.contains("next step")
                    || body.contains("i will")
                    || body.contains("i'll")
                    || body.contains("follow-up")
                    || body.contains("queued")
                    || body.contains("review")
                    || body.contains("continue")
            }
            _ => false,
        };
        if is_match {
            out.push(candidate_from_message(candidate_kind, message));
        }
    }
    out
}

fn collect_open_owner_questions(
    thread_messages: &[ChannelMessageView],
) -> Vec<CommunicationStateCandidate> {
    let latest_outbound_at = thread_messages
        .iter()
        .find(|item| item.direction == "outbound")
        .map(|item| item.external_created_at.clone());
    thread_messages
        .iter()
        .filter(|item| item.direction == "inbound")
        .filter(|item| {
            let text = format!("{}\n{}", item.subject, item.body_text);
            text.contains('?')
                || text.to_ascii_lowercase().contains("please")
                || text.to_ascii_lowercase().contains("can you")
        })
        .filter(|item| {
            latest_outbound_at
                .as_ref()
                .map(|outbound| item.external_created_at >= *outbound)
                .unwrap_or(true)
        })
        .map(|item| candidate_from_message("open_question", item))
        .collect()
}

fn candidate_from_message(kind: &str, message: &ChannelMessageView) -> CommunicationStateCandidate {
    CommunicationStateCandidate {
        kind: kind.to_string(),
        message_key: message.message_key.clone(),
        channel: message.channel.clone(),
        thread_key: message.thread_key.clone(),
        created_at: message.external_created_at.clone(),
        summary: preview_text(&message.body_text, &message.subject),
    }
}

fn map_channel_message_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelMessageView> {
    let metadata_json: String = row.get(16)?;
    let metadata = serde_json::from_str(&metadata_json)
        .unwrap_or_else(|_| json!({"raw_metadata": metadata_json}));
    Ok(ChannelMessageView {
        message_key: row.get(0)?,
        channel: row.get(1)?,
        account_key: row.get(2)?,
        thread_key: row.get(3)?,
        remote_id: row.get(4)?,
        direction: row.get(5)?,
        folder_hint: row.get(6)?,
        sender_display: row.get(7)?,
        sender_address: row.get(8)?,
        subject: row.get(9)?,
        preview: row.get(10)?,
        body_text: row.get(11)?,
        status: row.get(12)?,
        seen: row.get::<_, i64>(13)? != 0,
        external_created_at: row.get(14)?,
        observed_at: row.get(15)?,
        metadata,
        routing: RoutingView {
            route_status: row.get(17)?,
            lease_owner: row.get(18)?,
            leased_at: row.get(19)?,
            acked_at: row.get(20)?,
            last_error: row.get(21)?,
            updated_at: row.get(22)?,
        },
    })
}

fn routed_inbound_message_from_view(item: ChannelMessageView) -> RoutedInboundMessage {
    let preferred_reply_modality = item
        .metadata
        .get("preferredReplyModality")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let workspace_root =
        workspace_root_from_queue_metadata_or_prompt(&item.metadata, &item.body_text);
    RoutedInboundMessage {
        message_key: item.message_key,
        channel: item.channel,
        account_key: item.account_key,
        thread_key: item.thread_key,
        sender_display: item.sender_display,
        sender_address: item.sender_address,
        subject: item.subject,
        preview: item.preview,
        body_text: item.body_text,
        external_created_at: item.external_created_at,
        workspace_root,
        metadata: item.metadata,
        preferred_reply_modality,
    }
}

fn list_queue_tasks_from_conn(conn: &Connection, limit: usize) -> Result<Vec<QueueTaskView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at),
            r.lease_expires_at, r.lease_worker_id, r.first_pending_at,
            r.failure_class, COALESCE(r.failure_attempt_count, 0), r.retry_not_before,
            r.hold_reason, r.wait_entity_type, r.wait_entity_id,
            COALESCE(r.priority_time_credit_hours, 0), COALESCE(r.attempt, 0), r.crew_member_id, r.crew_assigned_member_id
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'blocked' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                WHEN 'cancelled' THEN 5
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![QUEUE_CHANNEL_NAME, limit as i64],
        map_queue_task_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_queue_tasks_from_conn_with_statuses(
    conn: &Connection,
    statuses: &[String],
    limit: usize,
) -> Result<Vec<QueueTaskView>> {
    let placeholders = (0..statuses.len())
        .map(|index| format!("?{}", index + 3))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at),
            r.lease_expires_at, r.lease_worker_id, r.first_pending_at,
            r.failure_class, COALESCE(r.failure_attempt_count, 0), r.retry_not_before,
            r.hold_reason, r.wait_entity_type, r.wait_entity_id,
            COALESCE(r.priority_time_credit_hours, 0), COALESCE(r.attempt, 0), r.crew_member_id, r.crew_assigned_member_id
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND lower(COALESCE(r.route_status, 'pending')) IN ({placeholders})
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'blocked' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                WHEN 'cancelled' THEN 5
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT ?2
        "#
    );
    let mut values = Vec::with_capacity(statuses.len() + 2);
    values.push(SqlValue::Text(QUEUE_CHANNEL_NAME.to_string()));
    values.push(SqlValue::Integer(limit as i64));
    values.extend(statuses.iter().cloned().map(SqlValue::Text));
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), map_queue_task_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn count_queue_tasks_from_conn_with_statuses(
    conn: &Connection,
    statuses: &[String],
) -> Result<usize> {
    let placeholders = (0..statuses.len())
        .map(|index| format!("?{}", index + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT COUNT(*)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND lower(COALESCE(r.route_status, 'pending')) IN ({placeholders})
        "#
    );
    let mut values = Vec::with_capacity(statuses.len() + 1);
    values.push(SqlValue::Text(QUEUE_CHANNEL_NAME.to_string()));
    values.extend(statuses.iter().cloned().map(SqlValue::Text));
    let count: i64 = conn.query_row(&sql, params_from_iter(values), |row| row.get(0))?;
    Ok(count.max(0) as usize)
}

fn load_queue_message_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<ChannelMessageView>> {
    conn.query_row(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at)
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND m.message_key = ?2
        LIMIT 1
        "#,
        params![QUEUE_CHANNEL_NAME, message_key],
        map_channel_message_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn load_queue_task_from_conn(
    conn: &Connection,
    message_key: &str,
) -> Result<Option<QueueTaskView>> {
    Ok(
        load_queue_tasks_by_message_key_from_conn(conn, &[message_key.to_string()])?
            .remove(message_key),
    )
}

pub(crate) fn load_queue_tasks_by_message_key_from_conn(
    conn: &Connection,
    message_keys: &[String],
) -> Result<BTreeMap<String, QueueTaskView>> {
    let mut tasks = BTreeMap::new();
    if message_keys.is_empty() {
        return Ok(tasks);
    }
    for chunk in message_keys.chunks(500) {
        let placeholders = (0..chunk.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT
                m.message_key,
                m.channel,
                m.account_key,
                m.thread_key,
                m.remote_id,
                m.direction,
                m.folder_hint,
                m.sender_display,
                m.sender_address,
                m.subject,
                m.preview,
                m.body_text,
                m.status,
                m.seen,
                m.external_created_at,
                m.observed_at,
                m.metadata_json,
                COALESCE(r.route_status, 'pending'),
                r.lease_owner,
                r.leased_at,
                r.acked_at,
                r.last_error,
                COALESCE(r.updated_at, m.observed_at),
            r.lease_expires_at, r.lease_worker_id, r.first_pending_at,
            r.failure_class, COALESCE(r.failure_attempt_count, 0), r.retry_not_before,
            r.hold_reason, r.wait_entity_type, r.wait_entity_id,
            COALESCE(r.priority_time_credit_hours, 0), COALESCE(r.attempt, 0), r.crew_member_id, r.crew_assigned_member_id
            FROM communication_messages m
            LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
            WHERE m.channel = ?1
              AND m.direction = 'inbound'
              AND m.message_key IN ({placeholders})
            "#
        );
        let mut values = Vec::with_capacity(chunk.len() + 1);
        values.push(SqlValue::Text(QUEUE_CHANNEL_NAME.to_string()));
        values.extend(chunk.iter().cloned().map(SqlValue::Text));
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), map_queue_task_row)?;
        for row in rows {
            let task = row?;
            tasks.insert(task.message_key.clone(), task);
        }
    }
    Ok(tasks)
}

fn load_queue_task_for_business_os_command_from_conn(
    conn: &Connection,
    command_id: &str,
) -> Result<Option<QueueTaskView>> {
    conn.query_row(
        r#"
        SELECT
            m.message_key,
            m.channel,
            m.account_key,
            m.thread_key,
            m.remote_id,
            m.direction,
            m.folder_hint,
            m.sender_display,
            m.sender_address,
            m.subject,
            m.preview,
            m.body_text,
            m.status,
            m.seen,
            m.external_created_at,
            m.observed_at,
            m.metadata_json,
            COALESCE(r.route_status, 'pending'),
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            COALESCE(r.updated_at, m.observed_at),
            r.lease_expires_at, r.lease_worker_id, r.first_pending_at,
            r.failure_class, COALESCE(r.failure_attempt_count, 0), r.retry_not_before,
            r.hold_reason, r.wait_entity_type, r.wait_entity_id,
            COALESCE(r.priority_time_credit_hours, 0), COALESCE(r.attempt, 0), r.crew_member_id, r.crew_assigned_member_id
        FROM communication_messages m
        LEFT JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?1
          AND m.direction = 'inbound'
          AND json_valid(m.metadata_json)
          AND json_extract(m.metadata_json, '$.business_os_command_id') = ?2
        ORDER BY
            CASE COALESCE(r.route_status, 'pending')
                WHEN 'pending' THEN 0
                WHEN 'leased' THEN 1
                WHEN 'blocked' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'handled' THEN 4
                WHEN 'cancelled' THEN 5
                ELSE 9
            END ASC,
            m.external_created_at ASC,
            m.observed_at ASC
        LIMIT 1
        "#,
        params![QUEUE_CHANNEL_NAME, command_id],
        map_queue_task_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn map_queue_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueTaskView> {
    let mut task = queue_task_from_message(map_channel_message_row(row)?)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))?;
    task.lease_expires_at = row.get(23)?;
    task.lease_worker_id = row.get(24)?;
    task.first_pending_at = row.get(25)?;
    task.failure_class = row.get(26)?;
    task.failure_attempt_count = row.get(27)?;
    task.retry_not_before = row.get(28)?;
    task.hold_reason = row.get(29)?;
    task.wait_entity_type = row.get(30)?;
    task.wait_entity_id = row.get(31)?;
    task.priority_time_credit_hours = row.get(32)?;
    task.attempt = row.get(33)?;
    task.crew_member_id = row.get(34)?;
    task.crew_assigned_member_id = row.get(35)?;
    Ok(task)
}

fn queue_task_from_message(message: ChannelMessageView) -> Result<QueueTaskView> {
    if message.channel != QUEUE_CHANNEL_NAME || message.direction != "inbound" {
        anyhow::bail!("message is not a queue task");
    }
    let priority = current_queue_priority(&message);
    let created_at = message
        .metadata
        .get("created_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message.observed_at.clone());
    let sort_at = message
        .metadata
        .get("sort_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message.external_created_at.clone());
    let prompt = message.body_text;
    let workspace_root = workspace_root_from_queue_metadata_or_prompt(&message.metadata, &prompt);
    let route_status = canonical_queue_route_status(&message.routing.route_status)?;
    let metadata_status_note = || {
        message
            .metadata
            .get("status_note")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    };
    let status_note = if route_status == QueueRouteStatus::Failed {
        message
            .routing
            .last_error
            .clone()
            .or_else(metadata_status_note)
    } else {
        metadata_status_note()
    };
    Ok(QueueTaskView {
        message_key: message.message_key,
        thread_key: message.thread_key,
        title: message.subject,
        prompt,
        workspace_root,
        ticket_self_work_id: message
            .metadata
            .get("ticket_self_work_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        priority,
        suggested_skill: message
            .metadata
            .get("skill")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        parent_message_key: message
            .metadata
            .get("parent_message_key")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        metadata: message.metadata,
        route_status: route_status.as_str().to_string(),
        status_note,
        lease_owner: message.routing.lease_owner,
        leased_at: message.routing.leased_at,
        acked_at: message.routing.acked_at,
        created_at,
        sort_at,
        updated_at: message.routing.updated_at,
        lease_expires_at: None,
        lease_worker_id: None,
        first_pending_at: None,
        failure_class: None,
        failure_attempt_count: 0,
        retry_not_before: None,
        hold_reason: None,
        wait_entity_type: None,
        wait_entity_id: None,
        priority_time_credit_hours: 0,
        attempt: 0,
        crew_member_id: None,
        crew_assigned_member_id: None,
    })
}

fn ensure_queue_account(conn: &mut Connection) -> Result<()> {
    ensure_account(
        conn,
        QUEUE_ACCOUNT_KEY,
        QUEUE_CHANNEL_NAME,
        QUEUE_ACCOUNT_ADDRESS,
        QUEUE_PROVIDER,
        json!({"source": "ctox-queue"}),
    )
}

fn set_routing_status(
    conn: &Connection,
    message_key: &str,
    route_status: &str,
    now: &str,
    actor: &str,
    reason: &str,
    status_note: Option<&str>,
    terminal_policy_grant: Option<TerminalPolicyGrant>,
) -> Result<()> {
    let route_status = canonical_queue_route_status(route_status)?;
    let failure_note = if route_status == QueueRouteStatus::Failed {
        let note = status_note
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("failed queue route status requires a non-empty status_note/failure reason")?;
        Some(note)
    } else {
        None
    };
    let previous_route_status = current_queue_route_status(conn, message_key)?;
    let transition_reason = if route_status == QueueRouteStatus::Failed {
        failure_note.unwrap_or(reason)
    } else {
        status_note
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(reason)
    };
    enforce_queue_route_status_transition_with_grant(
        conn,
        message_key,
        &previous_route_status,
        route_status.as_str(),
        actor,
        transition_reason,
        terminal_policy_grant,
    )?;
    let acked_at = if matches!(
        route_status,
        QueueRouteStatus::Handled | QueueRouteStatus::Cancelled
    ) {
        Some(now)
    } else {
        None
    };
    conn.execute(
        r#"
        INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, acked_at, last_error, updated_at
        )
        VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5)
        ON CONFLICT(message_key) DO UPDATE SET
            route_status=excluded.route_status,
            lease_owner=NULL,
            leased_at=NULL,
            lease_worker_id=NULL,
            lease_expires_at=CASE WHEN excluded.route_status='pending' THEN NULL ELSE communication_routing_state.lease_expires_at END,
            retry_not_before=CASE WHEN excluded.route_status='pending' THEN NULL ELSE communication_routing_state.retry_not_before END,
            hold_reason=CASE WHEN excluded.route_status='pending' THEN NULL ELSE communication_routing_state.hold_reason END,
            wait_entity_type=CASE WHEN excluded.route_status='pending' THEN NULL ELSE communication_routing_state.wait_entity_type END,
            wait_entity_id=CASE WHEN excluded.route_status='pending' THEN NULL ELSE communication_routing_state.wait_entity_id END,
            acked_at=excluded.acked_at,
            last_error=excluded.last_error,
            updated_at=excluded.updated_at
        "#,
        params![
            message_key,
            route_status.as_str(),
            acked_at,
            failure_note,
            now
        ],
    )?;
    Ok(())
}

pub(crate) fn current_queue_route_status(conn: &Connection, message_key: &str) -> Result<String> {
    let raw = conn
        .query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key = ?1 LIMIT 1",
            params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let route_status = match raw {
        Some(raw) => canonical_queue_route_status(&raw)
            .with_context(|| format!("unknown queue route status for message `{message_key}`"))?,
        None => QueueRouteStatus::Pending,
    };
    Ok(route_status.as_str().to_string())
}

pub(crate) fn enforce_queue_route_status_transition(
    conn: &Connection,
    message_key: &str,
    from_route_status: &str,
    to_route_status: &str,
    actor: &str,
    reason: &str,
) -> Result<()> {
    enforce_queue_route_status_transition_with_grant(
        conn,
        message_key,
        from_route_status,
        to_route_status,
        actor,
        reason,
        None,
    )
}

fn enforce_queue_route_status_transition_with_grant(
    conn: &Connection,
    message_key: &str,
    from_route_status: &str,
    to_route_status: &str,
    actor: &str,
    reason: &str,
    terminal_policy_grant: Option<TerminalPolicyGrant>,
) -> Result<()> {
    let from_route_status = canonical_queue_route_status(from_route_status)?;
    let to_route_status = canonical_queue_route_status(to_route_status)?;
    let from_state = queue_route_status_core_state(from_route_status);
    let to_state = queue_route_status_core_state(to_route_status);
    if from_state == to_state {
        return Ok(());
    }
    if to_state == CoreState::Completed
        && queue_completed_has_terminal_success_proof(conn, message_key)?
    {
        return Ok(());
    }
    if to_state == CoreState::ReworkRequired && queue_rework_has_witness_proof(conn, message_key)? {
        return Ok(());
    }
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "from_route_status".to_string(),
        from_route_status.as_str().to_string(),
    );
    metadata.insert(
        "to_route_status".to_string(),
        to_route_status.as_str().to_string(),
    );
    metadata.insert("reason".to_string(), reason.to_string());
    if to_state == CoreState::Failed {
        metadata.insert("failure_reason".to_string(), reason.to_string());
        metadata.insert(
            "failure_class".to_string(),
            "queue_route_failure".to_string(),
        );
    }
    if to_state == CoreState::Completed {
        if let Some(policy_proof) = terminal_policy_grant.map(TerminalPolicyGrant::proof) {
            metadata.insert(
                "terminal_policy_proof".to_string(),
                policy_proof.to_string(),
            );
        }
    }
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: message_key.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state,
            to_state,
            event: queue_route_status_core_event(to_route_status),
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        },
    )?;
    Ok(())
}

/// Root-based wrapper: does this queue item already carry an accepted
/// terminal-success Completed proof? Queue repair uses it to pre-classify a
/// `complete` action before it reaches the Completed gate, so an unproven
/// complete is refused-and-skipped instead of aborting the whole repair pass.
pub fn queue_complete_action_has_terminal_proof(root: &Path, message_key: &str) -> Result<bool> {
    let db_path = resolve_db_path(root, None);
    let conn = open_channel_db(&db_path)?;
    queue_completed_has_terminal_success_proof(&conn, message_key)
}

fn queue_completed_has_terminal_success_proof(
    conn: &Connection,
    message_key: &str,
) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'QueueItem'
          AND entity_id = ?1
          AND to_state = 'Completed'
          AND accepted = 1
          AND json_valid(request_json) = 1
          AND (
                json_extract(request_json, '$.metadata.reviewed_work_terminal_success') = 'true'
             OR (
                    json_type(request_json, '$.metadata.terminal_policy_proof') = 'text'
                AND TRIM(json_extract(request_json, '$.metadata.terminal_policy_proof')) <> ''
             )
          )
        "#,
        params![message_key],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn queue_rework_has_witness_proof(conn: &Connection, message_key: &str) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'QueueItem'
          AND entity_id = ?1
          AND to_state = 'ReworkRequired'
          AND accepted = 1
          AND json_valid(request_json) = 1
          AND (
                json_extract(request_json, '$.metadata.review_checkpoint') = 'true'
             OR json_extract(request_json, '$.metadata.validator_rework') = 'true'
          )
        "#,
        params![message_key],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn queue_route_status_core_state(route_status: QueueRouteStatus) -> CoreState {
    match route_status {
        QueueRouteStatus::Pending => CoreState::Pending,
        QueueRouteStatus::Leased => CoreState::Leased,
        QueueRouteStatus::Running => CoreState::Running,
        QueueRouteStatus::Blocked => CoreState::Blocked,
        QueueRouteStatus::ReviewRework => CoreState::ReworkRequired,
        QueueRouteStatus::Failed => CoreState::Failed,
        QueueRouteStatus::Handled => CoreState::Completed,
        QueueRouteStatus::Cancelled => CoreState::Superseded,
    }
}

fn queue_route_status_core_event(route_status: QueueRouteStatus) -> CoreEvent {
    match route_status {
        QueueRouteStatus::Leased => CoreEvent::Lease,
        QueueRouteStatus::Pending => CoreEvent::Release,
        QueueRouteStatus::Blocked => CoreEvent::Block,
        QueueRouteStatus::ReviewRework => CoreEvent::RequireRework,
        QueueRouteStatus::Failed => CoreEvent::Fail,
        QueueRouteStatus::Cancelled => CoreEvent::Supersede,
        QueueRouteStatus::Handled => CoreEvent::Complete,
        QueueRouteStatus::Running => CoreEvent::Retry,
    }
}

fn canonical_queue_priority(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_lowercase();
    match normalized.as_str() {
        "urgent" | "high" | "normal" | "low" => Ok(normalized),
        _ => anyhow::bail!("unsupported queue priority '{raw}' (expected urgent|high|normal|low)"),
    }
}

fn canonical_queue_route_status(raw: &str) -> Result<QueueRouteStatus> {
    QueueRouteStatus::parse(raw).with_context(|| {
        format!(
            "unsupported queue route status '{raw}' (expected pending|leased|running|blocked|failed|handled|cancelled|review_rework)"
        )
    })
}

fn current_queue_priority(message: &ChannelMessageView) -> String {
    message
        .metadata
        .get("priority")
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .trim()
        .to_lowercase()
}

fn queue_metadata_object(metadata: &Value) -> serde_json::Map<String, Value> {
    metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new)
}

fn normalize_workspace_root(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn workspace_root_from_queue_metadata_or_prompt(
    metadata: &Value,
    prompt: &str,
) -> Option<String> {
    queue_metadata_object(metadata)
        .get("workspace_root")
        .and_then(Value::as_str)
        .and_then(|value| normalize_workspace_root(Some(value)))
        .or_else(|| legacy_workspace_root_from_prompt(prompt))
}

pub fn legacy_workspace_root_from_prompt(prompt: &str) -> Option<String> {
    for marker in [
        "Work only inside this workspace:",
        "Arbeite ausschließlich im Verzeichnis ",
        "Arbeite im Verzeichnis ",
        "Arbeite ausschließlich im Workspace ",
        "Arbeite im Workspace ",
    ] {
        if let Some(path) = extract_workspace_root_after_marker(prompt, marker) {
            return Some(path);
        }
    }
    None
}

fn extract_workspace_root_after_marker(prompt: &str, marker: &str) -> Option<String> {
    let start = prompt.find(marker)? + marker.len();
    let tail = prompt[start..].trim_start();
    let line = tail.lines().next()?.trim();
    let candidate = if let Some(stripped) = line.strip_prefix('/') {
        format!("/{stripped}")
    } else if let Some(index) = line.find('/') {
        line[index..].to_string()
    } else {
        return None;
    };
    let trimmed = candidate
        .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '"' | '\''));
    normalize_workspace_root(Some(trimmed))
}

fn queue_sort_at(priority: &str, now: &str) -> Result<String> {
    let base = DateTime::parse_from_rfc3339(now)
        .with_context(|| format!("failed to parse queue timestamp '{now}'"))?
        .with_timezone(&Utc);
    let shifted = match priority {
        "urgent" => base - Duration::hours(24),
        "high" => base - Duration::hours(1),
        "normal" => base,
        "low" => base + Duration::hours(1),
        _ => anyhow::bail!("unsupported queue priority '{priority}'"),
    };
    Ok(shifted.to_rfc3339())
}

pub(crate) struct CommunicationSyncRun<'a> {
    pub run_key: &'a str,
    pub channel: &'a str,
    pub account_key: &'a str,
    pub folder_hint: &'a str,
    pub started_at: &'a str,
    pub finished_at: &'a str,
    pub ok: bool,
    pub fetched_count: i64,
    pub stored_count: i64,
    pub error_text: &'a str,
    pub metadata_json: &'a str,
}

pub(crate) fn ensure_account(
    conn: &mut Connection,
    account_key: &str,
    channel: &str,
    address: &str,
    provider: &str,
    profile_json: Value,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    ensure_account_tx(&tx, account_key, channel, address, provider, profile_json)?;
    tx.commit()?;
    Ok(())
}

fn upsert_owner_profile(conn: &mut Connection, display_name: &str) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO owner_profiles (
            owner_key, display_name, metadata_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(owner_key) DO UPDATE SET
            display_name=excluded.display_name,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params!["primary", display_name.trim(), r#"{}"#, now],
    )?;
    Ok(())
}

fn sync_identity_profiles(
    conn: &mut Connection,
    settings: &BTreeMap<String, String>,
) -> Result<()> {
    let owner_name = settings
        .get("CTOX_OWNER_NAME")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Owner");
    if let Some(owner_email) = settings
        .get("CTOX_OWNER_EMAIL_ADDRESS")
        .map(|value| normalize_email_address(value))
        .filter(|value| !value.is_empty())
    {
        upsert_identity_profile(
            conn,
            &owner_email,
            owner_name,
            json!({
                "email": owner_email,
                "role": "owner",
                "allow_admin_actions": true,
                "allow_sudo_actions": true,
                "mail_instruction_scope": "full_admin",
            }),
        )?;
    }

    let founder_roles = parse_founder_email_roles(settings);
    for founder_email in parse_founder_email_addresses(settings) {
        let founder_role = founder_roles
            .get(&founder_email)
            .cloned()
            .unwrap_or_else(|| "Founder".to_string());
        upsert_identity_profile(
            conn,
            &founder_email,
            &founder_email,
            json!({
                "email": founder_email,
                "role": "founder",
                "role_title": founder_role,
                "allow_admin_actions": true,
                "allow_sudo_actions": false,
                "mail_instruction_scope": "founder_strategic",
            }),
        )?;
    }

    for admin in parse_admin_email_policies(settings) {
        upsert_identity_profile(
            conn,
            &admin.email,
            &admin.email,
            json!({
                "email": admin.email,
                "role": "admin",
                "allow_admin_actions": true,
                "allow_sudo_actions": admin.can_sudo,
                "mail_instruction_scope": if admin.can_sudo { "admin_with_sudo" } else { "admin_without_sudo" },
            }),
        )?;
    }
    Ok(())
}

fn upsert_identity_profile(
    conn: &mut Connection,
    owner_key: &str,
    display_name: &str,
    metadata: Value,
) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO owner_profiles (
            owner_key, display_name, metadata_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?4)
        ON CONFLICT(owner_key) DO UPDATE SET
            display_name=excluded.display_name,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            owner_key,
            display_name.trim(),
            serde_json::to_string(&metadata)?,
            now
        ],
    )?;
    Ok(())
}

fn load_owner_name(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            r#"
        SELECT display_name
        FROM owner_profiles
        WHERE owner_key = 'primary'
        LIMIT 1
        "#,
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .filter(|name| !name.trim().is_empty()))
}

#[cfg(test)]
mod queue_task_metadata_tests {
    use super::*;

    #[test]
    fn queue_task_metadata_round_trips_typed_business_os_identity() {
        let root = tempfile::tempdir().expect("temp root");
        let created = create_queue_task(
            root.path(),
            QueueTaskCreateRequest {
                title: "Create contracts app".to_string(),
                prompt: "Build the requested app without relying on prompt metadata markers."
                    .to_string(),
                thread_key: "business-os/app-creator/contracts".to_string(),
                workspace_root: Some(root.path().display().to_string()),
                priority: "high".to_string(),
                suggested_skill: Some("business-os-app-module-development".to_string()),
                parent_message_key: None,
                extra_metadata: Some(serde_json::json!({
                    "source": "business-os",
                    "business_os_command_id": "cmd-contracts",
                    "business_os_module": "creator",
                    "business_os_command_type": "ctox.business_os.app.create",
                    "business_os_record_id": "contracts"
                })),
            },
        )
        .expect("create queue task");

        let loaded = load_queue_task(root.path(), &created.message_key)
            .expect("load queue task")
            .expect("queue task exists");

        assert_eq!(
            loaded
                .metadata
                .get("business_os_command_type")
                .and_then(Value::as_str),
            Some("ctox.business_os.app.create")
        );
        assert_eq!(
            loaded
                .metadata
                .get("business_os_record_id")
                .and_then(Value::as_str),
            Some("contracts")
        );
    }
}

#[cfg(test)]
mod tests;
