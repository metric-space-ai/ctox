mod source_skills;
pub(crate) use source_skills::{
    add_or_update_runbook_item, compose_ticket_source_skill_reply, create_or_update_main_skill,
    create_or_update_runbook, create_or_update_skillbook, import_ticket_source_skill_bundle,
    import_ticket_source_skill_resources, query_ticket_source_skill,
    refresh_runbook_item_embedding, resolve_ticket_source_skill_for_target,
    review_ticket_note_with_source_skill, show_ticket_source_skill,
    suggested_skill_for_live_ticket_source,
};
use source_skills::{default_skill_for_self_work_kind, resolve_skill_bundle_dir_hint};

mod approval_mode;
use approval_mode::ControlApprovalMode;
mod case_state;
mod event_route_status;
use event_route_status::TicketEventRouteStatus;
mod cases;
pub use cases::list_cases;
#[cfg(test)]
use cases::load_ticket_clarification_request;
pub(crate) use cases::run_business_os_ticket_command;
use cases::{
    close_case, create_dry_run, create_learning_candidate, create_ticket_clarification_request,
    decide_case_approval, decide_learning_candidate, list_audit_records, list_cases_on_conn,
    list_learning_candidates, load_case, load_latest_dry_run_for_case, load_learning_candidate,
    publish_ticket_clarification_request, record_execution_action, record_verification,
    resolve_ticket_clarification_request, resolve_waiting_clarifications_from_inbound_events,
    writeback_comment, writeback_transition, AuditRequest,
};

mod work_item_status;
pub(crate) use work_item_status::WorkItemStatus;

mod work_items;
#[cfg(test)]
use work_items::ticket_self_work_spawn_budget;
pub(crate) use work_items::{
    append_ticket_self_work_note, apply_ticket_workflow_delta, assign_ticket_self_work_item,
    list_ticket_self_work_assignments, list_ticket_self_work_items, list_ticket_self_work_notes,
    load_ticket_self_work_item, load_ticket_self_work_items_by_work_id_from_conn,
    load_ticket_workflow, materialize_ready_workflow_steps,
    materialize_ready_workflow_steps_for_workflow, publish_ticket_self_work_item,
    put_ticket_knowledge_entry, put_ticket_self_work_item, put_ticket_workflow_step,
    set_ticket_approval_gate_state_from_authorized_reply, set_ticket_self_work_state,
    set_ticket_self_work_state_with_failure_reason, start_ticket_workflow,
    transition_ticket_self_work_item, workflow_mark_step_queue_ready, workflow_prompt_block,
};
use work_items::{list_ticket_self_work_items_on_conn, put_ticket_knowledge_entry_internal};

mod cli;
pub use cli::handle_ticket_command;

use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use regex::Regex;
use rusqlite::params;
use rusqlite::params_from_iter;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, UNIX_EPOCH};

use crate::core_state::guard::{
    enforce_core_spawn, enforce_core_transition, ensure_core_transition_guard_schema,
    CoreSpawnRequest,
};
use crate::core_state::{
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
};
use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::supervisor;
use crate::mission::ticket_adapters;
use crate::mission::ticket_gateway;
use crate::mission::ticket_protocol;
use crate::mission::ticket_translation;
use crate::service::harness_flow::{
    record_harness_flow_event_lossy, RecordHarnessFlowEventRequest,
};

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_LIST_LIMIT: usize = 20;
pub(crate) fn ticket_self_work_has_terminal_success_proof(
    root: &Path,
    work_id: &str,
) -> Result<bool> {
    let conn = open_ticket_db(root)?;
    work_items::work_item_has_terminal_success_proof(&conn, work_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalPolicyGrantKind {
    OutboundTicketEvent,
    BaselineObservedInboundTicketEvent,
    AuthorizedApprovalReply {
        approval_message_key: String,
        body_sha256_prefix: String,
    },
}

/// Ticket terminal-policy capability created only after the corresponding
/// structured condition has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalPolicyGrant(TerminalPolicyGrantKind);

impl TerminalPolicyGrant {
    fn outbound_ticket_event() -> Self {
        Self(TerminalPolicyGrantKind::OutboundTicketEvent)
    }

    fn baseline_observed_inbound_ticket_event() -> Self {
        Self(TerminalPolicyGrantKind::BaselineObservedInboundTicketEvent)
    }

    fn authorized_approval_reply(approval_message_key: &str, body_sha256: &str) -> Self {
        Self(TerminalPolicyGrantKind::AuthorizedApprovalReply {
            approval_message_key: approval_message_key.to_string(),
            body_sha256_prefix: body_sha256.chars().take(16).collect(),
        })
    }

    fn proof(&self) -> String {
        match &self.0 {
            TerminalPolicyGrantKind::OutboundTicketEvent => {
                "policy:ticket-event-outbound-terminal".to_string()
            }
            TerminalPolicyGrantKind::BaselineObservedInboundTicketEvent => {
                "policy:ticket-event-baseline-observed-terminal".to_string()
            }
            TerminalPolicyGrantKind::AuthorizedApprovalReply {
                approval_message_key,
                body_sha256_prefix,
            } => format!(
                "policy:authorized-approval-reply:{approval_message_key}:{body_sha256_prefix}"
            ),
        }
    }
}
const DEFAULT_AUDIT_LIMIT: usize = 30;
const DEFAULT_APPROVAL_MODE: &str = "human_approval_required";
const DEFAULT_AUTONOMY_LEVEL: &str = "A0";
const DEFAULT_SUPPORT_MODE: &str = "support_case";
const DEFAULT_RISK_LEVEL: &str = "unknown";
const DEFAULT_TICKET_SKILL_EMBEDDING_MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";
pub(crate) const WORKFLOW_CASE_KIND: &str = "workflow-case";
pub(crate) const WORKFLOW_STEP_KIND: &str = "workflow-step";
pub(crate) const WORKFLOW_ORCHESTRATOR_SKILL: &str = "ticket-workflow-orchestrator";
const WORKFLOW_ROLE_CASE: &str = "case";
const WORKFLOW_ROLE_LEAF: &str = "leaf";
const WORKFLOW_ROLE_REDUCER: &str = "reducer";
const WORKFLOW_MATERIALIZE_DEFAULT_LIMIT: usize = 16;
const WORKFLOW_MAX_STEPS_PER_WORKFLOW: usize = 256;
const REQUIRED_KNOWLEDGE_DOMAINS: &[&str] = &[
    "source_profile",
    "label_catalog",
    "glossary",
    "service_catalog",
    "infrastructure_assets",
    "team_model",
    "access_model",
    "monitoring_landscape",
];

static TICKET_SCHEMA_READY: OnceLock<Mutex<HashSet<TicketSchemaCacheKey>>> = OnceLock::new();
static TICKET_SELF_WORK_LIST_CACHE: OnceLock<
    Mutex<BTreeMap<TicketSelfWorkListCacheKey, TicketSelfWorkListCacheEntry>>,
> = OnceLock::new();
static TICKET_WORKFLOW_MATERIALIZE_CACHE: OnceLock<
    Mutex<BTreeMap<TicketWorkflowMaterializeCacheKey, TicketWorkflowMaterializeCacheEntry>>,
> = OnceLock::new();

const TICKET_SELF_WORK_LIST_CACHE_MAX_ENTRIES: usize = 256;
const TICKET_WORKFLOW_MATERIALIZE_CACHE_MAX_ENTRIES: usize = 128;

type TicketFileChangeStamp = (u64, u128);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TicketStoreChangeStamp {
    main: TicketFileChangeStamp,
    wal: TicketFileChangeStamp,
    journal: TicketFileChangeStamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TicketCaseStatusStamp {
    database_exists: bool,
    table_exists: bool,
    open_case_count: usize,
    latest_open_case_updated_at: String,
}

type TicketSelfWorkListCacheStamp = TicketStoreChangeStamp;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TicketSelfWorkListCacheKey {
    database: TicketSchemaCacheKey,
    system: Option<String>,
    state: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone)]
struct TicketSelfWorkListCacheEntry {
    stamp: TicketSelfWorkListCacheStamp,
    items: Vec<TicketSelfWorkItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TicketWorkflowMaterializeCacheKey {
    database: TicketSchemaCacheKey,
    workflow_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone)]
struct TicketWorkflowMaterializeCacheEntry {
    stamp: TicketStoreChangeStamp,
    result: TicketWorkflowMaterializeResult,
}

#[cfg(test)]
static TICKET_SELF_WORK_LIST_CACHE_MISS_COUNTS: OnceLock<
    Mutex<BTreeMap<TicketSelfWorkListCacheKey, usize>>,
> = OnceLock::new();
#[cfg(test)]
static TICKET_WORKFLOW_MATERIALIZE_CACHE_MISS_COUNTS: OnceLock<
    Mutex<BTreeMap<TicketWorkflowMaterializeCacheKey, usize>>,
> = OnceLock::new();
#[cfg(test)]
static TICKET_SELF_WORK_ASSIGNMENT_BATCH_HYDRATION_CALLS: OnceLock<Mutex<usize>> = OnceLock::new();
#[cfg(test)]
static TICKET_DB_OPEN_CALL_COUNTS: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> = OnceLock::new();

thread_local! {
    static TICKET_RECONCILE_DB: RefCell<Option<CachedTicketConnection>> = RefCell::new(None);
}

struct CachedTicketConnection {
    key: TicketSchemaCacheKey,
    conn: Connection,
}

#[cfg(unix)]
type TicketSchemaCacheKey = (PathBuf, u64, u64);
#[cfg(not(unix))]
type TicketSchemaCacheKey = PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct TicketItemView {
    pub ticket_key: String,
    pub source_system: String,
    pub remote_ticket_id: String,
    pub title: String,
    pub body_text: String,
    pub remote_status: String,
    pub priority: Option<String>,
    pub requester: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_synced_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketEventView {
    pub event_key: String,
    pub ticket_key: String,
    pub source_system: String,
    pub remote_event_id: String,
    pub direction: String,
    pub event_type: String,
    pub summary: String,
    pub body_text: String,
    pub metadata: Value,
    pub external_created_at: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TicketEventFailureClass {
    Retryable,
    Terminal,
}

impl TicketEventFailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutedTicketEvent {
    pub event_key: String,
    pub ticket_key: String,
    pub source_system: String,
    pub remote_event_id: String,
    pub event_type: String,
    pub summary: String,
    pub body_text: String,
    pub title: String,
    pub remote_status: String,
    pub label: String,
    pub bundle_label: String,
    pub bundle_version: i64,
    pub case_id: String,
    pub dry_run_id: String,
    pub dry_run_artifact: Value,
    pub support_mode: String,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub risk_level: String,
    pub thread_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketLabelAssignmentView {
    pub ticket_key: String,
    pub label: String,
    pub assigned_by: String,
    pub rationale: Option<String>,
    pub evidence: Value,
    pub assigned_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlBundleView {
    pub label: String,
    pub bundle_version: i64,
    pub runbook_id: String,
    pub runbook_version: String,
    pub policy_id: String,
    pub policy_version: String,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub verification_profile_id: String,
    pub writeback_profile_id: String,
    pub support_mode: String,
    pub default_risk_level: String,
    pub execution_actions: Vec<String>,
    pub notes: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutonomyGrantView {
    pub label: String,
    pub grant_version: i64,
    pub bundle_version: i64,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub approved_by: String,
    pub source_candidate_id: Option<String>,
    pub rationale: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningCandidateView {
    pub candidate_id: String,
    pub case_id: String,
    pub ticket_key: String,
    pub label: String,
    pub bundle_label: String,
    pub bundle_version: i64,
    pub summary: String,
    pub proposed_actions: Vec<String>,
    pub evidence: Value,
    pub status: String,
    pub proposed_at: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
    pub decision_notes: Option<String>,
    pub promoted_autonomy_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketCaseView {
    pub case_id: String,
    pub ticket_key: String,
    pub label: String,
    pub bundle_label: String,
    pub bundle_version: i64,
    pub state: String,
    pub approval_mode: String,
    pub autonomy_level: String,
    pub support_mode: String,
    pub risk_level: String,
    pub opened_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketClarificationRequestView {
    pub clarification_id: String,
    pub ticket_key: String,
    pub case_id: Option<String>,
    pub work_id: Option<String>,
    pub target_type: String,
    pub target_channel: String,
    pub question: String,
    pub missing_inputs: Vec<String>,
    pub unblock_criteria: Option<String>,
    pub status: String,
    pub outbound_message_key: Option<String>,
    pub inbound_response_key: Option<String>,
    pub inbound_response_body: Option<String>,
    pub resume_state: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub sent_at: Option<String>,
    pub resolved_at: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
struct EffectiveControlResolution {
    approval_mode: String,
    autonomy_level: String,
    missing_approvals: Vec<String>,
    grant: Option<AutonomyGrantView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunActionView {
    pub action_class: String,
    pub execution_mode: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunRecordView {
    pub dry_run_id: String,
    pub case_id: String,
    pub ticket_key: String,
    pub label: String,
    pub bundle_label: String,
    pub bundle_version: i64,
    pub artifact: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketEventRoutingView {
    pub event_key: String,
    pub route_status: String,
    pub lease_owner: Option<String>,
    pub leased_at: Option<String>,
    pub lease_expires_at: Option<String>,
    pub failure_class: Option<String>,
    pub failure_attempt_count: i64,
    pub retry_not_before: Option<String>,
    pub failure_proof: Option<String>,
    pub hold_reason: Option<String>,
    pub wait_entity_type: Option<String>,
    pub wait_entity_id: Option<String>,
    pub acked_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSourceControlView {
    pub source_system: String,
    pub adoption_mode: String,
    pub baseline_external_created_cutoff: String,
    pub attached_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSourceSkillBindingView {
    pub source_system: String,
    pub skill_name: String,
    pub archetype: String,
    pub status: String,
    pub origin: String,
    pub artifact_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSourceSkillShowView {
    pub binding: TicketSourceSkillBindingView,
    pub artifact_path: Option<String>,
    pub skill_markdown_path: Option<String>,
    pub skill_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketSourceMainSkillRecord {
    main_skill_id: String,
    title: String,
    primary_channel: String,
    entry_action: String,
    #[serde(default)]
    resolver_contract: Value,
    #[serde(default)]
    execution_contract: Value,
    #[serde(default)]
    resolve_flow: Vec<String>,
    #[serde(default)]
    writeback_flow: Vec<String>,
    #[serde(default)]
    linked_skillbooks: Vec<String>,
    #[serde(default)]
    linked_runbooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketSourceSkillbookRecord {
    skillbook_id: String,
    title: String,
    version: String,
    mission: String,
    #[serde(default)]
    non_negotiable_rules: Vec<String>,
    runtime_policy: String,
    answer_contract: String,
    #[serde(default)]
    workflow_backbone: Vec<String>,
    #[serde(default)]
    routing_taxonomy: Vec<String>,
    #[serde(default)]
    linked_runbooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketSourceRunbookRecord {
    runbook_id: String,
    skillbook_id: String,
    title: String,
    version: String,
    status: String,
    problem_domain: String,
    #[serde(default)]
    item_labels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum TicketSourceRunbookBundle {
    Single(TicketSourceRunbookRecord),
    Catalog {
        runbooks: Vec<TicketSourceRunbookRecord>,
    },
}

impl TicketSourceRunbookBundle {
    fn into_runbooks(self) -> Vec<TicketSourceRunbookRecord> {
        match self {
            Self::Single(runbook) => vec![runbook],
            Self::Catalog { runbooks } => runbooks,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketSourceRunbookItemRecord {
    item_id: String,
    runbook_id: String,
    skillbook_id: String,
    label: String,
    title: String,
    problem_class: String,
    #[serde(default)]
    trigger_phrases: Vec<String>,
    #[serde(default)]
    entry_conditions: Vec<String>,
    #[serde(default)]
    earliest_blocker: String,
    #[serde(default)]
    expected_guidance: String,
    #[serde(default)]
    tool_actions: Value,
    #[serde(default)]
    verification: Vec<String>,
    #[serde(default)]
    writeback_policy: Value,
    #[serde(default)]
    escalate_when: Vec<String>,
    #[serde(default)]
    sources: Value,
    #[serde(default)]
    pages: Vec<String>,
    chunk_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TicketSourceKnowledgeResourceRecord {
    resource_id: String,
    title: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    source_id: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    canonical_url: String,
    #[serde(default)]
    snapshot_hash: String,
    #[serde(default)]
    evidence_eligible: bool,
    #[serde(default)]
    linked_runbook_items: Vec<String>,
    #[serde(flatten)]
    metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
struct TicketSourceSkillMatchView {
    item_id: String,
    label: String,
    title: String,
    problem_class: String,
    score: f64,
    expected_guidance: String,
    earliest_blocker: String,
    escalate_when: Vec<String>,
    pages: Vec<String>,
    tool_actions: Value,
    writeback_policy: Value,
}

#[derive(Debug, Clone, Serialize)]
struct TicketSourceSkillReplyView {
    decision: String,
    source_system: String,
    ticket_key: String,
    case_id: Option<String>,
    matched_label: String,
    item_id: String,
    reply_subject: String,
    reply_body: String,
    manual_reference: Option<String>,
    writeback_policy: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSourceSkillNoteReviewFinding {
    pub kind: String,
    pub excerpt: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSourceSkillNoteReviewView {
    pub source_system: String,
    pub ticket_key: String,
    pub query: String,
    pub matched_family: Option<String>,
    pub matched_family_score: Option<f64>,
    pub desk_ready: bool,
    pub language_clean: bool,
    pub copy_safe: bool,
    pub concise: bool,
    pub grounded_in_ticket: bool,
    pub findings: Vec<TicketSourceSkillNoteReviewFinding>,
    pub note_guidance: Option<String>,
    pub operator_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TicketDispatchPreflightIssue {
    pub system: String,
    pub code: String,
    pub severity: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketKnowledgeEntryView {
    pub entry_id: String,
    pub source_system: String,
    pub domain: String,
    pub knowledge_key: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub content: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketKnowledgeLoadView {
    pub load_id: String,
    pub ticket_key: String,
    pub source_system: String,
    pub domains: Vec<String>,
    pub loaded_entries: Vec<TicketKnowledgeEntryView>,
    pub gap_domains: Vec<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSelfWorkItemView {
    pub work_id: String,
    pub source_system: String,
    pub kind: String,
    pub title: String,
    pub body_text: String,
    pub state: String,
    pub suggested_skill: Option<String>,
    pub metadata: Value,
    pub assigned_to: Option<String>,
    pub assigned_by: Option<String>,
    pub assigned_at: Option<String>,
    pub remote_ticket_id: Option<String>,
    pub remote_locator: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSelfWorkAssignmentView {
    pub assignment_id: String,
    pub work_id: String,
    pub assigned_to: String,
    pub assigned_by: String,
    pub rationale: Option<String>,
    pub remote_event_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketSelfWorkNoteView {
    pub note_id: String,
    pub work_id: String,
    pub body_text: String,
    pub visibility: String,
    pub authored_by: String,
    pub remote_event_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketWorkflowStepView {
    pub work_id: String,
    pub step_id: String,
    pub role: String,
    pub phase: String,
    pub title: String,
    pub state: String,
    pub status: String,
    pub predecessor_work_ids: Vec<String>,
    pub predecessor_step_ids: Vec<String>,
    pub ready: bool,
    pub suggested_skill: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketWorkflowView {
    pub workflow_id: String,
    pub title: String,
    pub goal: Option<String>,
    pub state: String,
    pub case_work_id: Option<String>,
    pub steps: Vec<TicketWorkflowStepView>,
    pub ready_steps: Vec<String>,
    pub waiting_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketWorkflowMaterializeResult {
    pub workflow_id: Option<String>,
    pub materialized_count: usize,
    pub materialized: Vec<TicketSelfWorkItemView>,
    pub skipped_count: usize,
}

#[derive(Debug, Clone)]
struct TicketWorkflowStartInput {
    source_system: String,
    title: String,
    goal: String,
    thread_key: Option<String>,
    workspace_root: Option<String>,
    skill: Option<String>,
    priority: Option<String>,
    first_phase: String,
    first_phase_goal: Option<String>,
    first_exit_gate: Option<String>,
    first_step_title: Option<String>,
    first_step_prompt: Option<String>,
    queue_now: bool,
}

#[derive(Debug, Clone)]
struct TicketWorkflowStepInput {
    workflow_id: String,
    role: String,
    phase: String,
    step_id: Option<String>,
    title: String,
    body_text: String,
    phase_goal: Option<String>,
    exit_gate: Option<String>,
    predecessor_work_ids: Vec<String>,
    predecessor_step_ids: Vec<String>,
    skill: Option<String>,
    priority: Option<String>,
    metadata: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkflowDelta {
    #[serde(default)]
    phase_decision: Option<String>,
    #[serde(default)]
    create_steps: Vec<WorkflowDeltaCreateStep>,
    #[serde(default)]
    update_steps: Vec<WorkflowDeltaUpdateStep>,
    #[serde(default)]
    queue_now: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkflowDeltaCreateStep {
    #[serde(default)]
    step_id: Option<String>,
    #[serde(default)]
    role: Option<String>,
    phase: String,
    title: String,
    #[serde(default, alias = "body")]
    prompt: String,
    #[serde(default)]
    phase_goal: Option<String>,
    #[serde(default)]
    exit_gate: Option<String>,
    #[serde(default)]
    predecessors: Vec<String>,
    #[serde(default)]
    predecessor_work_ids: Vec<String>,
    #[serde(default)]
    predecessor_steps: Vec<String>,
    #[serde(default)]
    predecessor_step_ids: Vec<String>,
    #[serde(default, alias = "suggested_skill")]
    skill: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkflowDeltaUpdateStep {
    #[serde(default)]
    work_id: Option<String>,
    #[serde(default)]
    step_id: Option<String>,
    #[serde(default, alias = "status")]
    workflow_step_status: Option<String>,
    #[serde(default)]
    evidence: Value,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TicketAuditRecord {
    pub audit_id: String,
    pub ticket_key: String,
    pub case_id: Option<String>,
    pub actor_type: String,
    pub action_type: String,
    pub label: Option<String>,
    pub bundle_label: Option<String>,
    pub bundle_version: Option<i64>,
    pub details: Value,
    pub created_at: String,
}

pub(crate) const BUSINESS_OS_TICKET_COLLECTIONS: &[&str] = &[
    "ctox_ticket_items",
    "ctox_ticket_events",
    "ctox_ticket_event_routing_state",
    "ctox_ticket_cases",
    "ctox_ticket_self_work_items",
    "ctox_ticket_self_work_notes",
    "ctox_ticket_label_assignments",
    "ctox_ticket_control_bundles",
    "ctox_ticket_approvals",
    "ctox_ticket_verifications",
    "ctox_ticket_writebacks",
    "ctox_ticket_clarification_requests",
];

#[derive(Debug, Clone)]
pub(crate) struct AdapterTicketMirrorRequest<'a> {
    pub system: &'a str,
    pub remote_ticket_id: &'a str,
    pub title: &'a str,
    pub body_text: &'a str,
    pub remote_status: &'a str,
    pub priority: Option<&'a str>,
    pub requester: Option<&'a str>,
    pub metadata: Value,
    pub external_created_at: &'a str,
    pub external_updated_at: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct AdapterTicketEventRequest<'a> {
    pub system: &'a str,
    pub remote_ticket_id: &'a str,
    pub remote_event_id: &'a str,
    pub direction: &'a str,
    pub event_type: &'a str,
    pub summary: &'a str,
    pub body_text: &'a str,
    pub metadata: Value,
    pub external_created_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdapterTicketUpsertResult {
    pub key: String,
    pub changed: bool,
}

// The internal-work terminology firewall keeps legacy self-work strings out
// of plan/review/autonomy (see `internal_work_terminology_firewall_keeps_
// self_work_legacy_only`). They are persisted DB values — table name,
// wait entity_type, evidence prefix — that only this module may spell out;
// callers reference the neutral constants below instead.
pub(crate) const LEGACY_WORK_ITEM_TABLE: &str = "ticket_self_work_items";
pub(crate) const LEGACY_WORK_ITEM_WAIT_ENTITY_TYPE: &str = "ticket-self-work";
pub(crate) fn legacy_work_item_wait_evidence_ref(work_id: &str, state: &str) -> String {
    format!("{LEGACY_WORK_ITEM_WAIT_ENTITY_TYPE}:{work_id}:{state}")
}

#[derive(Debug, Clone)]
struct ControlBundleInput {
    label: String,
    runbook_id: String,
    runbook_version: String,
    policy_id: String,
    policy_version: String,
    approval_mode: String,
    autonomy_level: String,
    verification_profile_id: String,
    writeback_profile_id: String,
    support_mode: String,
    default_risk_level: String,
    execution_actions: Vec<String>,
    notes: Option<String>,
}

#[derive(Debug, Clone)]
struct TicketClarificationRequestInput {
    case_id: Option<String>,
    ticket_key: Option<String>,
    work_id: Option<String>,
    target_type: String,
    target_channel: String,
    question: String,
    missing_inputs: Vec<String>,
    unblock_criteria: Option<String>,
    resume_state: String,
    created_by: String,
    metadata: Value,
}

#[derive(Debug, Clone)]
struct AutonomyGrantInput {
    label: String,
    bundle_version: Option<i64>,
    approval_mode: String,
    autonomy_level: String,
    approved_by: String,
    source_candidate_id: Option<String>,
    rationale: Option<String>,
}

#[derive(Debug, Clone)]
struct TicketKnowledgeUpsertInput {
    source_system: String,
    domain: String,
    knowledge_key: String,
    title: String,
    summary: String,
    status: String,
    content: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketSelfWorkUpsertInput {
    pub(crate) source_system: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) body_text: String,
    pub(crate) state: String,
    pub(crate) metadata: Value,
}

pub(crate) fn sync_ticket_system(root: &Path, system: &str) -> Result<Value> {
    let Some(adapter) = ticket_adapters::adapter_for_system(system) else {
        anyhow::bail!("unsupported ticket system: {system}");
    };
    let batch = adapter.sync_batch(root)?;
    let applied = ticket_translation::apply_ticket_sync_batch(root, &batch)?;
    let resolved_clarification_count =
        resolve_waiting_clarifications_from_inbound_events(root, &applied.system)?;
    let observed_knowledge = refresh_observed_ticket_knowledge(root, &applied.system)?;
    let self_work_count =
        list_ticket_self_work_items(root, Some(&applied.system), None, 10_000)?.len();
    Ok(json!({
        "ok": true,
        "system": applied.system,
        "fetched_count": applied.fetched_count,
        "stored_ticket_count": applied.stored_ticket_count,
        "stored_event_count": applied.stored_event_count,
        "source_control": applied.source_control,
        "knowledge_count": observed_knowledge.len(),
        "self_work_count": self_work_count,
        "resolved_clarification_count": resolved_clarification_count,
        "metadata": batch.metadata,
    }))
}

pub(crate) fn configured_ticket_systems(
    settings: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    settings
        .get("CTOX_TICKET_SYSTEMS")
        .map(String::as_str)
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let normalized = item.to_ascii_lowercase();
            seen.insert(normalized.clone()).then_some(normalized)
        })
        .collect()
}

pub(crate) fn preflight_configured_ticket_systems(
    root: &Path,
    settings: &std::collections::BTreeMap<String, String>,
) -> Vec<TicketDispatchPreflightIssue> {
    let mut issues = Vec::new();
    for system in configured_ticket_systems(settings) {
        let Some(adapter) = ticket_adapters::adapter_for_system(&system) else {
            issues.push(TicketDispatchPreflightIssue {
                system,
                code: "unsupported_ticket_system".to_string(),
                severity: "error".to_string(),
                reason: "configured ticket system has no CTOX adapter".to_string(),
            });
            continue;
        };
        let capabilities = adapter.capabilities();
        if !capabilities.can_sync {
            issues.push(TicketDispatchPreflightIssue {
                system: system.clone(),
                code: "sync_not_supported".to_string(),
                severity: "error".to_string(),
                reason: "adapter does not declare ticket sync capability".to_string(),
            });
        }
        if system == "zammad" {
            let runtime = ticket_gateway::runtime_settings_from_settings(
                root,
                ticket_gateway::TicketAdapterKind::Zammad,
                settings,
            );
            let has_base_url = runtime
                .get("CTO_ZAMMAD_BASE_URL")
                .map(String::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            let has_token = runtime
                .get("CTO_ZAMMAD_TOKEN")
                .map(String::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            let has_basic = runtime
                .get("CTO_ZAMMAD_USER")
                .map(String::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
                && runtime
                    .get("CTO_ZAMMAD_PASSWORD")
                    .map(String::as_str)
                    .map(str::trim)
                    .is_some_and(|value| !value.is_empty());
            if !has_base_url {
                issues.push(TicketDispatchPreflightIssue {
                    system: system.clone(),
                    code: "missing_zammad_base_url".to_string(),
                    severity: "error".to_string(),
                    reason: "missing CTO_ZAMMAD_BASE_URL".to_string(),
                });
            }
            if !has_token && !has_basic {
                issues.push(TicketDispatchPreflightIssue {
                    system: system.clone(),
                    code: "missing_zammad_auth".to_string(),
                    severity: "error".to_string(),
                    reason:
                        "missing Zammad auth: set CTO_ZAMMAD_TOKEN or CTO_ZAMMAD_USER + CTO_ZAMMAD_PASSWORD"
                            .to_string(),
                });
            }
        }
    }
    issues
}

fn test_ticket_system(root: &Path, system: &str) -> Result<Value> {
    let Some(adapter) = ticket_adapters::adapter_for_system(system) else {
        anyhow::bail!("unsupported ticket system: {system}");
    };
    adapter.test(root)
}

fn ticket_system_capabilities(system: &str) -> Result<Value> {
    let Some(adapter) = ticket_adapters::adapter_for_system(system) else {
        anyhow::bail!("unsupported ticket system: {system}");
    };
    Ok(json!({
        "ok": true,
        "system": system,
        "capabilities": adapter.capabilities(),
    }))
}

pub(crate) fn ensure_ticket_source_control_for_sync(
    root: &Path,
    batch: &ticket_protocol::TicketSyncBatch,
) -> Result<TicketSourceControlView> {
    if let Some(existing) = load_ticket_source_control(root, &batch.system)? {
        return Ok(existing);
    }
    let now = now_iso_string();
    let cutoff = batch
        .events
        .iter()
        .map(|event| event.external_created_at.as_str())
        .chain(
            batch
                .tickets
                .iter()
                .map(|ticket| ticket.external_updated_at.as_str()),
        )
        .max()
        .unwrap_or(now.as_str())
        .to_string();
    let mut conn = open_ticket_db(root)?;
    conn.execute(
        r#"
        INSERT INTO ticket_source_controls (
            source_system, adoption_mode, baseline_external_created_cutoff, attached_at, updated_at
        ) VALUES (?1, 'baseline_observe_only', ?2, ?3, ?3)
        ON CONFLICT(source_system) DO NOTHING
        "#,
        params![batch.system, cutoff, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*ticket-source:{}*", batch.system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "source_adopted",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "source_system": batch.system,
                "adoption_mode": "baseline_observe_only",
                "baseline_external_created_cutoff": cutoff,
                "fetched_ticket_count": batch.fetched_ticket_count,
            }),
        },
    )?;
    load_ticket_source_control(root, &batch.system)?
        .context("failed to load ticket source control after sync adoption")
}

pub(crate) fn list_ticket_source_controls(root: &Path) -> Result<Vec<TicketSourceControlView>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT source_system, adoption_mode, baseline_external_created_cutoff, attached_at, updated_at
        FROM ticket_source_controls
        ORDER BY source_system ASC
        "#,
    )?;
    let rows = statement.query_map([], map_ticket_source_control_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn list_ticket_source_skill_bindings(
    root: &Path,
    system: Option<&str>,
) -> Result<Vec<TicketSourceSkillBindingView>> {
    let conn = open_ticket_db(root)?;
    if let Some(system) = system {
        let mut statement = conn.prepare(
            r#"
            SELECT source_system, skill_name, archetype, status, origin, artifact_path, notes, created_at, updated_at
            FROM ticket_source_skill_bindings
            WHERE source_system = ?1
            ORDER BY updated_at DESC
            "#,
        )?;
        let rows = statement.query_map(params![system], map_ticket_source_skill_binding_row)?;
        return rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from);
    }
    let mut statement = conn.prepare(
        r#"
        SELECT source_system, skill_name, archetype, status, origin, artifact_path, notes, created_at, updated_at
        FROM ticket_source_skill_bindings
        ORDER BY updated_at DESC, source_system ASC
        "#,
    )?;
    let rows = statement.query_map([], map_ticket_source_skill_binding_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_active_ticket_source_skill_binding_from_conn(
    conn: &Connection,
    system: &str,
) -> Result<Option<TicketSourceSkillBindingView>> {
    conn.query_row(
        r#"
        SELECT source_system, skill_name, archetype, status, origin, artifact_path, notes, created_at, updated_at
        FROM ticket_source_skill_bindings
        WHERE source_system = ?1
          AND status = 'active'
        LIMIT 1
        "#,
        params![system],
        map_ticket_source_skill_binding_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn put_ticket_source_skill_binding(
    root: &Path,
    system: &str,
    skill_name: &str,
    archetype: &str,
    status: &str,
    origin: &str,
    artifact_path: Option<&str>,
    notes: Option<&str>,
) -> Result<TicketSourceSkillBindingView> {
    let system = system.trim();
    let skill_name = skill_name.trim();
    let archetype = archetype.trim();
    let status = status.trim();
    let origin = origin.trim();
    anyhow::ensure!(!system.is_empty(), "source system must not be empty");
    anyhow::ensure!(!skill_name.is_empty(), "skill name must not be empty");
    anyhow::ensure!(!archetype.is_empty(), "skill archetype must not be empty");
    anyhow::ensure!(
        matches!(status, "active" | "inactive"),
        "unsupported source skill status: {status}"
    );
    anyhow::ensure!(!origin.is_empty(), "source skill origin must not be empty");
    let normalized_artifact_path = artifact_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if let Some(raw) = normalized_artifact_path.as_deref() {
        if let Some(dir) = resolve_skill_bundle_dir_hint(root, raw) {
            let _ = crate::skill_store::upsert_skill_bundle_from_dir(root, &dir);
        }
    }
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    put_ticket_source_skill_binding_with_conn(
        &conn,
        system,
        skill_name,
        archetype,
        status,
        origin,
        normalized_artifact_path.as_deref(),
        notes,
        &now,
    )
}

fn put_ticket_source_skill_binding_with_conn(
    conn: &Connection,
    system: &str,
    skill_name: &str,
    archetype: &str,
    status: &str,
    origin: &str,
    artifact_path: Option<&str>,
    notes: Option<&str>,
    now: &str,
) -> Result<TicketSourceSkillBindingView> {
    anyhow::ensure!(!system.trim().is_empty(), "source system must not be empty");
    anyhow::ensure!(
        !skill_name.trim().is_empty(),
        "skill name must not be empty"
    );
    anyhow::ensure!(
        !archetype.trim().is_empty(),
        "skill archetype must not be empty"
    );
    anyhow::ensure!(
        matches!(status.trim(), "active" | "inactive"),
        "unsupported source skill status: {status}"
    );
    anyhow::ensure!(
        !origin.trim().is_empty(),
        "source skill origin must not be empty"
    );
    conn.execute(
        r#"
        INSERT INTO ticket_source_skill_bindings (
            source_system, skill_name, archetype, status, origin, artifact_path, notes, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(source_system) DO UPDATE SET
            skill_name=excluded.skill_name,
            archetype=excluded.archetype,
            status=excluded.status,
            origin=excluded.origin,
            artifact_path=excluded.artifact_path,
            notes=excluded.notes,
            updated_at=excluded.updated_at
        "#,
        params![
            system,
            skill_name,
            archetype,
            status,
            origin,
            artifact_path,
            notes.map(str::trim).filter(|value| !value.is_empty()),
            now,
            now,
        ],
    )?;
    if status == "active" {
        load_active_ticket_source_skill_binding_from_conn(&conn, system)?
            .context("source skill binding missing after upsert")
    } else {
        conn.query_row(
            r#"
            SELECT source_system, skill_name, archetype, status, origin, artifact_path, notes, created_at, updated_at
            FROM ticket_source_skill_bindings
            WHERE source_system = ?1
            LIMIT 1
            "#,
            params![system],
            map_ticket_source_skill_binding_row,
        )
        .optional()?
        .context("source skill binding missing after upsert")
    }
}

pub(crate) fn load_ticket_source_control(
    root: &Path,
    system: &str,
) -> Result<Option<TicketSourceControlView>> {
    let conn = open_ticket_db(root)?;
    load_ticket_source_control_from_conn(&conn, system)
}

fn load_ticket_source_control_from_conn(
    conn: &Connection,
    system: &str,
) -> Result<Option<TicketSourceControlView>> {
    conn.query_row(
        r#"
        SELECT source_system, adoption_mode, baseline_external_created_cutoff, attached_at, updated_at
        FROM ticket_source_controls
        WHERE source_system = ?1
        LIMIT 1
        "#,
        params![system],
        map_ticket_source_control_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn first_string_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.iter().find_map(first_string_from_value),
        _ => None,
    }
}

fn first_string_from_named_metadata(metadata: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = metadata.get(*key).and_then(first_string_from_value) {
            return Some(value);
        }
    }
    None
}

fn looks_like_ctox_internal_ticket(title: &str, body_text: &str) -> bool {
    let title = title.trim();
    if title.starts_with("CTOX:") {
        return true;
    }
    let lowered = body_text.to_lowercase();
    lowered.contains("visible onboarding work item")
        || lowered.contains("generated from mirrored")
        || lowered.contains("review the attached ticket system")
        || lowered.contains("ctox pilot thread")
}

fn extract_ticket_history_records(root: &Path, system: &str) -> Result<Vec<Value>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT
            ti.ticket_key,
            ti.remote_ticket_id,
            ti.title,
            ti.body_text,
            ti.remote_status,
            ti.priority,
            ti.requester,
            ti.metadata_json,
            ti.created_at,
            ti.updated_at,
            (
                SELECT label
                FROM ticket_label_assignments tla
                WHERE tla.ticket_key = ti.ticket_key
                LIMIT 1
            ) AS ctox_label,
            (
                SELECT te.body_text
                FROM ticket_events te
                WHERE te.ticket_key = ti.ticket_key
                  AND te.direction = 'outbound'
                ORDER BY te.external_created_at DESC, te.observed_at DESC
                LIMIT 1
            ) AS latest_outbound_body,
            (
                SELECT te.body_text
                FROM ticket_events te
                WHERE te.ticket_key = ti.ticket_key
                  AND te.direction = 'inbound'
                ORDER BY te.external_created_at DESC, te.observed_at DESC
                LIMIT 1
            ) AS latest_inbound_body,
            (
                SELECT te.event_type
                FROM ticket_events te
                WHERE te.ticket_key = ti.ticket_key
                  AND te.direction = 'inbound'
                ORDER BY te.external_created_at DESC, te.observed_at DESC
                LIMIT 1
            ) AS latest_inbound_event_type
        FROM ticket_items ti
        WHERE ti.source_system = ?1
          AND NOT EXISTS (
              SELECT 1
              FROM ticket_self_work_items swi
              WHERE swi.source_system = ti.source_system
                AND swi.remote_ticket_id = ti.remote_ticket_id
          )
        ORDER BY ti.updated_at DESC
        "#,
    )?;
    let rows = statement.query_map(params![system], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<String>>(13)?,
        ))
    })?;

    let mut records = Vec::new();
    for row in rows {
        let (
            ticket_key,
            remote_ticket_id,
            title,
            body_text,
            remote_status,
            priority,
            requester,
            metadata_raw,
            created_at,
            updated_at,
            ctox_label,
            latest_outbound_body,
            latest_inbound_body,
            latest_inbound_event_type,
        ) = row?;
        if looks_like_ctox_internal_ticket(&title, &body_text) {
            continue;
        }
        let metadata = parse_json_column(metadata_raw);
        let channel = first_string_from_named_metadata(
            &metadata,
            &["channel", "source_channel", "article_type", "via"],
        )
        .or(latest_inbound_event_type.clone());
        let request_type = first_string_from_named_metadata(
            &metadata,
            &["ticket_type", "type", "kind", "request_type"],
        )
        .unwrap_or_else(|| "ticket".to_string());
        let category = first_string_from_named_metadata(
            &metadata,
            &[
                "group_name",
                "group",
                "queue",
                "service",
                "application",
                "product",
            ],
        )
        .or(ctox_label
            .as_deref()
            .and_then(|label| label.split('/').next())
            .map(ToOwned::to_owned))
        .unwrap_or_else(|| "general".to_string());
        let subcategory = first_string_from_named_metadata(
            &metadata,
            &["subcategory", "sub_type", "tag", "tags", "label", "labels"],
        )
        .or(ctox_label
            .as_deref()
            .and_then(|label| label.split('/').nth(1))
            .map(ToOwned::to_owned))
        .unwrap_or_else(|| "uncategorized".to_string());
        let action_text = latest_outbound_body
            .clone()
            .or(latest_inbound_body.clone())
            .unwrap_or_default();
        records.push(json!({
            "ticket_id": remote_ticket_id,
            "ticket_key": ticket_key,
            "title": title,
            "request_type": request_type,
            "category": category,
            "subcategory": subcategory,
            "channel": channel,
            "state": remote_status,
            "impact": priority.clone(),
            "priority": priority,
            "requester": requester,
            "request_text": body_text,
            "action_text": action_text,
            "owner": first_string_from_named_metadata(&metadata, &["owner", "owner_name", "assignee", "agent", "user"]),
            "group": first_string_from_named_metadata(&metadata, &["group_name", "group", "queue"]),
            "source_system": system,
            "created_at": created_at,
            "updated_at": updated_at,
        }));
    }
    Ok(records)
}

pub(crate) fn export_ticket_history_dataset(
    root: &Path,
    system: &str,
    output: &Path,
) -> Result<Value> {
    let records = extract_ticket_history_records(root, system)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = String::new();
    for record in &records {
        body.push_str(&serde_json::to_string(record)?);
        body.push('\n');
    }
    std::fs::write(output, body)?;
    Ok(json!({
        "ok": true,
        "system": system,
        "output": output.display().to_string(),
        "record_count": records.len(),
    }))
}

pub(crate) fn refresh_observed_ticket_knowledge(
    root: &Path,
    system: &str,
) -> Result<Vec<TicketKnowledgeEntryView>> {
    let mut conn = open_ticket_db(root)?;
    let mut metadata_keys = BTreeSet::new();
    let mut states = BTreeSet::new();
    let mut priorities = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut labels = BTreeSet::new();
    let mut requesters = BTreeSet::new();
    let mut owners = BTreeSet::new();
    let mut service_candidates = BTreeSet::new();
    let mut asset_candidates = BTreeSet::new();

    let mut statement = conn.prepare(
        r#"
        SELECT title, body_text, remote_status, priority, requester, metadata_json
        FROM ticket_items
        WHERE source_system = ?1
        ORDER BY updated_at DESC
        "#,
    )?;
    let rows = statement.query_map(params![system], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut ticket_count = 0usize;
    for row in rows {
        let (title, body_text, remote_status, priority, requester, metadata_raw) = row?;
        ticket_count += 1;
        if !remote_status.trim().is_empty() {
            states.insert(remote_status.trim().to_string());
        }
        if let Some(priority) = priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            priorities.insert(priority.to_string());
        }
        if let Some(requester) = requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            requesters.insert(requester.to_string());
        }

        let metadata = parse_json_column(metadata_raw);
        if let Some(object) = metadata.as_object() {
            for key in object.keys() {
                metadata_keys.insert(key.clone());
            }
            collect_strings_from_named_metadata(
                &metadata,
                &["group", "group_name", "queue"],
                &mut groups,
            );
            collect_strings_from_named_metadata(
                &metadata,
                &["tag", "tags", "label", "labels", "category", "categories"],
                &mut labels,
            );
            collect_strings_from_named_metadata(
                &metadata,
                &["owner", "owner_name", "assignee", "agent", "user"],
                &mut owners,
            );
            collect_strings_from_named_metadata(
                &metadata,
                &["service", "application", "product", "system"],
                &mut service_candidates,
            );
            collect_strings_from_named_metadata(
                &metadata,
                &[
                    "asset",
                    "device",
                    "host",
                    "hostname",
                    "fqdn",
                    "ip",
                    "ip_address",
                ],
                &mut asset_candidates,
            );
        }

        collect_bracketed_prefix(&title, &mut service_candidates);
        collect_asset_like_tokens(&title, &mut asset_candidates);
        collect_asset_like_tokens(&body_text, &mut asset_candidates);
    }
    drop(statement);

    let control = load_ticket_source_control_from_conn(&conn, system)?;
    let metadata_key_list = truncate_set(&metadata_keys, 24);
    let state_list = truncate_set(&states, 24);
    let priority_list = truncate_set(&priorities, 16);
    let group_list = truncate_set(&groups, 20);
    let label_list = truncate_set(&labels, 32);
    let requester_list = truncate_set(&requesters, 32);
    let owner_list = truncate_set(&owners, 32);
    let service_list = truncate_set(&service_candidates, 24);
    let asset_list = truncate_set(&asset_candidates, 24);
    let glossary_terms = {
        let mut terms = BTreeSet::new();
        for term in group_list
            .iter()
            .chain(label_list.iter())
            .chain(service_list.iter())
            .chain(asset_list.iter())
            .chain(metadata_key_list.iter())
        {
            if !term.trim().is_empty() {
                terms.insert(term.clone());
            }
        }
        truncate_set(&terms, 40)
    };

    let source_profile = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "source_profile".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed operating profile"),
            summary: format!(
                "Observed {} mirrored tickets with {} states, {} groups, {} metadata keys.",
                ticket_count,
                state_list.len(),
                group_list.len(),
                metadata_key_list.len()
            ),
            status: "observed".to_string(),
            content: json!({
                "ticket_count": ticket_count,
                "observed_states": state_list.clone(),
                "observed_priorities": priority_list.clone(),
                "observed_groups": group_list.clone(),
                "observed_metadata_keys": metadata_key_list.clone(),
                "adoption_mode": control.as_ref().map(|item| item.adoption_mode.clone()),
                "baseline_external_created_cutoff": control.as_ref().map(|item| item.baseline_external_created_cutoff.clone()),
            }),
        },
    )?;
    let label_catalog = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "label_catalog".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed label catalog"),
            summary: format!(
                "Observed {} label/tag candidates and {} queue/group markers.",
                label_list.len(),
                group_list.len()
            ),
            status: "observed".to_string(),
            content: json!({
                "observed_labels": label_list.clone(),
                "observed_groups": group_list.clone(),
            }),
        },
    )?;
    let glossary = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "glossary".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed glossary"),
            summary: if glossary_terms.is_empty() {
                "No stable glossary terms have been inferred yet.".to_string()
            } else {
                format!(
                    "Observed {} candidate glossary terms.",
                    glossary_terms.len()
                )
            },
            status: if glossary_terms.is_empty() {
                "draft".to_string()
            } else {
                "observed".to_string()
            },
            content: json!({
                "candidate_terms": glossary_terms.clone(),
            }),
        },
    )?;
    let service_catalog = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "service_catalog".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed service catalog"),
            summary: if service_list.is_empty() {
                "No stable service candidates have been inferred yet.".to_string()
            } else {
                format!("Observed {} service candidates.", service_list.len())
            },
            status: if service_list.is_empty() {
                "draft".to_string()
            } else {
                "observed".to_string()
            },
            content: json!({
                "candidate_services": service_list.clone(),
            }),
        },
    )?;
    let infrastructure_assets = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "infrastructure_assets".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed infrastructure assets"),
            summary: if asset_list.is_empty() {
                "No stable infrastructure assets have been inferred yet.".to_string()
            } else {
                format!("Observed {} asset candidates.", asset_list.len())
            },
            status: if asset_list.is_empty() {
                "draft".to_string()
            } else {
                "observed".to_string()
            },
            content: json!({
                "candidate_assets": asset_list.clone(),
            }),
        },
    )?;
    let team_model = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "team_model".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed team model"),
            summary: format!(
                "Observed {} requesters, {} owners/agents, and {} groups.",
                requester_list.len(),
                owner_list.len(),
                group_list.len()
            ),
            status: "observed".to_string(),
            content: json!({
                "observed_requesters": requester_list.clone(),
                "observed_owners": owner_list.clone(),
                "observed_groups": group_list.clone(),
            }),
        },
    )?;
    let access_model = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "access_model".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed access model"),
            summary: if owner_list.is_empty() && group_list.is_empty() {
                "No stable access or approval model has been inferred yet.".to_string()
            } else {
                format!(
                    "Observed {} owners/agents, {} groups, and {} requesters that shape access boundaries.",
                    owner_list.len(),
                    group_list.len(),
                    requester_list.len()
                )
            },
            status: if owner_list.is_empty() && group_list.is_empty() {
                "draft".to_string()
            } else {
                "observed".to_string()
            },
            content: json!({
                "observed_requesters": requester_list.clone(),
                "observed_owners": owner_list.clone(),
                "observed_groups": group_list.clone(),
                "access_request_channels": ["mail", "jami", "local_secret_store"],
            }),
        },
    )?;
    let monitoring_landscape = put_ticket_knowledge_entry_internal(
        &mut conn,
        TicketKnowledgeUpsertInput {
            source_system: system.to_string(),
            domain: "monitoring_landscape".to_string(),
            knowledge_key: "observed".to_string(),
            title: format!("{system} observed monitoring landscape"),
            summary: "No monitoring snapshot has been ingested yet; monitoring understanding is still a knowledge gap.".to_string(),
            status: "draft".to_string(),
            content: json!({
                "sources": [],
                "services": service_list.clone(),
                "assets": asset_list.clone(),
                "coverage_status": "missing_snapshot",
            }),
        },
    )?;
    Ok(vec![
        source_profile,
        label_catalog,
        glossary,
        service_catalog,
        infrastructure_assets,
        team_model,
        access_model,
        monitoring_landscape,
    ])
}

pub(crate) fn list_ticket_knowledge_entries(
    root: &Path,
    system: Option<&str>,
    domain: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketKnowledgeEntryView>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, content_json, created_at, updated_at
        FROM ticket_knowledge_entries
        WHERE (?1 IS NULL OR source_system = ?1)
          AND (?2 IS NULL OR domain = ?2)
          AND (?3 IS NULL OR status = ?3)
        ORDER BY source_system ASC, domain ASC, updated_at DESC
        LIMIT ?4
        "#,
    )?;
    let rows = statement.query_map(
        params![system, domain, status, limit as i64],
        map_ticket_knowledge_entry_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn load_ticket_knowledge_entry(
    root: &Path,
    system: &str,
    domain: &str,
    key: &str,
) -> Result<Option<TicketKnowledgeEntryView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, content_json, created_at, updated_at
        FROM ticket_knowledge_entries
        WHERE source_system = ?1 AND domain = ?2 AND knowledge_key = ?3
        LIMIT 1
        "#,
        params![system, domain, key],
        map_ticket_knowledge_entry_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_preferred_ticket_knowledge_entry(
    conn: &Connection,
    system: &str,
    domain: &str,
) -> Result<Option<TicketKnowledgeEntryView>> {
    conn.query_row(
        r#"
        SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, content_json, created_at, updated_at
        FROM ticket_knowledge_entries
        WHERE source_system = ?1 AND domain = ?2
        ORDER BY
            CASE status
                WHEN 'confirmed' THEN 0
                WHEN 'observed' THEN 1
                WHEN 'draft' THEN 2
                ELSE 3
            END,
            updated_at DESC
        LIMIT 1
        "#,
        params![system, domain],
        map_ticket_knowledge_entry_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn create_ticket_knowledge_load(
    root: &Path,
    ticket_key: &str,
    domains: Option<&[String]>,
) -> Result<TicketKnowledgeLoadView> {
    let mut conn = open_ticket_db(root)?;
    let ticket = load_ticket(root, ticket_key)?.context("ticket not found for knowledge load")?;
    let requested_domains = domains
        .map(|items| {
            items
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            REQUIRED_KNOWLEDGE_DOMAINS
                .iter()
                .map(|item| item.to_string())
                .collect()
        });

    let mut loaded_entries = Vec::new();
    let mut gap_domains = Vec::new();
    for domain in &requested_domains {
        if let Some(entry) =
            load_preferred_ticket_knowledge_entry(&conn, &ticket.source_system, domain)?
        {
            loaded_entries.push(entry);
        } else {
            gap_domains.push(domain.clone());
        }
    }
    let now = now_iso_string();
    let load_id = format!("knowledge-load:{}:{}", ticket_key, stable_digest(&now));
    let status = if gap_domains.is_empty() {
        "ready"
    } else {
        "gapped"
    };
    conn.execute(
        r#"
        INSERT INTO ticket_knowledge_loads (
            load_id, ticket_key, source_system, domains_json, loaded_entries_json,
            gap_domains_json, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            load_id,
            ticket_key,
            ticket.source_system,
            serde_json::to_string(&requested_domains)?,
            serde_json::to_string(&loaded_entries)?,
            serde_json::to_string(&gap_domains)?,
            status,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key,
            case_id: None,
            actor_type: "knowledge_gate",
            action_type: "knowledge_load",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "load_id": load_id,
                "source_system": ticket.source_system,
                "domains": requested_domains,
                "loaded_domains": loaded_entries.iter().map(|item| item.domain.clone()).collect::<Vec<_>>(),
                "gap_domains": gap_domains,
                "status": status,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "knowledge.loaded",
            title: "Knowledge loaded",
            body_text: if gap_domains.is_empty() {
                "Knowledge gate loaded all requested domains."
            } else {
                "Knowledge gate loaded with missing domains."
            },
            message_key: None,
            work_id: None,
            ticket_key: Some(ticket_key),
            attempt_index: None,
            metadata: json!({
                "load_id": load_id,
                "source_system": ticket.source_system,
                "domains": requested_domains,
                "loaded_count": loaded_entries.len(),
                "gap_domains": gap_domains,
                "status": status,
            }),
        },
    );
    Ok(TicketKnowledgeLoadView {
        load_id,
        ticket_key: ticket_key.to_string(),
        source_system: ticket.source_system,
        domains: requested_domains,
        loaded_entries,
        gap_domains,
        status: status.to_string(),
        created_at: now,
    })
}

fn collect_strings_from_named_metadata(
    metadata: &Value,
    keys: &[&str],
    target: &mut BTreeSet<String>,
) {
    for key in keys {
        if let Some(value) = metadata.get(*key) {
            collect_strings_from_value(value, target);
        }
    }
}

fn collect_strings_from_value(value: &Value, target: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                target.insert(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_strings_from_value(item, target);
            }
        }
        _ => {}
    }
}

fn collect_bracketed_prefix(text: &str, target: &mut BTreeSet<String>) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let candidate = rest[..end].trim();
            if !candidate.is_empty() {
                target.insert(candidate.to_string());
            }
        }
    }
}

fn collect_asset_like_tokens(text: &str, target: &mut BTreeSet<String>) {
    for token in text.split_whitespace() {
        let cleaned = token
            .trim_matches(|ch: char| {
                !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '_'
            })
            .trim();
        if cleaned.is_empty() {
            continue;
        }
        let looks_like_host = cleaned.contains('.')
            || cleaned.chars().any(|ch| ch.is_ascii_digit()) && cleaned.contains('-');
        if looks_like_host && cleaned.len() >= 4 {
            target.insert(cleaned.to_string());
        }
    }
}

fn truncate_set(set: &BTreeSet<String>, limit: usize) -> Vec<String> {
    set.iter().take(limit).cloned().collect::<Vec<_>>()
}

fn parse_domain_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
}

fn summarize_monitoring_snapshot(snapshot: &Value) -> String {
    let sources = snapshot
        .get("sources")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let alerts = snapshot
        .get("alerts")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let services = snapshot
        .get("services")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    format!(
        "Ingested monitoring snapshot with {} sources, {} services, and {} active alerts.",
        sources, services, alerts
    )
}

pub(crate) fn lease_pending_ticket_events(
    root: &Path,
    limit: usize,
    lease_owner: &str,
) -> Result<Vec<TicketEventView>> {
    lease_pending_ticket_events_for_sources(root, limit, lease_owner, None)
}

pub(crate) fn lease_pending_ticket_events_for_sources(
    root: &Path,
    limit: usize,
    lease_owner: &str,
    allowed_sources: Option<&HashSet<String>>,
) -> Result<Vec<TicketEventView>> {
    let conn = open_ticket_db(root)?;
    ensure_ticket_event_routing_rows(&conn)?;
    let allowed = allowed_sources
        .map(|sources| {
            sources
                .iter()
                .map(|source| source.trim().to_ascii_lowercase())
                .filter(|source| !source.is_empty())
                .collect::<BTreeSet<_>>()
        })
        .filter(|sources| !sources.is_empty());
    if allowed_sources.is_some() && allowed.is_none() {
        return Ok(Vec::new());
    }
    let mut sql = r#"
        SELECT e.event_key, e.ticket_key, e.source_system, e.remote_event_id, e.direction,
               e.event_type, e.summary, e.body_text, e.metadata_json, e.external_created_at, e.observed_at
        FROM ticket_events e
        JOIN ticket_event_routing_state r ON r.event_key = e.event_key
        WHERE e.direction = 'inbound'
          AND r.route_status = 'pending'
          AND (r.retry_not_before IS NULL OR r.retry_not_before='' OR r.retry_not_before<=?3)
        ORDER BY e.external_created_at ASC, e.observed_at ASC
        LIMIT ?2
        "#
    .to_string();
    if let Some(sources) = allowed.as_ref() {
        let source_list = sources
            .iter()
            .map(|source| format!("'{}'", source.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        sql = sql.replace(
            "ORDER BY e.external_created_at ASC, e.observed_at ASC",
            &format!(
                "AND lower(e.source_system) IN ({source_list})\n        ORDER BY e.external_created_at ASC, e.observed_at ASC"
            ),
        );
    }
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        params![lease_owner, limit as i64, now_iso_string()],
        map_ticket_event_row,
    )?;
    let events = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let tx = conn.unchecked_transaction()?;
    let leased_at = now_iso_string();
    let lease_expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    for event in &events {
        let previous_route_status = current_ticket_event_route_status(&tx, &event.event_key)?;
        enforce_ticket_event_route_status_transition(
            &tx,
            &event.event_key,
            &previous_route_status,
            "leased",
            lease_owner,
            "lease_pending_ticket_events",
            None,
        )?;
        tx.execute(
            r#"
            INSERT INTO ticket_event_routing_state (
                event_key, route_status, lease_owner, leased_at, lease_expires_at,
                acked_at, updated_at
            ) VALUES (?1, 'leased', ?2, ?3, ?4, NULL, ?3)
            ON CONFLICT(event_key) DO UPDATE SET
                route_status='leased',
                lease_owner=excluded.lease_owner,
                leased_at=excluded.leased_at,
                lease_expires_at=excluded.lease_expires_at,
                updated_at=excluded.updated_at
            WHERE ticket_event_routing_state.route_status='pending'
            "#,
            params![event.event_key, lease_owner, leased_at, lease_expires_at],
        )?;
    }
    tx.commit()?;
    Ok(events)
}

pub(crate) fn ack_leased_ticket_events(
    root: &Path,
    event_keys: &[String],
    status: &str,
) -> Result<usize> {
    let canonical_status = canonical_ticket_event_route_status(status)?;
    if canonical_status == "failed" {
        return fail_ticket_events(
            root,
            event_keys,
            TicketEventFailureClass::Terminal,
            "terminal ticket-event failure acknowledged by policy/review path",
        );
    }
    let conn = open_ticket_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let mut updated = 0usize;
    for event_key in event_keys {
        let previous_route_status = current_ticket_event_route_status(&tx, event_key)?;
        enforce_ticket_event_route_status_transition(
            &tx,
            event_key,
            &previous_route_status,
            canonical_status,
            "ctox-ticket-ack",
            "ack_leased_ticket_events",
            None,
        )?;
        updated += tx.execute(
            r#"
            INSERT INTO ticket_event_routing_state (
                event_key, route_status, lease_owner, leased_at, acked_at, updated_at
            )
            SELECT ?1, ?2, NULL, NULL,
                   CASE WHEN ?2 IN ('handled', 'duplicate', 'blocked') THEN ?3 ELSE NULL END,
                   ?3
            FROM ticket_events
            WHERE event_key = ?1
            ON CONFLICT(event_key) DO UPDATE SET
                route_status=excluded.route_status,
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=excluded.acked_at,
                updated_at=excluded.updated_at
            "#,
            params![event_key, canonical_status, now],
        )?;
    }
    tx.commit()?;
    Ok(updated)
}

pub(crate) fn block_ticket_events_for_wait(
    root: &Path,
    event_keys: &[String],
    wait_ref: &crate::mission::plan::WaitRef,
    summary: &str,
) -> Result<usize> {
    let updated = ack_leased_ticket_events(root, event_keys, "blocked")?;
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    for event_key in event_keys {
        conn.execute(
            r#"
            UPDATE ticket_event_routing_state
            SET hold_reason='waiting_external', wait_entity_type=?2,
                wait_entity_id=?3, retry_not_before=NULL,
                failure_proof=?4, lease_expires_at=NULL, updated_at=?5
            WHERE event_key=?1 AND route_status='blocked'
            "#,
            params![
                event_key,
                wait_ref.entity_type,
                wait_ref.entity_id,
                summary.trim(),
                now,
            ],
        )?;
    }
    Ok(updated)
}

pub(crate) fn wake_ticket_events_waiting_for(
    root: &Path,
    entity_type: &str,
    entity_id: &str,
) -> Result<usize> {
    let conn = open_ticket_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let event_keys = {
        let mut statement = tx.prepare(
            r#"
            SELECT event_key
            FROM ticket_event_routing_state
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
    let mut updated = 0usize;
    for event_key in event_keys {
        let changed = tx.execute(
            r#"UPDATE ticket_event_routing_state
               SET route_status='pending', hold_reason=NULL, wait_entity_type=NULL,
                   wait_entity_id=NULL, retry_not_before=NULL, acked_at=NULL, updated_at=?2
               WHERE event_key=?1 AND route_status='blocked' AND hold_reason='waiting_external'"#,
            params![event_key, now],
        )?;
        if changed != 0 {
            enforce_ticket_event_route_status_transition(
                &tx,
                &event_key,
                "blocked",
                "pending",
                "ctox-wait-wakeup",
                "wake_ticket_events_waiting_for",
                None,
            )?;
            updated += changed;
        }
    }
    tx.commit()?;
    Ok(updated)
}

pub(crate) fn fail_ticket_events(
    root: &Path,
    event_keys: &[String],
    failure_class: TicketEventFailureClass,
    reason: &str,
) -> Result<usize> {
    let conn = open_ticket_db(root)?;
    let tx = conn.unchecked_transaction()?;
    let now = now_iso_string();
    let mut updated = 0usize;
    for event_key in event_keys {
        let previous_attempts: i64 = tx
            .query_row(
                "SELECT failure_attempt_count FROM ticket_event_routing_state WHERE event_key=?1",
                params![event_key],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let attempts = previous_attempts.saturating_add(1);
        let exhausted = matches!(failure_class, TicketEventFailureClass::Terminal) || attempts >= 3;
        let next_status = if exhausted { "failed" } else { "pending" };
        let retry_not_before = if exhausted {
            None
        } else {
            let exponent = u32::try_from(attempts.saturating_sub(1))
                .unwrap_or(16)
                .min(16);
            let seconds = 300_i64
                .saturating_mul(2_i64.saturating_pow(exponent))
                .min(3_600);
            Some((chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339())
        };
        let failure_proof = exhausted.then(|| {
            format!(
                "ticket-event-terminal-failure class={} attempts={} reason={}",
                failure_class.as_str(),
                attempts,
                reason.trim()
            )
        });
        let previous_status = current_ticket_event_route_status(&tx, event_key)?;
        enforce_ticket_event_route_status_transition(
            &tx,
            event_key,
            &previous_status,
            next_status,
            "ctox-ticket-failure-classifier",
            reason,
            Some(failure_class.as_str()),
        )?;
        updated += tx.execute(
            r#"
            UPDATE ticket_event_routing_state
            SET route_status=?2, lease_owner=NULL, leased_at=NULL, lease_expires_at=NULL,
                failure_class=?3, failure_attempt_count=?4, retry_not_before=?5,
                failure_proof=?6, updated_at=?7
            WHERE event_key=?1
            "#,
            params![
                event_key,
                next_status,
                failure_class.as_str(),
                attempts,
                retry_not_before,
                failure_proof,
                now,
            ],
        )?;
    }
    tx.commit()?;
    Ok(updated)
}

pub(crate) fn release_stale_ticket_event_leases(
    root: &Path,
    _lease_owner: &str,
    active_event_keys: &HashSet<String>,
) -> Result<Vec<String>> {
    with_reconcile_ticket_db(root, |conn| {
        let mut statement = conn.prepare(
            r#"
        SELECT event_key
        FROM ticket_event_routing_state
        WHERE route_status = 'leased'
          AND lease_expires_at IS NOT NULL
          AND datetime(lease_expires_at) <= datetime('now')
        ORDER BY leased_at ASC, updated_at ASC
        LIMIT 128
        "#,
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);

        let now = now_iso_string();
        let mut released = Vec::new();
        for event_key in candidates {
            if active_event_keys.contains(&event_key) {
                continue;
            }
            let previous_route_status = current_ticket_event_route_status(conn, &event_key)?;
            enforce_ticket_event_route_status_transition(
                conn,
                &event_key,
                &previous_route_status,
                "pending",
                "ctox-ticket-reconcile",
                "release_stale_ticket_event_leases",
                None,
            )?;
            conn.execute(
                r#"
            UPDATE ticket_event_routing_state
            SET route_status='pending',
                lease_owner=NULL,
                leased_at=NULL,
                lease_expires_at=NULL,
                acked_at=NULL,
                updated_at=?2
            WHERE event_key = ?1
              AND route_status = 'leased'
            "#,
                params![event_key, now],
            )?;
            released.push(event_key);
        }
        Ok(released)
    })
}

pub(crate) fn renew_ticket_event_leases(
    root: &Path,
    lease_owner: &str,
    event_keys: &[String],
) -> Result<usize> {
    if event_keys.is_empty() {
        return Ok(0);
    }
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    let expires = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    let mut renewed = 0usize;
    for event_key in event_keys {
        renewed += conn.execute(
            r#"
            UPDATE ticket_event_routing_state
            SET lease_expires_at=?3, updated_at=?4
            WHERE event_key=?1 AND route_status='leased' AND lease_owner=?2
            "#,
            params![event_key, lease_owner, expires, now],
        )?;
    }
    Ok(renewed)
}

pub(crate) fn release_ready_blocked_ticket_events(
    root: &Path,
    limit: usize,
) -> Result<Vec<String>> {
    with_reconcile_ticket_db(root, |conn| {
        let mut statement = conn.prepare(
        r#"
        SELECT e.event_key, e.ticket_key, e.source_system, e.remote_event_id, e.direction,
               e.event_type, e.summary, e.body_text, e.metadata_json, e.external_created_at, e.observed_at
        FROM ticket_events e
        JOIN ticket_event_routing_state r ON r.event_key = e.event_key
        WHERE e.direction = 'inbound'
          AND r.route_status = 'blocked'
          AND COALESCE(r.hold_reason, '') != 'waiting_external'
        ORDER BY e.external_created_at ASC, e.observed_at ASC
        LIMIT ?1
        "#,
    )?;
        let rows = statement.query_map(params![limit as i64], map_ticket_event_row)?;
        let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);

        let now = now_iso_string();
        let mut released = Vec::new();
        for event in candidates {
            if ticket_event_ready_for_preparation(root, &event).is_err() {
                continue;
            }
            let previous_route_status = current_ticket_event_route_status(conn, &event.event_key)?;
            enforce_ticket_event_route_status_transition(
                conn,
                &event.event_key,
                &previous_route_status,
                "pending",
                "ctox-ticket-router",
                "release_ready_blocked_ticket_events",
                None,
            )?;
            conn.execute(
                r#"
            UPDATE ticket_event_routing_state
            SET route_status='pending',
                lease_owner=NULL,
                leased_at=NULL,
                acked_at=NULL,
                updated_at=?2
            WHERE event_key = ?1
              AND route_status = 'blocked'
            "#,
                params![event.event_key, now],
            )?;
            released.push(event.event_key);
        }
        Ok(released)
    })
}

fn ticket_event_ready_for_preparation(root: &Path, event: &TicketEventView) -> Result<()> {
    let ticket = load_ticket(root, &event.ticket_key)?.context("ticket not found for event")?;
    let conn = open_ticket_db(root)?;
    let mut missing = Vec::new();
    for domain in REQUIRED_KNOWLEDGE_DOMAINS {
        if load_preferred_ticket_knowledge_entry(&conn, &ticket.source_system, domain)?.is_none() {
            missing.push((*domain).to_string());
        }
    }
    if !missing.is_empty() {
        anyhow::bail!(
            "ticket knowledge gate: missing required knowledge domains for {}: {}",
            event.ticket_key,
            missing.join(", ")
        );
    }
    drop(conn);
    let _ = resolve_ticket_control(root, &event.ticket_key)?;
    Ok(())
}

fn load_ticket_self_work_item_for_ticket_key(
    conn: &Connection,
    ticket_key: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    conn.query_row(
        r#"
        SELECT sw.work_id, sw.source_system, sw.kind, sw.title, sw.body_text, sw.state,
               sw.metadata_json, sw.remote_ticket_id, sw.remote_locator, sw.created_at, sw.updated_at,
               ta.assigned_to, ta.assigned_by, ta.created_at
        FROM ticket_self_work_items sw
        JOIN ticket_items ti
          ON ti.source_system = sw.source_system
         AND ti.remote_ticket_id = sw.remote_ticket_id
        LEFT JOIN ticket_self_work_assignments ta
          ON ta.assignment_id = (
              SELECT assignment_id
              FROM ticket_self_work_assignments
              WHERE work_id = sw.work_id
              ORDER BY created_at DESC
              LIMIT 1
          )
        WHERE ti.ticket_key = ?1
        ORDER BY sw.updated_at DESC
        LIMIT 1
        "#,
        params![ticket_key],
        map_ticket_self_work_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn synthetic_label_assignment_for_self_work(
    ticket_key: &str,
    item: &TicketSelfWorkItemView,
) -> TicketLabelAssignmentView {
    TicketLabelAssignmentView {
        ticket_key: ticket_key.to_string(),
        label: format!("self-work/{}", item.kind.trim()),
        assigned_by: "ctox".to_string(),
        rationale: Some("synthetic self-work control routing".to_string()),
        evidence: json!({
            "work_id": item.work_id,
            "kind": item.kind,
            "source": "ticket_self_work"
        }),
        assigned_at: item.updated_at.clone(),
        updated_at: item.updated_at.clone(),
    }
}

fn synthetic_bundle_for_self_work(
    item: &TicketSelfWorkItemView,
    label_assignment: &TicketLabelAssignmentView,
) -> ControlBundleView {
    ControlBundleView {
        label: label_assignment.label.clone(),
        bundle_version: 1,
        runbook_id: format!("self-work:{}", item.kind.trim()),
        runbook_version: "v1".to_string(),
        policy_id: "self-work-controlled".to_string(),
        policy_version: "v1".to_string(),
        approval_mode: DEFAULT_APPROVAL_MODE.to_string(),
        autonomy_level: DEFAULT_AUTONOMY_LEVEL.to_string(),
        verification_profile_id: "verify-self-work".to_string(),
        writeback_profile_id: "writeback-comment".to_string(),
        support_mode: "internal_self_work".to_string(),
        default_risk_level: DEFAULT_RISK_LEVEL.to_string(),
        execution_actions: vec![
            "observe".to_string(),
            "analyze".to_string(),
            "draft_communication".to_string(),
        ],
        notes: Some(format!(
            "Synthetic control bundle for published internal work kind {}",
            item.kind.trim()
        )),
        updated_at: item.updated_at.clone(),
    }
}

fn resolve_ticket_control(
    root: &Path,
    ticket_key: &str,
) -> Result<(
    TicketLabelAssignmentView,
    ControlBundleView,
    EffectiveControlResolution,
)> {
    if let Some(label_assignment) = load_ticket_label_assignment(root, ticket_key)? {
        let bundle = load_control_bundle(root, &label_assignment.label)?
            .context("no active control bundle for ticket label")?;
        let grant =
            load_active_autonomy_grant(root, &label_assignment.label, bundle.bundle_version)?;
        let effective_control = resolve_effective_control(&bundle, grant)?;
        return Ok((label_assignment, bundle, effective_control));
    }

    let conn = open_ticket_db(root)?;
    let self_work = load_ticket_self_work_item_for_ticket_key(&conn, ticket_key)?
        .context("ticket has no primary label assignment")?;
    let label_assignment = synthetic_label_assignment_for_self_work(ticket_key, &self_work);
    let bundle = synthetic_bundle_for_self_work(&self_work, &label_assignment);
    let effective_control = resolve_effective_control(&bundle, None)?;
    Ok((label_assignment, bundle, effective_control))
}

pub(crate) fn prepare_ticket_event_for_prompt(
    root: &Path,
    event_key: &str,
) -> Result<RoutedTicketEvent> {
    let event = load_ticket_event(root, event_key)?.context("ticket event not found")?;
    let ticket = load_ticket(root, &event.ticket_key)?.context("ticket not found for event")?;
    let (label_assignment, bundle, _) = resolve_ticket_control(root, &event.ticket_key)?;
    let understanding = format!(
        "{} | {} | {}",
        ticket.title.trim(),
        event.event_type.trim(),
        collapse_inline(event.summary.trim(), 160)
    );
    let dry_run = create_dry_run(root, &event.ticket_key, Some(&understanding), None)?;
    let case = load_case(root, &dry_run.case_id)?.context("ticket case missing after dry run")?;
    let thread_key = ticket_thread_key(&ticket);
    Ok(RoutedTicketEvent {
        event_key: event.event_key,
        ticket_key: event.ticket_key,
        source_system: event.source_system,
        remote_event_id: event.remote_event_id,
        event_type: event.event_type,
        summary: event.summary,
        body_text: event.body_text,
        title: ticket.title,
        remote_status: ticket.remote_status,
        label: label_assignment.label,
        bundle_label: bundle.label,
        bundle_version: bundle.bundle_version,
        case_id: case.case_id,
        dry_run_id: dry_run.dry_run_id,
        dry_run_artifact: dry_run.artifact,
        support_mode: case.support_mode.clone(),
        approval_mode: case.approval_mode.clone(),
        autonomy_level: case.autonomy_level.clone(),
        risk_level: case.risk_level,
        thread_key,
    })
}

pub(crate) fn suggested_skill_for_routed_event(
    root: &Path,
    event: &RoutedTicketEvent,
) -> Result<Option<String>> {
    let conn = open_ticket_db(root)?;
    let Some(self_work) = load_ticket_self_work_item_for_ticket_key(&conn, &event.ticket_key)?
    else {
        return Ok(None);
    };
    let metadata = self_work.metadata.clone();
    let explicit = metadata
        .get("skill")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok(explicit.or_else(|| default_skill_for_self_work_kind(&self_work.kind)))
}

pub(crate) fn preferred_skill_for_ticket_source(
    root: &Path,
    source_system: &str,
) -> Result<Option<String>> {
    let conn = open_ticket_db(root)?;
    if let Some(binding) = load_active_ticket_source_skill_binding_from_conn(&conn, source_system)?
    {
        return Ok(Some(binding.skill_name));
    }
    if load_ticket_source_control_from_conn(&conn, source_system)?.is_some() {
        return Ok(Some("system-onboarding".to_string()));
    }
    Ok(None)
}

pub(crate) fn upsert_ticket_from_adapter(
    root: &Path,
    request: AdapterTicketMirrorRequest<'_>,
) -> Result<AdapterTicketUpsertResult> {
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    let ticket_key = canonical_ticket_key(request.system, request.remote_ticket_id);
    let metadata_json = serde_json::to_string(&request.metadata)?;
    let title = request.title.trim();
    let body_text = request.body_text.trim();
    let remote_status = request.remote_status.trim();
    let priority = request.priority.map(str::trim);
    let requester = request.requester.map(str::trim);
    let existing = conn
        .query_row(
            r#"
            SELECT title, body_text, remote_status, priority, requester,
                   metadata_json, created_at, updated_at
            FROM ticket_items
            WHERE ticket_key = ?1
            LIMIT 1
            "#,
            params![ticket_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    if existing.as_ref().is_some_and(
        |(
            existing_title,
            existing_body_text,
            existing_status,
            existing_priority,
            existing_requester,
            existing_metadata,
            existing_created_at,
            existing_updated_at,
        )| {
            existing_title == title
                && existing_body_text == body_text
                && existing_status == remote_status
                && existing_priority.as_deref() == priority
                && existing_requester.as_deref() == requester
                && existing_metadata == &metadata_json
                && existing_created_at == request.external_created_at
                && existing_updated_at == request.external_updated_at
        },
    ) {
        return Ok(AdapterTicketUpsertResult {
            key: ticket_key,
            changed: false,
        });
    }
    conn.execute(
        r#"
        INSERT INTO ticket_items (
            ticket_key, source_system, remote_ticket_id, title, body_text, remote_status,
            priority, requester, metadata_json, created_at, updated_at, last_synced_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(ticket_key) DO UPDATE SET
            title=excluded.title,
            body_text=excluded.body_text,
            remote_status=excluded.remote_status,
            priority=excluded.priority,
            requester=excluded.requester,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at,
            last_synced_at=excluded.last_synced_at
        "#,
        params![
            ticket_key,
            request.system,
            request.remote_ticket_id,
            title,
            body_text,
            remote_status,
            priority,
            requester,
            metadata_json,
            request.external_created_at,
            request.external_updated_at,
            now,
        ],
    )?;
    Ok(AdapterTicketUpsertResult {
        key: ticket_key,
        changed: true,
    })
}

pub(crate) fn upsert_ticket_event_from_adapter(
    root: &Path,
    request: AdapterTicketEventRequest<'_>,
) -> Result<AdapterTicketUpsertResult> {
    let conn = open_ticket_db(root)?;
    let observed_at = now_iso_string();
    let ticket_key = canonical_ticket_key(request.system, request.remote_ticket_id);
    let event_key = canonical_event_key(request.system, request.remote_event_id);
    let effective_direction =
        if is_remote_event_marked_outbound(&conn, request.system, request.remote_event_id)? {
            "outbound"
        } else {
            request.direction
        };
    let event_type = request.event_type.trim();
    let summary = request.summary.trim();
    let body_text = request.body_text.trim();
    let metadata_json = serde_json::to_string(&request.metadata)?;
    let initial_route_status = if effective_direction == "outbound" {
        "handled"
    } else {
        initial_route_status_for_inbound_event(&conn, request.system, request.external_created_at)?
    };
    let terminal_policy_grant = match (effective_direction, initial_route_status) {
        ("outbound", "handled") => Some(TerminalPolicyGrant::outbound_ticket_event()),
        ("inbound", "observed") => {
            Some(TerminalPolicyGrant::baseline_observed_inbound_ticket_event())
        }
        _ => None,
    };
    let existing = conn
        .query_row(
            r#"
            SELECT direction, event_type, summary, body_text, metadata_json, external_created_at
            FROM ticket_events
            WHERE event_key = ?1
            LIMIT 1
            "#,
            params![event_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    if existing.as_ref().is_some_and(
        |(
            existing_direction,
            existing_event_type,
            existing_summary,
            existing_body_text,
            existing_metadata,
            existing_created_at,
        )| {
            existing_direction == effective_direction
                && existing_event_type == event_type
                && existing_summary == summary
                && existing_body_text == body_text
                && existing_metadata == &metadata_json
                && existing_created_at == request.external_created_at
        },
    ) {
        let existing_route_status = conn
            .query_row(
                "SELECT route_status FROM ticket_event_routing_state WHERE event_key = ?1 LIMIT 1",
                params![event_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(existing_route_status) = existing_route_status {
            canonical_ticket_event_route_status(&existing_route_status)?;
            return Ok(AdapterTicketUpsertResult {
                key: event_key,
                changed: false,
            });
        }
        force_ticket_event_routed_state(
            &conn,
            &event_key,
            initial_route_status,
            terminal_policy_grant.as_ref(),
        )?;
        return Ok(AdapterTicketUpsertResult {
            key: event_key,
            changed: true,
        });
    }
    conn.execute(
        r#"
        INSERT INTO ticket_events (
            event_key, ticket_key, source_system, remote_event_id, direction, event_type,
            summary, body_text, metadata_json, external_created_at, observed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(event_key) DO UPDATE SET
            summary=excluded.summary,
            body_text=excluded.body_text,
            metadata_json=excluded.metadata_json,
            observed_at=excluded.observed_at
        "#,
        params![
            event_key,
            ticket_key,
            request.system,
            request.remote_event_id,
            effective_direction,
            event_type,
            summary,
            body_text,
            metadata_json,
            request.external_created_at,
            observed_at,
        ],
    )?;
    force_ticket_event_routed_state(
        &conn,
        &event_key,
        initial_route_status,
        terminal_policy_grant.as_ref(),
    )?;
    Ok(AdapterTicketUpsertResult {
        key: event_key,
        changed: true,
    })
}

fn mark_remote_events_outbound(
    root: &Path,
    system: &str,
    remote_event_ids: &[String],
) -> Result<()> {
    if remote_event_ids.is_empty() {
        return Ok(());
    }
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    for remote_event_id in remote_event_ids {
        conn.execute(
            r#"
            INSERT INTO ticket_outbound_event_marks (
                source_system, remote_event_id, marked_at
            ) VALUES (?1, ?2, ?3)
            ON CONFLICT(source_system, remote_event_id) DO UPDATE SET
                marked_at=excluded.marked_at
            "#,
            params![system, remote_event_id, now],
        )?;
        let event_key = canonical_event_key(system, remote_event_id);
        conn.execute(
            "UPDATE ticket_events SET direction = 'outbound' WHERE event_key = ?1",
            params![event_key],
        )?;
        force_ticket_event_routed_state(
            &conn,
            &event_key,
            "handled",
            Some(&TerminalPolicyGrant::outbound_ticket_event()),
        )?;
    }
    Ok(())
}

fn force_ticket_event_routed_state(
    conn: &Connection,
    event_key: &str,
    route_status: &str,
    terminal_policy_grant: Option<&TerminalPolicyGrant>,
) -> Result<()> {
    let now = now_iso_string();
    force_ticket_event_routed_state_at(conn, event_key, route_status, &now, terminal_policy_grant)
}

fn force_ticket_event_routed_state_at(
    conn: &Connection,
    event_key: &str,
    route_status: &str,
    updated_at: &str,
    terminal_policy_grant: Option<&TerminalPolicyGrant>,
) -> Result<()> {
    let previous_route_status = current_ticket_event_route_status(conn, event_key)?;
    enforce_ticket_event_route_status_transition_with_grant(
        conn,
        event_key,
        &previous_route_status,
        route_status,
        "ctox-ticket-routing",
        "force_ticket_event_routed_state",
        None,
        terminal_policy_grant,
    )?;
    conn.execute(
        r#"
        INSERT INTO ticket_event_routing_state (
            event_key, route_status, lease_owner, leased_at, acked_at, updated_at
        ) VALUES (
            ?1,
            ?2,
            NULL,
            NULL,
            CASE WHEN ?2 IN ('handled', 'observed', 'duplicate', 'blocked') THEN ?3 ELSE NULL END,
            ?3
        )
        ON CONFLICT(event_key) DO UPDATE SET
            route_status=excluded.route_status,
            lease_owner=NULL,
            leased_at=NULL,
            acked_at=excluded.acked_at,
            updated_at=excluded.updated_at
        "#,
        params![event_key, route_status, updated_at],
    )?;
    Ok(())
}

fn current_ticket_event_route_status(conn: &Connection, event_key: &str) -> Result<String> {
    let status = conn
        .query_row(
            "SELECT route_status FROM ticket_event_routing_state WHERE event_key = ?1 LIMIT 1",
            params![event_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "pending".to_string());
    Ok(canonical_ticket_event_route_status(&status)?.to_string())
}

fn enforce_ticket_event_route_status_transition(
    conn: &Connection,
    event_key: &str,
    from_status: &str,
    to_status: &str,
    actor: &str,
    reason: &str,
    failure_class: Option<&str>,
) -> Result<()> {
    enforce_ticket_event_route_status_transition_with_grant(
        conn,
        event_key,
        from_status,
        to_status,
        actor,
        reason,
        failure_class,
        None,
    )
}

fn enforce_ticket_event_route_status_transition_with_grant(
    conn: &Connection,
    event_key: &str,
    from_status: &str,
    to_status: &str,
    actor: &str,
    reason: &str,
    failure_class: Option<&str>,
    terminal_policy_grant: Option<&TerminalPolicyGrant>,
) -> Result<()> {
    let from_status = canonical_ticket_event_route_status(from_status)?;
    let to_status = canonical_ticket_event_route_status(to_status)?;
    if from_status == to_status {
        return Ok(());
    }
    let from_core = ticket_event_route_core_state(from_status)?;
    let to_core = ticket_event_route_core_state(to_status)?;
    let entity_id = format!("ticket-event:{event_key}");
    if to_core == CoreState::Completed && ticket_event_has_terminal_success_proof(conn, &entity_id)?
    {
        return Ok(());
    }
    let mut metadata = BTreeMap::new();
    metadata.insert("from_route_status".to_string(), from_status.to_string());
    metadata.insert("to_route_status".to_string(), to_status.to_string());
    metadata.insert("reason".to_string(), reason.to_string());
    if to_core == CoreState::Failed {
        metadata.insert("failure_reason".to_string(), reason.trim().to_string());
        metadata.insert(
            "failure_class".to_string(),
            failure_class.unwrap_or("terminal").to_string(),
        );
    }
    if to_core == CoreState::Completed {
        if let Some(policy_proof) = terminal_policy_grant.map(TerminalPolicyGrant::proof) {
            metadata.insert("terminal_policy_proof".to_string(), policy_proof);
        }
    }
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id,
            lane: RuntimeLane::P2MissionDelivery,
            from_state: from_core,
            to_state: to_core,
            event: ticket_event_route_core_event(to_status)?,
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        },
    )?;
    Ok(())
}

fn ticket_event_has_terminal_success_proof(conn: &Connection, entity_id: &str) -> Result<bool> {
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
        params![entity_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn ticket_event_route_core_state(route_status: &str) -> Result<CoreState> {
    Ok(TicketEventRouteStatus::parse(route_status)?.core_state())
}

fn ticket_event_route_core_event(route_status: &str) -> Result<CoreEvent> {
    Ok(TicketEventRouteStatus::parse(route_status)?.core_event())
}

fn initial_route_status_for_inbound_event(
    conn: &Connection,
    system: &str,
    external_created_at: &str,
) -> Result<&'static str> {
    let control = load_ticket_source_control_from_conn(conn, system)?;
    if let Some(control) = control {
        if control.adoption_mode == "baseline_observe_only"
            && external_created_at.trim() <= control.baseline_external_created_cutoff.trim()
        {
            return Ok("observed");
        }
    }
    Ok("pending")
}

pub(crate) fn record_ticket_sync_run(
    root: &Path,
    system: &str,
    fetched_count: usize,
    stored_tickets: usize,
    stored_events: usize,
) -> Result<()> {
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    let run_id = format!("ticket-sync:{}:{}", system, stable_digest(&now));
    conn.execute(
        r#"
        INSERT INTO ticket_sync_runs (
            run_id, source_system, fetched_count, stored_ticket_count, stored_event_count,
            status, error_text, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'ok', '', ?6)
        "#,
        params![
            run_id,
            system,
            fetched_count as i64,
            stored_tickets as i64,
            stored_events as i64,
            now,
        ],
    )?;
    Ok(())
}

pub(crate) fn record_ticket_sync_failure(root: &Path, system: &str, error: &str) -> Result<()> {
    let conn = open_ticket_db(root)?;
    let now = now_iso_string();
    let run_id = format!("ticket-sync:{}:{}", system, stable_digest(&now));
    conn.execute(
        r#"
        INSERT INTO ticket_sync_runs (
            run_id, source_system, fetched_count, stored_ticket_count, stored_event_count,
            status, error_text, created_at
        ) VALUES (?1, ?2, 0, 0, 0, 'failed', ?3, ?4)
        "#,
        params![run_id, system, collapse_inline(error, 1000), now],
    )?;
    Ok(())
}

fn list_tickets(root: &Path, system: Option<&str>, limit: usize) -> Result<Vec<TicketItemView>> {
    let conn = open_ticket_db(root)?;
    list_tickets_on_conn(&conn, system, limit)
}

fn list_tickets_on_conn(
    conn: &Connection,
    system: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketItemView>> {
    let sql = if system.is_some() {
        r#"
        SELECT ticket_key, source_system, remote_ticket_id, title, body_text, remote_status,
               priority, requester, metadata_json, created_at, updated_at, last_synced_at
        FROM ticket_items
        WHERE source_system = ?1
        ORDER BY updated_at DESC
        LIMIT ?2
        "#
    } else {
        r#"
        SELECT ticket_key, source_system, remote_ticket_id, title, body_text, remote_status,
               priority, requester, metadata_json, created_at, updated_at, last_synced_at
        FROM ticket_items
        ORDER BY updated_at DESC
        LIMIT ?1
        "#
    };
    let mut statement = conn.prepare(sql)?;
    let rows = if let Some(system) = system {
        statement.query_map(params![system, limit as i64], map_ticket_row)?
    } else {
        statement.query_map(params![limit as i64], map_ticket_row)?
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_ticket(root: &Path, ticket_key: &str) -> Result<Option<TicketItemView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT ticket_key, source_system, remote_ticket_id, title, body_text, remote_status,
               priority, requester, metadata_json, created_at, updated_at, last_synced_at
        FROM ticket_items
        WHERE ticket_key = ?1
        LIMIT 1
        "#,
        params![ticket_key],
        map_ticket_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_ticket_event(root: &Path, event_key: &str) -> Result<Option<TicketEventView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT event_key, ticket_key, source_system, remote_event_id, direction, event_type,
               summary, body_text, metadata_json, external_created_at, observed_at
        FROM ticket_events
        WHERE event_key = ?1
        LIMIT 1
        "#,
        params![event_key],
        map_ticket_event_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn list_ticket_history(
    root: &Path,
    ticket_key: &str,
    limit: usize,
) -> Result<Vec<TicketEventView>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT event_key, ticket_key, source_system, remote_event_id, direction, event_type,
               summary, body_text, metadata_json, external_created_at, observed_at
        FROM ticket_events
        WHERE ticket_key = ?1
        ORDER BY external_created_at DESC, observed_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![ticket_key, limit as i64], map_ticket_event_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

pub(crate) fn business_os_ticket_projection_documents(
    root: &Path,
    limit: usize,
) -> Result<BTreeMap<String, Vec<Value>>> {
    let conn = open_ticket_db(root)?;
    let mut documents = BTreeMap::new();

    documents.insert(
        "ctox_ticket_items".to_string(),
        list_tickets_on_conn(&conn, None, limit)?
            .into_iter()
            .map(|item| {
                ticket_projection_document(item, |value| {
                    (
                        value
                            .get("ticket_key")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("updated_at")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    documents.insert(
        "ctox_ticket_events".to_string(),
        list_recent_ticket_events_for_business_os(&conn, limit)?
            .into_iter()
            .map(|event| {
                ticket_projection_document(event, |value| {
                    (
                        value
                            .get("event_key")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("observed_at")
                            .and_then(Value::as_str)
                            .or_else(|| value.get("external_created_at").and_then(Value::as_str))
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    documents.insert(
        "ctox_ticket_event_routing_state".to_string(),
        list_ticket_event_routing_for_business_os(&conn, limit)?,
    );

    documents.insert(
        "ctox_ticket_cases".to_string(),
        list_cases_on_conn(&conn, None, limit)?
            .into_iter()
            .map(|case| {
                ticket_projection_document(case, |value| {
                    (
                        value
                            .get("case_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("updated_at")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    documents.insert(
        "ctox_ticket_self_work_items".to_string(),
        list_ticket_self_work_items_on_conn(&conn, None, None, limit)?
            .into_iter()
            .map(|item| {
                ticket_projection_document(item, |value| {
                    (
                        value
                            .get("work_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("updated_at")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    documents.insert(
        "ctox_ticket_self_work_notes".to_string(),
        list_ticket_self_work_notes_for_business_os(&conn, limit)?,
    );
    documents.insert(
        "ctox_ticket_label_assignments".to_string(),
        list_ticket_label_assignments_for_business_os(&conn, limit)?,
    );
    documents.insert(
        "ctox_ticket_control_bundles".to_string(),
        list_control_bundles_on_conn(&conn)?
            .into_iter()
            .map(|bundle| {
                ticket_projection_document(bundle, |value| {
                    (
                        value
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        value
                            .get("updated_at")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );
    documents.insert(
        "ctox_ticket_approvals".to_string(),
        list_ticket_approvals_for_business_os(&conn, limit)?,
    );
    documents.insert(
        "ctox_ticket_verifications".to_string(),
        list_ticket_verifications_for_business_os(&conn, limit)?,
    );
    documents.insert(
        "ctox_ticket_writebacks".to_string(),
        list_ticket_writebacks_for_business_os(&conn, limit)?,
    );
    documents.insert(
        "ctox_ticket_clarification_requests".to_string(),
        list_ticket_clarification_requests_for_business_os(&conn, limit)?,
    );

    Ok(documents)
}

fn ticket_projection_document<T, F>(value: T, id_and_updated_at: F) -> Result<Value>
where
    T: Serialize,
    F: FnOnce(&Value) -> (String, String),
{
    let mut document = serde_json::to_value(value)?;
    let (id, updated_at) = id_and_updated_at(&document);
    let updated_at_ms = iso_to_epoch_ms(&updated_at);
    if let Some(object) = document.as_object_mut() {
        object.insert("id".to_string(), Value::String(id));
        object.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
        object.insert("is_deleted".to_string(), Value::Bool(false));
    }
    Ok(document)
}

fn iso_to_epoch_ms(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|parsed| parsed.timestamp_millis())
        .unwrap_or(0)
}

fn list_recent_ticket_events_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<TicketEventView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT event_key, ticket_key, source_system, remote_event_id, direction, event_type,
               summary, body_text, metadata_json, external_created_at, observed_at
        FROM ticket_events
        ORDER BY external_created_at DESC, observed_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], map_ticket_event_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_event_routing_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT event_key, route_status, lease_owner, leased_at, acked_at, updated_at,
               lease_expires_at, failure_class, failure_attempt_count,
               retry_not_before, failure_proof, hold_reason,
               wait_entity_type, wait_entity_id
        FROM ticket_event_routing_state
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let event_key: String = row.get(0)?;
        let updated_at: String = row.get(5)?;
        Ok(json!({
            "id": event_key,
            "event_key": event_key,
            "route_status": row.get::<_, String>(1)?,
            "lease_owner": row.get::<_, Option<String>>(2)?,
            "leased_at": row.get::<_, Option<String>>(3)?,
            "acked_at": row.get::<_, Option<String>>(4)?,
            "lease_expires_at": row.get::<_, Option<String>>(6)?,
            "failure_class": row.get::<_, Option<String>>(7)?,
            "failure_attempt_count": row.get::<_, i64>(8)?,
            "retry_not_before": row.get::<_, Option<String>>(9)?,
            "failure_proof": row.get::<_, Option<String>>(10)?,
            "hold_reason": row.get::<_, Option<String>>(11)?,
            "wait_entity_type": row.get::<_, Option<String>>(12)?,
            "wait_entity_id": row.get::<_, Option<String>>(13)?,
            "updated_at": updated_at,
            "updated_at_ms": iso_to_epoch_ms(&updated_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_self_work_notes_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        FROM ticket_self_work_notes
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let note_id: String = row.get(0)?;
        let created_at: String = row.get(6)?;
        Ok(json!({
            "id": note_id,
            "note_id": note_id,
            "work_id": row.get::<_, String>(1)?,
            "body_text": row.get::<_, String>(2)?,
            "visibility": row.get::<_, String>(3)?,
            "authored_by": row.get::<_, String>(4)?,
            "remote_event_id": row.get::<_, Option<String>>(5)?,
            "created_at": created_at,
            "updated_at_ms": iso_to_epoch_ms(&created_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_label_assignments_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT ticket_key, label, assigned_by, rationale, evidence_json, assigned_at, updated_at
        FROM ticket_label_assignments
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let ticket_key: String = row.get(0)?;
        let updated_at: String = row.get(6)?;
        let evidence_raw: String = row.get(4)?;
        Ok(json!({
            "id": ticket_key,
            "ticket_key": ticket_key,
            "label": row.get::<_, String>(1)?,
            "assigned_by": row.get::<_, String>(2)?,
            "rationale": row.get::<_, Option<String>>(3)?,
            "evidence": parse_json_or_empty(&evidence_raw),
            "assigned_at": row.get::<_, String>(5)?,
            "updated_at": updated_at,
            "updated_at_ms": iso_to_epoch_ms(&updated_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_approvals_for_business_os(conn: &Connection, limit: usize) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT approval_id, case_id, status, decided_by, rationale, created_at
        FROM ticket_approvals
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let approval_id: String = row.get(0)?;
        let created_at: String = row.get(5)?;
        Ok(json!({
            "id": approval_id,
            "approval_id": approval_id,
            "case_id": row.get::<_, String>(1)?,
            "status": row.get::<_, String>(2)?,
            "decided_by": row.get::<_, String>(3)?,
            "rationale": row.get::<_, Option<String>>(4)?,
            "created_at": created_at,
            "updated_at_ms": iso_to_epoch_ms(&created_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_verifications_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT verification_id, case_id, status, summary, created_at
        FROM ticket_verifications
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let verification_id: String = row.get(0)?;
        let created_at: String = row.get(4)?;
        Ok(json!({
            "id": verification_id,
            "verification_id": verification_id,
            "case_id": row.get::<_, String>(1)?,
            "status": row.get::<_, String>(2)?,
            "summary": row.get::<_, Option<String>>(3)?,
            "created_at": created_at,
            "updated_at_ms": iso_to_epoch_ms(&created_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_writebacks_for_business_os(conn: &Connection, limit: usize) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT writeback_id, case_id, ticket_key, operation, payload_json, status, created_at
        FROM ticket_writebacks
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let writeback_id: String = row.get(0)?;
        let created_at: String = row.get(6)?;
        let payload_raw: String = row.get(4)?;
        Ok(json!({
            "id": writeback_id,
            "writeback_id": writeback_id,
            "case_id": row.get::<_, String>(1)?,
            "ticket_key": row.get::<_, String>(2)?,
            "operation": row.get::<_, String>(3)?,
            "payload": parse_json_or_empty(&payload_raw),
            "status": row.get::<_, String>(5)?,
            "created_at": created_at,
            "updated_at_ms": iso_to_epoch_ms(&created_at),
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_clarification_requests_for_business_os(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<Value>> {
    let mut statement = conn.prepare(
        r#"
        SELECT clarification_id, ticket_key, case_id, work_id, target_type, target_channel,
               question, missing_inputs_json, unblock_criteria, status, outbound_message_key,
               inbound_response_key, inbound_response_body, resume_state, created_by,
               created_at, updated_at, sent_at, resolved_at, metadata_json
        FROM ticket_clarification_requests
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        let item = map_ticket_clarification_row(row)?;
        let updated_at_ms = iso_to_epoch_ms(&item.updated_at);
        Ok(json!({
            "id": item.clarification_id.clone(),
            "clarification_id": item.clarification_id,
            "ticket_key": item.ticket_key,
            "case_id": item.case_id,
            "work_id": item.work_id,
            "target_type": item.target_type,
            "target_channel": item.target_channel,
            "question": item.question,
            "missing_inputs": item.missing_inputs,
            "unblock_criteria": item.unblock_criteria,
            "status": item.status,
            "outbound_message_key": item.outbound_message_key,
            "inbound_response_key": item.inbound_response_key,
            "inbound_response_body": item.inbound_response_body,
            "resume_state": item.resume_state,
            "created_by": item.created_by,
            "created_at": item.created_at,
            "updated_at": item.updated_at,
            "updated_at_ms": updated_at_ms,
            "sent_at": item.sent_at,
            "resolved_at": item.resolved_at,
            "metadata": item.metadata,
            "is_deleted": false
        }))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn parse_json_or_empty(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!({}))
}

fn parse_json_string_array_lossy(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn set_ticket_label(
    root: &Path,
    ticket_key: &str,
    label: &str,
    assigned_by: &str,
    rationale: Option<&str>,
    evidence: Value,
) -> Result<TicketLabelAssignmentView> {
    let mut conn = open_ticket_db(root)?;
    if load_ticket(root, ticket_key)?.is_none() {
        anyhow::bail!("ticket not found: {ticket_key}");
    }
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_label_assignments (
            ticket_key, label, assigned_by, rationale, evidence_json, assigned_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
        ON CONFLICT(ticket_key) DO UPDATE SET
            label=excluded.label,
            assigned_by=excluded.assigned_by,
            rationale=excluded.rationale,
            evidence_json=excluded.evidence_json,
            updated_at=excluded.updated_at
        "#,
        params![
            ticket_key,
            label.trim(),
            assigned_by.trim(),
            rationale.map(str::trim),
            serde_json::to_string(&evidence)?,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key,
            case_id: None,
            actor_type: "labeler",
            action_type: "ticket_label_assignment",
            label: Some(label.trim()),
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "assigned_by": assigned_by.trim(),
                "rationale": rationale.map(str::trim),
                "evidence": evidence,
            }),
        },
    )?;
    load_ticket_label_assignment(root, ticket_key)?
        .context("failed to load ticket label assignment after upsert")
}

fn load_ticket_label_assignment(
    root: &Path,
    ticket_key: &str,
) -> Result<Option<TicketLabelAssignmentView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT ticket_key, label, assigned_by, rationale, evidence_json, assigned_at, updated_at
        FROM ticket_label_assignments
        WHERE ticket_key = ?1
        LIMIT 1
        "#,
        params![ticket_key],
        |row| {
            Ok(TicketLabelAssignmentView {
                ticket_key: row.get(0)?,
                label: row.get(1)?,
                assigned_by: row.get(2)?,
                rationale: row.get(3)?,
                evidence: parse_json_column(row.get::<_, String>(4)?),
                assigned_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn put_control_bundle(root: &Path, input: ControlBundleInput) -> Result<ControlBundleView> {
    let mut conn = open_ticket_db(root)?;
    let now = now_iso_string();
    let current_version = conn
        .query_row(
            "SELECT bundle_version FROM ticket_control_bundles WHERE label = ?1 LIMIT 1",
            params![input.label],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    let next_version = current_version + 1;
    conn.execute(
        r#"
        INSERT INTO ticket_control_bundles (
            label, bundle_version, runbook_id, runbook_version, policy_id, policy_version,
            approval_mode, autonomy_level, verification_profile_id, writeback_profile_id,
            support_mode, default_risk_level, execution_actions_json, notes, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(label) DO UPDATE SET
            bundle_version=excluded.bundle_version,
            runbook_id=excluded.runbook_id,
            runbook_version=excluded.runbook_version,
            policy_id=excluded.policy_id,
            policy_version=excluded.policy_version,
            approval_mode=excluded.approval_mode,
            autonomy_level=excluded.autonomy_level,
            verification_profile_id=excluded.verification_profile_id,
            writeback_profile_id=excluded.writeback_profile_id,
            support_mode=excluded.support_mode,
            default_risk_level=excluded.default_risk_level,
            execution_actions_json=excluded.execution_actions_json,
            notes=excluded.notes,
            updated_at=excluded.updated_at
        "#,
        params![
            input.label,
            next_version,
            input.runbook_id,
            input.runbook_version,
            input.policy_id,
            input.policy_version,
            input.approval_mode,
            input.autonomy_level,
            input.verification_profile_id,
            input.writeback_profile_id,
            input.support_mode,
            input.default_risk_level,
            serde_json::to_string(&input.execution_actions)?,
            input.notes,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: "*control-bundle*",
            case_id: None,
            actor_type: "bundle_manager",
            action_type: "control_bundle_upsert",
            label: Some(&input.label),
            bundle_label: Some(&input.label),
            bundle_version: Some(next_version),
            details: json!({
                "runbook_id": input.runbook_id,
                "runbook_version": input.runbook_version,
                "policy_id": input.policy_id,
                "policy_version": input.policy_version,
                "approval_mode": input.approval_mode,
                "autonomy_level": input.autonomy_level,
                "verification_profile_id": input.verification_profile_id,
                "writeback_profile_id": input.writeback_profile_id,
                "support_mode": input.support_mode,
                "default_risk_level": input.default_risk_level,
                "execution_actions": input.execution_actions,
                "notes": input.notes,
            }),
        },
    )?;
    load_control_bundle(root, &input.label)?.context("failed to load control bundle after upsert")
}

fn list_control_bundles(root: &Path) -> Result<Vec<ControlBundleView>> {
    let conn = open_ticket_db(root)?;
    list_control_bundles_on_conn(&conn)
}

fn list_control_bundles_on_conn(conn: &Connection) -> Result<Vec<ControlBundleView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT label, bundle_version, runbook_id, runbook_version, policy_id, policy_version,
               approval_mode, autonomy_level, verification_profile_id, writeback_profile_id,
               support_mode, default_risk_level, execution_actions_json, notes, updated_at
        FROM ticket_control_bundles
        ORDER BY label ASC
        "#,
    )?;
    let rows = statement.query_map([], map_control_bundle_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_control_bundle(root: &Path, label: &str) -> Result<Option<ControlBundleView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT label, bundle_version, runbook_id, runbook_version, policy_id, policy_version,
               approval_mode, autonomy_level, verification_profile_id, writeback_profile_id,
               support_mode, default_risk_level, execution_actions_json, notes, updated_at
        FROM ticket_control_bundles
        WHERE label = ?1
        LIMIT 1
        "#,
        params![label],
        map_control_bundle_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn put_autonomy_grant(root: &Path, input: AutonomyGrantInput) -> Result<AutonomyGrantView> {
    let mut conn = open_ticket_db(root)?;
    let bundle = load_control_bundle(root, &input.label)?
        .context("cannot grant autonomy without an active control bundle")?;
    let bundle_version = input.bundle_version.unwrap_or(bundle.bundle_version);
    if bundle_version != bundle.bundle_version {
        anyhow::bail!(
            "bundle version mismatch for label {}; current active version is {}",
            input.label,
            bundle.bundle_version
        );
    }
    if let Some(candidate_id) = input.source_candidate_id.as_deref() {
        let candidate = load_learning_candidate(root, candidate_id)?
            .context("learning candidate not found for autonomy grant")?;
        if candidate.status != "approved" {
            anyhow::bail!(
                "learning candidate {} is not approved; current status is {}",
                candidate_id,
                candidate.status
            );
        }
        if candidate.label != input.label || candidate.bundle_version != bundle_version {
            anyhow::bail!(
                "learning candidate {} does not match label {} bundle version {}",
                candidate_id,
                input.label,
                bundle_version
            );
        }
    }

    let approval_mode = ControlApprovalMode::parse(&input.approval_mode)?;
    let autonomy_level = canonical_autonomy_level(&input.autonomy_level)?;
    let now = now_iso_string();
    let grant_version = conn
        .query_row(
            "SELECT grant_version FROM ticket_autonomy_grants WHERE label = ?1 LIMIT 1",
            params![input.label],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0)
        + 1;
    conn.execute(
        r#"
        INSERT INTO ticket_autonomy_grants (
            label, grant_version, bundle_version, approval_mode, autonomy_level,
            approved_by, source_candidate_id, rationale, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(label) DO UPDATE SET
            grant_version=excluded.grant_version,
            bundle_version=excluded.bundle_version,
            approval_mode=excluded.approval_mode,
            autonomy_level=excluded.autonomy_level,
            approved_by=excluded.approved_by,
            source_candidate_id=excluded.source_candidate_id,
            rationale=excluded.rationale,
            updated_at=excluded.updated_at
        "#,
        params![
            input.label,
            grant_version,
            bundle_version,
            approval_mode.as_str(),
            autonomy_level,
            input.approved_by.trim(),
            input.source_candidate_id,
            input.rationale.as_deref().map(str::trim),
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: "*autonomy-grant*",
            case_id: None,
            actor_type: "approver",
            action_type: "autonomy_grant_change",
            label: Some(&input.label),
            bundle_label: Some(&input.label),
            bundle_version: Some(bundle_version),
            details: json!({
                "grant_version": grant_version,
                "approval_mode": approval_mode.as_str(),
                "autonomy_level": autonomy_level,
                "approved_by": input.approved_by.trim(),
                "source_candidate_id": input.source_candidate_id,
                "rationale": input.rationale.as_deref().map(str::trim),
            }),
        },
    )?;
    load_autonomy_grant(root, &input.label)?.context("failed to load autonomy grant after upsert")
}

fn list_autonomy_grants(root: &Path) -> Result<Vec<AutonomyGrantView>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT label, grant_version, bundle_version, approval_mode, autonomy_level,
               approved_by, source_candidate_id, rationale, updated_at
        FROM ticket_autonomy_grants
        ORDER BY label ASC
        "#,
    )?;
    let rows = statement.query_map([], map_autonomy_grant_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_autonomy_grant(root: &Path, label: &str) -> Result<Option<AutonomyGrantView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT label, grant_version, bundle_version, approval_mode, autonomy_level,
               approved_by, source_candidate_id, rationale, updated_at
        FROM ticket_autonomy_grants
        WHERE label = ?1
        LIMIT 1
        "#,
        params![label],
        map_autonomy_grant_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_active_autonomy_grant(
    root: &Path,
    label: &str,
    bundle_version: i64,
) -> Result<Option<AutonomyGrantView>> {
    Ok(load_autonomy_grant(root, label)?.filter(|grant| grant.bundle_version == bundle_version))
}

fn resolve_effective_control(
    bundle: &ControlBundleView,
    grant: Option<AutonomyGrantView>,
) -> Result<EffectiveControlResolution> {
    let requested_approval_mode = ControlApprovalMode::parse(&bundle.approval_mode)?;
    let requested_autonomy_level = canonical_autonomy_level(&bundle.autonomy_level)?;
    let allowed_approval_mode = grant
        .as_ref()
        .map(|item| ControlApprovalMode::parse(&item.approval_mode))
        .transpose()?
        .unwrap_or(ControlApprovalMode::HumanApprovalRequired);
    let allowed_autonomy_level = grant
        .as_ref()
        .map(|item| canonical_autonomy_level(&item.autonomy_level))
        .transpose()?
        .unwrap_or(DEFAULT_AUTONOMY_LEVEL);

    let approval_mode =
        more_restrictive_approval_mode(requested_approval_mode, allowed_approval_mode);
    let autonomy_level =
        more_restrictive_autonomy_level(requested_autonomy_level, allowed_autonomy_level)
            .to_string();
    let mut missing_approvals = approval_mode.missing_approvals();
    if grant.is_none()
        && (approval_mode.as_str() != bundle.approval_mode.as_str()
            || autonomy_level != bundle.autonomy_level)
    {
        missing_approvals.push(
            "no active autonomy grant for the current label bundle; using safe default controls"
                .to_string(),
        );
    }

    Ok(EffectiveControlResolution {
        approval_mode: approval_mode.as_str().to_string(),
        autonomy_level,
        missing_approvals,
        grant,
    })
}

fn record_audit(conn: &Connection, request: AuditRequest<'_>) -> Result<()> {
    let now = now_iso_string();
    let audit_id = format!(
        "audit:{}:{}:{}",
        request.actor_type,
        request.action_type,
        stable_digest(&(request.ticket_key.to_string() + now.as_str()))
    );
    conn.execute(
        r#"
        INSERT INTO ticket_audit_log (
            audit_id, ticket_key, case_id, actor_type, action_type, label, bundle_label,
            bundle_version, details_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            audit_id,
            request.ticket_key,
            request.case_id,
            request.actor_type,
            request.action_type,
            request.label,
            request.bundle_label,
            request.bundle_version,
            serde_json::to_string(&request.details)?,
            now,
        ],
    )?;
    Ok(())
}

fn open_ticket_db(root: &Path) -> Result<Connection> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ticket db parent {}", parent.display()))?;
    }
    #[cfg(test)]
    record_ticket_db_open_for_tests(&path);
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open ticket db {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for tickets")?;
    ensure_schema_once(&path, &conn)?;
    Ok(conn)
}

fn with_reconcile_ticket_db<T>(root: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let path = resolve_db_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ticket db parent {}", parent.display()))?;
    }
    TICKET_RECONCILE_DB.with(|cell| {
        let mut cached = cell.borrow_mut();
        let key = ticket_schema_cache_key(&path);
        let needs_open = cached
            .as_ref()
            .map(|entry| entry.key != key)
            .unwrap_or(true);
        if needs_open {
            let conn = Connection::open(&path)
                .with_context(|| format!("failed to open ticket db {}", path.display()))?;
            conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
                .context("failed to configure SQLite busy_timeout for tickets")?;
            ensure_schema_once(&path, &conn)?;
            let key = ticket_schema_cache_key(&path);
            *cached = Some(CachedTicketConnection { key, conn });
        }
        let conn = &cached
            .as_ref()
            .expect("ticket reconcile db initialized")
            .conn;
        f(conn)
    })
}

fn ensure_schema_once(path: &Path, conn: &Connection) -> Result<()> {
    let key = ticket_schema_cache_key(path);
    let ready = TICKET_SCHEMA_READY.get_or_init(|| Mutex::new(HashSet::new()));
    let mut ready = ready
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if ready.contains(&key) {
        return Ok(());
    }
    ensure_schema(conn)?;
    ready.insert(key);
    Ok(())
}

#[cfg(unix)]
fn ticket_schema_cache_key(path: &Path) -> TicketSchemaCacheKey {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| absolute_ticket_db_path(path));
    let metadata = std::fs::metadata(&canonical)
        .or_else(|_| std::fs::metadata(path))
        .ok();
    let (device, inode) = metadata
        .map(|metadata| (metadata.dev(), metadata.ino()))
        .unwrap_or((0, 0));
    (canonical, device, inode)
}

#[cfg(not(unix))]
fn ticket_schema_cache_key(path: &Path) -> TicketSchemaCacheKey {
    std::fs::canonicalize(path).unwrap_or_else(|_| absolute_ticket_db_path(path))
}

fn ticket_self_work_list_cache_key(
    path: &Path,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> TicketSelfWorkListCacheKey {
    TicketSelfWorkListCacheKey {
        database: ticket_schema_cache_key(path),
        system: system.map(ToOwned::to_owned),
        state: state.map(ToOwned::to_owned),
        limit,
    }
}

fn ticket_workflow_materialize_cache_key(
    path: &Path,
    workflow_id: Option<&str>,
    limit: usize,
) -> TicketWorkflowMaterializeCacheKey {
    TicketWorkflowMaterializeCacheKey {
        database: ticket_schema_cache_key(path),
        workflow_id: workflow_id.map(ToOwned::to_owned),
        limit,
    }
}

fn cached_ticket_self_work_list(
    key: &TicketSelfWorkListCacheKey,
    stamp: &TicketSelfWorkListCacheStamp,
) -> Option<Vec<TicketSelfWorkItemView>> {
    let cache = TICKET_SELF_WORK_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .get(key)
        .filter(|entry| &entry.stamp == stamp)
        .map(|entry| entry.items.clone())
}

fn cached_ticket_workflow_materialize_result(
    key: &TicketWorkflowMaterializeCacheKey,
    stamp: &TicketStoreChangeStamp,
) -> Option<TicketWorkflowMaterializeResult> {
    let cache = TICKET_WORKFLOW_MATERIALIZE_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .get(key)
        .filter(|entry| &entry.stamp == stamp)
        .map(|entry| entry.result.clone())
}

fn store_ticket_self_work_list_cache(
    key: TicketSelfWorkListCacheKey,
    stamp: TicketSelfWorkListCacheStamp,
    items: Vec<TicketSelfWorkItemView>,
) {
    let cache = TICKET_SELF_WORK_LIST_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= TICKET_SELF_WORK_LIST_CACHE_MAX_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(key, TicketSelfWorkListCacheEntry { stamp, items });
}

fn store_ticket_workflow_materialize_cache(
    key: TicketWorkflowMaterializeCacheKey,
    stamp: TicketStoreChangeStamp,
    result: TicketWorkflowMaterializeResult,
) {
    let cache = TICKET_WORKFLOW_MATERIALIZE_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= TICKET_WORKFLOW_MATERIALIZE_CACHE_MAX_ENTRIES && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(key, TicketWorkflowMaterializeCacheEntry { stamp, result });
}

fn clear_ticket_self_work_list_cache() {
    if let Some(cache) = TICKET_SELF_WORK_LIST_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
    if let Some(cache) = TICKET_WORKFLOW_MATERIALIZE_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

fn absolute_ticket_db_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn ticket_self_work_list_cache_stamp(path: &Path) -> TicketSelfWorkListCacheStamp {
    ticket_store_change_stamp_for_path(path)
}

pub(crate) fn ticket_store_change_stamp(root: &Path) -> TicketStoreChangeStamp {
    ticket_store_change_stamp_for_path(&resolve_db_path(root))
}

pub(crate) fn ticket_case_status_stamp(root: &Path) -> Result<TicketCaseStatusStamp> {
    let path = resolve_db_path(root);
    if !path.exists() {
        return Ok(empty_ticket_case_status_stamp(false, false));
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open ticket db {} for status stamp",
            path.display()
        )
    })?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure SQLite busy_timeout for ticket status stamp")?;
    let table_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'ticket_cases' LIMIT 1",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !table_exists {
        return Ok(empty_ticket_case_status_stamp(true, false));
    }
    let (open_case_count, latest_open_case_updated_at) = conn.query_row(
        r#"
        SELECT COUNT(*), COALESCE(MAX(updated_at), '')
        FROM ticket_cases
        WHERE state <> 'closed'
        "#,
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    Ok(TicketCaseStatusStamp {
        database_exists: true,
        table_exists: true,
        open_case_count: open_case_count.max(0) as usize,
        latest_open_case_updated_at,
    })
}

fn empty_ticket_case_status_stamp(
    database_exists: bool,
    table_exists: bool,
) -> TicketCaseStatusStamp {
    TicketCaseStatusStamp {
        database_exists,
        table_exists,
        open_case_count: 0,
        latest_open_case_updated_at: String::new(),
    }
}

fn ticket_store_change_stamp_for_path(path: &Path) -> TicketStoreChangeStamp {
    TicketStoreChangeStamp {
        main: ticket_file_change_stamp(path),
        wal: ticket_file_change_stamp(&sqlite_sidecar_path(path, "-wal")),
        journal: ticket_file_change_stamp(&sqlite_sidecar_path(path, "-journal")),
    }
}

fn ticket_file_change_stamp(path: &Path) -> TicketFileChangeStamp {
    let Ok(metadata) = fs::metadata(path) else {
        return (0, 0);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (metadata.len(), modified_at)
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", path.display(), suffix))
}

#[cfg(test)]
fn record_ticket_self_work_list_cache_miss_for_tests(key: &TicketSelfWorkListCacheKey) {
    let counts =
        TICKET_SELF_WORK_LIST_CACHE_MISS_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
fn record_ticket_workflow_materialize_cache_miss_for_tests(
    key: &TicketWorkflowMaterializeCacheKey,
) {
    let counts =
        TICKET_WORKFLOW_MATERIALIZE_CACHE_MISS_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(key.clone()).or_insert(0) += 1;
}

#[cfg(test)]
fn record_ticket_self_work_assignment_batch_hydration_for_tests() {
    let calls = TICKET_SELF_WORK_ASSIGNMENT_BATCH_HYDRATION_CALLS.get_or_init(|| Mutex::new(0));
    let mut calls = calls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *calls += 1;
}

#[cfg(test)]
fn ticket_self_work_assignment_batch_hydration_call_count_for_tests() -> usize {
    let Some(calls) = TICKET_SELF_WORK_ASSIGNMENT_BATCH_HYDRATION_CALLS.get() else {
        return 0;
    };
    let calls = calls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *calls
}

#[cfg(test)]
fn record_ticket_db_open_for_tests(path: &Path) {
    let counts = TICKET_DB_OPEN_CALL_COUNTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(path.to_path_buf()).or_insert(0) += 1;
}

#[cfg(test)]
pub(crate) fn reset_ticket_db_open_call_count_for_tests(path: &Path) {
    if let Some(counts) = TICKET_DB_OPEN_CALL_COUNTS.get() {
        counts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(path);
    }
}

#[cfg(test)]
pub(crate) fn ticket_db_open_call_count_for_tests(path: &Path) -> usize {
    let Some(counts) = TICKET_DB_OPEN_CALL_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(path).copied().unwrap_or(0)
}

#[cfg(test)]
fn ticket_self_work_list_cache_miss_count_for_tests(
    path: &Path,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> usize {
    let key = ticket_self_work_list_cache_key(path, system, state, limit);
    let Some(counts) = TICKET_SELF_WORK_LIST_CACHE_MISS_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

#[cfg(test)]
fn ticket_workflow_materialize_cache_miss_count_for_tests(
    path: &Path,
    workflow_id: Option<&str>,
    limit: usize,
) -> usize {
    let key = ticket_workflow_materialize_cache_key(path, workflow_id, limit);
    let Some(counts) = TICKET_WORKFLOW_MATERIALIZE_CACHE_MISS_COUNTS.get() else {
        return 0;
    };
    let counts = counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.get(&key).copied().unwrap_or(0)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    let busy_timeout_ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout={busy_timeout_ms};

        CREATE TABLE IF NOT EXISTS ticket_items (
            ticket_key TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            remote_ticket_id TEXT NOT NULL,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL,
            remote_status TEXT NOT NULL,
            priority TEXT,
            requester TEXT,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_synced_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_events (
            event_key TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL,
            source_system TEXT NOT NULL,
            remote_event_id TEXT NOT NULL,
            direction TEXT NOT NULL,
            event_type TEXT NOT NULL,
            summary TEXT NOT NULL,
            body_text TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            external_created_at TEXT NOT NULL,
            observed_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_events_ticket_time
            ON ticket_events(ticket_key, external_created_at DESC, observed_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_event_routing_state (
            event_key TEXT PRIMARY KEY,
            route_status TEXT NOT NULL,
            lease_owner TEXT,
            leased_at TEXT,
            lease_expires_at TEXT,
            failure_class TEXT,
            failure_attempt_count INTEGER NOT NULL DEFAULT 0,
            retry_not_before TEXT,
            failure_proof TEXT,
            hold_reason TEXT,
            wait_entity_type TEXT,
            wait_entity_id TEXT,
            acked_at TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_event_routing_status_owner
            ON ticket_event_routing_state(route_status, lease_owner, leased_at, updated_at);

        CREATE TABLE IF NOT EXISTS ticket_outbound_event_marks (
            source_system TEXT NOT NULL,
            remote_event_id TEXT NOT NULL,
            marked_at TEXT NOT NULL,
            PRIMARY KEY (source_system, remote_event_id)
        );

        CREATE TABLE IF NOT EXISTS ticket_source_controls (
            source_system TEXT PRIMARY KEY,
            adoption_mode TEXT NOT NULL,
            baseline_external_created_cutoff TEXT NOT NULL,
            attached_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_source_skill_bindings (
            source_system TEXT PRIMARY KEY,
            skill_name TEXT NOT NULL,
            archetype TEXT NOT NULL,
            status TEXT NOT NULL,
            origin TEXT NOT NULL,
            artifact_path TEXT,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_main_skills (
            main_skill_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            primary_channel TEXT NOT NULL,
            entry_action TEXT NOT NULL,
            resolver_contract_json TEXT NOT NULL,
            execution_contract_json TEXT NOT NULL,
            resolve_flow_json TEXT NOT NULL,
            writeback_flow_json TEXT NOT NULL,
            linked_skillbooks_json TEXT NOT NULL,
            linked_runbooks_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_skillbooks (
            skillbook_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            version TEXT NOT NULL,
            status TEXT NOT NULL,
            summary TEXT NOT NULL,
            mission TEXT NOT NULL,
            non_negotiable_rules_json TEXT NOT NULL,
            runtime_policy TEXT NOT NULL,
            answer_contract TEXT NOT NULL,
            workflow_backbone_json TEXT NOT NULL,
            routing_taxonomy_json TEXT NOT NULL,
            linked_runbooks_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_runbooks (
            runbook_id TEXT PRIMARY KEY,
            skillbook_id TEXT NOT NULL,
            title TEXT NOT NULL,
            version TEXT NOT NULL,
            status TEXT NOT NULL,
            summary TEXT NOT NULL,
            problem_domain TEXT NOT NULL,
            item_labels_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_runbook_items (
            item_id TEXT PRIMARY KEY,
            runbook_id TEXT NOT NULL,
            skillbook_id TEXT NOT NULL,
            label TEXT NOT NULL,
            title TEXT NOT NULL,
            problem_class TEXT NOT NULL,
            chunk_text TEXT NOT NULL,
            structured_json TEXT NOT NULL,
            status TEXT NOT NULL,
            version TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_runbook_items_lookup
            ON knowledge_runbook_items(runbook_id, label, updated_at DESC);

        CREATE TABLE IF NOT EXISTS knowledge_resources (
            resource_id TEXT PRIMARY KEY,
            skillbook_id TEXT NOT NULL,
            title TEXT NOT NULL,
            kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            role TEXT NOT NULL,
            canonical_url TEXT NOT NULL,
            snapshot_hash TEXT NOT NULL,
            evidence_eligible INTEGER NOT NULL DEFAULT 0,
            linked_runbook_items_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_resources_skillbook
            ON knowledge_resources(skillbook_id, updated_at DESC);

        CREATE TABLE IF NOT EXISTS knowledge_data_tables (
            table_id      TEXT PRIMARY KEY,
            domain        TEXT NOT NULL,
            table_key     TEXT NOT NULL,
            source_system TEXT NOT NULL,
            title         TEXT NOT NULL,
            description   TEXT NOT NULL,
            parquet_path  TEXT NOT NULL,
            schema_hash   TEXT NOT NULL DEFAULT '',
            row_count     INTEGER NOT NULL DEFAULT 0,
            bytes         INTEGER NOT NULL DEFAULT 0,
            tags_json     TEXT NOT NULL DEFAULT '{{}}',
            archived_at   TEXT,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            UNIQUE(source_system, domain, table_key)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_data_tables_domain
            ON knowledge_data_tables(domain, updated_at DESC);

        CREATE INDEX IF NOT EXISTS idx_knowledge_data_tables_source
            ON knowledge_data_tables(source_system, updated_at DESC);

        CREATE TABLE IF NOT EXISTS knowledge_embeddings (
            item_id TEXT NOT NULL,
            embedding_model TEXT NOT NULL,
            embedding_json TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (item_id, embedding_model)
        );

        CREATE TABLE IF NOT EXISTS ticket_knowledge_entries (
            entry_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            domain TEXT NOT NULL,
            knowledge_key TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            status TEXT NOT NULL,
            content_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(source_system, domain, knowledge_key)
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_knowledge_scope
            ON ticket_knowledge_entries(source_system, domain, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_knowledge_loads (
            load_id TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL,
            source_system TEXT NOT NULL,
            domains_json TEXT NOT NULL,
            loaded_entries_json TEXT NOT NULL,
            gap_domains_json TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_knowledge_loads_ticket_time
            ON ticket_knowledge_loads(ticket_key, created_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_self_work_items (
            work_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL,
            state TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            remote_ticket_id TEXT,
            remote_locator TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_self_work_scope
            ON ticket_self_work_items(source_system, state, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_self_work_assignments (
            assignment_id TEXT PRIMARY KEY,
            work_id TEXT NOT NULL,
            assigned_to TEXT NOT NULL,
            assigned_by TEXT NOT NULL,
            rationale TEXT,
            remote_event_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_self_work_assignments_work_time
            ON ticket_self_work_assignments(work_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_self_work_notes (
            note_id TEXT PRIMARY KEY,
            work_id TEXT NOT NULL,
            body_text TEXT NOT NULL,
            visibility TEXT NOT NULL,
            authored_by TEXT NOT NULL,
            remote_event_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_self_work_notes_work_time
            ON ticket_self_work_notes(work_id, created_at ASC);

        CREATE TABLE IF NOT EXISTS ticket_label_assignments (
            ticket_key TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            assigned_by TEXT NOT NULL,
            rationale TEXT,
            evidence_json TEXT NOT NULL,
            assigned_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_control_bundles (
            label TEXT PRIMARY KEY,
            bundle_version INTEGER NOT NULL,
            runbook_id TEXT NOT NULL,
            runbook_version TEXT NOT NULL,
            policy_id TEXT NOT NULL,
            policy_version TEXT NOT NULL,
            approval_mode TEXT NOT NULL,
            autonomy_level TEXT NOT NULL,
            verification_profile_id TEXT NOT NULL,
            writeback_profile_id TEXT NOT NULL,
            support_mode TEXT NOT NULL,
            default_risk_level TEXT NOT NULL,
            execution_actions_json TEXT NOT NULL,
            notes TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_autonomy_grants (
            label TEXT PRIMARY KEY,
            grant_version INTEGER NOT NULL,
            bundle_version INTEGER NOT NULL,
            approval_mode TEXT NOT NULL,
            autonomy_level TEXT NOT NULL,
            approved_by TEXT NOT NULL,
            source_candidate_id TEXT,
            rationale TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_cases (
            case_id TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL,
            label TEXT NOT NULL,
            bundle_label TEXT NOT NULL,
            bundle_version INTEGER NOT NULL,
            state TEXT NOT NULL,
            approval_mode TEXT NOT NULL,
            autonomy_level TEXT NOT NULL,
            support_mode TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            opened_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            closed_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_cases_ticket
            ON ticket_cases(ticket_key, updated_at DESC);

        CREATE INDEX IF NOT EXISTS idx_ticket_cases_state_time
            ON ticket_cases(state, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_dry_runs (
            dry_run_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            ticket_key TEXT NOT NULL,
            label TEXT NOT NULL,
            bundle_label TEXT NOT NULL,
            bundle_version INTEGER NOT NULL,
            artifact_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_approvals (
            approval_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            status TEXT NOT NULL,
            decided_by TEXT NOT NULL,
            rationale TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_execution_actions (
            action_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            ticket_key TEXT NOT NULL,
            summary TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_verifications (
            verification_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            status TEXT NOT NULL,
            summary TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_learning_candidates (
            candidate_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            ticket_key TEXT NOT NULL,
            label TEXT NOT NULL,
            bundle_label TEXT NOT NULL,
            bundle_version INTEGER NOT NULL,
            summary TEXT NOT NULL,
            proposed_actions_json TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            status TEXT NOT NULL,
            proposed_at TEXT NOT NULL,
            decided_at TEXT,
            decided_by TEXT,
            decision_notes TEXT,
            promoted_autonomy_level TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_learning_candidates_label_time
            ON ticket_learning_candidates(label, proposed_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_writebacks (
            writeback_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            ticket_key TEXT NOT NULL,
            operation TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_clarification_requests (
            clarification_id TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL,
            case_id TEXT,
            work_id TEXT,
            target_type TEXT NOT NULL,
            target_channel TEXT NOT NULL,
            question TEXT NOT NULL,
            missing_inputs_json TEXT NOT NULL,
            unblock_criteria TEXT,
            status TEXT NOT NULL,
            outbound_message_key TEXT,
            inbound_response_key TEXT,
            inbound_response_body TEXT,
            resume_state TEXT NOT NULL,
            created_by TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            sent_at TEXT,
            resolved_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_clarifications_ticket_status
            ON ticket_clarification_requests(ticket_key, status, updated_at DESC);

        CREATE INDEX IF NOT EXISTS idx_ticket_clarifications_case_status
            ON ticket_clarification_requests(case_id, status, updated_at DESC);

        CREATE TABLE IF NOT EXISTS ticket_sync_runs (
            run_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            fetched_count INTEGER NOT NULL,
            stored_ticket_count INTEGER NOT NULL,
            stored_event_count INTEGER NOT NULL,
            status TEXT NOT NULL,
            error_text TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ticket_audit_log (
            audit_id TEXT PRIMARY KEY,
            ticket_key TEXT NOT NULL,
            case_id TEXT,
            actor_type TEXT NOT NULL,
            action_type TEXT NOT NULL,
            label TEXT,
            bundle_label TEXT,
            bundle_version INTEGER,
            details_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ticket_audit_ticket_time
            ON ticket_audit_log(ticket_key, created_at DESC);
        "#,
    ))?;
    ensure_ticket_event_recovery_columns(conn)?;
    ensure_ticket_event_routing_rows(conn)?;
    Ok(())
}

fn ensure_ticket_event_recovery_columns(conn: &Connection) -> Result<()> {
    for (column, definition) in [
        ("lease_expires_at", "TEXT"),
        ("failure_class", "TEXT"),
        ("failure_attempt_count", "INTEGER NOT NULL DEFAULT 0"),
        ("retry_not_before", "TEXT"),
        ("failure_proof", "TEXT"),
        ("hold_reason", "TEXT"),
        ("wait_entity_type", "TEXT"),
        ("wait_entity_id", "TEXT"),
    ] {
        let exists: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('ticket_event_routing_state') WHERE name=?1)",
            params![column],
            |row| row.get(0),
        )?;
        if exists == 0 {
            match conn.execute_batch(&format!(
                "ALTER TABLE ticket_event_routing_state ADD COLUMN {column} {definition};"
            )) {
                Ok(()) => {}
                Err(err)
                    if err
                        .to_string()
                        .to_ascii_lowercase()
                        .contains("duplicate column name") => {}
                Err(err) => return Err(err.into()),
            }
        }
    }
    conn.execute(
        r#"
        UPDATE ticket_event_routing_state
        SET lease_expires_at=datetime(leased_at, '+15 minutes')
        WHERE route_status='leased' AND lease_expires_at IS NULL
        "#,
        [],
    )?;
    Ok(())
}

fn ensure_ticket_event_routing_rows(conn: &Connection) -> Result<()> {
    let mut statement = conn.prepare(
        r#"
        SELECT
            e.event_key,
            CASE
                WHEN e.direction = 'outbound' THEN 'handled'
                ELSE 'pending'
            END,
            e.observed_at
        FROM ticket_events e
        LEFT JOIN ticket_event_routing_state r ON r.event_key = e.event_key
        WHERE r.event_key IS NULL
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let missing = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (event_key, route_status, observed_at) in missing {
        let terminal_policy_grant =
            (route_status == "handled").then_some(TerminalPolicyGrant::outbound_ticket_event());
        force_ticket_event_routed_state_at(
            conn,
            &event_key,
            &route_status,
            &observed_at,
            terminal_policy_grant.as_ref(),
        )?;
    }
    migrate_ticket_self_work_items_schema(conn)?;
    Ok(())
}

fn migrate_ticket_self_work_items_schema(conn: &Connection) -> Result<()> {
    let table_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if !table_sql.contains("UNIQUE(source_system, kind)") {
        return Ok(());
    }
    // ctox-allow-direct-state-write: schema migration copies existing states 1:1.
    // Wrap rename + recreate + copy + drop in one transaction so a crash mid-migration
    // cannot leave an empty live table while the rows sit orphaned in the legacy table
    // (the early-return guard above would then skip recovery forever). The legacy table is
    // only dropped after an in-transaction row-count match.
    let tx = conn
        .unchecked_transaction()
        .context("failed to begin ticket_self_work_items migration")?;
    tx.execute_batch(
        r#"
        ALTER TABLE ticket_self_work_items RENAME TO ticket_self_work_items_legacy_unique;

        CREATE TABLE ticket_self_work_items (
            work_id TEXT PRIMARY KEY,
            source_system TEXT NOT NULL,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            body_text TEXT NOT NULL,
            state TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            remote_ticket_id TEXT,
            remote_locator TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        INSERT INTO ticket_self_work_items (
            work_id, source_system, kind, title, body_text, state, metadata_json,
            remote_ticket_id, remote_locator, created_at, updated_at
        )
        SELECT
            work_id, source_system, kind, title, body_text, state, metadata_json,
            remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items_legacy_unique;
        "#,
    )?;
    let legacy_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM ticket_self_work_items_legacy_unique",
        [],
        |row| row.get(0),
    )?;
    let migrated_count: i64 =
        tx.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
            row.get(0)
        })?;
    if migrated_count != legacy_count {
        anyhow::bail!(
            "ticket_self_work_items migration copied {migrated_count} of {legacy_count} rows; rolling back"
        );
    }
    tx.execute_batch(
        r#"
        DROP TABLE ticket_self_work_items_legacy_unique;

        CREATE INDEX IF NOT EXISTS idx_ticket_self_work_scope
            ON ticket_self_work_items(source_system, state, updated_at DESC);
        "#,
    )?;
    tx.commit()
        .context("failed to commit ticket_self_work_items migration")?;
    Ok(())
}

fn schema_state(conn: &Connection) -> Result<Value> {
    let ticket_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_items", [], |row| row.get(0))?;
    let event_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_events", [], |row| row.get(0))?;
    let bundle_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_control_bundles", [], |row| {
            row.get(0)
        })?;
    let grant_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_autonomy_grants", [], |row| {
            row.get(0)
        })?;
    let routed_event_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ticket_event_routing_state",
        [],
        |row| row.get(0),
    )?;
    let outbound_mark_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ticket_outbound_event_marks",
        [],
        |row| row.get(0),
    )?;
    let source_control_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_source_controls", [], |row| {
            row.get(0)
        })?;
    let knowledge_main_skill_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_main_skills", [], |row| {
            row.get(0)
        })?;
    let knowledge_skillbook_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_skillbooks", [], |row| {
            row.get(0)
        })?;
    let knowledge_runbook_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_runbooks", [], |row| {
            row.get(0)
        })?;
    let knowledge_runbook_item_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_runbook_items", [], |row| {
            row.get(0)
        })?;
    let knowledge_resource_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_resources", [], |row| {
            row.get(0)
        })?;
    let knowledge_embedding_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM knowledge_embeddings", [], |row| {
            row.get(0)
        })?;
    let knowledge_entry_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_knowledge_entries", [], |row| {
            row.get(0)
        })?;
    let knowledge_load_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_knowledge_loads", [], |row| {
            row.get(0)
        })?;
    let self_work_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
            row.get(0)
        })?;
    let self_work_assignment_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ticket_self_work_assignments",
        [],
        |row| row.get(0),
    )?;
    let self_work_note_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM ticket_self_work_notes", [], |row| {
            row.get(0)
        })?;
    let learning_candidate_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ticket_learning_candidates",
        [],
        |row| row.get(0),
    )?;
    Ok(json!({
        "tickets": ticket_count,
        "events": event_count,
        "control_bundles": bundle_count,
        "autonomy_grants": grant_count,
        "learning_candidates": learning_candidate_count,
        "outbound_event_marks": outbound_mark_count,
        "routed_events": routed_event_count,
        "source_controls": source_control_count,
        "knowledge_main_skills": knowledge_main_skill_count,
        "knowledge_skillbooks": knowledge_skillbook_count,
        "knowledge_runbooks": knowledge_runbook_count,
        "knowledge_runbook_items": knowledge_runbook_item_count,
        "knowledge_resources": knowledge_resource_count,
        "knowledge_embeddings": knowledge_embedding_count,
        "knowledge_entries": knowledge_entry_count,
        "knowledge_loads": knowledge_load_count,
        "self_work_items": self_work_count,
        "self_work_assignments": self_work_assignment_count,
        "self_work_notes": self_work_note_count,
    }))
}

fn resolve_db_path(root: &Path) -> std::path::PathBuf {
    root.join(DEFAULT_DB_RELATIVE_PATH)
}

fn canonical_ticket_key(system: &str, remote_ticket_id: &str) -> String {
    format!("{}:{}", system.trim(), remote_ticket_id.trim())
}

fn canonical_event_key(system: &str, remote_event_id: &str) -> String {
    format!("{}:{}", system.trim(), remote_event_id.trim())
}

fn now_iso_string() -> String {
    Utc::now().to_rfc3339()
}

fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..12].to_string()
}

fn ticket_thread_key(ticket: &TicketItemView) -> String {
    format!(
        "ticket/{}/{}",
        normalize_token(&ticket.source_system),
        normalize_token(&ticket.remote_ticket_id)
    )
}

fn collapse_inline(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        collapsed
    } else {
        let clipped = collapsed
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{clipped}…")
    }
}

fn normalize_token(raw: &str) -> String {
    let normalized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn canonical_ticket_event_route_status(raw: &str) -> Result<&'static str> {
    Ok(TicketEventRouteStatus::parse(raw)?.as_str())
}

fn more_restrictive_approval_mode(
    left: ControlApprovalMode,
    right: ControlApprovalMode,
) -> ControlApprovalMode {
    if left.rank() <= right.rank() {
        left
    } else {
        right
    }
}

fn canonical_autonomy_level(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "A0" => Ok("A0"),
        "A1" => Ok("A1"),
        "A2" => Ok("A2"),
        "A3" => Ok("A3"),
        "A4" => Ok("A4"),
        other => anyhow::bail!("unsupported autonomy level: {other}"),
    }
}

fn autonomy_level_rank(level: &str) -> Result<u8> {
    match canonical_autonomy_level(level)? {
        "A0" => Ok(0),
        "A1" => Ok(1),
        "A2" => Ok(2),
        "A3" => Ok(3),
        "A4" => Ok(4),
        _ => unreachable!(),
    }
}

fn more_restrictive_autonomy_level<'a>(left: &'a str, right: &'a str) -> &'a str {
    let left_rank = autonomy_level_rank(left).unwrap_or(0);
    let right_rank = autonomy_level_rank(right).unwrap_or(0);
    if left_rank <= right_rank {
        left
    } else {
        right
    }
}

fn parse_limit(args: &[String], default: usize) -> usize {
    find_flag_value(args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn flag_present(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn is_remote_event_marked_outbound(
    conn: &Connection,
    system: &str,
    remote_event_id: &str,
) -> Result<bool> {
    conn.query_row(
        r#"
        SELECT 1
        FROM ticket_outbound_event_marks
        WHERE source_system = ?1 AND remote_event_id = ?2
        LIMIT 1
        "#,
        params![system, remote_event_id],
        |_row| Ok(true),
    )
    .optional()
    .map(|value| value.unwrap_or(false))
    .map_err(anyhow::Error::from)
}

fn positional_after_flags(args: &[String]) -> Vec<String> {
    let mut values = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        values.push(arg.clone());
    }
    values
}

fn parse_json_value(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse json: {raw}"))
}

fn parse_json_string_array(raw: &str) -> Result<Vec<String>> {
    let value: Value = parse_json_value(raw)?;
    let Some(items) = value.as_array() else {
        anyhow::bail!("expected a JSON array of strings");
    };
    let parsed = items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .context("expected a JSON array of strings")
        })
        .collect::<Result<Vec<_>>>()?;
    if parsed.is_empty() {
        anyhow::bail!("execution actions array must not be empty");
    }
    Ok(parsed)
}

fn parse_json_column(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
}

fn parse_json_string_column(raw: String) -> Vec<String> {
    parse_json_column(raw)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect()
}

fn default_execution_actions() -> Vec<String> {
    vec![
        "observe".to_string(),
        "analyze".to_string(),
        "draft_communication".to_string(),
    ]
}

fn action_rationale(action: &str) -> &'static str {
    match action {
        "observe" => "collect current ticket and environment facts without causing side effects",
        "analyze" => "reason about likely cause, scope, and next safe action",
        "draft_communication" => {
            "prepare an owner- or requester-visible update without sending it yet"
        }
        "local_safe_change" => "bounded local change with low blast radius",
        "repo_change" => "code or artifact change inside the tracked workspace",
        "remote_write" => "non-local write into an external system",
        "privileged_change" => "change requiring elevated authority or privileged access",
        "service_affecting_change" => "change that can impact a running service or user experience",
        _ => "bundle-defined action class",
    }
}

fn required_evidence_for_bundle(bundle: &ControlBundleView) -> Vec<String> {
    vec![
        format!("verification profile: {}", bundle.verification_profile_id),
        format!("writeback profile: {}", bundle.writeback_profile_id),
        format!("policy: {} {}", bundle.policy_id, bundle.policy_version),
    ]
}

fn canonical_approval_status(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "approved" => Ok("approved"),
        "rejected" => Ok("rejected"),
        other => anyhow::bail!("unsupported approval status: {other}"),
    }
}

fn canonical_learning_candidate_status(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "proposed" => Ok("proposed"),
        "approved" => Ok("approved"),
        "rejected" => Ok("rejected"),
        other => anyhow::bail!("unsupported learning candidate status: {other}"),
    }
}

fn canonical_verification_status(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "passed" => Ok("passed"),
        "failed" => Ok("failed"),
        other => anyhow::bail!("unsupported verification status: {other}"),
    }
}

fn map_ticket_source_control_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketSourceControlView> {
    Ok(TicketSourceControlView {
        source_system: row.get(0)?,
        adoption_mode: row.get(1)?,
        baseline_external_created_cutoff: row.get(2)?,
        attached_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn map_ticket_source_skill_binding_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketSourceSkillBindingView> {
    Ok(TicketSourceSkillBindingView {
        source_system: row.get(0)?,
        skill_name: row.get(1)?,
        archetype: row.get(2)?,
        status: row.get(3)?,
        origin: row.get(4)?,
        artifact_path: row.get(5)?,
        notes: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_ticket_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketItemView> {
    Ok(TicketItemView {
        ticket_key: row.get(0)?,
        source_system: row.get(1)?,
        remote_ticket_id: row.get(2)?,
        title: row.get(3)?,
        body_text: row.get(4)?,
        remote_status: row.get(5)?,
        priority: row.get(6)?,
        requester: row.get(7)?,
        metadata: parse_json_column(row.get::<_, String>(8)?),
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        last_synced_at: row.get(11)?,
    })
}

fn map_ticket_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketEventView> {
    Ok(TicketEventView {
        event_key: row.get(0)?,
        ticket_key: row.get(1)?,
        source_system: row.get(2)?,
        remote_event_id: row.get(3)?,
        direction: row.get(4)?,
        event_type: row.get(5)?,
        summary: row.get(6)?,
        body_text: row.get(7)?,
        metadata: parse_json_column(row.get::<_, String>(8)?),
        external_created_at: row.get(9)?,
        observed_at: row.get(10)?,
    })
}

fn map_control_bundle_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ControlBundleView> {
    let execution_actions = parse_json_column(row.get::<_, String>(12)?)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    Ok(ControlBundleView {
        label: row.get(0)?,
        bundle_version: row.get(1)?,
        runbook_id: row.get(2)?,
        runbook_version: row.get(3)?,
        policy_id: row.get(4)?,
        policy_version: row.get(5)?,
        approval_mode: row.get(6)?,
        autonomy_level: row.get(7)?,
        verification_profile_id: row.get(8)?,
        writeback_profile_id: row.get(9)?,
        support_mode: row.get(10)?,
        default_risk_level: row.get(11)?,
        execution_actions,
        notes: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_autonomy_grant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomyGrantView> {
    Ok(AutonomyGrantView {
        label: row.get(0)?,
        grant_version: row.get(1)?,
        bundle_version: row.get(2)?,
        approval_mode: row.get(3)?,
        autonomy_level: row.get(4)?,
        approved_by: row.get(5)?,
        source_candidate_id: row.get(6)?,
        rationale: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_learning_candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningCandidateView> {
    let proposed_actions = parse_json_column(row.get::<_, String>(7)?)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    Ok(LearningCandidateView {
        candidate_id: row.get(0)?,
        case_id: row.get(1)?,
        ticket_key: row.get(2)?,
        label: row.get(3)?,
        bundle_label: row.get(4)?,
        bundle_version: row.get(5)?,
        summary: row.get(6)?,
        proposed_actions,
        evidence: parse_json_column(row.get::<_, String>(8)?),
        status: row.get(9)?,
        proposed_at: row.get(10)?,
        decided_at: row.get(11)?,
        decided_by: row.get(12)?,
        decision_notes: row.get(13)?,
        promoted_autonomy_level: row.get(14)?,
    })
}

fn map_case_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketCaseView> {
    Ok(TicketCaseView {
        case_id: row.get(0)?,
        ticket_key: row.get(1)?,
        label: row.get(2)?,
        bundle_label: row.get(3)?,
        bundle_version: row.get(4)?,
        state: row.get(5)?,
        approval_mode: row.get(6)?,
        autonomy_level: row.get(7)?,
        support_mode: row.get(8)?,
        risk_level: row.get(9)?,
        opened_at: row.get(10)?,
        updated_at: row.get(11)?,
        closed_at: row.get(12)?,
    })
}

fn map_ticket_clarification_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketClarificationRequestView> {
    let missing_inputs_raw: String = row.get(7)?;
    let metadata_raw: String = row.get(19)?;
    Ok(TicketClarificationRequestView {
        clarification_id: row.get(0)?,
        ticket_key: row.get(1)?,
        case_id: row.get(2)?,
        work_id: row.get(3)?,
        target_type: row.get(4)?,
        target_channel: row.get(5)?,
        question: row.get(6)?,
        missing_inputs: parse_json_string_array_lossy(&missing_inputs_raw),
        unblock_criteria: row.get(8)?,
        status: row.get(9)?,
        outbound_message_key: row.get(10)?,
        inbound_response_key: row.get(11)?,
        inbound_response_body: row.get(12)?,
        resume_state: row.get(13)?,
        created_by: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        sent_at: row.get(17)?,
        resolved_at: row.get(18)?,
        metadata: parse_json_column(metadata_raw),
    })
}

fn map_audit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketAuditRecord> {
    Ok(TicketAuditRecord {
        audit_id: row.get(0)?,
        ticket_key: row.get(1)?,
        case_id: row.get(2)?,
        actor_type: row.get(3)?,
        action_type: row.get(4)?,
        label: row.get(5)?,
        bundle_label: row.get(6)?,
        bundle_version: row.get(7)?,
        details: parse_json_column(row.get::<_, String>(8)?),
        created_at: row.get(9)?,
    })
}

fn map_ticket_knowledge_entry_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketKnowledgeEntryView> {
    Ok(TicketKnowledgeEntryView {
        entry_id: row.get(0)?,
        source_system: row.get(1)?,
        domain: row.get(2)?,
        knowledge_key: row.get(3)?,
        title: row.get(4)?,
        summary: row.get(5)?,
        status: row.get(6)?,
        content: parse_json_column(row.get::<_, String>(7)?),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_ticket_self_work_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TicketSelfWorkItemView> {
    let kind: String = row.get(2)?;
    let raw_state: String = row.get(5)?;
    let state = WorkItemStatus::parse(&raw_state).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown persisted ticket work-item status `{raw_state}`"),
            )),
        )
    })?;
    let mut metadata = parse_json_column(row.get::<_, String>(6)?);
    if let Some(raw_status) = metadata.get("workflow_step_status").and_then(Value::as_str) {
        let status = WorkItemStatus::parse(raw_status).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown persisted workflow work-item status `{raw_status}`"),
                )),
            )
        })?;
        if let Some(object) = metadata.as_object_mut() {
            object.insert(
                "workflow_step_status".to_string(),
                Value::String(status.as_str().to_string()),
            );
        }
    }
    let suggested_skill = metadata
        .get("skill")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| default_skill_for_self_work_kind(&kind));
    Ok(TicketSelfWorkItemView {
        work_id: row.get(0)?,
        source_system: row.get(1)?,
        kind,
        title: row.get(3)?,
        body_text: row.get(4)?,
        state: state.as_str().to_string(),
        suggested_skill,
        metadata,
        assigned_to: None,
        assigned_by: None,
        assigned_at: None,
        remote_ticket_id: row.get(7)?,
        remote_locator: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_ticket_self_work_assignment_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketSelfWorkAssignmentView> {
    Ok(TicketSelfWorkAssignmentView {
        assignment_id: row.get(0)?,
        work_id: row.get(1)?,
        assigned_to: row.get(2)?,
        assigned_by: row.get(3)?,
        rationale: row.get(4)?,
        remote_event_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_ticket_self_work_note_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TicketSelfWorkNoteView> {
    Ok(TicketSelfWorkNoteView {
        note_id: row.get(0)?,
        work_id: row.get(1)?,
        body_text: row.get(2)?,
        visibility: row.get(3)?,
        authored_by: row.get(4)?,
        remote_event_id: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests;
