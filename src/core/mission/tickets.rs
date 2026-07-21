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

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::model_registry;
use crate::inference::runtime_kernel;
use crate::inference::supervisor;
use crate::mission::ticket_adapters;
use crate::mission::ticket_gateway;
use crate::mission::ticket_protocol;
use crate::mission::ticket_translation;
use crate::service::core_state_machine::{
    CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
};
use crate::service::core_transition_guard::{
    enforce_core_spawn, enforce_core_transition, ensure_core_transition_guard_schema,
    CoreSpawnRequest,
};
use crate::service::harness_flow::{
    record_harness_flow_event_lossy, RecordHarnessFlowEventRequest,
};

const DEFAULT_DB_RELATIVE_PATH: &str = "runtime/ctox.sqlite3";
const DEFAULT_LIST_LIMIT: usize = 20;
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
const WORKFLOW_STEP_STATUS_READY: &str = "ready";
const WORKFLOW_STEP_STATUS_WAITING: &str = "waiting";
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

pub fn handle_ticket_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_ticket_db(root)?;
            print_json(&json!({
                "ok": true,
                "db_path": resolve_db_path(root),
                "initialized": schema_state(&conn)?,
            }))
        }
        "sync" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket sync --system <local>")?;
            let result = sync_ticket_system(root, system)?;
            print_json(&result)
        }
        "test" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket test --system <local>")?;
            let result = test_ticket_system(root, system)?;
            print_json(&result)
        }
        "capabilities" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket capabilities --system <name>")?;
            let result = ticket_system_capabilities(system)?;
            print_json(&result)
        }
        "sources" => {
            let controls = list_ticket_source_controls(root)?;
            print_json(&json!({"ok": true, "count": controls.len(), "sources": controls}))
        }
        "source-skills" => {
            let system = find_flag_value(args, "--system");
            let bindings = list_ticket_source_skill_bindings(root, system)?;
            print_json(&json!({"ok": true, "count": bindings.len(), "source_skills": bindings}))
        }
        "source-skill-set" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket source-skill-set --system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]")?;
            let skill = required_flag_value(args, "--skill")
                .context("usage: ctox ticket source-skill-set --system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]")?;
            let archetype = find_flag_value(args, "--archetype").unwrap_or("operating-model");
            let status = find_flag_value(args, "--status").unwrap_or("active");
            let origin = find_flag_value(args, "--origin").unwrap_or("ticket-onboarding");
            let artifact_path = find_flag_value(args, "--artifact-path");
            let notes = find_flag_value(args, "--notes");
            let binding = put_ticket_source_skill_binding(
                root,
                system,
                skill,
                archetype,
                status,
                origin,
                artifact_path,
                notes,
            )?;
            print_json(&json!({"ok": true, "source_skill": binding}))
        }
        "source-skill-show" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket source-skill-show --system <name>")?;
            let view = show_ticket_source_skill(root, system)?;
            print_json(&json!({"ok": true, "source_skill": view}))
        }
        "source-skill-query" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket source-skill-query --system <name> --query <text> [--top-k <n>]",
            )?;
            let query = required_flag_value(args, "--query").context(
                "usage: ctox ticket source-skill-query --system <name> --query <text> [--top-k <n>]",
            )?;
            let top_k = find_flag_value(args, "--top-k")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(3);
            let result = query_ticket_source_skill(root, system, query, top_k)?;
            print_json(&result)
        }
        "source-skill-import-bundle" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket source-skill-import-bundle --system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]",
            )?;
            let bundle_dir = required_flag_value(args, "--bundle-dir").context(
                "usage: ctox ticket source-skill-import-bundle --system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]",
            )?;
            let result = import_ticket_source_skill_bundle(
                root,
                system,
                bundle_dir,
                find_flag_value(args, "--embedding-model"),
                flag_present(args, "--skip-embeddings"),
            )?;
            print_json(&result)
        }
        "source-skill-resolve" => {
            let top_k = find_flag_value(args, "--top-k")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(3);
            let result = resolve_ticket_source_skill_for_target(
                root,
                find_flag_value(args, "--ticket-key"),
                find_flag_value(args, "--case-id"),
                top_k,
            )?;
            print_json(&result)
        }
        "source-skill-compose-reply" => {
            let result = compose_ticket_source_skill_reply(
                root,
                find_flag_value(args, "--ticket-key"),
                find_flag_value(args, "--case-id"),
                find_flag_value(args, "--send-policy").unwrap_or("suggestion"),
                find_flag_value(args, "--subject"),
                flag_present(args, "--body-only"),
            )?;
            match result {
                Value::String(body) => {
                    println!("{body}");
                    Ok(())
                }
                other => print_json(&other),
            }
        }
        "source-skill-review-note" => {
            let body = required_flag_value(args, "--body").context(
                "usage: ctox ticket source-skill-review-note (--ticket-key <key> | --case-id <id>) --body <text> [--top-k <n>]",
            )?;
            let top_k = find_flag_value(args, "--top-k")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(1);
            if let Some(ticket_key) = find_flag_value(args, "--ticket-key") {
                let review = review_ticket_note_with_source_skill(root, ticket_key, body, top_k)?;
                print_json(&json!({"ok": true, "review": review}))
            } else if let Some(case_id) = find_flag_value(args, "--case-id") {
                let case = load_case(root, case_id)?.context("ticket case not found")?;
                let review =
                    review_ticket_note_with_source_skill(root, &case.ticket_key, body, top_k)?;
                print_json(&json!({"ok": true, "review": review}))
            } else {
                anyhow::bail!(
                    "usage: ctox ticket source-skill-review-note (--ticket-key <key> | --case-id <id>) --body <text> [--top-k <n>]"
                );
            }
        }
        "history-export" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket history-export --system <name> --output <path>")?;
            let output = required_flag_value(args, "--output")
                .context("usage: ctox ticket history-export --system <name> --output <path>")?;
            let result = export_ticket_history_dataset(root, system, Path::new(output))?;
            print_json(&result)
        }
        "knowledge-bootstrap" => {
            let system = required_flag_value(args, "--system")
                .context("usage: ctox ticket knowledge-bootstrap --system <name>")?;
            let entries = refresh_observed_ticket_knowledge(root, system)?;
            print_json(
                &json!({"ok": true, "system": system, "count": entries.len(), "entries": entries}),
            )
        }
        "knowledge-list" => {
            let system = find_flag_value(args, "--system");
            let domain = find_flag_value(args, "--domain");
            let status = find_flag_value(args, "--status");
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let entries = list_ticket_knowledge_entries(root, system, domain, status, limit)?;
            print_json(&json!({"ok": true, "count": entries.len(), "entries": entries}))
        }
        "knowledge-show" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket knowledge-show --system <name> --domain <name> --key <value>",
            )?;
            let domain = required_flag_value(args, "--domain").context(
                "usage: ctox ticket knowledge-show --system <name> --domain <name> --key <value>",
            )?;
            let key = required_flag_value(args, "--key").context(
                "usage: ctox ticket knowledge-show --system <name> --domain <name> --key <value>",
            )?;
            let entry = load_ticket_knowledge_entry(root, system, domain, key)?
                .context("ticket knowledge entry not found")?;
            print_json(&json!({"ok": true, "entry": entry}))
        }
        "knowledge-load" => {
            let ticket_key = required_flag_value(args, "--ticket-key").context(
                "usage: ctox ticket knowledge-load --ticket-key <key> [--domains <csv>]",
            )?;
            let domains = find_flag_value(args, "--domains").map(parse_domain_csv);
            let load = create_ticket_knowledge_load(root, ticket_key, domains.as_deref())?;
            print_json(&json!({"ok": true, "knowledge_load": load}))
        }
        "monitoring-ingest" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket monitoring-ingest --system <name> --snapshot-json <json> [--key <value>] [--title <text>] [--summary <text>] [--status <value>]",
            )?;
            let snapshot_raw = required_flag_value(args, "--snapshot-json").context(
                "usage: ctox ticket monitoring-ingest --system <name> --snapshot-json <json> [--key <value>] [--title <text>] [--summary <text>] [--status <value>]",
            )?;
            let snapshot = parse_json_value(snapshot_raw)?;
            let knowledge_key = find_flag_value(args, "--key").unwrap_or("observed");
            let status = find_flag_value(args, "--status").unwrap_or("observed");
            let title = find_flag_value(args, "--title")
                .map(str::to_string)
                .unwrap_or_else(|| format!("{system} monitoring landscape"));
            let summary = find_flag_value(args, "--summary")
                .map(str::to_string)
                .unwrap_or_else(|| summarize_monitoring_snapshot(&snapshot));
            let entry = put_ticket_knowledge_entry(
                root,
                TicketKnowledgeUpsertInput {
                    source_system: system.to_string(),
                    domain: "monitoring_landscape".to_string(),
                    knowledge_key: knowledge_key.to_string(),
                    title,
                    summary,
                    status: status.to_string(),
                    content: snapshot,
                },
            )?;
            print_json(&json!({"ok": true, "entry": entry}))
        }
        "access-request-put" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket access-request-put --system <name> --title <title> --body <text> [--required-scopes <csv>] [--secret-refs <csv>] [--channels <csv>] [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let title = required_flag_value(args, "--title").context(
                "usage: ctox ticket access-request-put --system <name> --title <title> --body <text> [--required-scopes <csv>] [--secret-refs <csv>] [--channels <csv>] [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let body = required_flag_value(args, "--body").context(
                "usage: ctox ticket access-request-put --system <name> --title <title> --body <text> [--required-scopes <csv>] [--secret-refs <csv>] [--channels <csv>] [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let required_scopes = find_flag_value(args, "--required-scopes")
                .map(parse_domain_csv)
                .unwrap_or_default();
            let secret_refs = find_flag_value(args, "--secret-refs")
                .map(parse_domain_csv)
                .unwrap_or_default();
            let channels = find_flag_value(args, "--channels")
                .map(parse_domain_csv)
                .unwrap_or_else(|| vec!["mail".to_string()]);
            let explicit_skill = find_flag_value(args, "--skill")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let mut metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            if let Some(object) = metadata.as_object_mut() {
                object.insert("required_scopes".to_string(), json!(required_scopes));
                object.insert("secret_refs".to_string(), json!(secret_refs));
                object.insert("channels".to_string(), json!(channels));
                if !object.contains_key("skill") {
                    object.insert(
                        "skill".to_string(),
                        json!(
                            explicit_skill
                                .clone()
                                .unwrap_or_else(|| "ticket-access-and-secrets".to_string())
                        ),
                    );
                }
            }
            let item = put_ticket_self_work_item(
                root,
                TicketSelfWorkUpsertInput {
                    source_system: system.to_string(),
                    kind: "access-request".to_string(),
                    title: title.to_string(),
                    body_text: body.to_string(),
                    state: "open".to_string(),
                    metadata,
                },
                flag_present(args, "--publish"),
            )?;
            print_json(&json!({"ok": true, "item": item}))
        }
        "self-work-list" | "internal-work-list" => {
            let system = find_flag_value(args, "--system");
            let state = find_flag_value(args, "--state");
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let items = list_ticket_self_work_items(root, system, state, limit)?;
            print_json(&json!({"ok": true, "count": items.len(), "items": items}))
        }
        "self-work-show" | "internal-work-show" => {
            let work_id = required_flag_value(args, "--work-id")
                .context("usage: ctox ticket internal-work-show --work-id <id>")?;
            let item = load_ticket_self_work_item(root, work_id)?
                .context("ticket internal work item not found")?;
            let assignments = list_ticket_self_work_assignments(root, work_id, DEFAULT_LIST_LIMIT)?;
            let notes = list_ticket_self_work_notes(root, work_id, DEFAULT_LIST_LIMIT)?;
            print_json(
                &json!({"ok": true, "item": item, "assignments": assignments, "notes": notes}),
            )
        }
        "self-work-put" | "internal-work-put" => {
            let system = required_flag_value(args, "--system").context(
                "usage: ctox ticket internal-work-put --system <name> --kind <kind> --title <title> --body <text> [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let kind = required_flag_value(args, "--kind").context(
                "usage: ctox ticket internal-work-put --system <name> --kind <kind> --title <title> --body <text> [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let title = required_flag_value(args, "--title").context(
                "usage: ctox ticket internal-work-put --system <name> --kind <kind> --title <title> --body <text> [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let body = required_flag_value(args, "--body").context(
                "usage: ctox ticket internal-work-put --system <name> --kind <kind> --title <title> --body <text> [--skill <name>] [--metadata-json <json>] [--publish]",
            )?;
            let explicit_skill = find_flag_value(args, "--skill")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let mut metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            if let Some(skill) = explicit_skill {
                if let Some(object) = metadata.as_object_mut() {
                    object.insert("skill".to_string(), json!(skill));
                }
            }
            let item = put_ticket_self_work_item(
                root,
                TicketSelfWorkUpsertInput {
                    source_system: system.to_string(),
                    kind: kind.to_string(),
                    title: title.to_string(),
                    body_text: body.to_string(),
                    state: "open".to_string(),
                    metadata,
                },
                flag_present(args, "--publish"),
            )?;
            print_json(&json!({"ok": true, "item": item}))
        }
        "self-work-publish" | "internal-work-publish" => {
            let work_id = required_flag_value(args, "--work-id")
                .context("usage: ctox ticket internal-work-publish --work-id <id>")?;
            let item = publish_ticket_self_work_item(root, work_id)?;
            print_json(&json!({"ok": true, "item": item}))
        }
        "self-work-assign" | "internal-work-assign" => {
            let work_id = required_flag_value(args, "--work-id").context(
                "usage: ctox ticket internal-work-assign --work-id <id> --assignee <name> [--assigned-by <actor>] [--rationale <text>]",
            )?;
            let assignee = required_flag_value(args, "--assignee").context(
                "usage: ctox ticket internal-work-assign --work-id <id> --assignee <name> [--assigned-by <actor>] [--rationale <text>]",
            )?;
            let item = assign_ticket_self_work_item(
                root,
                work_id,
                assignee,
                find_flag_value(args, "--assigned-by").unwrap_or("ctox"),
                find_flag_value(args, "--rationale"),
            )?;
            print_json(&json!({"ok": true, "item": item}))
        }
        "self-work-note" | "internal-work-note" => {
            let work_id = required_flag_value(args, "--work-id").context(
                "usage: ctox ticket internal-work-note --work-id <id> --body <text> [--authored-by <actor>] [--visibility <internal|public>]",
            )?;
            let body = required_flag_value(args, "--body").context(
                "usage: ctox ticket internal-work-note --work-id <id> --body <text> [--authored-by <actor>] [--visibility <internal|public>]",
            )?;
            let note = append_ticket_self_work_note(
                root,
                work_id,
                body,
                find_flag_value(args, "--authored-by").unwrap_or("ctox"),
                find_flag_value(args, "--visibility").unwrap_or("internal"),
            )?;
            print_json(&json!({"ok": true, "note": note}))
        }
        "self-work-transition" | "internal-work-transition" => {
            let work_id = required_flag_value(args, "--work-id").context(
                "usage: ctox ticket internal-work-transition --work-id <id> --state <value> [--transitioned-by <actor>] [--note <text>] [--visibility <internal|public>]",
            )?;
            let state = required_flag_value(args, "--state").context(
                "usage: ctox ticket internal-work-transition --work-id <id> --state <value> [--transitioned-by <actor>] [--note <text>] [--visibility <internal|public>]",
            )?;
            let item = transition_ticket_self_work_item(
                root,
                work_id,
                state,
                find_flag_value(args, "--transitioned-by").unwrap_or("ctox"),
                find_flag_value(args, "--note"),
                find_flag_value(args, "--visibility").unwrap_or("internal"),
            )?;
            print_json(&json!({"ok": true, "item": item}))
        }
        "workflow-start" => {
            let title = required_flag_value(args, "--title").context(
                "usage: ctox ticket workflow-start --title <title> --goal <text> [--system <name>] [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--phase <name>] [--phase-goal <text>] [--exit-gate <text>] [--first-step-title <title>] [--first-step-prompt <text>] [--queue-now]",
            )?;
            let goal = required_flag_value(args, "--goal").context(
                "usage: ctox ticket workflow-start --title <title> --goal <text> [--system <name>] [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--phase <name>] [--phase-goal <text>] [--exit-gate <text>] [--first-step-title <title>] [--first-step-prompt <text>] [--queue-now]",
            )?;
            let workflow = start_ticket_workflow(
                root,
                TicketWorkflowStartInput {
                    source_system: find_flag_value(args, "--system")
                        .unwrap_or("internal")
                        .to_string(),
                    title: title.to_string(),
                    goal: goal.to_string(),
                    thread_key: find_flag_value(args, "--thread-key").map(ToOwned::to_owned),
                    workspace_root: find_flag_value(args, "--workspace-root")
                        .map(ToOwned::to_owned),
                    skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
                    priority: find_flag_value(args, "--priority").map(ToOwned::to_owned),
                    first_phase: find_flag_value(args, "--phase")
                        .unwrap_or("plan")
                        .to_string(),
                    first_phase_goal: find_flag_value(args, "--phase-goal").map(ToOwned::to_owned),
                    first_exit_gate: find_flag_value(args, "--exit-gate").map(ToOwned::to_owned),
                    first_step_title: find_flag_value(args, "--first-step-title")
                        .map(ToOwned::to_owned),
                    first_step_prompt: find_flag_value(args, "--first-step-prompt")
                        .map(ToOwned::to_owned),
                    queue_now: flag_present(args, "--queue-now"),
                },
            )?;
            print_json(&json!({"ok": true, "workflow": workflow}))
        }
        "workflow-step-put" => {
            let workflow_id = required_flag_value(args, "--workflow-id").context(
                "usage: ctox ticket workflow-step-put --workflow-id <id> --phase <phase> --title <title> --body <text> [--step-id <id>] [--role <leaf|reducer>] [--predecessors <csv>] [--predecessor-steps <csv>] [--phase-goal <text>] [--exit-gate <text>] [--skill <name>] [--priority <urgent|high|normal|low>] [--metadata-json <json>] [--queue-now]",
            )?;
            let phase = required_flag_value(args, "--phase").context(
                "usage: ctox ticket workflow-step-put --workflow-id <id> --phase <phase> --title <title> --body <text> [--step-id <id>] [--role <leaf|reducer>] [--predecessors <csv>] [--predecessor-steps <csv>] [--phase-goal <text>] [--exit-gate <text>] [--skill <name>] [--priority <urgent|high|normal|low>] [--metadata-json <json>] [--queue-now]",
            )?;
            let title = required_flag_value(args, "--title").context(
                "usage: ctox ticket workflow-step-put --workflow-id <id> --phase <phase> --title <title> --body <text> [--step-id <id>] [--role <leaf|reducer>] [--predecessors <csv>] [--predecessor-steps <csv>] [--phase-goal <text>] [--exit-gate <text>] [--skill <name>] [--priority <urgent|high|normal|low>] [--metadata-json <json>] [--queue-now]",
            )?;
            let body = required_flag_value(args, "--body").context(
                "usage: ctox ticket workflow-step-put --workflow-id <id> --phase <phase> --title <title> --body <text> [--step-id <id>] [--role <leaf|reducer>] [--predecessors <csv>] [--predecessor-steps <csv>] [--phase-goal <text>] [--exit-gate <text>] [--skill <name>] [--priority <urgent|high|normal|low>] [--metadata-json <json>] [--queue-now]",
            )?;
            let metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let item = put_ticket_workflow_step(
                root,
                TicketWorkflowStepInput {
                    workflow_id: workflow_id.to_string(),
                    role: find_flag_value(args, "--role")
                        .unwrap_or(WORKFLOW_ROLE_LEAF)
                        .to_string(),
                    phase: phase.to_string(),
                    step_id: find_flag_value(args, "--step-id").map(ToOwned::to_owned),
                    title: title.to_string(),
                    body_text: body.to_string(),
                    phase_goal: find_flag_value(args, "--phase-goal").map(ToOwned::to_owned),
                    exit_gate: find_flag_value(args, "--exit-gate").map(ToOwned::to_owned),
                    predecessor_work_ids: find_flag_value(args, "--predecessors")
                        .map(parse_domain_csv)
                        .unwrap_or_default(),
                    predecessor_step_ids: find_flag_value(args, "--predecessor-steps")
                        .map(parse_domain_csv)
                        .unwrap_or_default(),
                    skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
                    priority: find_flag_value(args, "--priority").map(ToOwned::to_owned),
                    metadata,
                },
            )?;
            let queued = if flag_present(args, "--queue-now") {
                Some(workflow_mark_step_queue_ready(root, &item.work_id)?)
            } else {
                None
            };
            print_json(&json!({"ok": true, "item": item, "queued": queued}))
        }
        "workflow-apply-delta" => {
            let workflow_id = required_flag_value(args, "--workflow-id").context(
                "usage: ctox ticket workflow-apply-delta --workflow-id <id> --delta-json <json> [--queue-now]",
            )?;
            let delta_json = required_flag_value(args, "--delta-json").context(
                "usage: ctox ticket workflow-apply-delta --workflow-id <id> --delta-json <json> [--queue-now]",
            )?;
            let delta_value = parse_json_value(delta_json)?;
            let result = apply_ticket_workflow_delta(
                root,
                workflow_id,
                delta_value,
                flag_present(args, "--queue-now"),
            )?;
            print_json(&json!({"ok": true, "result": result}))
        }
        "workflow-materialize" => {
            let workflow_id = find_flag_value(args, "--workflow-id");
            let limit = parse_limit(args, WORKFLOW_MATERIALIZE_DEFAULT_LIMIT);
            let result = materialize_ready_workflow_steps_for_workflow(root, workflow_id, limit)?;
            print_json(&json!({"ok": true, "result": result}))
        }
        "workflow-show" => {
            let workflow_id = required_flag_value(args, "--workflow-id")
                .context("usage: ctox ticket workflow-show --workflow-id <id>")?;
            let workflow = load_ticket_workflow(root, workflow_id)?
                .context("ticket workflow not found")?;
            print_json(&json!({"ok": true, "workflow": workflow}))
        }
        "take" => {
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let lease_owner = find_flag_value(args, "--lease-owner").unwrap_or("codex");
            let events = lease_pending_ticket_events(root, limit, lease_owner)?;
            print_json(&json!({"ok": true, "count": events.len(), "events": events}))
        }
        "ack" => {
            let status = required_flag_value(args, "--status").context(
                "usage: ctox ticket ack --status <handled|failed|duplicate|blocked> <event-key>...",
            )?;
            let event_keys = positional_after_flags(&args[1..]);
            if event_keys.is_empty() {
                anyhow::bail!(
                    "usage: ctox ticket ack --status <handled|failed|duplicate|blocked> <event-key>..."
                );
            }
            let updated = ack_leased_ticket_events(root, &event_keys, status)?;
            print_json(
                &json!({"ok": true, "updated": updated, "status": status, "event_keys": event_keys}),
            )
        }
        "list" => {
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let system = find_flag_value(args, "--system");
            let tickets = list_tickets(root, system, limit)?;
            print_json(&json!({"ok": true, "count": tickets.len(), "tickets": tickets}))
        }
        "show" => {
            let ticket_key = required_flag_value(args, "--ticket-key")
                .context("usage: ctox ticket show --ticket-key <key>")?;
            let ticket = load_ticket(root, ticket_key)?.context("ticket not found")?;
            let label_assignment = load_ticket_label_assignment(root, ticket_key)?;
            print_json(&json!({
                "ok": true,
                "ticket": ticket,
                "label_assignment": label_assignment,
            }))
        }
        "history" => {
            let ticket_key = required_flag_value(args, "--ticket-key")
                .context("usage: ctox ticket history --ticket-key <key> [--limit <n>]")?;
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let events = list_ticket_history(root, ticket_key, limit)?;
            print_json(&json!({"ok": true, "count": events.len(), "events": events}))
        }
        "label-set" => {
            let ticket_key = required_flag_value(args, "--ticket-key")
                .context("usage: ctox ticket label-set --ticket-key <key> --label <label>")?;
            let label = required_flag_value(args, "--label")
                .context("usage: ctox ticket label-set --ticket-key <key> --label <label>")?;
            let assigned_by = find_flag_value(args, "--assigned-by").unwrap_or("manual");
            let rationale = find_flag_value(args, "--rationale");
            let evidence = find_flag_value(args, "--evidence-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let assignment =
                set_ticket_label(root, ticket_key, label, assigned_by, rationale, evidence)?;
            print_json(&json!({"ok": true, "assignment": assignment}))
        }
        "label-show" => {
            let ticket_key = required_flag_value(args, "--ticket-key")
                .context("usage: ctox ticket label-show --ticket-key <key>")?;
            let assignment = load_ticket_label_assignment(root, ticket_key)?
                .context("ticket label assignment not found")?;
            print_json(&json!({"ok": true, "assignment": assignment}))
        }
        "bundle-put" => {
            let label = required_flag_value(args, "--label").context(
                "usage: ctox ticket bundle-put --label <label> --runbook-id <id> --policy-id <id>",
            )?;
            let runbook_id = required_flag_value(args, "--runbook-id").context(
                "usage: ctox ticket bundle-put --label <label> --runbook-id <id> --policy-id <id>",
            )?;
            let policy_id = required_flag_value(args, "--policy-id").context(
                "usage: ctox ticket bundle-put --label <label> --runbook-id <id> --policy-id <id>",
            )?;
            let actions = find_flag_value(args, "--actions")
                .map(parse_json_string_array)
                .transpose()?
                .unwrap_or_else(default_execution_actions);
            let bundle = put_control_bundle(
                root,
                ControlBundleInput {
                    label: label.to_string(),
                    runbook_id: runbook_id.to_string(),
                    runbook_version: find_flag_value(args, "--runbook-version")
                        .unwrap_or("v1")
                        .to_string(),
                    policy_id: policy_id.to_string(),
                    policy_version: find_flag_value(args, "--policy-version")
                        .unwrap_or("v1")
                        .to_string(),
                    approval_mode: find_flag_value(args, "--approval-mode")
                        .unwrap_or(DEFAULT_APPROVAL_MODE)
                        .to_string(),
                    autonomy_level: find_flag_value(args, "--autonomy-level")
                        .unwrap_or(DEFAULT_AUTONOMY_LEVEL)
                        .to_string(),
                    verification_profile_id: find_flag_value(args, "--verification-profile-id")
                        .unwrap_or("default-verification")
                        .to_string(),
                    writeback_profile_id: find_flag_value(args, "--writeback-profile-id")
                        .unwrap_or("default-writeback")
                        .to_string(),
                    support_mode: find_flag_value(args, "--support-mode")
                        .unwrap_or(DEFAULT_SUPPORT_MODE)
                        .to_string(),
                    default_risk_level: find_flag_value(args, "--risk-level")
                        .unwrap_or(DEFAULT_RISK_LEVEL)
                        .to_string(),
                    execution_actions: actions,
                    notes: find_flag_value(args, "--notes").map(ToOwned::to_owned),
                },
            )?;
            print_json(&json!({"ok": true, "bundle": bundle}))
        }
        "bundle-list" => {
            let bundles = list_control_bundles(root)?;
            print_json(&json!({"ok": true, "count": bundles.len(), "bundles": bundles}))
        }
        "autonomy-grant-set" => {
            let label = required_flag_value(args, "--label").context(
                "usage: ctox ticket autonomy-grant-set --label <label> --approval-mode <mode> --autonomy-level <level>",
            )?;
            let approval_mode = required_flag_value(args, "--approval-mode").context(
                "usage: ctox ticket autonomy-grant-set --label <label> --approval-mode <mode> --autonomy-level <level>",
            )?;
            let autonomy_level = required_flag_value(args, "--autonomy-level").context(
                "usage: ctox ticket autonomy-grant-set --label <label> --approval-mode <mode> --autonomy-level <level>",
            )?;
            let bundle_version = find_flag_value(args, "--bundle-version")
                .and_then(|value| value.parse::<i64>().ok());
            let grant = put_autonomy_grant(
                root,
                AutonomyGrantInput {
                    label: label.to_string(),
                    bundle_version,
                    approval_mode: approval_mode.to_string(),
                    autonomy_level: autonomy_level.to_string(),
                    approved_by: find_flag_value(args, "--approved-by")
                        .unwrap_or("owner")
                        .to_string(),
                    source_candidate_id: find_flag_value(args, "--candidate-id")
                        .map(ToOwned::to_owned),
                    rationale: find_flag_value(args, "--rationale").map(ToOwned::to_owned),
                },
            )?;
            print_json(&json!({"ok": true, "grant": grant}))
        }
        "autonomy-grant-list" => {
            let grants = list_autonomy_grants(root)?;
            print_json(&json!({"ok": true, "count": grants.len(), "grants": grants}))
        }
        "dry-run" => {
            let ticket_key = required_flag_value(args, "--ticket-key").context(
                "usage: ctox ticket dry-run --ticket-key <key> [--understanding <text>]",
            )?;
            let record = create_dry_run(
                root,
                ticket_key,
                find_flag_value(args, "--understanding"),
                find_flag_value(args, "--risk-level"),
            )?;
            print_json(&json!({"ok": true, "dry_run": record}))
        }
        "cases" => {
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let ticket_key = find_flag_value(args, "--ticket-key");
            let cases = list_cases(root, ticket_key, limit)?;
            print_json(&json!({"ok": true, "count": cases.len(), "cases": cases}))
        }
        "case-show" => {
            let case_id = required_flag_value(args, "--case-id")
                .context("usage: ctox ticket case-show --case-id <id>")?;
            let case = load_case(root, case_id)?.context("ticket case not found")?;
            let dry_run = load_latest_dry_run_for_case(root, case_id)?;
            print_json(&json!({"ok": true, "case": case, "dry_run": dry_run}))
        }
        "approve" => {
            let case_id = required_flag_value(args, "--case-id").context(
                "usage: ctox ticket approve --case-id <id> --status <approved|rejected>",
            )?;
            let status = required_flag_value(args, "--status").context(
                "usage: ctox ticket approve --case-id <id> --status <approved|rejected>",
            )?;
            let case = decide_case_approval(
                root,
                case_id,
                status,
                find_flag_value(args, "--decided-by").unwrap_or("owner"),
                find_flag_value(args, "--rationale"),
            )?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "execute" => {
            let case_id = required_flag_value(args, "--case-id")
                .context("usage: ctox ticket execute --case-id <id> --summary <text>")?;
            let summary = required_flag_value(args, "--summary")
                .context("usage: ctox ticket execute --case-id <id> --summary <text>")?;
            let case = record_execution_action(root, case_id, summary)?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "verify" => {
            let case_id = required_flag_value(args, "--case-id")
                .context("usage: ctox ticket verify --case-id <id> --status <passed|failed> [--summary <text>]")?;
            let status = required_flag_value(args, "--status")
                .context("usage: ctox ticket verify --case-id <id> --status <passed|failed> [--summary <text>]")?;
            let case =
                record_verification(root, case_id, status, find_flag_value(args, "--summary"))?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "clarification-request" => {
            let case_id = required_flag_value(args, "--case-id").context(
                "usage: ctox ticket clarification-request --case-id <id> --question <text> [--target-type requester|owner|internal] [--target-channel ticket|email|jami|tui] [--missing-inputs <csv>] [--publish-reviewed]",
            )?;
            let question = required_flag_value(args, "--question").context(
                "usage: ctox ticket clarification-request --case-id <id> --question <text> [--target-type requester|owner|internal] [--target-channel ticket|email|jami|tui] [--missing-inputs <csv>] [--publish-reviewed]",
            )?;
            let request = create_ticket_clarification_request(
                root,
                TicketClarificationRequestInput {
                    case_id: Some(case_id.to_string()),
                    ticket_key: None,
                    work_id: find_flag_value(args, "--work-id").map(ToOwned::to_owned),
                    target_type: find_flag_value(args, "--target-type")
                        .unwrap_or("requester")
                        .to_string(),
                    target_channel: find_flag_value(args, "--target-channel")
                        .unwrap_or("ticket")
                        .to_string(),
                    question: question.to_string(),
                    missing_inputs: find_flag_value(args, "--missing-inputs")
                        .map(parse_domain_csv)
                        .unwrap_or_default(),
                    unblock_criteria: find_flag_value(args, "--unblock-criteria")
                        .map(ToOwned::to_owned),
                    resume_state: find_flag_value(args, "--resume-state")
                        .unwrap_or("executable")
                        .to_string(),
                    created_by: find_flag_value(args, "--created-by")
                        .unwrap_or("ctox")
                        .to_string(),
                    metadata: find_flag_value(args, "--metadata-json")
                        .map(parse_json_value)
                        .transpose()?
                        .unwrap_or_else(|| json!({})),
                },
            )?;
            let clarification = if flag_present(args, "--publish-reviewed") {
                publish_ticket_clarification_request(
                    root,
                    &request.clarification_id,
                    find_flag_value(args, "--reviewed-by").unwrap_or("ctox-review"),
                    find_flag_value(args, "--review-summary")
                        .unwrap_or("Clarification question reviewed for this ticket."),
                )?
            } else {
                request
            };
            print_json(&json!({"ok": true, "clarification": clarification}))
        }
        "clarification-resolve" => {
            let clarification_id = required_flag_value(args, "--clarification-id").context(
                "usage: ctox ticket clarification-resolve --clarification-id <id> --response-key <key> [--body <text>]",
            )?;
            let response_key = required_flag_value(args, "--response-key").context(
                "usage: ctox ticket clarification-resolve --clarification-id <id> --response-key <key> [--body <text>]",
            )?;
            let clarification = resolve_ticket_clarification_request(
                root,
                clarification_id,
                response_key,
                find_flag_value(args, "--body"),
                find_flag_value(args, "--resolved-by").unwrap_or("ctox"),
            )?;
            print_json(&json!({"ok": true, "clarification": clarification}))
        }
        "learn-candidate-create" => {
            let case_id = required_flag_value(args, "--case-id").context(
                "usage: ctox ticket learn-candidate-create --case-id <id> --summary <text> [--actions <json-array>] [--evidence-json <json>]",
            )?;
            let summary = required_flag_value(args, "--summary").context(
                "usage: ctox ticket learn-candidate-create --case-id <id> --summary <text> [--actions <json-array>] [--evidence-json <json>]",
            )?;
            let actions = find_flag_value(args, "--actions")
                .map(parse_json_string_array)
                .transpose()?;
            let evidence = find_flag_value(args, "--evidence-json")
                .map(parse_json_value)
                .transpose()?;
            let candidate =
                create_learning_candidate(root, case_id, summary, actions.as_deref(), evidence)?;
            print_json(&json!({"ok": true, "candidate": candidate}))
        }
        "learn-candidate-list" => {
            let limit = parse_limit(args, DEFAULT_LIST_LIMIT);
            let candidates = list_learning_candidates(
                root,
                find_flag_value(args, "--label"),
                find_flag_value(args, "--status"),
                limit,
            )?;
            print_json(&json!({"ok": true, "count": candidates.len(), "candidates": candidates}))
        }
        "learn-candidate-decide" => {
            let candidate_id = required_flag_value(args, "--candidate-id").context(
                "usage: ctox ticket learn-candidate-decide --candidate-id <id> --status <approved|rejected>",
            )?;
            let status = required_flag_value(args, "--status").context(
                "usage: ctox ticket learn-candidate-decide --candidate-id <id> --status <approved|rejected>",
            )?;
            let candidate = decide_learning_candidate(
                root,
                candidate_id,
                status,
                find_flag_value(args, "--decided-by").unwrap_or("owner"),
                find_flag_value(args, "--notes"),
                find_flag_value(args, "--promote-autonomy-level"),
            )?;
            print_json(&json!({"ok": true, "candidate": candidate}))
        }
        "writeback-comment" => {
            let case_id = required_flag_value(args, "--case-id")
                .context("usage: ctox ticket writeback-comment --case-id <id> --body <text>")?;
            let body = required_flag_value(args, "--body")
                .context("usage: ctox ticket writeback-comment --case-id <id> --body <text>")?;
            let case = writeback_comment(root, case_id, body, flag_present(args, "--internal"))?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "writeback-transition" => {
            let case_id = required_flag_value(args, "--case-id").context(
                "usage: ctox ticket writeback-transition --case-id <id> --state <value> [--body <text>] [--internal]",
            )?;
            let state = required_flag_value(args, "--state").context(
                "usage: ctox ticket writeback-transition --case-id <id> --state <value> [--body <text>] [--internal]",
            )?;
            let case = writeback_transition(
                root,
                case_id,
                state,
                find_flag_value(args, "--body"),
                flag_present(args, "--internal"),
            )?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "close" => {
            let case_id = required_flag_value(args, "--case-id")
                .context("usage: ctox ticket close --case-id <id> [--summary <text>]")?;
            let case = close_case(root, case_id, find_flag_value(args, "--summary"))?;
            print_json(&json!({"ok": true, "case": case}))
        }
        "audit" => {
            let limit = parse_limit(args, DEFAULT_AUDIT_LIMIT);
            let ticket_key = find_flag_value(args, "--ticket-key");
            let records = list_audit_records(root, ticket_key, limit)?;
            print_json(&json!({"ok": true, "count": records.len(), "records": records}))
        }
        "local" => crate::mission::ticket_local_native::handle_local_command(root, &args[1..]),
        _ => anyhow::bail!(
            "usage:\n  ctox ticket init\n  ctox ticket sync --system <local|zammad>\n  ctox ticket test --system <local|zammad>\n  ctox ticket capabilities --system <name>\n  ctox ticket sources\n  ctox ticket source-skills [--system <name>]\n  ctox ticket source-skill-set --system <name> --skill <name> [--archetype <value>] [--status <active|inactive>] [--origin <value>] [--artifact-path <path>] [--notes <text>]\n  ctox ticket source-skill-show --system <name>\n  ctox ticket source-skill-query --system <name> --query <text> [--top-k <n>]\n  ctox ticket source-skill-import-bundle --system <name> --bundle-dir <path> [--embedding-model <model>] [--skip-embeddings]\n  ctox ticket source-skill-resolve (--ticket-key <key> | --case-id <id>) [--top-k <n>]\n  ctox ticket source-skill-compose-reply (--ticket-key <key> | --case-id <id>) [--send-policy <suggestion|draft|send>] [--subject <text>] [--body-only]\n  ctox ticket source-skill-review-note (--ticket-key <key> | --case-id <id>) --body <text> [--top-k <n>]\n  ctox ticket history-export --system <name> --output <path>\n  ctox ticket knowledge-bootstrap --system <name>\n  ctox ticket knowledge-list [--system <name>] [--domain <name>] [--status <value>] [--limit <n>]\n  ctox ticket knowledge-show --system <name> --domain <name> --key <value>\n  ctox ticket knowledge-load --ticket-key <key> [--domains <csv>]\n  ctox ticket monitoring-ingest --system <name> --snapshot-json <json> [--key <value>] [--title <text>] [--summary <text>] [--status <value>]\n  ctox ticket access-request-put --system <name> --title <title> --body <text> [--required-scopes <csv>] [--secret-refs <csv>] [--channels <csv>] [--skill <name>] [--metadata-json <json>] [--publish]\n  ctox ticket internal-work-put --system <name> --kind <kind> --title <title> --body <text> [--skill <name>] [--metadata-json <json>] [--publish]\n  ctox ticket internal-work-show --work-id <id>\n  ctox ticket internal-work-publish --work-id <id>\n  ctox ticket internal-work-assign --work-id <id> --assignee <name> [--assigned-by <actor>] [--rationale <text>]\n  ctox ticket internal-work-note --work-id <id> --body <text> [--authored-by <actor>] [--visibility <internal|public>]\n  ctox ticket internal-work-transition --work-id <id> --state <value> [--transitioned-by <actor>] [--note <text>] [--visibility <internal|public>]\n  ctox ticket internal-work-list [--system <name>] [--state <value>] [--limit <n>]\n  ctox ticket take [--lease-owner <owner>] [--limit <n>]\n  ctox ticket ack --status <handled|failed|duplicate|blocked> <event-key>...\n  ctox ticket list [--system <name>] [--limit <n>]\n  ctox ticket show --ticket-key <key>\n  ctox ticket history --ticket-key <key> [--limit <n>]\n  ctox ticket label-set --ticket-key <key> --label <label> [--assigned-by <actor>] [--rationale <text>] [--evidence-json <json>]\n  ctox ticket label-show --ticket-key <key>\n  ctox ticket bundle-put --label <label> --runbook-id <id> --policy-id <id> [--runbook-version <v>] [--policy-version <v>] [--approval-mode <mode>] [--autonomy-level <level>] [--verification-profile-id <id>] [--writeback-profile-id <id>] [--support-mode <mode>] [--risk-level <level>] [--actions <json-array>] [--notes <text>]\n  ctox ticket bundle-list\n  ctox ticket autonomy-grant-set --label <label> --approval-mode <mode> --autonomy-level <level> [--bundle-version <n>] [--approved-by <actor>] [--candidate-id <id>] [--rationale <text>]\n  ctox ticket autonomy-grant-list\n  ctox ticket dry-run --ticket-key <key> [--understanding <text>] [--risk-level <level>]\n  ctox ticket cases [--ticket-key <key>] [--limit <n>]\n  ctox ticket case-show --case-id <id>\n  ctox ticket approve --case-id <id> --status <approved|rejected> [--decided-by <actor>] [--rationale <text>]\n  ctox ticket execute --case-id <id> --summary <text>\n  ctox ticket verify --case-id <id> --status <passed|failed> [--summary <text>]\n  ctox ticket clarification-request --case-id <id> --question <text> [--target-type requester|owner|internal] [--target-channel ticket|email|jami|tui] [--missing-inputs <csv>] [--publish-reviewed]\n  ctox ticket clarification-resolve --clarification-id <id> --response-key <key> [--body <text>]\n  ctox ticket learn-candidate-create --case-id <id> --summary <text> [--actions <json-array>] [--evidence-json <json>]\n  ctox ticket learn-candidate-list [--label <label>] [--status <value>] [--limit <n>]\n  ctox ticket learn-candidate-decide --candidate-id <id> --status <approved|rejected> [--decided-by <actor>] [--notes <text>] [--promote-autonomy-level <level>]\n  ctox ticket writeback-comment --case-id <id> --body <text> [--internal]\n  ctox ticket writeback-transition --case-id <id> --state <value> [--body <text>] [--internal]\n  ctox ticket close --case-id <id> [--summary <text>]\n  ctox ticket audit [--ticket-key <key>] [--limit <n>]\n  ctox ticket local <subcommand> ..."
        ),
    }
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

pub(crate) fn put_ticket_self_work_item(
    root: &Path,
    input: TicketSelfWorkUpsertInput,
    publish: bool,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = upsert_ticket_self_work_item_internal(
        &mut conn,
        TicketSelfWorkUpsertInput {
            source_system: input.source_system,
            kind: input.kind,
            title: input.title,
            body_text: input.body_text,
            state: if publish {
                "publishing".to_string()
            } else {
                input.state
            },
            metadata: input.metadata,
        },
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_item_upsert",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "state": item.state,
                "remote_ticket_id": item.remote_ticket_id,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_created",
            title: "Ticket internal work item created",
            body_text: &item.title,
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
                "remote_ticket_id": item.remote_ticket_id,
            }),
        },
    );
    if let Err(err) = enforce_ticket_self_work_spawn(&conn, &item) {
        let now = Utc::now().to_rfc3339();
        let fallback_state = if item.kind.to_ascii_lowercase().contains("review") {
            "failed"
        } else {
            "blocked"
        };
        let fallback_reason = if fallback_state == "failed" {
            "ticket_self_work_spawn_rejected_terminal"
        } else {
            "ticket_self_work_spawn_rejected"
        };
        let failure_reason = err.to_string();
        let transition_result = enforce_ticket_self_work_state_transition(
            &conn,
            &item.work_id,
            &item.state,
            fallback_state,
            "ctox-core-spawn-gate",
            fallback_reason,
            if fallback_state == "failed" {
                Some(failure_reason.as_str())
            } else {
                None
            },
            None,
        );
        if let Err(transition_err) = transition_result {
            anyhow::bail!(
                "core spawn gate rejected ticket internal work `{}` ({}), and core state guard rejected fallback `{}` transition: {}; original spawn rejection: {}",
                item.work_id,
                item.kind,
                fallback_state,
                transition_err,
                err
            );
        }
        let _ = conn.execute(
            r#"
            UPDATE ticket_self_work_items
            SET state = ?2, updated_at = ?3
            WHERE work_id = ?1
            "#,
            params![&item.work_id, fallback_state, now],
        );
        clear_ticket_self_work_list_cache();
        anyhow::bail!(
            "core spawn gate rejected ticket internal work `{}` ({}): {}",
            item.work_id,
            item.kind,
            err
        );
    }
    if publish {
        publish_ticket_self_work_item(root, &item.work_id)
    } else {
        Ok(item)
    }
}

fn enforce_ticket_self_work_spawn(conn: &Connection, item: &TicketSelfWorkItemView) -> Result<()> {
    let thread_key = metadata_string_value(&item.metadata, "thread_key")
        .or_else(|| metadata_string_value(&item.metadata, "queue_thread_key"))
        .unwrap_or_else(|| item.source_system.clone());
    let (parent_entity_type, parent_entity_id) = if let Some(parent_work_id) =
        metadata_string_value(&item.metadata, "parent_work_id")
            .or_else(|| metadata_string_value(&item.metadata, "ticket_self_work_id"))
    {
        ("WorkItem".to_string(), parent_work_id)
    } else if let Some(queue_message_key) =
        metadata_string_value(&item.metadata, "queue_message_key")
    {
        ("QueueTask".to_string(), queue_message_key)
    } else if let Some(parent_message_key) =
        metadata_string_value(&item.metadata, "parent_message_key")
            .or_else(|| metadata_string_value(&item.metadata, "inbound_message_key"))
    {
        ("Message".to_string(), parent_message_key)
    } else if !thread_key.trim().is_empty() {
        ("Thread".to_string(), thread_key.clone())
    } else {
        ("ControlPlane".to_string(), "ticket-self-work".to_string())
    };
    let (budget_key, max_attempts) =
        ticket_self_work_spawn_budget(&item.kind, &thread_key, &item.metadata);
    let mut edge_metadata = BTreeMap::new();
    edge_metadata.insert("thread_key".to_string(), thread_key);
    edge_metadata.insert("self_work_kind".to_string(), item.kind.clone());
    edge_metadata.insert("source_system".to_string(), item.source_system.clone());
    if let Some(source_label) = metadata_string_value(&item.metadata, "source_label") {
        edge_metadata.insert("source_label".to_string(), source_label);
    }
    if let Some(queue_message_key) = metadata_string_value(&item.metadata, "queue_message_key") {
        edge_metadata.insert("queue_message_key".to_string(), queue_message_key);
    }
    if let Some(workspace_root) = metadata_string_value(&item.metadata, "workspace_root") {
        edge_metadata.insert("workspace_root".to_string(), workspace_root);
    }
    if let Some(run_class) = metadata_string_value(&item.metadata, "core_run_class")
        .or_else(|| metadata_string_value(&item.metadata, "run_class"))
    {
        edge_metadata.insert("core_run_class".to_string(), run_class);
    }
    if let Some(dedupe_key) = metadata_string_value(&item.metadata, "dedupe_key") {
        edge_metadata.insert("dedupe_key".to_string(), dedupe_key);
    }

    enforce_core_spawn(
        conn,
        &CoreSpawnRequest {
            parent_entity_type,
            parent_entity_id,
            child_entity_type: "WorkItem".to_string(),
            child_entity_id: item.work_id.clone(),
            spawn_kind: format!("self-work:{}", item.kind),
            spawn_reason: "ticket_self_work_put".to_string(),
            actor: "ctox-ticket".to_string(),
            checkpoint_key: metadata_string_value(&item.metadata, "dedupe_key"),
            budget_key: Some(budget_key),
            max_attempts: Some(max_attempts),
            metadata: edge_metadata,
        },
    )?;
    Ok(())
}

fn ticket_self_work_spawn_budget(kind: &str, thread_key: &str, metadata: &Value) -> (String, i64) {
    let lowered = kind.to_ascii_lowercase();
    if lowered.contains("review") {
        // A communication thread can carry many independent durable work
        // episodes over its lifetime. Spend the finite review budget against
        // the current parent episode, while leaving all historical spawn
        // edges untouched for audit.
        let episode = metadata_string_value(metadata, "work_episode_id")
            .or_else(|| metadata_string_value(metadata, "parent_work_id"))
            .or_else(|| metadata_string_value(metadata, "ticket_self_work_id"))
            .or_else(|| metadata_string_value(metadata, "queue_message_key"))
            .or_else(|| metadata_string_value(metadata, "parent_message_key"))
            .or_else(|| metadata_string_value(metadata, "inbound_message_key"))
            .or_else(|| metadata_string_value(metadata, "dedupe_key"))
            .unwrap_or_else(|| thread_key.to_string());
        return (format!("review-spawn:{kind}:episode:{episode}"), 5);
    }
    if kind == "founder-communication-rework" {
        let key = metadata_string_value(metadata, "inbound_message_key")
            .or_else(|| metadata_string_value(metadata, "parent_message_key"))
            .unwrap_or_else(|| thread_key.to_string());
        return (format!("founder-rework-spawn:{key}"), 2);
    }
    let key = metadata_string_value(metadata, "dedupe_key").unwrap_or_else(|| {
        format!(
            "{}:{}",
            thread_key,
            item_title_budget_component(metadata).unwrap_or_default()
        )
    });
    (format!("service-self-work-spawn:{kind}:{key}"), 64)
}

fn item_title_budget_component(metadata: &Value) -> Option<String> {
    metadata_string_value(metadata, "title").map(|value| value.chars().take(80).collect())
}

fn metadata_string_value(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn publish_ticket_self_work_item(
    root: &Path,
    work_id: &str,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = conn
        .query_row(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id = ?1
            LIMIT 1
            "#,
            params![work_id],
            map_ticket_self_work_row,
        )
        .optional()?
        .context("ticket internal work item not found")?;
    let adapter = ticket_adapters::adapter_for_system(&item.source_system)
        .context("no adapter available to publish ticket internal work item")?;
    if !adapter.capabilities().can_create_self_work_items {
        anyhow::bail!(
            "ticket adapter {} cannot publish internal work items",
            item.source_system
        );
    }
    if item.remote_ticket_id.is_some() {
        return Ok(item);
    }
    let published = adapter.publish_self_work_item(
        root,
        ticket_protocol::TicketSelfWorkPublishRequest {
            title: &item.title,
            body: &item.body_text,
        },
    )?;
    let published_item = mark_ticket_self_work_published(
        &mut conn,
        &item.work_id,
        published.remote_ticket_id.as_deref(),
        published.remote_locator.as_deref(),
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", published_item.source_system),
            case_id: None,
            actor_type: "adapter",
            action_type: "self_work_item_published",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": published_item.work_id,
                "kind": published_item.kind,
                "remote_ticket_id": published_item.remote_ticket_id,
                "remote_locator": published_item.remote_locator,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_published",
            title: "Ticket internal work item published",
            body_text: &published_item.title,
            message_key: self_work_message_key(&published_item),
            work_id: Some(&published_item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": published_item.source_system,
                "kind": published_item.kind,
                "state": published_item.state,
                "remote_ticket_id": published_item.remote_ticket_id,
                "remote_locator": published_item.remote_locator,
            }),
        },
    );
    Ok(published_item)
}

pub(crate) fn assign_ticket_self_work_item(
    root: &Path,
    work_id: &str,
    assignee: &str,
    assigned_by: &str,
    rationale: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = conn
        .query_row(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id = ?1
            LIMIT 1
            "#,
            params![work_id],
            map_ticket_self_work_row,
        )
        .optional()?
        .context("ticket internal work item not found")?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to assign ticket internal work item")?;
        if !adapter.capabilities().can_assign_self_work_items {
            anyhow::bail!(
                "ticket adapter {} cannot assign internal work items",
                item.source_system
            );
        }
        let result = adapter.assign_self_work_item(
            root,
            ticket_protocol::TicketSelfWorkAssignRequest {
                remote_ticket_id,
                assignee,
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    let assignment = insert_ticket_self_work_assignment(
        &mut conn,
        work_id,
        assignee,
        assigned_by,
        rationale,
        remote_event_ids.first().map(String::as_str),
    )?;
    touch_ticket_self_work_item(&mut conn, work_id)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found after assignment")?;
    let item = hydrate_ticket_self_work_item(&conn, item)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_assigned",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "assigned_to": assignment.assigned_to,
                "assigned_by": assignment.assigned_by,
                "rationale": assignment.rationale,
            }),
        },
    )?;
    Ok(item)
}

pub(crate) fn append_ticket_self_work_note(
    root: &Path,
    work_id: &str,
    body: &str,
    authored_by: &str,
    visibility: &str,
) -> Result<TicketSelfWorkNoteView> {
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to note ticket internal work item")?;
        if !adapter.capabilities().can_append_self_work_notes {
            anyhow::bail!(
                "ticket adapter {} cannot append internal work notes",
                item.source_system
            );
        }
        let result = adapter.append_self_work_note(
            root,
            ticket_protocol::TicketSelfWorkNoteRequest {
                remote_ticket_id,
                body,
                internal: visibility != "public",
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    let note = insert_ticket_self_work_note(
        &mut conn,
        work_id,
        body,
        visibility,
        authored_by,
        remote_event_ids.first().map(String::as_str),
    )?;
    touch_ticket_self_work_item(&mut conn, work_id)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_note_appended",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "visibility": note.visibility,
                "authored_by": note.authored_by,
            }),
        },
    )?;
    Ok(note)
}

pub(crate) fn transition_ticket_self_work_item(
    root: &Path,
    work_id: &str,
    state: &str,
    transitioned_by: &str,
    note: Option<&str>,
    visibility: &str,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    let mut remote_event_ids = Vec::new();
    if let Some(remote_ticket_id) = item.remote_ticket_id.as_deref() {
        let adapter = ticket_adapters::adapter_for_system(&item.source_system)
            .context("no adapter available to transition ticket internal work item")?;
        if !adapter.capabilities().can_transition_self_work_items {
            anyhow::bail!(
                "ticket adapter {} cannot transition internal work items",
                item.source_system
            );
        }
        let result = adapter.transition_self_work_item(
            root,
            ticket_protocol::TicketSelfWorkTransitionRequest {
                remote_ticket_id,
                state,
                note_body: note,
                internal_note: visibility != "public",
            },
        )?;
        remote_event_ids = result.remote_event_ids;
    }
    if let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) {
        let _ = insert_ticket_self_work_note(
            &mut conn,
            work_id,
            note,
            visibility,
            transitioned_by,
            remote_event_ids.first().map(String::as_str),
        )?;
    }
    let item = set_ticket_self_work_state_internal(&mut conn, work_id, state, note)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_transitioned",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "state": item.state,
                "transitioned_by": transitioned_by,
                "visibility": visibility,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_transitioned",
            title: "Ticket internal work state changed",
            body_text: note.unwrap_or(state),
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
                "transitioned_by": transitioned_by,
                "visibility": visibility,
            }),
        },
    );
    Ok(item)
}

pub(crate) fn list_ticket_self_work_items(
    root: &Path,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let db_path = resolve_db_path(root);
    let cache_key = ticket_self_work_list_cache_key(&db_path, system, state, limit);
    let initial_stamp = ticket_self_work_list_cache_stamp(&db_path);
    if let Some(items) = cached_ticket_self_work_list(&cache_key, &initial_stamp) {
        return Ok(items);
    }

    let conn = open_ticket_db(root)?;
    let items = list_ticket_self_work_items_on_conn(&conn, system, state, limit)?;
    drop(conn);
    let cache_key = ticket_self_work_list_cache_key(&db_path, system, state, limit);
    let stamp = ticket_self_work_list_cache_stamp(&db_path);
    #[cfg(test)]
    record_ticket_self_work_list_cache_miss_for_tests(&cache_key);
    if stamp == initial_stamp {
        store_ticket_self_work_list_cache(cache_key, stamp, items.clone());
    }
    Ok(items)
}

fn list_ticket_self_work_items_on_conn(
    conn: &Connection,
    system: Option<&str>,
    state: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE (?1 IS NULL OR source_system = ?1)
          AND (?2 IS NULL OR state = ?2)
        ORDER BY updated_at DESC
        LIMIT ?3
        "#,
    )?;
    let rows = statement.query_map(
        params![system, state, limit as i64],
        map_ticket_self_work_row,
    )?;
    let items = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    hydrate_ticket_self_work_items_with_latest_assignments(conn, items)
}

pub(crate) fn load_ticket_self_work_item(
    root: &Path,
    work_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let conn = open_ticket_db(root)?;
    let item = conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .optional()
    .map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(&conn, item))
        .transpose()
}

fn start_ticket_workflow(
    root: &Path,
    input: TicketWorkflowStartInput,
) -> Result<TicketWorkflowView> {
    let title = input.title.trim();
    let goal = input.goal.trim();
    if title.is_empty() || goal.is_empty() {
        anyhow::bail!("workflow title and goal must be non-empty");
    }
    let scope = input
        .thread_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(input.source_system.trim());
    let normalized_scope = normalize_token(scope);
    let workflow_scope = if normalized_scope.is_empty() {
        "ctox".to_string()
    } else {
        normalized_scope
    };
    let workflow_id = format!(
        "workflow:{}:{}",
        workflow_scope,
        stable_digest(&format!("{}:{}:{}", input.source_system, title, goal))
    );
    let mut metadata = json!({"workflow_id": workflow_id, "workflow_role": WORKFLOW_ROLE_CASE, "workflow_status": "active", "workflow_goal": goal, "dedupe_key": format!("ticket-workflow-case:{}", workflow_id), "source_label": "ticket-workflow", "skill": WORKFLOW_ORCHESTRATOR_SKILL});
    if let Some(object) = metadata.as_object_mut() {
        if let Some(thread_key) = input
            .thread_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("thread_key".to_string(), json!(thread_key));
        }
        if let Some(workspace_root) = input
            .workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workspace_root".to_string(), json!(workspace_root));
        }
        if let Some(priority) = input
            .priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("priority".to_string(), json!(priority));
        }
    }
    let case = put_ticket_self_work_item(
        root,
        TicketSelfWorkUpsertInput {
            source_system: input.source_system.clone(),
            kind: WORKFLOW_CASE_KIND.to_string(),
            title: title.to_string(),
            body_text: goal.to_string(),
            state: "open".to_string(),
            metadata,
        },
        false,
    )?;
    let first_step_title = input
        .first_step_title
        .unwrap_or_else(|| format!("Plan workflow phase: {}", input.first_phase.trim()));
    let first_step_prompt = input.first_step_prompt.unwrap_or_else(|| format!("Create the first executable CTOX workflow delta for this long-running goal.\n\nGoal:\n{}\n\nReturn or apply a workflow delta with bounded follow-up tickets. Do not execute the whole goal in one turn.", goal));
    let first_step = put_ticket_workflow_step(
        root,
        TicketWorkflowStepInput {
            workflow_id: workflow_id.clone(),
            role: WORKFLOW_ROLE_REDUCER.to_string(),
            phase: input.first_phase,
            step_id: Some("phase-0-reducer".to_string()),
            title: first_step_title,
            body_text: first_step_prompt,
            phase_goal: input.first_phase_goal,
            exit_gate: input.first_exit_gate,
            predecessor_work_ids: Vec::new(),
            predecessor_step_ids: Vec::new(),
            skill: input
                .skill
                .or_else(|| Some(WORKFLOW_ORCHESTRATOR_SKILL.to_string())),
            priority: input.priority,
            metadata: json!({ "workflow_created_by": case.work_id }),
        },
    )?;
    if input.queue_now {
        let _ = workflow_mark_step_queue_ready(root, &first_step.work_id)?;
    }
    load_ticket_workflow(root, &workflow_id)?.context("ticket workflow not found after creation")
}

fn put_ticket_workflow_step(
    root: &Path,
    input: TicketWorkflowStepInput,
) -> Result<TicketSelfWorkItemView> {
    let conn = open_ticket_db(root)?;
    let case = load_ticket_workflow_case_internal(&conn, &input.workflow_id)?
        .context("workflow case not found")?;
    drop(conn);
    let role = normalize_workflow_role(&input.role)?;
    let phase = input.phase.trim();
    let title = input.title.trim();
    let body_text = input.body_text.trim();
    if phase.is_empty() || title.is_empty() || body_text.is_empty() {
        anyhow::bail!("workflow step phase, title, and body must be non-empty");
    }
    let (mut predecessor_work_ids, inferred_step_ids) =
        split_workflow_predecessor_refs(input.predecessor_work_ids);
    let mut predecessor_step_ids = input.predecessor_step_ids;
    predecessor_step_ids.extend(inferred_step_ids);
    dedupe_strings(&mut predecessor_work_ids);
    dedupe_strings(&mut predecessor_step_ids);
    let step_id = input.step_id.unwrap_or_else(|| {
        format!(
            "{}:{}",
            normalize_token(phase),
            stable_digest(&format!("{}:{}", phase, title))
        )
    });
    let workflow_id = input.workflow_id;
    let case_priority = metadata_string_value(&case.metadata, "priority");
    let status = if predecessor_work_ids.is_empty() && predecessor_step_ids.is_empty() {
        WORKFLOW_STEP_STATUS_READY
    } else {
        WORKFLOW_STEP_STATUS_WAITING
    };
    let mut metadata = if input.metadata.is_object() {
        input.metadata
    } else {
        json!({})
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("workflow_id".to_string(), json!(workflow_id.clone()));
        object.insert("workflow_role".to_string(), json!(role.clone()));
        object.insert("workflow_step_id".to_string(), json!(step_id.clone()));
        object.insert("workflow_phase".to_string(), json!(phase));
        object.insert("workflow_step_status".to_string(), json!(status));
        object.insert(
            "workflow_predecessor_work_ids".to_string(),
            json!(predecessor_work_ids),
        );
        object.insert(
            "workflow_predecessor_step_ids".to_string(),
            json!(predecessor_step_ids),
        );
        object.insert("parent_work_id".to_string(), json!(case.work_id));
        object.insert("source_label".to_string(), json!("ticket-workflow"));
        object.insert(
            "dedupe_key".to_string(),
            json!(format!("ticket-workflow-step:{}:{}", workflow_id, step_id)),
        );
        if let Some(thread_key) = metadata_string_value(&case.metadata, "thread_key") {
            object.insert("thread_key".to_string(), json!(thread_key));
        }
        if let Some(workspace_root) = metadata_string_value(&case.metadata, "workspace_root") {
            object.insert("workspace_root".to_string(), json!(workspace_root));
        }
        if let Some(priority) = input
            .priority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| case_priority.as_deref().map(str::trim))
        {
            object.insert("priority".to_string(), json!(priority));
        }
        if let Some(phase_goal) = input
            .phase_goal
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workflow_phase_goal".to_string(), json!(phase_goal));
        }
        if let Some(exit_gate) = input
            .exit_gate
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("workflow_exit_gate".to_string(), json!(exit_gate));
        }
        let skill = input.skill.or_else(|| {
            if role == WORKFLOW_ROLE_REDUCER {
                Some(WORKFLOW_ORCHESTRATOR_SKILL.to_string())
            } else {
                None
            }
        });
        if let Some(skill) = skill
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            object.insert("skill".to_string(), json!(skill));
        }
    }
    let step_dedupe_key = format!("ticket-workflow-step:{}:{}", workflow_id, step_id);
    let prospective_work_id = format!(
        "self-work:{}:{}",
        case.source_system,
        stable_digest(&step_dedupe_key)
    );
    {
        let conn = open_ticket_db(root)?;
        let is_new_step = load_ticket_self_work_item_raw(&conn, &prospective_work_id)?.is_none();
        if is_new_step {
            let existing_count = count_ticket_workflow_steps_internal(&conn, &workflow_id)?;
            if existing_count as usize + 1 > WORKFLOW_MAX_STEPS_PER_WORKFLOW {
                anyhow::bail!(
                    "workflow `{}` already has {} steps, at the per-workflow ceiling of {}; refusing to materialize step `{}`",
                    workflow_id,
                    existing_count,
                    WORKFLOW_MAX_STEPS_PER_WORKFLOW,
                    step_id
                );
            }
        }
    }
    put_ticket_self_work_item(
        root,
        TicketSelfWorkUpsertInput {
            source_system: case.source_system,
            kind: WORKFLOW_STEP_KIND.to_string(),
            title: title.to_string(),
            body_text: body_text.to_string(),
            state: "open".to_string(),
            metadata,
        },
        false,
    )
}

fn apply_ticket_workflow_delta(
    root: &Path,
    workflow_id: &str,
    delta_value: Value,
    queue_now: bool,
) -> Result<Value> {
    let delta: WorkflowDelta = serde_json::from_value(delta_value)
        .context("workflow delta must match the CTOX workflow delta schema")?;
    if delta.create_steps.len() > WORKFLOW_MATERIALIZE_DEFAULT_LIMIT {
        anyhow::bail!(
            "workflow delta creates too many steps at once ({} > {})",
            delta.create_steps.len(),
            WORKFLOW_MATERIALIZE_DEFAULT_LIMIT
        );
    }
    let mut updated = Vec::new();
    for update in delta.update_steps {
        let item = locate_workflow_step(
            root,
            workflow_id,
            update.work_id.as_deref(),
            update.step_id.as_deref(),
        )?
        .context("workflow delta update references an unknown step")?;
        let mut metadata_update = json!({});
        if let Some(object) = metadata_update.as_object_mut() {
            if let Some(status) = update
                .workflow_step_status
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                object.insert("workflow_step_status".to_string(), json!(status));
            }
            if !update.evidence.is_null() {
                object.insert("workflow_step_evidence".to_string(), update.evidence);
            }
            if let Some(extra) = update.metadata.as_object() {
                for (key, value) in extra {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        let merged = merge_ticket_self_work_metadata(root, &item.work_id, metadata_update)?;
        if let Some(note) = update
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let _ = append_ticket_self_work_note(
                root,
                &item.work_id,
                note,
                WORKFLOW_ORCHESTRATOR_SKILL,
                "internal",
            );
        }
        updated.push(merged.work_id);
    }
    let queue_now_refs = delta.queue_now.clone();
    let mut created_items = Vec::new();
    for create in delta.create_steps {
        if create.prompt.trim().is_empty() {
            anyhow::bail!(
                "workflow delta create_steps entry `{}` has an empty prompt",
                create.title
            );
        }
        let mut predecessor_refs = create.predecessors;
        predecessor_refs.extend(create.predecessor_work_ids);
        let (predecessor_work_ids, inferred_step_ids) =
            split_workflow_predecessor_refs(predecessor_refs);
        let mut predecessor_step_ids = create.predecessor_steps;
        predecessor_step_ids.extend(create.predecessor_step_ids);
        predecessor_step_ids.extend(inferred_step_ids);
        dedupe_strings(&mut predecessor_step_ids);
        let item = put_ticket_workflow_step(
            root,
            TicketWorkflowStepInput {
                workflow_id: workflow_id.to_string(),
                role: create
                    .role
                    .unwrap_or_else(|| WORKFLOW_ROLE_LEAF.to_string()),
                phase: create.phase,
                step_id: create.step_id,
                title: create.title,
                body_text: create.prompt,
                phase_goal: create.phase_goal,
                exit_gate: create.exit_gate,
                predecessor_work_ids,
                predecessor_step_ids,
                skill: create.skill,
                priority: create.priority,
                metadata: create.metadata,
            },
        )?;
        created_items.push(item);
    }
    let mut explicitly_queued = Vec::new();
    for reference in queue_now_refs {
        if let Some(item) =
            locate_workflow_step(root, workflow_id, Some(&reference), Some(&reference))?
        {
            if workflow_step_ready_for_queue(root, &item.work_id)? {
                explicitly_queued.push(workflow_mark_step_queue_ready(root, &item.work_id)?);
            }
        }
    }
    let materialized = if queue_now || !explicitly_queued.is_empty() {
        materialize_ready_workflow_steps_for_workflow(
            root,
            Some(workflow_id),
            WORKFLOW_MATERIALIZE_DEFAULT_LIMIT,
        )?
    } else {
        TicketWorkflowMaterializeResult {
            workflow_id: Some(workflow_id.to_string()),
            materialized_count: 0,
            materialized: Vec::new(),
            skipped_count: 0,
        }
    };
    Ok(
        json!({ "workflow_id": workflow_id, "phase_decision": delta.phase_decision, "updated_work_ids": updated, "created_work_ids": created_items.iter().map(|item| item.work_id.clone()).collect::<Vec<_>>(), "explicitly_queued_work_ids": explicitly_queued.iter().map(|item| item.work_id.clone()).collect::<Vec<_>>(), "materialized": materialized }),
    )
}

pub(crate) fn materialize_ready_workflow_steps(
    root: &Path,
    limit: usize,
) -> Result<TicketWorkflowMaterializeResult> {
    materialize_ready_workflow_steps_for_workflow(root, None, limit)
}

pub(crate) fn materialize_ready_workflow_steps_for_workflow(
    root: &Path,
    workflow_id: Option<&str>,
    limit: usize,
) -> Result<TicketWorkflowMaterializeResult> {
    if limit == 0 {
        return Ok(TicketWorkflowMaterializeResult {
            workflow_id: workflow_id.map(ToOwned::to_owned),
            materialized_count: 0,
            materialized: Vec::new(),
            skipped_count: 0,
        });
    }
    let db_path = resolve_db_path(root);
    let cache_key = ticket_workflow_materialize_cache_key(&db_path, workflow_id, limit);
    let initial_stamp = ticket_store_change_stamp_for_path(&db_path);
    if let Some(result) = cached_ticket_workflow_materialize_result(&cache_key, &initial_stamp) {
        return Ok(result);
    }

    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND (?2 IS NULL OR json_extract(metadata_json, '$.workflow_id') = ?2) ORDER BY created_at ASC LIMIT 512"#)?;
    let rows = statement.query_map(
        params![WORKFLOW_STEP_KIND, workflow_id],
        map_ticket_self_work_row,
    )?;
    let candidates = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    drop(statement);
    let mut ready_work_ids = Vec::new();
    let mut skipped_count = 0usize;
    for candidate in candidates {
        let item = hydrate_ticket_self_work_item(&conn, candidate)?;
        if workflow_step_is_runnable_state(&item.state)
            && workflow_step_ready_internal(&conn, &item)?
        {
            ready_work_ids.push(item.work_id.clone());
            if ready_work_ids.len() >= limit {
                break;
            }
        } else {
            skipped_count += 1;
        }
    }
    drop(conn);
    let mut materialized = Vec::new();
    for work_id in ready_work_ids {
        materialized.push(workflow_mark_step_queue_ready(root, &work_id)?);
    }
    let result = TicketWorkflowMaterializeResult {
        workflow_id: workflow_id.map(ToOwned::to_owned),
        materialized_count: materialized.len(),
        materialized,
        skipped_count,
    };
    let final_stamp = ticket_store_change_stamp_for_path(&db_path);
    #[cfg(test)]
    record_ticket_workflow_materialize_cache_miss_for_tests(&cache_key);
    if result.materialized.is_empty() && final_stamp == initial_stamp {
        store_ticket_workflow_materialize_cache(cache_key, final_stamp, result.clone());
    }
    Ok(result)
}

pub(crate) fn workflow_prompt_block(root: &Path, limit: usize) -> Result<Option<String>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND state NOT IN ('closed', 'done', 'completed', 'handled', 'cancelled', 'superseded', 'failed') ORDER BY updated_at DESC LIMIT ?2"#)?;
    let rows = statement.query_map(
        params![WORKFLOW_CASE_KIND, limit.max(1) as i64],
        map_ticket_self_work_row,
    )?;
    let cases = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    drop(statement);
    if cases.is_empty() {
        return Ok(None);
    }
    let mut lines = vec!["ticket_workflows:".to_string()];
    for case in cases {
        let case = hydrate_ticket_self_work_item(&conn, case)?;
        let Some(workflow_id) = metadata_string_value(&case.metadata, "workflow_id") else {
            continue;
        };
        let steps = list_ticket_workflow_steps_internal(&conn, &workflow_id, 64)?;
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        let mut running = Vec::new();
        for step in &steps {
            if matches!(
                normalize_token(&step.state).as_str(),
                "queued" | "published" | "running" | "executing" | "in_progress"
            ) {
                running.push(step);
            } else if workflow_step_is_runnable_state(&step.state)
                && workflow_step_ready_internal(&conn, step).unwrap_or(false)
            {
                ready.push(step);
            } else if !workflow_step_satisfied(step) {
                waiting.push(step);
            }
        }
        lines.push(format!(
            "- {} [{}] state={} ready={} running={} waiting={} goal={}",
            workflow_id,
            workflow_clip(&case.title, 80),
            case.state,
            ready.len(),
            running.len(),
            waiting.len(),
            workflow_clip(
                &metadata_string_value(&case.metadata, "workflow_goal")
                    .unwrap_or_else(|| case.body_text.clone()),
                120
            )
        ));
        for step in ready.iter().take(3) {
            lines.push(format!(
                "  ready: {} phase={} work_id={} title={}",
                workflow_step_id(step),
                workflow_step_phase(step),
                step.work_id,
                workflow_clip(&step.title, 90)
            ));
        }
        for step in running.iter().take(3) {
            lines.push(format!(
                "  active: {} phase={} state={} work_id={} title={}",
                workflow_step_id(step),
                workflow_step_phase(step),
                step.state,
                step.work_id,
                workflow_clip(&step.title, 90)
            ));
        }
    }
    if lines.len() == 1 {
        Ok(None)
    } else {
        Ok(Some(lines.join("\n")))
    }
}

fn load_ticket_workflow(root: &Path, workflow_id: &str) -> Result<Option<TicketWorkflowView>> {
    let conn = open_ticket_db(root)?;
    let case = load_ticket_workflow_case_internal(&conn, workflow_id)?;
    let steps = list_ticket_workflow_steps_internal(&conn, workflow_id, 512)?;
    if case.is_none() && steps.is_empty() {
        return Ok(None);
    }
    let mut step_views = Vec::new();
    let mut ready_steps = Vec::new();
    let mut waiting_steps = Vec::new();
    for step in steps {
        let ready = workflow_step_is_runnable_state(&step.state)
            && workflow_step_ready_internal(&conn, &step)?;
        let step_id = workflow_step_id(&step);
        if ready {
            ready_steps.push(step_id.clone());
        } else if !workflow_step_satisfied(&step) {
            waiting_steps.push(step_id.clone());
        }
        step_views.push(TicketWorkflowStepView {
            work_id: step.work_id.clone(),
            step_id,
            role: workflow_step_role(&step),
            phase: workflow_step_phase(&step),
            title: step.title.clone(),
            state: step.state.clone(),
            status: workflow_step_status(&step),
            predecessor_work_ids: workflow_predecessor_work_ids(&step.metadata),
            predecessor_step_ids: workflow_predecessor_step_ids(&step.metadata),
            ready,
            suggested_skill: step.suggested_skill.clone(),
            updated_at: step.updated_at.clone(),
        });
    }
    let (title, goal, state, case_work_id) = if let Some(case) = case {
        (
            case.title,
            metadata_string_value(&case.metadata, "workflow_goal").or(Some(case.body_text)),
            case.state,
            Some(case.work_id),
        )
    } else {
        (
            workflow_id.to_string(),
            None,
            "missing-case".to_string(),
            None,
        )
    };
    Ok(Some(TicketWorkflowView {
        workflow_id: workflow_id.to_string(),
        title,
        goal,
        state,
        case_work_id,
        steps: step_views,
        ready_steps,
        waiting_steps,
    }))
}

fn load_ticket_workflow_case_internal(
    conn: &Connection,
    workflow_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 ORDER BY created_at ASC LIMIT 1"#, params![WORKFLOW_CASE_KIND, workflow_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(conn, item))
        .transpose()
}
fn list_ticket_workflow_steps_internal(
    conn: &Connection,
    workflow_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkItemView>> {
    let mut statement = conn.prepare(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 ORDER BY created_at ASC LIMIT ?3"#)?;
    let rows = statement.query_map(
        params![WORKFLOW_STEP_KIND, workflow_id, limit as i64],
        map_ticket_self_work_row,
    )?;
    let steps = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    steps
        .into_iter()
        .map(|item| hydrate_ticket_self_work_item(conn, item))
        .collect()
}
fn count_ticket_workflow_steps_internal(conn: &Connection, workflow_id: &str) -> Result<i64> {
    conn.query_row(
        r#"SELECT COUNT(*) FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2"#,
        params![WORKFLOW_STEP_KIND, workflow_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(anyhow::Error::from)
}
fn locate_workflow_step(
    root: &Path,
    workflow_id: &str,
    work_id: Option<&str>,
    step_id: Option<&str>,
) -> Result<Option<TicketSelfWorkItemView>> {
    let conn = open_ticket_db(root)?;
    if let Some(work_id) = work_id.map(str::trim).filter(|value| !value.is_empty()) {
        if let Some(item) = load_ticket_self_work_item_raw(&conn, work_id)? {
            if item.kind == WORKFLOW_STEP_KIND
                && metadata_string_value(&item.metadata, "workflow_id").as_deref()
                    == Some(workflow_id)
            {
                return hydrate_ticket_self_work_item(&conn, item).map(Some);
            }
        }
    }
    let Some(step_id) = step_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 AND json_extract(metadata_json, '$.workflow_step_id') = ?3 ORDER BY created_at DESC LIMIT 1"#, params![WORKFLOW_STEP_KIND, workflow_id, step_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(&conn, item))
        .transpose()
}
fn workflow_mark_step_queue_ready(root: &Path, work_id: &str) -> Result<TicketSelfWorkItemView> {
    if !workflow_step_ready_for_queue(root, work_id)? {
        anyhow::bail!("workflow step `{work_id}` is not ready for queue materialization");
    }
    let mut item = merge_ticket_self_work_metadata(
        root,
        work_id,
        json!({"workflow_step_status": WORKFLOW_STEP_STATUS_READY}),
    )?;
    if item.assigned_to.as_deref() != Some("self") {
        item = assign_ticket_self_work_item(
            root,
            work_id,
            "self",
            WORKFLOW_ORCHESTRATOR_SKILL,
            Some("workflow predecessor conditions are satisfied"),
        )?;
    }
    match normalize_token(&item.state).as_str() {
        "queued" | "published" | "running" | "executing" | "in_progress" => Ok(item),
        _ => transition_ticket_self_work_item(
            root,
            work_id,
            "queued",
            WORKFLOW_ORCHESTRATOR_SKILL,
            Some("Workflow predecessor conditions are satisfied; queued as the next bounded step."),
            "internal",
        ),
    }
}
fn workflow_step_ready_for_queue(root: &Path, work_id: &str) -> Result<bool> {
    let conn = open_ticket_db(root)?;
    let Some(item) = load_ticket_self_work_item_raw(&conn, work_id)? else {
        return Ok(false);
    };
    Ok(workflow_step_is_runnable_state(&item.state) && workflow_step_ready_internal(&conn, &item)?)
}
fn workflow_step_ready_internal(conn: &Connection, item: &TicketSelfWorkItemView) -> Result<bool> {
    if item.kind != WORKFLOW_STEP_KIND || workflow_step_satisfied(item) {
        return Ok(false);
    }
    for work_id in workflow_predecessor_work_ids(&item.metadata) {
        let Some(predecessor) = load_ticket_self_work_item_raw(conn, &work_id)? else {
            return Ok(false);
        };
        if !workflow_step_satisfied(&predecessor) {
            return Ok(false);
        }
    }
    let workflow_id = metadata_string_value(&item.metadata, "workflow_id").unwrap_or_default();
    for step_id in workflow_predecessor_step_ids(&item.metadata) {
        let Some(predecessor) = locate_workflow_step_in_conn(conn, &workflow_id, &step_id)? else {
            return Ok(false);
        };
        if !workflow_step_satisfied(&predecessor) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn locate_workflow_step_in_conn(
    conn: &Connection,
    workflow_id: &str,
    step_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    let item = conn.query_row(r#"SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at FROM ticket_self_work_items WHERE kind = ?1 AND json_extract(metadata_json, '$.workflow_id') = ?2 AND json_extract(metadata_json, '$.workflow_step_id') = ?3 ORDER BY created_at DESC LIMIT 1"#, params![WORKFLOW_STEP_KIND, workflow_id, step_id], map_ticket_self_work_row).optional().map_err(anyhow::Error::from)?;
    item.map(|item| hydrate_ticket_self_work_item(conn, item))
        .transpose()
}
fn merge_ticket_self_work_metadata(
    root: &Path,
    work_id: &str,
    update: Value,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found")?;
    let mut metadata = if item.metadata.is_object() {
        item.metadata
    } else {
        json!({})
    };
    let updated_keys = update
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    merge_json_object_values(&mut metadata, &update);
    conn.execute(r#"UPDATE ticket_self_work_items SET metadata_json = ?2, updated_at = ?3 WHERE work_id = ?1"#, params![work_id, serde_json::to_string(&metadata)?, now_iso_string()])?;
    clear_ticket_self_work_list_cache();
    let item = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("ticket internal work item not found after metadata update")?;
    let item = hydrate_ticket_self_work_item(&conn, item)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_metadata_updated",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "updated_keys": updated_keys,
                "workflow_id": metadata_string_value(&item.metadata, "workflow_id"),
                "workflow_step_id": metadata_string_value(&item.metadata, "workflow_step_id"),
            }),
        },
    )?;
    Ok(item)
}
fn merge_json_object_values(target: &mut Value, update: &Value) {
    let (Some(target), Some(update)) = (target.as_object_mut(), update.as_object()) else {
        return;
    };
    for (key, value) in update {
        target.insert(key.clone(), value.clone());
    }
}
fn workflow_step_satisfied(item: &TicketSelfWorkItemView) -> bool {
    matches!(
        normalize_token(&item.state).as_str(),
        "closed" | "done" | "completed" | "handled" | "verified"
    ) || matches!(
        normalize_token(&workflow_step_status(item)).as_str(),
        "verified" | "passed" | "satisfied" | "closed" | "done" | "completed"
    )
}
fn workflow_step_is_runnable_state(state: &str) -> bool {
    matches!(
        normalize_token(state).as_str(),
        "" | "created" | "open" | "blocked" | "restored"
    )
}
fn workflow_step_id(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_step_id")
        .unwrap_or_else(|| item.work_id.clone())
}
fn workflow_step_role(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_role")
        .unwrap_or_else(|| WORKFLOW_ROLE_LEAF.to_string())
}
fn workflow_step_phase(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_phase")
        .unwrap_or_else(|| "unspecified".to_string())
}
fn workflow_step_status(item: &TicketSelfWorkItemView) -> String {
    metadata_string_value(&item.metadata, "workflow_step_status")
        .unwrap_or_else(|| item.state.clone())
}
fn workflow_predecessor_work_ids(metadata: &Value) -> Vec<String> {
    workflow_metadata_strings(metadata, "workflow_predecessor_work_ids")
}
fn workflow_predecessor_step_ids(metadata: &Value) -> Vec<String> {
    workflow_metadata_strings(metadata, "workflow_predecessor_step_ids")
}
fn workflow_metadata_strings(metadata: &Value, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    match metadata.get(key) {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    values.push(text.to_string());
                }
            }
        }
        Some(Value::String(text)) => values.extend(parse_domain_csv(text)),
        _ => {}
    }
    dedupe_strings(&mut values);
    values
}
fn normalize_workflow_role(role: &str) -> Result<String> {
    match normalize_token(role).as_str() {
        "" | "leaf" => Ok(WORKFLOW_ROLE_LEAF.to_string()),
        "reducer" | "planner" | "orchestrator" => Ok(WORKFLOW_ROLE_REDUCER.to_string()),
        other => anyhow::bail!("unsupported workflow role `{other}`"),
    }
}
fn split_workflow_predecessor_refs(values: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut work_ids = Vec::new();
    let mut step_ids = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.starts_with("self-work:") {
            work_ids.push(value.to_string());
        } else {
            step_ids.push(value.to_string());
        }
    }
    dedupe_strings(&mut work_ids);
    dedupe_strings(&mut step_ids);
    (work_ids, step_ids)
}
fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || seen.contains(trimmed) {
            return false;
        }
        seen.insert(trimmed.to_string());
        true
    });
}
fn workflow_clip(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let clipped: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{clipped}...")
}

pub(crate) fn set_ticket_self_work_state(
    root: &Path,
    work_id: &str,
    state: &str,
) -> Result<TicketSelfWorkItemView> {
    set_ticket_self_work_state_with_failure_reason(root, work_id, state, None)
}

pub(crate) fn set_ticket_self_work_state_with_failure_reason(
    root: &Path,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let item = set_ticket_self_work_state_internal(&mut conn, work_id, state, failure_reason)?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "self_work_state_set",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "kind": item.kind,
                "state": item.state,
            }),
        },
    )?;
    record_harness_flow_event_lossy(
        root,
        RecordHarnessFlowEventRequest {
            event_kind: "ticket.self_work_state_set",
            title: "Ticket internal work state set",
            body_text: state,
            message_key: self_work_message_key(&item),
            work_id: Some(&item.work_id),
            ticket_key: None,
            attempt_index: None,
            metadata: json!({
                "source_system": item.source_system,
                "kind": item.kind,
                "state": item.state,
            }),
        },
    );
    Ok(item)
}

pub(crate) fn set_ticket_approval_gate_state_from_authorized_reply(
    root: &Path,
    work_id: &str,
    state: &str,
    approval_message_key: &str,
) -> Result<TicketSelfWorkItemView> {
    let mut conn = open_ticket_db(root)?;
    let existing = load_ticket_self_work_item_raw(&conn, work_id)?
        .context("approval gate internal work item not found")?;
    if existing.kind != "approval-gate" {
        anyhow::bail!("authorized approval reply target is not an approval gate");
    }
    let expected_action = match state {
        "closed" => "approve",
        "failed" => "reject",
        _ => anyhow::bail!("authorized approval reply target state must be closed or failed"),
    };
    let ledger: Option<(String, String, String)> = conn
        .query_row(
            r#"
            SELECT action, sender_address, body_sha256
            FROM ticket_approval_reply_ledger
            WHERE message_key=?1 AND work_id=?2
              AND decision_status IN ('observed', 'applied')
            LIMIT 1
            "#,
            params![approval_message_key, work_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (action, sender_address, body_sha256) =
        ledger.context("authorized approval reply is missing its durable ledger proof")?;
    if action != expected_action
        || sender_address.trim().is_empty()
        || body_sha256.trim().is_empty()
    {
        anyhow::bail!("authorized approval reply ledger proof does not match the requested state");
    }
    let terminal_policy_proof = format!(
        "policy:authorized-approval-reply:{}:{}",
        approval_message_key,
        body_sha256.chars().take(16).collect::<String>()
    );
    let failure_reason =
        (state == "failed").then_some("authorized approval reply rejected the internal work item");
    let item = set_ticket_self_work_state_internal_with_policy(
        &mut conn,
        work_id,
        state,
        failure_reason,
        Some(&terminal_policy_proof),
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &format!("*self-work:{}*", item.source_system),
            case_id: None,
            actor_type: "control_plane",
            action_type: "approval_reply_state_set",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "work_id": item.work_id,
                "state": item.state,
                "approval_message_key": approval_message_key,
                "terminal_policy_proof": terminal_policy_proof,
            }),
        },
    )?;
    Ok(item)
}

fn list_ticket_self_work_assignments(
    root: &Path,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkAssignmentView>> {
    let conn = open_ticket_db(root)?;
    list_ticket_self_work_assignments_internal(&conn, work_id, limit)
}

fn list_ticket_self_work_assignments_internal(
    conn: &Connection,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkAssignmentView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM ticket_self_work_assignments
        WHERE work_id = ?1
        ORDER BY created_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![work_id, limit as i64],
        map_ticket_self_work_assignment_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn list_ticket_self_work_notes(
    root: &Path,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkNoteView>> {
    let conn = open_ticket_db(root)?;
    list_ticket_self_work_notes_internal(&conn, work_id, limit)
}

fn list_ticket_self_work_notes_internal(
    conn: &Connection,
    work_id: &str,
    limit: usize,
) -> Result<Vec<TicketSelfWorkNoteView>> {
    let mut statement = conn.prepare(
        r#"
        SELECT note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        FROM ticket_self_work_notes
        WHERE work_id = ?1
        ORDER BY created_at ASC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(
        params![work_id, limit as i64],
        map_ticket_self_work_note_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_ticket_self_work_item_raw(
    conn: &Connection,
    work_id: &str,
) -> Result<Option<TicketSelfWorkItemView>> {
    conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

pub(crate) fn load_ticket_self_work_items_by_work_id_from_conn(
    conn: &Connection,
    work_ids: &[String],
) -> Result<BTreeMap<String, TicketSelfWorkItemView>> {
    let mut items_by_work_id = BTreeMap::new();
    if work_ids.is_empty() {
        return Ok(items_by_work_id);
    }
    for chunk in work_ids.chunks(500) {
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
            FROM ticket_self_work_items
            WHERE work_id IN ({placeholders})
            "#
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(chunk.iter()), map_ticket_self_work_row)?;
        let items = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)?;
        for item in hydrate_ticket_self_work_items_with_latest_assignments(conn, items)? {
            items_by_work_id.insert(item.work_id.clone(), item);
        }
    }
    Ok(items_by_work_id)
}

fn hydrate_ticket_self_work_item(
    conn: &Connection,
    mut item: TicketSelfWorkItemView,
) -> Result<TicketSelfWorkItemView> {
    if let Some(assignment) = list_ticket_self_work_assignments_internal(conn, &item.work_id, 1)?
        .into_iter()
        .next()
    {
        item.assigned_to = Some(assignment.assigned_to);
        item.assigned_by = Some(assignment.assigned_by);
        item.assigned_at = Some(assignment.created_at);
    }
    Ok(item)
}

fn hydrate_ticket_self_work_items_with_latest_assignments(
    conn: &Connection,
    mut items: Vec<TicketSelfWorkItemView>,
) -> Result<Vec<TicketSelfWorkItemView>> {
    if items.is_empty() {
        return Ok(items);
    }
    let work_ids = items
        .iter()
        .map(|item| item.work_id.clone())
        .collect::<Vec<_>>();
    let placeholders = std::iter::repeat("?")
        .take(work_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM (
            SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY work_id
                       ORDER BY created_at DESC, assignment_id DESC
                   ) AS assignment_rank
            FROM ticket_self_work_assignments
            WHERE work_id IN ({placeholders})
        )
        WHERE assignment_rank = 1
        "#
    );
    #[cfg(test)]
    record_ticket_self_work_assignment_batch_hydration_for_tests();
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(work_ids.iter()), |row| {
        map_ticket_self_work_assignment_row(row)
    })?;
    let latest = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|assignment| (assignment.work_id.clone(), assignment))
        .collect::<BTreeMap<_, _>>();
    for item in &mut items {
        if let Some(assignment) = latest.get(&item.work_id) {
            item.assigned_to = Some(assignment.assigned_to.clone());
            item.assigned_by = Some(assignment.assigned_by.clone());
            item.assigned_at = Some(assignment.created_at.clone());
        }
    }
    Ok(items)
}

fn self_work_message_key(item: &TicketSelfWorkItemView) -> Option<&str> {
    ["queue_message_key", "parent_message_key", "message_key"]
        .iter()
        .find_map(|key| {
            item.metadata
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

fn enforce_ticket_self_work_state_transition(
    conn: &Connection,
    work_id: &str,
    from_state: &str,
    to_state: &str,
    actor: &str,
    reason: &str,
    failure_reason: Option<&str>,
    terminal_policy_proof: Option<&str>,
) -> Result<()> {
    let from_core = ticket_self_work_core_state(from_state)?;
    let to_core = ticket_self_work_core_state(to_state)?;
    if to_core == CoreState::Closed && work_item_has_terminal_success_proof(conn, work_id)? {
        return Ok(());
    }
    if to_core == CoreState::ReworkRequired && work_item_has_rework_witness_proof(conn, work_id)? {
        return Ok(());
    }
    let mut metadata = BTreeMap::new();
    metadata.insert("from_state".to_string(), from_state.to_string());
    metadata.insert("to_state".to_string(), to_state.to_string());
    metadata.insert("reason".to_string(), reason.to_string());
    if let Some(policy_proof) = terminal_policy_proof
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        metadata.insert(
            "terminal_policy_proof".to_string(),
            policy_proof.to_string(),
        );
    }
    if to_core == CoreState::Failed {
        let failure_reason = failure_reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context(
                "ticket internal work failed transition requires a non-empty failure reason",
            )?;
        metadata.insert("failure_reason".to_string(), failure_reason.to_string());
        metadata.insert(
            "failure_class".to_string(),
            "ticket_self_work_failure".to_string(),
        );
    }
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: work_id.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: from_core,
            to_state: to_core,
            event: ticket_self_work_core_event(to_state),
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs {
                verification_id: if to_core == CoreState::Closed {
                    Some(format!("ticket-self-work-state-close:{work_id}"))
                } else {
                    None
                },
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(())
}

fn work_item_has_rework_witness_proof(conn: &Connection, work_id: &str) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'WorkItem'
          AND entity_id = ?1
          AND to_state = 'ReworkRequired'
          AND accepted = 1
          AND (
                request_json LIKE '%"review_checkpoint":"true"%'
             OR request_json LIKE '%"validator_rework":"true"%'
          )
        "#,
        params![work_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn work_item_has_terminal_success_proof(conn: &Connection, work_id: &str) -> Result<bool> {
    ensure_core_transition_guard_schema(conn)?;
    let count = conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM ctox_core_transition_proofs
        WHERE entity_type = 'WorkItem'
          AND entity_id = ?1
          AND to_state = 'Closed'
          AND accepted = 1
          AND (
                request_json LIKE '%"reviewed_work_terminal_success":"true"%'
             OR request_json LIKE '%"terminal_policy_proof"%'
          )
        "#,
        params![work_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn ticket_self_work_core_state(raw: &str) -> Result<CoreState> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "created" => Ok(CoreState::Created),
        "open" | "queued" | "restored" | "publishing" => Ok(CoreState::Planned),
        "published" | "running" | "executing" | "in_progress" => Ok(CoreState::Executing),
        "awaiting_review" | "review" | "reviewing" => Ok(CoreState::AwaitingReview),
        "rework_required" | "review_rework" | "rework" => Ok(CoreState::ReworkRequired),
        "awaiting_verification" | "verification" => Ok(CoreState::AwaitingVerification),
        "verified" => Ok(CoreState::Verified),
        "blocked" | "spilled" => Ok(CoreState::Blocked),
        "failed" => Ok(CoreState::Failed),
        "closed" | "done" | "completed" | "handled" => Ok(CoreState::Closed),
        "cancelled" | "superseded" => Ok(CoreState::Superseded),
        other => {
            anyhow::bail!("ticket internal work state is not mapped to core state machine: {other}")
        }
    }
}

fn ticket_self_work_core_event(state: &str) -> CoreEvent {
    match state.trim().to_ascii_lowercase().as_str() {
        "open" | "queued" | "restored" | "publishing" => CoreEvent::Plan,
        "published" | "running" | "executing" | "in_progress" => CoreEvent::Execute,
        "awaiting_review" | "review" | "reviewing" => CoreEvent::RequestReview,
        "rework_required" | "review_rework" | "rework" => CoreEvent::RequireRework,
        "awaiting_verification" | "verification" => CoreEvent::Verify,
        "verified" => CoreEvent::Verify,
        "blocked" | "spilled" => CoreEvent::Block,
        "failed" => CoreEvent::Fail,
        "closed" | "done" | "completed" | "handled" => CoreEvent::Close,
        "cancelled" | "superseded" => CoreEvent::Supersede,
        _ => CoreEvent::CreateTicket,
    }
}

fn set_ticket_self_work_state_internal(
    conn: &mut Connection,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    set_ticket_self_work_state_internal_with_policy(conn, work_id, state, failure_reason, None)
}

fn set_ticket_self_work_state_internal_with_policy(
    conn: &mut Connection,
    work_id: &str,
    state: &str,
    failure_reason: Option<&str>,
    terminal_policy_proof: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let existing = load_ticket_self_work_item_raw(conn, work_id)?
        .context("ticket internal work item not found")?;
    enforce_ticket_self_work_state_transition(
        conn,
        work_id,
        &existing.state,
        state,
        "ctox-ticket",
        "set_ticket_self_work_state",
        failure_reason,
        terminal_policy_proof,
    )?;
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE ticket_self_work_items
        SET state = ?2,
            updated_at = ?3
        WHERE work_id = ?1
        "#,
        params![work_id, state, now],
    )?;
    clear_ticket_self_work_list_cache();
    let item = load_ticket_self_work_item_raw(conn, work_id)?
        .context("ticket internal work item not found")?;
    hydrate_ticket_self_work_item(conn, item)
}

fn touch_ticket_self_work_item(conn: &mut Connection, work_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE ticket_self_work_items SET updated_at = ?2 WHERE work_id = ?1",
        params![work_id, now_iso_string()],
    )?;
    clear_ticket_self_work_list_cache();
    Ok(())
}

fn insert_ticket_self_work_assignment(
    conn: &mut Connection,
    work_id: &str,
    assigned_to: &str,
    assigned_by: &str,
    rationale: Option<&str>,
    remote_event_id: Option<&str>,
) -> Result<TicketSelfWorkAssignmentView> {
    let now = now_iso_string();
    let assignment_id = format!(
        "swa:{}:{}",
        work_id,
        stable_digest(&(assigned_to.to_string() + now.as_str()))
    );
    conn.execute(
        r#"
        INSERT INTO ticket_self_work_assignments (
            assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            assignment_id,
            work_id,
            assigned_to.trim(),
            assigned_by.trim(),
            rationale,
            remote_event_id,
            now
        ],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT assignment_id, work_id, assigned_to, assigned_by, rationale, remote_event_id, created_at
        FROM ticket_self_work_assignments
        WHERE assignment_id = ?1
        LIMIT 1
        "#,
        params![assignment_id],
        map_ticket_self_work_assignment_row,
    ).map_err(anyhow::Error::from)
}

fn insert_ticket_self_work_note(
    conn: &mut Connection,
    work_id: &str,
    body: &str,
    visibility: &str,
    authored_by: &str,
    remote_event_id: Option<&str>,
) -> Result<TicketSelfWorkNoteView> {
    let now = now_iso_string();
    let note_id = format!(
        "swn:{}:{}",
        work_id,
        stable_digest(&(body.to_string() + now.as_str()))
    );
    conn.execute(
        r#"
        INSERT INTO ticket_self_work_notes (
            note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            note_id,
            work_id,
            body.trim(),
            visibility.trim(),
            authored_by.trim(),
            remote_event_id,
            now
        ],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT note_id, work_id, body_text, visibility, authored_by, remote_event_id, created_at
        FROM ticket_self_work_notes
        WHERE note_id = ?1
        LIMIT 1
        "#,
        params![note_id],
        map_ticket_self_work_note_row,
    )
    .map_err(anyhow::Error::from)
}

fn put_ticket_knowledge_entry_internal(
    conn: &mut Connection,
    input: TicketKnowledgeUpsertInput,
) -> Result<TicketKnowledgeEntryView> {
    let now = now_iso_string();
    let entry_id = format!(
        "knowledge:{}:{}:{}",
        input.source_system,
        input.domain,
        stable_digest(&input.knowledge_key)
    );
    conn.execute(
        r#"
        INSERT INTO ticket_knowledge_entries (
            entry_id, source_system, domain, knowledge_key, title, summary, status,
            content_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        ON CONFLICT(source_system, domain, knowledge_key) DO UPDATE SET
            title=excluded.title,
            summary=excluded.summary,
            status=excluded.status,
            content_json=excluded.content_json,
            updated_at=excluded.updated_at
        "#,
        params![
            entry_id,
            input.source_system,
            input.domain,
            input.knowledge_key,
            input.title,
            input.summary,
            input.status,
            serde_json::to_string(&input.content)?,
            now,
        ],
    )?;
    conn.query_row(
        r#"
        SELECT entry_id, source_system, domain, knowledge_key, title, summary, status, content_json, created_at, updated_at
        FROM ticket_knowledge_entries
        WHERE source_system = ?1 AND domain = ?2 AND knowledge_key = ?3
        LIMIT 1
        "#,
        params![input.source_system, input.domain, input.knowledge_key],
        map_ticket_knowledge_entry_row,
    )
    .map_err(anyhow::Error::from)
}

fn put_ticket_knowledge_entry(
    root: &Path,
    input: TicketKnowledgeUpsertInput,
) -> Result<TicketKnowledgeEntryView> {
    let mut conn = open_ticket_db(root)?;
    put_ticket_knowledge_entry_internal(&mut conn, input)
}

fn upsert_ticket_self_work_item_internal(
    conn: &mut Connection,
    input: TicketSelfWorkUpsertInput,
) -> Result<TicketSelfWorkItemView> {
    let now = now_iso_string();
    let dedupe_key = input
        .metadata
        .get("dedupe_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let work_id = format!(
        "self-work:{}:{}",
        input.source_system,
        stable_digest(dedupe_key.as_deref().unwrap_or(&format!(
            "{}:{}:{}:{}",
            input.kind, input.title, input.body_text, now
        )),)
    );
    if let Some(existing) = load_ticket_self_work_item_raw(conn, &work_id)? {
        enforce_ticket_self_work_state_transition(
            conn,
            &existing.work_id,
            &existing.state,
            &input.state,
            "ctox-ticket",
            "self_work_item_upsert",
            None,
            None,
        )?;
    }
    conn.execute(
        r#"
        INSERT INTO ticket_self_work_items (
            work_id, source_system, kind, title, body_text, state, metadata_json,
            remote_ticket_id, remote_locator, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8)
        ON CONFLICT(work_id) DO UPDATE SET
            title=excluded.title,
            body_text=excluded.body_text,
            state=CASE
                WHEN ticket_self_work_items.state = 'published' THEN ticket_self_work_items.state
                ELSE excluded.state
            END,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            work_id,
            input.source_system,
            input.kind,
            input.title,
            input.body_text,
            input.state,
            serde_json::to_string(&input.metadata)?,
            now,
        ],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .map_err(anyhow::Error::from)
}

fn mark_ticket_self_work_published(
    conn: &mut Connection,
    work_id: &str,
    remote_ticket_id: Option<&str>,
    remote_locator: Option<&str>,
) -> Result<TicketSelfWorkItemView> {
    let existing = load_ticket_self_work_item_raw(conn, work_id)?
        .context("ticket internal work item not found")?;
    enforce_ticket_self_work_state_transition(
        conn,
        work_id,
        &existing.state,
        "published",
        "ctox-ticket",
        "mark_ticket_self_work_published",
        None,
        None,
    )?;
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE ticket_self_work_items
        SET state = 'published',
            remote_ticket_id = ?2,
            remote_locator = ?3,
            updated_at = ?4
        WHERE work_id = ?1
        "#,
        params![work_id, remote_ticket_id, remote_locator, now],
    )?;
    clear_ticket_self_work_list_cache();
    conn.query_row(
        r#"
        SELECT work_id, source_system, kind, title, body_text, state, metadata_json, remote_ticket_id, remote_locator, created_at, updated_at
        FROM ticket_self_work_items
        WHERE work_id = ?1
        LIMIT 1
        "#,
        params![work_id],
        map_ticket_self_work_row,
    )
    .map_err(anyhow::Error::from)
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

fn resolve_source_skill_artifact_path(
    root: &Path,
    binding: &TicketSourceSkillBindingView,
) -> Option<std::path::PathBuf> {
    if let Ok(Some(path)) =
        crate::skill_store::resolve_materialized_skill_dir(root, &binding.skill_name)
    {
        return Some(path);
    }
    let raw = binding.artifact_path.as_deref()?.trim();
    resolve_skill_bundle_dir_hint(root, raw)
}

fn resolve_skill_bundle_dir_hint(root: &Path, raw: &str) -> Option<std::path::PathBuf> {
    if raw.trim().is_empty() {
        return None;
    }
    let path = Path::new(raw.trim());
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    candidate.exists().then_some(candidate)
}

fn resolve_repo_script_path(root: &Path, relative: &str) -> Option<std::path::PathBuf> {
    let root_candidate = root.join(relative);
    if root_candidate.exists() {
        return Some(root_candidate);
    }
    if let Ok(current_dir) = std::env::current_dir() {
        let cwd_candidate = current_dir.join(relative);
        if cwd_candidate.exists() {
            return Some(cwd_candidate);
        }
    }
    None
}

pub(crate) fn show_ticket_source_skill(
    root: &Path,
    system: &str,
) -> Result<TicketSourceSkillShowView> {
    let conn = open_ticket_db(root)?;
    let binding = load_active_ticket_source_skill_binding_from_conn(&conn, system)?
        .context("active source skill binding not found")?;
    let artifact_path = resolve_source_skill_artifact_path(root, &binding);
    let skill_markdown_path = artifact_path
        .as_ref()
        .map(|path| path.join("SKILL.md"))
        .filter(|path| path.exists());
    let skill_preview = skill_markdown_path
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?
        .map(|content| {
            content
                .lines()
                .filter(|line| !line.trim_start().starts_with("---"))
                .filter(|line| !line.trim().is_empty())
                .take(14)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.trim().is_empty());
    Ok(TicketSourceSkillShowView {
        binding,
        artifact_path: artifact_path.map(|path| path.display().to_string()),
        skill_markdown_path: skill_markdown_path.map(|path| path.display().to_string()),
        skill_preview,
    })
}

pub(crate) fn query_ticket_source_skill(
    root: &Path,
    system: &str,
    query: &str,
    top_k: usize,
) -> Result<Value> {
    let conn = open_ticket_db(root)?;
    let binding = load_active_ticket_source_skill_binding_from_conn(&conn, system)?
        .context("active source skill binding not found")?;
    match binding.archetype.as_str() {
        "operating-model" => {
            let artifact_path = resolve_source_skill_artifact_path(root, &binding)
                .context("active source skill binding does not have a usable artifact path")?;
            let script = resolve_repo_script_path(
                root,
                "skills/system/knowledge_bootstrap/ticket-operating-model-bootstrap/scripts/query_ticket_operating_model.py",
            )
            .context("ticket operating-model query helper is not available in this runtime root")?;
            if !script.exists() {
                anyhow::bail!(
                    "ticket operating-model query helper not found at {}",
                    script.display()
                );
            }
            let output = Command::new("python3")
                .arg(&script)
                .arg("--model-dir")
                .arg(&artifact_path)
                .arg("--query")
                .arg(query)
                .arg("--top-k")
                .arg(top_k.to_string())
                .output()
                .with_context(|| format!("failed to run {}", script.display()))?;
            if !output.status.success() {
                anyhow::bail!(
                    "source skill query failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            let payload: Value = serde_json::from_slice(&output.stdout)
                .context("source skill query returned invalid json")?;
            Ok(json!({
                "ok": true,
                "source_system": system,
                "binding": binding,
                "artifact_path": artifact_path.display().to_string(),
                "result": payload,
            }))
        }
        "skillbook-runbook" => {
            let (main_skill, retrieval_mode, matches) =
                query_ticket_skillbook_runbook_bundle(root, &conn, &binding, query, top_k)?;
            Ok(json!({
                "ok": true,
                "source_system": system,
                "binding": binding,
                "result": {
                    "retrieval_mode": retrieval_mode,
                    "main_skill": {
                        "main_skill_id": main_skill.main_skill_id,
                        "title": main_skill.title,
                        "primary_channel": main_skill.primary_channel,
                    },
                    "count": matches.len(),
                    "matches": matches,
                },
            }))
        }
        other => anyhow::bail!("source skill query is not supported for archetype {other}"),
    }
}

pub(crate) fn import_ticket_source_skill_bundle(
    root: &Path,
    system: &str,
    bundle_dir: &str,
    embedding_model_override: Option<&str>,
    skip_embeddings: bool,
) -> Result<Value> {
    let bundle_path = resolve_bundle_dir(root, bundle_dir)?;
    let main_skill: TicketSourceMainSkillRecord =
        read_json_file(&bundle_path.join("main_skill.json"))?;
    let skillbook: TicketSourceSkillbookRecord =
        read_json_file(&bundle_path.join("skillbook.json"))?;
    let runbooks = read_json_file::<TicketSourceRunbookBundle>(&bundle_path.join("runbook.json"))?
        .into_runbooks();
    let items: Vec<TicketSourceRunbookItemRecord> =
        read_jsonl_file(&bundle_path.join("runbook_items.jsonl"))?;
    let resources_path = bundle_path.join("resources.jsonl");
    let resources: Option<Vec<TicketSourceKnowledgeResourceRecord>> = if resources_path.is_file() {
        Some(read_jsonl_file(&resources_path)?)
    } else {
        None
    };
    anyhow::ensure!(
        !runbooks.is_empty(),
        "bundle {} does not contain runbooks",
        bundle_path.display()
    );
    anyhow::ensure!(
        !items.is_empty(),
        "bundle {} does not contain runbook items",
        bundle_path.display()
    );
    validate_ticket_source_skill_bundle(
        &main_skill,
        &skillbook,
        &runbooks,
        &items,
        resources.as_deref().unwrap_or_default(),
    )?;

    let now = now_iso_string();
    let embedding_model = embedding_model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(default_ticket_skill_embedding_model);

    let embeddings = if skip_embeddings {
        Vec::new()
    } else {
        let inputs = items
            .iter()
            .map(|item| item.chunk_text.clone())
            .collect::<Vec<_>>();
        embed_texts_for_ticket_skills(root, &inputs, &embedding_model)?
    };

    let runbooks_by_id = runbooks
        .iter()
        .map(|runbook| (runbook.runbook_id.as_str(), runbook))
        .collect::<BTreeMap<_, _>>();
    let runbook_ids = runbooks
        .iter()
        .map(|runbook| runbook.runbook_id.clone())
        .collect::<Vec<_>>();
    let mut conn = open_ticket_db(root)?;
    let binding;
    {
        let tx = conn.transaction()?;
        validate_ticket_source_skill_id_ownership(
            &tx,
            &skillbook.skillbook_id,
            &runbooks,
            &items,
            resources.as_deref(),
        )?;
        let incoming_item_ids = items
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut existing_items =
            tx.prepare("SELECT item_id FROM knowledge_runbook_items WHERE skillbook_id = ?1")?;
        let old_item_ids = existing_items
            .query_map(params![skillbook.skillbook_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(existing_items);
        for old_item_id in old_item_ids {
            if !incoming_item_ids.contains(old_item_id.as_str()) {
                tx.execute(
                    "DELETE FROM knowledge_embeddings WHERE item_id = ?1",
                    params![old_item_id],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM knowledge_runbook_items WHERE skillbook_id = ?1",
            params![skillbook.skillbook_id],
        )?;
        tx.execute(
            "DELETE FROM knowledge_runbooks WHERE skillbook_id = ?1",
            params![skillbook.skillbook_id],
        )?;
        if resources.is_some() {
            tx.execute(
                "DELETE FROM knowledge_resources WHERE skillbook_id = ?1",
                params![skillbook.skillbook_id],
            )?;
        }
        upsert_ticket_source_main_skill(&tx, &main_skill, &now)?;
        upsert_ticket_source_skillbook(&tx, &skillbook, &now)?;
        for runbook in &runbooks {
            upsert_ticket_source_runbook(&tx, runbook, &now)?;
        }
        for (index, item) in items.iter().enumerate() {
            let runbook = runbooks_by_id
                .get(item.runbook_id.as_str())
                .context("validated runbook parent missing during import")?;
            upsert_ticket_source_runbook_item(&tx, item, &runbook.version, &runbook.status, &now)?;
            if let Some(vector) = embeddings.get(index) {
                upsert_ticket_source_embedding(&tx, &item.item_id, &embedding_model, vector, &now)?;
            }
        }
        if let Some(resources) = &resources {
            for resource in resources {
                upsert_ticket_source_knowledge_resource(
                    &tx,
                    &skillbook.skillbook_id,
                    resource,
                    &now,
                )?;
            }
        }
        binding = put_ticket_source_skill_binding_with_conn(
            &tx,
            system,
            &main_skill.main_skill_id,
            "skillbook-runbook",
            "active",
            "bundle-import",
            Some(bundle_dir),
            Some(&format!(
                "Imported main skill {}, skillbook {}, {} runbooks",
                main_skill.main_skill_id,
                skillbook.skillbook_id,
                runbooks.len()
            )),
            &now,
        )?;
        record_audit(
            &tx,
            AuditRequest {
                ticket_key: &format!("*ticket-source:{}*", system),
                case_id: None,
                actor_type: "knowledge_importer",
                action_type: "source_skill_bundle_import",
                label: None,
                bundle_label: None,
                bundle_version: None,
                details: json!({
                    "system": system,
                    "main_skill_id": main_skill.main_skill_id,
                    "skillbook_id": skillbook.skillbook_id,
                    "runbook_ids": runbook_ids.clone(),
                    "runbook_count": runbooks.len(),
                    "item_count": items.len(),
                    "resource_count": resources.as_ref().map(Vec::len),
                    "embedding_model": if skip_embeddings { None::<String> } else { Some(embedding_model.clone()) },
                    "bundle_dir": bundle_path.display().to_string(),
                }),
            },
        )?;
        tx.commit()?;
    }
    Ok(json!({
        "ok": true,
        "binding": binding,
        "bundle_dir": bundle_path.display().to_string(),
        "main_skill_id": main_skill.main_skill_id,
        "skillbook_id": skillbook.skillbook_id,
        "runbook_ids": runbook_ids,
        "runbook_count": runbooks.len(),
        "item_count": items.len(),
        "resource_count": resources.as_ref().map(Vec::len),
        "embedding_model": if skip_embeddings { Value::Null } else { json!(embedding_model) },
        "embeddings_indexed": !skip_embeddings,
    }))
}

fn validate_ticket_source_skill_bundle(
    main_skill: &TicketSourceMainSkillRecord,
    skillbook: &TicketSourceSkillbookRecord,
    runbooks: &[TicketSourceRunbookRecord],
    items: &[TicketSourceRunbookItemRecord],
    resources: &[TicketSourceKnowledgeResourceRecord],
) -> Result<()> {
    anyhow::ensure!(
        main_skill
            .linked_skillbooks
            .iter()
            .any(|id| id == &skillbook.skillbook_id),
        "main skill {} does not link skillbook {}",
        main_skill.main_skill_id,
        skillbook.skillbook_id
    );

    let mut runbook_ids = BTreeSet::new();
    for runbook in runbooks {
        anyhow::ensure!(
            !runbook.runbook_id.trim().is_empty(),
            "bundle contains a runbook with an empty runbook_id"
        );
        anyhow::ensure!(
            runbook.skillbook_id == skillbook.skillbook_id,
            "runbook {} links skillbook {}, expected {}",
            runbook.runbook_id,
            runbook.skillbook_id,
            skillbook.skillbook_id
        );
        anyhow::ensure!(
            runbook_ids.insert(runbook.runbook_id.as_str()),
            "bundle contains duplicate runbook {}",
            runbook.runbook_id
        );
    }

    let linked_runbooks = skillbook
        .linked_runbooks
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    anyhow::ensure!(
        linked_runbooks == runbook_ids,
        "skillbook linked_runbooks do not match the runbook catalog"
    );
    let main_linked_runbooks = main_skill
        .linked_runbooks
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    anyhow::ensure!(
        runbook_ids.is_subset(&main_linked_runbooks),
        "main skill linked_runbooks omit runbooks from the catalog"
    );

    let mut item_ids = BTreeSet::new();
    for item in items {
        anyhow::ensure!(
            item_ids.insert(item.item_id.as_str()),
            "bundle contains duplicate runbook item {}",
            item.item_id
        );
        anyhow::ensure!(
            runbook_ids.contains(item.runbook_id.as_str()),
            "runbook item {} references missing runbook {}",
            item.item_id,
            item.runbook_id
        );
        anyhow::ensure!(
            item.skillbook_id == skillbook.skillbook_id,
            "runbook item {} links skillbook {}, expected {}",
            item.item_id,
            item.skillbook_id,
            skillbook.skillbook_id
        );
    }

    let known_item_ids = items
        .iter()
        .map(|item| item.item_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut resource_ids = BTreeSet::new();
    for resource in resources {
        anyhow::ensure!(
            !resource.resource_id.trim().is_empty(),
            "bundle contains a resource with an empty resource_id"
        );
        anyhow::ensure!(
            !resource.title.trim().is_empty(),
            "resource {} has an empty title",
            resource.resource_id
        );
        anyhow::ensure!(
            resource_ids.insert(resource.resource_id.as_str()),
            "bundle contains duplicate resource {}",
            resource.resource_id
        );
        for item_id in &resource.linked_runbook_items {
            anyhow::ensure!(
                known_item_ids.contains(item_id.as_str()),
                "resource {} references missing runbook item {}",
                resource.resource_id,
                item_id
            );
        }
    }
    Ok(())
}

fn validate_ticket_source_skill_id_ownership(
    conn: &Connection,
    skillbook_id: &str,
    runbooks: &[TicketSourceRunbookRecord],
    items: &[TicketSourceRunbookItemRecord],
    resources: Option<&[TicketSourceKnowledgeResourceRecord]>,
) -> Result<()> {
    for runbook in runbooks {
        let owner = conn
            .query_row(
                "SELECT skillbook_id FROM knowledge_runbooks WHERE runbook_id = ?1",
                params![runbook.runbook_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        anyhow::ensure!(
            owner.as_deref().is_none_or(|owner| owner == skillbook_id),
            "runbook {} is already owned by skillbook {}",
            runbook.runbook_id,
            owner.unwrap_or_default()
        );
    }
    for item in items {
        let owner = conn
            .query_row(
                "SELECT skillbook_id FROM knowledge_runbook_items WHERE item_id = ?1",
                params![item.item_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        anyhow::ensure!(
            owner.as_deref().is_none_or(|owner| owner == skillbook_id),
            "runbook item {} is already owned by skillbook {}",
            item.item_id,
            owner.unwrap_or_default()
        );
    }
    for resource in resources.unwrap_or_default() {
        let owner = conn
            .query_row(
                "SELECT skillbook_id FROM knowledge_resources WHERE resource_id = ?1",
                params![resource.resource_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        anyhow::ensure!(
            owner.as_deref().is_none_or(|owner| owner == skillbook_id),
            "resource {} is already owned by skillbook {}",
            resource.resource_id,
            owner.unwrap_or_default()
        );
    }
    Ok(())
}

pub(crate) fn resolve_ticket_source_skill_for_target(
    root: &Path,
    ticket_key: Option<&str>,
    case_id: Option<&str>,
    top_k: usize,
) -> Result<Value> {
    let (ticket, case) = resolve_ticket_and_case(root, ticket_key, case_id)?;
    let query = build_ticket_source_skill_query_text(&ticket);
    let result = query_ticket_source_skill(root, &ticket.source_system, &query, top_k)?;
    Ok(json!({
        "ok": true,
        "ticket_key": ticket.ticket_key,
        "case_id": case.as_ref().map(|item| item.case_id.clone()),
        "query": query,
        "resolution": result.get("result").cloned().unwrap_or_else(|| json!({})),
    }))
}

pub(crate) fn compose_ticket_source_skill_reply(
    root: &Path,
    ticket_key: Option<&str>,
    case_id: Option<&str>,
    send_policy: &str,
    subject_override: Option<&str>,
    body_only: bool,
) -> Result<Value> {
    let canonical_send_policy = canonical_source_skill_send_policy(send_policy)?;
    let (ticket, case) = resolve_ticket_and_case(root, ticket_key, case_id)?;
    let query = build_ticket_source_skill_query_text(&ticket);
    let conn = open_ticket_db(root)?;
    let binding = load_active_ticket_source_skill_binding_from_conn(&conn, &ticket.source_system)?
        .context("active source skill binding not found")?;
    anyhow::ensure!(
        binding.archetype == "skillbook-runbook",
        "reply composition is only supported for skillbook-runbook bindings"
    );
    let (main_skill, retrieval_mode, matches) =
        query_ticket_skillbook_runbook_bundle(root, &conn, &binding, &query, 3)?;
    let best = matches
        .first()
        .cloned()
        .context("no runbook item match found for reply composition")?;
    let second_score = matches.get(1).map(|item| item.score).unwrap_or(0.0);
    let score_gap = best.score - second_score;
    let confidence_clear = match retrieval_mode.as_str() {
        "embedding" => best.score >= 0.35 && score_gap >= 0.02,
        _ => best.score >= 0.08 && score_gap >= 0.02,
    };
    if !confidence_clear {
        return Ok(json!({
            "decision": "needs_review",
            "ticket_key": ticket.ticket_key,
            "case_id": case.as_ref().map(|item| item.case_id.clone()),
            "retrieval_mode": retrieval_mode,
            "matches": matches,
        }));
    }
    let skillbook = load_ticket_source_skillbook_from_conn(
        &conn,
        main_skill
            .linked_skillbooks
            .first()
            .map(String::as_str)
            .context("main skill has no linked skillbook")?,
    )?
    .context("linked skillbook not found in runtime db")?;
    let reply = compose_reply_from_runbook_item(
        &ticket,
        case.as_ref(),
        &main_skill,
        &skillbook,
        &best,
        canonical_send_policy,
        subject_override,
    )?;
    if body_only {
        return Ok(Value::String(reply.reply_body));
    }
    Ok(serde_json::to_value(reply)?)
}

fn resolve_ticket_and_case(
    root: &Path,
    ticket_key: Option<&str>,
    case_id: Option<&str>,
) -> Result<(TicketItemView, Option<TicketCaseView>)> {
    match (ticket_key, case_id) {
        (Some(ticket_key), None) => Ok((
            load_ticket(root, ticket_key)?.context("ticket not found")?,
            None,
        )),
        (None, Some(case_id)) => {
            let case = load_case(root, case_id)?.context("ticket case not found")?;
            let ticket =
                load_ticket(root, &case.ticket_key)?.context("ticket not found for case")?;
            Ok((ticket, Some(case)))
        }
        (Some(_), Some(_)) => anyhow::bail!("provide either --ticket-key or --case-id, not both"),
        (None, None) => anyhow::bail!("provide --ticket-key or --case-id"),
    }
}

fn query_ticket_skillbook_runbook_bundle(
    root: &Path,
    conn: &Connection,
    binding: &TicketSourceSkillBindingView,
    query: &str,
    top_k: usize,
) -> Result<(
    TicketSourceMainSkillRecord,
    String,
    Vec<TicketSourceSkillMatchView>,
)> {
    let main_skill = load_ticket_source_main_skill_from_conn(conn, &binding.skill_name)?
        .context("bound main skill is not present in runtime db; import the bundle first")?;
    anyhow::ensure!(
        !main_skill.linked_runbooks.is_empty(),
        "bound main skill does not link any runbooks"
    );
    let items = load_ticket_source_runbook_items_for_runbooks(conn, &main_skill.linked_runbooks)?;
    anyhow::ensure!(
        !items.is_empty(),
        "no runbook items are stored for the linked source skill runbooks"
    );
    let embeddings = load_ticket_source_embeddings_for_items(
        conn,
        &items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<_>>(),
    )?;
    let embedding_model = embeddings
        .values()
        .find_map(|(model, _)| Some(model.clone()));
    let (retrieval_mode, scored_matches) = if let Some(model) = embedding_model {
        let query_embedding = embed_texts_for_ticket_skills(root, &[query.to_string()], &model)?
            .into_iter()
            .next()
            .context("embedding service returned no query vector")?;
        let mut matches = items
            .iter()
            .filter_map(|item| {
                let (_, embedding) = embeddings.get(&item.item_id)?;
                Some(TicketSourceSkillMatchView {
                    item_id: item.item_id.clone(),
                    label: item.label.clone(),
                    title: item.title.clone(),
                    problem_class: item.problem_class.clone(),
                    score: cosine_similarity(&query_embedding, embedding),
                    expected_guidance: item.expected_guidance.clone(),
                    earliest_blocker: item.earliest_blocker.clone(),
                    escalate_when: item.escalate_when.clone(),
                    pages: item.pages.clone(),
                    tool_actions: item.tool_actions.clone(),
                    writeback_policy: item.writeback_policy.clone(),
                })
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
        });
        ("embedding".to_string(), matches)
    } else {
        let mut matches = items
            .iter()
            .map(|item| TicketSourceSkillMatchView {
                item_id: item.item_id.clone(),
                label: item.label.clone(),
                title: item.title.clone(),
                problem_class: item.problem_class.clone(),
                score: lexical_overlap_ratio(query, &item.chunk_text),
                expected_guidance: item.expected_guidance.clone(),
                earliest_blocker: item.earliest_blocker.clone(),
                escalate_when: item.escalate_when.clone(),
                pages: item.pages.clone(),
                tool_actions: item.tool_actions.clone(),
                writeback_policy: item.writeback_policy.clone(),
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
        });
        ("lexical_fallback".to_string(), matches)
    };
    let mut matches = scored_matches;
    matches.truncate(top_k.max(1));
    Ok((main_skill, retrieval_mode, matches))
}

fn compose_reply_from_runbook_item(
    ticket: &TicketItemView,
    case: Option<&TicketCaseView>,
    _main_skill: &TicketSourceMainSkillRecord,
    _skillbook: &TicketSourceSkillbookRecord,
    item: &TicketSourceSkillMatchView,
    send_policy: &str,
    subject_override: Option<&str>,
) -> Result<TicketSourceSkillReplyView> {
    let language = detect_ticket_reply_language(&format!("{}\n{}", ticket.title, ticket.body_text));
    let salutation = if language == "en" { "Hello," } else { "Hallo," };
    let manual_reference = if item.pages.is_empty() {
        None
    } else {
        Some(format!("Manual reference: {}", item.pages.join(", ")))
    };
    let mut paragraphs = vec![
        salutation.to_string(),
        item.expected_guidance.trim().to_string(),
    ];
    if let Some(reference) = manual_reference.clone() {
        paragraphs.push(reference);
    }
    let reply_body = paragraphs.join("\n\n");
    Ok(TicketSourceSkillReplyView {
        decision: send_policy.to_string(),
        source_system: ticket.source_system.clone(),
        ticket_key: ticket.ticket_key.clone(),
        case_id: case.map(|item| item.case_id.clone()),
        matched_label: item.label.clone(),
        item_id: item.item_id.clone(),
        reply_subject: subject_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("Re: {value}"))
            .unwrap_or_else(|| format!("Re: {}", ticket.title.trim())),
        reply_body,
        manual_reference,
        writeback_policy: item.writeback_policy.clone(),
    })
}

fn detect_ticket_reply_language(text: &str) -> &'static str {
    let lowered = text.to_lowercase();
    let english_markers = [
        "hello",
        "please",
        "password",
        "support",
        "registration",
        "login",
    ];
    if english_markers
        .iter()
        .filter(|marker| lowered.contains(**marker))
        .count()
        >= 2
    {
        "en"
    } else {
        "de"
    }
}

fn canonical_source_skill_send_policy(value: &str) -> Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "suggestion" | "suggest" => Ok("suggestion"),
        "draft" => Ok("draft"),
        "send" => Ok("send"),
        other => anyhow::bail!("unsupported send policy: {other}"),
    }
}

fn resolve_bundle_dir(root: &Path, raw: &str) -> Result<PathBuf> {
    let candidate = Path::new(raw.trim());
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    anyhow::ensure!(
        path.exists(),
        "bundle path does not exist: {}",
        path.display()
    );
    Ok(path)
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("invalid json in {}", path.display()))
}

fn read_jsonl_file<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    body.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("invalid jsonl in {}", path.display()))
}

fn default_ticket_skill_embedding_model() -> String {
    model_registry::default_auxiliary_model(engine::AuxiliaryRole::Embedding)
        .unwrap_or(DEFAULT_TICKET_SKILL_EMBEDDING_MODEL)
        .to_string()
}

fn embed_texts_for_ticket_skills(
    root: &Path,
    inputs: &[String],
    model: &str,
) -> Result<Vec<Vec<f64>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    supervisor::ensure_auxiliary_backend_launchable(root, engine::AuxiliaryRole::Embedding)
        .context("embedding backend is not launchable for ticket skill retrieval")?;
    supervisor::ensure_auxiliary_backend_ready(root, engine::AuxiliaryRole::Embedding, false)
        .context("failed to ensure managed embedding backend for ticket skill retrieval")?;
    let resolved_runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for ticket skill retrieval")?;
    if let Some(binding) =
        resolved_runtime.binding_for_auxiliary_role(engine::AuxiliaryRole::Embedding)
    {
        if !binding.transport.is_private_ipc() {
            anyhow::bail!(
                "ctox_core_local requires private IPC for local embedding inference; loopback HTTP transport is not allowed"
            );
        }
        let label = binding.transport.display_label();
        return embed_texts_for_ticket_skills_via_local_socket(&binding.transport, inputs, model)
            .with_context(|| format!("failed to reach embedding transport {label}"));
    }
    let base_url = resolved_runtime
        .auxiliary_base_url(engine::AuxiliaryRole::Embedding)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("embedding runtime is not resolved"))?;
    let response = ureq::post(&format!("{}/v1/embeddings", base_url.trim_end_matches('/')))
        .set("content-type", "application/json")
        .timeout(Duration::from_secs(30))
        .send_string(&serde_json::to_string(&json!({
            "model": model,
            "input": inputs,
        }))?)
        .with_context(|| format!("failed to reach embedding service at {}", base_url))?;
    let body = response
        .into_string()
        .context("failed to read embedding response")?;
    let payload: Value =
        serde_json::from_str(&body).context("failed to parse embedding response")?;
    let mut indexed = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    indexed.sort_by_key(|item| item.get("index").and_then(Value::as_u64).unwrap_or(0));
    let vectors = indexed
        .into_iter()
        .map(|item| {
            item.get("embedding")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                .filter(|values| !values.is_empty())
                .context("embedding response missing vectors")
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        vectors.len() == inputs.len(),
        "embedding response count mismatch: expected {}, got {}",
        inputs.len(),
        vectors.len()
    );
    Ok(vectors)
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TicketSkillEmbeddingSocketRequest<'a> {
    EmbeddingsCreate {
        model: &'a str,
        inputs: &'a [String],
        truncate_sequence: bool,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TicketSkillEmbeddingSocketResponse {
    Embeddings {
        #[allow(dead_code)]
        model: String,
        data: Vec<Vec<f32>>,
        #[serde(rename = "prompt_tokens")]
        _prompt_tokens: u32,
        #[serde(rename = "total_tokens")]
        _total_tokens: u32,
    },
    Error {
        code: String,
        message: String,
    },
}

fn embed_texts_for_ticket_skills_via_local_socket(
    transport: &LocalTransport,
    inputs: &[String],
    model: &str,
) -> Result<Vec<Vec<f64>>> {
    let timeout = Duration::from_secs(30);
    let label = transport.display_label();
    let mut stream = transport
        .connect_blocking(timeout)
        .with_context(|| format!("failed to connect via {label}"))?;
    let request = TicketSkillEmbeddingSocketRequest::EmbeddingsCreate {
        model,
        inputs,
        truncate_sequence: false,
    };
    let mut payload =
        serde_json::to_vec(&request).context("failed to encode ticket skill embedding request")?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .with_context(|| format!("failed to write request via {label}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush request via {label}"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read response via {label}"))?;
    anyhow::ensure!(
        !line.trim().is_empty(),
        "embedding socket returned an empty response"
    );
    match serde_json::from_str::<TicketSkillEmbeddingSocketResponse>(line.trim())
        .context("failed to parse embedding socket response")?
    {
        TicketSkillEmbeddingSocketResponse::Embeddings { data, .. } => Ok(data
            .into_iter()
            .map(|values| values.into_iter().map(|value| value as f64).collect())
            .collect()),
        TicketSkillEmbeddingSocketResponse::Error { code, message } => {
            anyhow::bail!("{code}: {message}")
        }
    }
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (l, r) in left.iter().zip(right.iter()) {
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn upsert_ticket_source_main_skill(
    conn: &Connection,
    record: &TicketSourceMainSkillRecord,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_main_skills (
            main_skill_id, title, primary_channel, entry_action, resolver_contract_json,
            execution_contract_json, resolve_flow_json, writeback_flow_json,
            linked_skillbooks_json, linked_runbooks_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
        ON CONFLICT(main_skill_id) DO UPDATE SET
            title=excluded.title,
            primary_channel=excluded.primary_channel,
            entry_action=excluded.entry_action,
            resolver_contract_json=excluded.resolver_contract_json,
            execution_contract_json=excluded.execution_contract_json,
            resolve_flow_json=excluded.resolve_flow_json,
            writeback_flow_json=excluded.writeback_flow_json,
            linked_skillbooks_json=excluded.linked_skillbooks_json,
            linked_runbooks_json=excluded.linked_runbooks_json,
            updated_at=excluded.updated_at
        "#,
        params![
            record.main_skill_id,
            record.title,
            record.primary_channel,
            record.entry_action,
            serde_json::to_string(&record.resolver_contract)?,
            serde_json::to_string(&record.execution_contract)?,
            serde_json::to_string(&record.resolve_flow)?,
            serde_json::to_string(&record.writeback_flow)?,
            serde_json::to_string(&record.linked_skillbooks)?,
            serde_json::to_string(&record.linked_runbooks)?,
            now,
        ],
    )?;
    Ok(())
}

fn upsert_ticket_source_skillbook(
    conn: &Connection,
    record: &TicketSourceSkillbookRecord,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_skillbooks (
            skillbook_id, title, version, status, summary, mission, non_negotiable_rules_json,
            runtime_policy, answer_contract, workflow_backbone_json, routing_taxonomy_json,
            linked_runbooks_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, 'active', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
        ON CONFLICT(skillbook_id) DO UPDATE SET
            title=excluded.title,
            version=excluded.version,
            status=excluded.status,
            summary=excluded.summary,
            mission=excluded.mission,
            non_negotiable_rules_json=excluded.non_negotiable_rules_json,
            runtime_policy=excluded.runtime_policy,
            answer_contract=excluded.answer_contract,
            workflow_backbone_json=excluded.workflow_backbone_json,
            routing_taxonomy_json=excluded.routing_taxonomy_json,
            linked_runbooks_json=excluded.linked_runbooks_json,
            updated_at=excluded.updated_at
        "#,
        params![
            record.skillbook_id,
            record.title,
            record.version,
            summarize_text(&record.mission, 220),
            record.mission,
            serde_json::to_string(&record.non_negotiable_rules)?,
            record.runtime_policy,
            record.answer_contract,
            serde_json::to_string(&record.workflow_backbone)?,
            serde_json::to_string(&record.routing_taxonomy)?,
            serde_json::to_string(&record.linked_runbooks)?,
            now,
        ],
    )?;
    Ok(())
}

fn upsert_ticket_source_runbook(
    conn: &Connection,
    record: &TicketSourceRunbookRecord,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_runbooks (
            runbook_id, skillbook_id, title, version, status, summary, problem_domain,
            item_labels_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        ON CONFLICT(runbook_id) DO UPDATE SET
            skillbook_id=excluded.skillbook_id,
            title=excluded.title,
            version=excluded.version,
            status=excluded.status,
            summary=excluded.summary,
            problem_domain=excluded.problem_domain,
            item_labels_json=excluded.item_labels_json,
            updated_at=excluded.updated_at
        "#,
        params![
            record.runbook_id,
            record.skillbook_id,
            record.title,
            record.version,
            record.status,
            summarize_text(&record.title, 220),
            record.problem_domain,
            serde_json::to_string(&record.item_labels)?,
            now,
        ],
    )?;
    Ok(())
}

fn upsert_ticket_source_runbook_item(
    conn: &Connection,
    record: &TicketSourceRunbookItemRecord,
    version: &str,
    status: &str,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_runbook_items (
            item_id, runbook_id, skillbook_id, label, title, problem_class, chunk_text,
            structured_json, status, version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
        ON CONFLICT(item_id) DO UPDATE SET
            runbook_id=excluded.runbook_id,
            skillbook_id=excluded.skillbook_id,
            label=excluded.label,
            title=excluded.title,
            problem_class=excluded.problem_class,
            chunk_text=excluded.chunk_text,
            structured_json=excluded.structured_json,
            status=excluded.status,
            version=excluded.version,
            updated_at=excluded.updated_at
        "#,
        params![
            record.item_id,
            record.runbook_id,
            record.skillbook_id,
            record.label,
            record.title,
            record.problem_class,
            record.chunk_text,
            serde_json::to_string(record)?,
            status,
            version,
            now,
        ],
    )?;
    Ok(())
}

fn upsert_ticket_source_knowledge_resource(
    conn: &Connection,
    skillbook_id: &str,
    record: &TicketSourceKnowledgeResourceRecord,
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_resources (
            resource_id, skillbook_id, title, kind, source_id, role, canonical_url,
            snapshot_hash, evidence_eligible, linked_runbook_items_json, metadata_json,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
        ON CONFLICT(resource_id) DO UPDATE SET
            skillbook_id=excluded.skillbook_id,
            title=excluded.title,
            kind=excluded.kind,
            source_id=excluded.source_id,
            role=excluded.role,
            canonical_url=excluded.canonical_url,
            snapshot_hash=excluded.snapshot_hash,
            evidence_eligible=excluded.evidence_eligible,
            linked_runbook_items_json=excluded.linked_runbook_items_json,
            metadata_json=excluded.metadata_json,
            updated_at=excluded.updated_at
        "#,
        params![
            record.resource_id,
            skillbook_id,
            record.title,
            record.kind,
            record.source_id,
            record.role,
            record.canonical_url,
            record.snapshot_hash,
            i64::from(record.evidence_eligible),
            serde_json::to_string(&record.linked_runbook_items)?,
            serde_json::to_string(record)?,
            now,
        ],
    )?;
    Ok(())
}

fn upsert_ticket_source_embedding(
    conn: &Connection,
    item_id: &str,
    embedding_model: &str,
    vector: &[f64],
    now: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO knowledge_embeddings (
            item_id, embedding_model, embedding_json, updated_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(item_id, embedding_model) DO UPDATE SET
            embedding_json=excluded.embedding_json,
            updated_at=excluded.updated_at
        "#,
        params![
            item_id,
            embedding_model,
            serde_json::to_string(vector)?,
            now
        ],
    )?;
    Ok(())
}

// ----- Incremental procedural-knowledge writers ---------------------------
//
// Builder-style entry points exposed for `ctox knowledge skill new /
// add-skillbook / add-runbook / add-item` (Tier 4). They construct the
// canonical `TicketSource*Record` shapes from primitive parameters and call
// the same upsert helpers used by `import_ticket_source_skill_bundle`. This
// is the only way to grow a skillbook / runbook turn-by-turn without
// preparing a full bundle directory on disk first.

/// Create or upsert a `knowledge_main_skills` row. Returns the
/// `TicketSourceMainSkillRecord` JSON view including the stored fields.
///
/// Optional contract fields default to empty / null when the caller cannot
/// fill them yet. The agent fills them progressively over later turns by
/// calling this same function again with the same `main_skill_id`.
pub(crate) fn create_or_update_main_skill(
    root: &Path,
    main_skill_id: &str,
    title: &str,
    primary_channel: &str,
    entry_action: &str,
    resolver_contract: Option<Value>,
    execution_contract: Option<Value>,
    resolve_flow: Vec<String>,
    writeback_flow: Vec<String>,
    linked_skillbooks: Vec<String>,
    linked_runbooks: Vec<String>,
) -> Result<Value> {
    anyhow::ensure!(!main_skill_id.trim().is_empty(), "main_skill_id required");
    anyhow::ensure!(!title.trim().is_empty(), "title required");
    anyhow::ensure!(
        !primary_channel.trim().is_empty(),
        "primary_channel required"
    );
    anyhow::ensure!(!entry_action.trim().is_empty(), "entry_action required");
    let record = TicketSourceMainSkillRecord {
        main_skill_id: main_skill_id.to_string(),
        title: title.to_string(),
        primary_channel: primary_channel.to_string(),
        entry_action: entry_action.to_string(),
        resolver_contract: resolver_contract.unwrap_or_else(|| Value::Object(Default::default())),
        execution_contract: execution_contract.unwrap_or_else(|| Value::Object(Default::default())),
        resolve_flow,
        writeback_flow,
        linked_skillbooks,
        linked_runbooks,
    };
    let now = now_iso_string();
    let conn = open_ticket_db(root)?;
    upsert_ticket_source_main_skill(&conn, &record, &now)?;
    Ok(serde_json::to_value(&record)?)
}

/// Create or upsert a `knowledge_skillbooks` row. The summary column is
/// auto-derived from `mission` via `summarize_text`; callers don't have to
/// supply it. Status is always `active` (matching the bundle-import path).
pub(crate) fn create_or_update_skillbook(
    root: &Path,
    skillbook_id: &str,
    title: &str,
    version: &str,
    mission: &str,
    runtime_policy: &str,
    answer_contract: &str,
    non_negotiable_rules: Vec<String>,
    workflow_backbone: Vec<String>,
    routing_taxonomy: Vec<String>,
    linked_runbooks: Vec<String>,
) -> Result<Value> {
    anyhow::ensure!(!skillbook_id.trim().is_empty(), "skillbook_id required");
    anyhow::ensure!(!title.trim().is_empty(), "title required");
    anyhow::ensure!(!version.trim().is_empty(), "version required");
    anyhow::ensure!(!mission.trim().is_empty(), "mission required");
    let record = TicketSourceSkillbookRecord {
        skillbook_id: skillbook_id.to_string(),
        title: title.to_string(),
        version: version.to_string(),
        mission: mission.to_string(),
        non_negotiable_rules,
        runtime_policy: runtime_policy.to_string(),
        answer_contract: answer_contract.to_string(),
        workflow_backbone,
        routing_taxonomy,
        linked_runbooks,
    };
    let now = now_iso_string();
    let conn = open_ticket_db(root)?;
    upsert_ticket_source_skillbook(&conn, &record, &now)?;
    Ok(serde_json::to_value(&record)?)
}

/// Create or upsert a `knowledge_runbooks` row.
pub(crate) fn create_or_update_runbook(
    root: &Path,
    runbook_id: &str,
    skillbook_id: &str,
    title: &str,
    version: &str,
    status: &str,
    problem_domain: &str,
    item_labels: Vec<String>,
) -> Result<Value> {
    anyhow::ensure!(!runbook_id.trim().is_empty(), "runbook_id required");
    anyhow::ensure!(!skillbook_id.trim().is_empty(), "skillbook_id required");
    anyhow::ensure!(!title.trim().is_empty(), "title required");
    anyhow::ensure!(!version.trim().is_empty(), "version required");
    anyhow::ensure!(!status.trim().is_empty(), "status required");
    let record = TicketSourceRunbookRecord {
        runbook_id: runbook_id.to_string(),
        skillbook_id: skillbook_id.to_string(),
        title: title.to_string(),
        version: version.to_string(),
        status: status.to_string(),
        problem_domain: problem_domain.to_string(),
        item_labels,
    };
    let now = now_iso_string();
    let conn = open_ticket_db(root)?;
    upsert_ticket_source_runbook(&conn, &record, &now)?;
    Ok(serde_json::to_value(&record)?)
}

/// Add (or update) a single labeled runbook item, and — unless
/// `skip_embedding` — refresh its embedding row through the standard
/// auxiliary embedding backend. The runbook's `item_labels` list is also
/// refreshed defensively so listing the runbook surfaces the new label.
pub(crate) fn add_or_update_runbook_item(
    root: &Path,
    item_id: &str,
    runbook_id: &str,
    skillbook_id: &str,
    label: &str,
    title: &str,
    problem_class: &str,
    chunk_text: &str,
    version: &str,
    status: &str,
    embedding_model_override: Option<&str>,
    skip_embedding: bool,
) -> Result<Value> {
    anyhow::ensure!(!item_id.trim().is_empty(), "item_id required");
    anyhow::ensure!(!runbook_id.trim().is_empty(), "runbook_id required");
    anyhow::ensure!(!skillbook_id.trim().is_empty(), "skillbook_id required");
    anyhow::ensure!(!label.trim().is_empty(), "label required");
    anyhow::ensure!(!title.trim().is_empty(), "title required");
    anyhow::ensure!(!problem_class.trim().is_empty(), "problem_class required");
    anyhow::ensure!(!chunk_text.trim().is_empty(), "chunk_text required");
    anyhow::ensure!(!version.trim().is_empty(), "version required");
    anyhow::ensure!(!status.trim().is_empty(), "status required");

    let record = TicketSourceRunbookItemRecord {
        item_id: item_id.to_string(),
        runbook_id: runbook_id.to_string(),
        skillbook_id: skillbook_id.to_string(),
        label: label.to_string(),
        title: title.to_string(),
        problem_class: problem_class.to_string(),
        trigger_phrases: Vec::new(),
        entry_conditions: Vec::new(),
        earliest_blocker: String::new(),
        expected_guidance: String::new(),
        tool_actions: Value::Object(Default::default()),
        verification: Vec::new(),
        writeback_policy: Value::Object(Default::default()),
        escalate_when: Vec::new(),
        sources: Value::Object(Default::default()),
        pages: Vec::new(),
        chunk_text: chunk_text.to_string(),
    };

    let now = now_iso_string();
    let conn = open_ticket_db(root)?;
    upsert_ticket_source_runbook_item(&conn, &record, version, status, &now)?;
    let labels = refresh_runbook_item_labels(&conn, runbook_id, &now)?;

    let (embedding_status, embedding_model_used) = if skip_embedding {
        ("skipped", None)
    } else {
        let model = embedding_model_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(default_ticket_skill_embedding_model);
        let inputs = vec![chunk_text.to_string()];
        match embed_texts_for_ticket_skills(root, &inputs, &model) {
            Ok(vectors) => {
                if let Some(vector) = vectors.first() {
                    upsert_ticket_source_embedding(&conn, item_id, &model, vector, &now)?;
                }
                ("indexed", Some(model))
            }
            Err(err) => {
                // Surface the error to the caller without rolling back the
                // item write — the row is durable, only the embedding is
                // missing. The agent can rerun `refresh-item-embedding` once
                // the backend is back.
                eprintln!("warning: embedding refresh for runbook item {item_id} failed: {err:#}");
                ("error", None)
            }
        }
    };

    Ok(json!({
        "item": record,
        "embedding": {
            "status": embedding_status,
            "model": embedding_model_used,
        },
        "runbook": {
            "runbook_id": runbook_id,
            "item_labels": labels,
        },
    }))
}

/// Recompute the embedding for an existing runbook item. Idempotent.
pub(crate) fn refresh_runbook_item_embedding(
    root: &Path,
    item_id: &str,
    embedding_model_override: Option<&str>,
) -> Result<Value> {
    anyhow::ensure!(!item_id.trim().is_empty(), "item_id required");
    let conn = open_ticket_db(root)?;
    let chunk_text: String = conn
        .query_row(
            "SELECT chunk_text FROM knowledge_runbook_items WHERE item_id = ?1",
            params![item_id],
            |row| row.get(0),
        )
        .optional()
        .context("query chunk_text for runbook item")?
        .with_context(|| format!("runbook item not found: {item_id}"))?;
    let model = embedding_model_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(default_ticket_skill_embedding_model);
    let inputs = vec![chunk_text];
    let vectors = embed_texts_for_ticket_skills(root, &inputs, &model)?;
    let now = now_iso_string();
    if let Some(vector) = vectors.first() {
        upsert_ticket_source_embedding(&conn, item_id, &model, vector, &now)?;
    } else {
        anyhow::bail!("embedding backend returned no vector for item {item_id}");
    }
    Ok(json!({
        "ok": true,
        "item_id": item_id,
        "embedding_model": model,
        "updated_at": now,
    }))
}

/// Pull the current set of labels for the runbook from
/// `knowledge_runbook_items` and write the deduplicated, label-sorted list
/// back into `knowledge_runbooks.item_labels_json`. Returns the new list.
fn refresh_runbook_item_labels(
    conn: &Connection,
    runbook_id: &str,
    now: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT label FROM knowledge_runbook_items WHERE runbook_id = ?1 ORDER BY label ASC",
    )?;
    let labels: Vec<String> = stmt
        .query_map(params![runbook_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    conn.execute(
        "UPDATE knowledge_runbooks SET item_labels_json = ?1, updated_at = ?2 WHERE runbook_id = ?3",
        params![serde_json::to_string(&labels)?, now, runbook_id],
    )?;
    Ok(labels)
}

fn load_ticket_source_main_skill_from_conn(
    conn: &Connection,
    main_skill_id: &str,
) -> Result<Option<TicketSourceMainSkillRecord>> {
    conn.query_row(
        r#"
        SELECT main_skill_id, title, primary_channel, entry_action, resolver_contract_json,
               execution_contract_json, resolve_flow_json, writeback_flow_json,
               linked_skillbooks_json, linked_runbooks_json
        FROM knowledge_main_skills
        WHERE main_skill_id = ?1
        LIMIT 1
        "#,
        params![main_skill_id],
        |row| {
            Ok(TicketSourceMainSkillRecord {
                main_skill_id: row.get(0)?,
                title: row.get(1)?,
                primary_channel: row.get(2)?,
                entry_action: row.get(3)?,
                resolver_contract: parse_json_column(row.get::<_, String>(4)?),
                execution_contract: parse_json_column(row.get::<_, String>(5)?),
                resolve_flow: parse_json_string_column(row.get::<_, String>(6)?),
                writeback_flow: parse_json_string_column(row.get::<_, String>(7)?),
                linked_skillbooks: parse_json_string_column(row.get::<_, String>(8)?),
                linked_runbooks: parse_json_string_column(row.get::<_, String>(9)?),
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_ticket_source_skillbook_from_conn(
    conn: &Connection,
    skillbook_id: &str,
) -> Result<Option<TicketSourceSkillbookRecord>> {
    conn.query_row(
        r#"
        SELECT skillbook_id, title, version, mission, non_negotiable_rules_json, runtime_policy,
               answer_contract, workflow_backbone_json, routing_taxonomy_json, linked_runbooks_json
        FROM knowledge_skillbooks
        WHERE skillbook_id = ?1
        LIMIT 1
        "#,
        params![skillbook_id],
        |row| {
            Ok(TicketSourceSkillbookRecord {
                skillbook_id: row.get(0)?,
                title: row.get(1)?,
                version: row.get(2)?,
                mission: row.get(3)?,
                non_negotiable_rules: parse_json_string_column(row.get::<_, String>(4)?),
                runtime_policy: row.get(5)?,
                answer_contract: row.get(6)?,
                workflow_backbone: parse_json_string_column(row.get::<_, String>(7)?),
                routing_taxonomy: parse_json_string_column(row.get::<_, String>(8)?),
                linked_runbooks: parse_json_string_column(row.get::<_, String>(9)?),
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_ticket_source_runbook_items_for_runbooks(
    conn: &Connection,
    runbook_ids: &[String],
) -> Result<Vec<TicketSourceRunbookItemRecord>> {
    let mut statement = conn.prepare(
        r#"
        SELECT structured_json
        FROM knowledge_runbook_items
        ORDER BY runbook_id ASC, label ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let filter = runbook_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut items = Vec::new();
    for row in rows {
        let raw = row?;
        let item: TicketSourceRunbookItemRecord = serde_json::from_str(&raw)?;
        if filter.contains(&item.runbook_id) {
            items.push(item);
        }
    }
    Ok(items)
}

fn load_ticket_source_embeddings_for_items(
    conn: &Connection,
    item_ids: &[String],
) -> Result<std::collections::BTreeMap<String, (String, Vec<f64>)>> {
    let mut statement = conn.prepare(
        r#"
        SELECT item_id, embedding_model, embedding_json
        FROM knowledge_embeddings
        ORDER BY updated_at DESC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let filter = item_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut map = std::collections::BTreeMap::new();
    for row in rows {
        let (item_id, model, raw_embedding) = row?;
        if !filter.contains(&item_id) || map.contains_key(&item_id) {
            continue;
        }
        let vector = serde_json::from_str::<Vec<f64>>(&raw_embedding).unwrap_or_default();
        if !vector.is_empty() {
            map.insert(item_id, (model, vector));
        }
    }
    Ok(map)
}

fn summarize_text(text: &str, limit: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        compact
    } else {
        compact.chars().take(limit).collect()
    }
}

fn build_ticket_source_skill_query_text(ticket: &TicketItemView) -> String {
    let title = ticket.title.trim();
    let body = ticket.body_text.trim();
    if body.is_empty() {
        return title.to_string();
    }
    let compact_body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped = compact_body.chars().take(260).collect::<String>();
    format!("{title}. {clipped}")
}

fn shorten_review_excerpt(text: &str, limit: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        compact
    } else {
        compact
            .chars()
            .take(limit.saturating_sub(3))
            .collect::<String>()
            + "..."
    }
}

fn lexical_overlap_ratio(left: &str, right: &str) -> f64 {
    let token_re = Regex::new(r"[A-Za-zÄÖÜäöüß0-9._/-]{3,}").expect("static token regex");
    let left_tokens = token_re
        .find_iter(left)
        .map(|m| m.as_str().to_lowercase())
        .collect::<BTreeSet<_>>();
    let right_tokens = token_re
        .find_iter(right)
        .map(|m| m.as_str().to_lowercase())
        .collect::<BTreeSet<_>>();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let union = left_tokens.union(&right_tokens).count();
    if union == 0 {
        return 0.0;
    }
    left_tokens.intersection(&right_tokens).count() as f64 / union as f64
}

fn normalized_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(crate) fn review_ticket_note_with_source_skill(
    root: &Path,
    ticket_key: &str,
    body: &str,
    top_k: usize,
) -> Result<TicketSourceSkillNoteReviewView> {
    let ticket = load_ticket(root, ticket_key)?.context("ticket not found")?;
    let query = build_ticket_source_skill_query_text(&ticket);
    let payload = query_ticket_source_skill(root, &ticket.source_system, &query, top_k)?;
    let binding_result = payload.get("result").cloned().unwrap_or_else(|| json!({}));
    let top_family = binding_result
        .get("families")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let matched_family = top_family
        .get("family_key")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let matched_family_score = top_family.get("score").and_then(Value::as_f64);
    let decision = top_family
        .get("decision_support")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let operator_summary = decision
        .get("operator_summary")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let note_guidance = decision
        .get("note_guidance")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let mut findings = Vec::new();
    let mut language_clean = true;
    let mut copy_safe = true;
    let note = body.trim();

    if note.len() < 24 {
        findings.push(TicketSourceSkillNoteReviewFinding {
            kind: "too_short".to_string(),
            excerpt: shorten_review_excerpt(note, 80),
            details: "The internal note is too short to explain concrete ticket progress."
                .to_string(),
        });
    }
    let concise = note.len() <= 420;
    if !concise {
        findings.push(TicketSourceSkillNoteReviewFinding {
            kind: "too_long".to_string(),
            excerpt: shorten_review_excerpt(note, 120),
            details: "The internal note is too long for a concise desk update.".to_string(),
        });
    }

    let leak_patterns = [
        (
            "internal_field_names",
            Regex::new(
                r"`(?:triage_focus|handling_steps|decision_support|operator_summary|family_key|historical_examples|close_when|note_guidance|caution_signals)`",
            )
            .expect("static leak regex"),
            "Avoid quoting internal skill field names in ticket communication.",
        ),
        (
            "code_style_identifiers",
            Regex::new(r"`[a-z0-9]+(?:_[a-z0-9]+){1,}`").expect("static code regex"),
            "Avoid code-like identifiers or schema names in the ticket note.",
        ),
        (
            "tooling_terms",
            Regex::new(r"\b(?:sqlite|json dump|parser|yaml|tooling internals|reference commands|ctox ticket)\b")
                .expect("static tooling regex"),
            "Avoid tooling or storage jargon in the ticket note.",
        ),
    ];
    for (kind, pattern, details) in leak_patterns {
        if let Some(hit) = pattern.find(note) {
            language_clean = false;
            findings.push(TicketSourceSkillNoteReviewFinding {
                kind: kind.to_string(),
                excerpt: shorten_review_excerpt(hit.as_str(), 80),
                details: details.to_string(),
            });
        }
    }

    let normalized_note = normalized_text(note);
    for source in operator_summary.iter().chain(note_guidance.iter()) {
        let normalized_source = normalized_text(source);
        let copied_by_overlap = lexical_overlap_ratio(note, source) >= 0.72;
        let copied_by_substring =
            !normalized_source.is_empty() && normalized_note.contains(&normalized_source);
        if copied_by_overlap || copied_by_substring {
            copy_safe = false;
            findings.push(TicketSourceSkillNoteReviewFinding {
                kind: "copied_skill_language".to_string(),
                excerpt: shorten_review_excerpt(source, 100),
                details: "The note is too close to the desk-skill guidance; write it freshly in desk language.".to_string(),
            });
        }
    }

    let grounded_in_title = lexical_overlap_ratio(note, &ticket.title) >= 0.08;
    let grounded_in_body = lexical_overlap_ratio(note, &ticket.body_text) >= 0.08;
    let grounded_in_ticket = grounded_in_title || grounded_in_body;
    if !grounded_in_ticket {
        findings.push(TicketSourceSkillNoteReviewFinding {
            kind: "not_ticket_grounded".to_string(),
            excerpt: shorten_review_excerpt(note, 100),
            details: "The note does not mention ticket-specific terms strongly enough.".to_string(),
        });
    }

    Ok(TicketSourceSkillNoteReviewView {
        source_system: ticket.source_system,
        ticket_key: ticket.ticket_key,
        query,
        matched_family,
        matched_family_score,
        desk_ready: language_clean
            && copy_safe
            && concise
            && grounded_in_ticket
            && note.len() >= 24,
        language_clean,
        copy_safe,
        concise,
        grounded_in_ticket,
        findings,
        note_guidance,
        operator_summary,
    })
}

pub(crate) fn suggested_skill_for_live_ticket_source(
    root: &Path,
    event: &RoutedTicketEvent,
) -> Result<Option<String>> {
    let explicit_self_work = suggested_skill_for_routed_event(root, event)?;
    if explicit_self_work.is_some() {
        return Ok(explicit_self_work);
    }
    let conn = open_ticket_db(root)?;
    Ok(
        load_active_ticket_source_skill_binding_from_conn(&conn, &event.source_system)?
            .map(|binding| binding.skill_name),
    )
}

fn default_skill_for_self_work_kind(kind: &str) -> Option<String> {
    let kind = kind.trim();
    if kind.is_empty() {
        return None;
    }
    match kind {
        "access-request" => Some("ticket-access-and-secrets".to_string()),
        "system-onboarding" => Some("system-onboarding".to_string()),
        "secret-hygiene" => Some("secret-hygiene".to_string()),
        "mission-follow-up" | "timeout-continuation" | "review-rework" => {
            Some("follow-up-orchestrator".to_string())
        }
        WORKFLOW_CASE_KIND | WORKFLOW_STEP_KIND => Some(WORKFLOW_ORCHESTRATOR_SKILL.to_string()),
        _ => None,
    }
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
        force_ticket_event_routed_state(&conn, &event_key, initial_route_status)?;
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
    force_ticket_event_routed_state(&conn, &event_key, initial_route_status)?;
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
        force_ticket_event_routed_state(&conn, &event_key, "handled")?;
    }
    Ok(())
}

fn force_ticket_event_routed_state(
    conn: &Connection,
    event_key: &str,
    route_status: &str,
) -> Result<()> {
    let now = now_iso_string();
    force_ticket_event_routed_state_at(conn, event_key, route_status, &now)
}

fn force_ticket_event_routed_state_at(
    conn: &Connection,
    event_key: &str,
    route_status: &str,
    updated_at: &str,
) -> Result<()> {
    let previous_route_status = current_ticket_event_route_status(conn, event_key)?;
    enforce_ticket_event_route_status_transition(
        conn,
        event_key,
        &previous_route_status,
        route_status,
        "ctox-ticket-routing",
        "force_ticket_event_routed_state",
        None,
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
    let from_status = canonical_ticket_event_route_status(from_status)?;
    let to_status = canonical_ticket_event_route_status(to_status)?;
    if from_status == to_status {
        return Ok(());
    }
    let from_core = ticket_event_route_core_state(from_status);
    let to_core = ticket_event_route_core_state(to_status);
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
        if let Some(policy_proof) = ticket_event_terminal_policy_proof(actor, reason) {
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
            event: ticket_event_route_core_event(to_status),
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
          AND (
                request_json LIKE '%"reviewed_work_terminal_success":"true"%'
             OR request_json LIKE '%"terminal_policy_proof"%'
          )
        "#,
        params![entity_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count > 0)
}

fn ticket_event_terminal_policy_proof(actor: &str, reason: &str) -> Option<String> {
    match (actor, reason) {
        ("ctox-ticket-routing", "force_ticket_event_routed_state") => {
            Some("policy:ticket-event-routing-observed-or-outbound-terminal".to_string())
        }
        _ => None,
    }
}

fn ticket_event_route_core_state(route_status: &str) -> CoreState {
    match route_status.trim().to_ascii_lowercase().as_str() {
        "leased" => CoreState::Leased,
        "blocked" => CoreState::Blocked,
        "failed" => CoreState::Failed,
        "handled" | "observed" => CoreState::Completed,
        "duplicate" => CoreState::Superseded,
        _ => CoreState::Pending,
    }
}

fn ticket_event_route_core_event(route_status: &str) -> CoreEvent {
    match route_status.trim().to_ascii_lowercase().as_str() {
        "leased" => CoreEvent::Lease,
        "blocked" => CoreEvent::Block,
        "failed" => CoreEvent::Fail,
        "handled" | "observed" => CoreEvent::Complete,
        "duplicate" => CoreEvent::Supersede,
        _ => CoreEvent::Release,
    }
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

    let approval_mode = canonical_control_approval_mode(&input.approval_mode)?;
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
            approval_mode,
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
                "approval_mode": approval_mode,
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
    let requested_approval_mode = canonical_control_approval_mode(&bundle.approval_mode)?;
    let requested_autonomy_level = canonical_autonomy_level(&bundle.autonomy_level)?;
    let allowed_approval_mode = grant
        .as_ref()
        .map(|item| canonical_control_approval_mode(&item.approval_mode))
        .transpose()?
        .unwrap_or(DEFAULT_APPROVAL_MODE);
    let allowed_autonomy_level = grant
        .as_ref()
        .map(|item| canonical_autonomy_level(&item.autonomy_level))
        .transpose()?
        .unwrap_or(DEFAULT_AUTONOMY_LEVEL);

    let approval_mode =
        more_restrictive_approval_mode(requested_approval_mode, allowed_approval_mode).to_string();
    let autonomy_level =
        more_restrictive_autonomy_level(requested_autonomy_level, allowed_autonomy_level)
            .to_string();
    let mut missing_approvals = missing_approvals_for_mode(&approval_mode);
    if grant.is_none()
        && (approval_mode != bundle.approval_mode || autonomy_level != bundle.autonomy_level)
    {
        missing_approvals.push(
            "no active autonomy grant for the current label bundle; using safe default controls"
                .to_string(),
        );
    }

    Ok(EffectiveControlResolution {
        approval_mode,
        autonomy_level,
        missing_approvals,
        grant,
    })
}

fn create_dry_run(
    root: &Path,
    ticket_key: &str,
    understanding: Option<&str>,
    risk_level_override: Option<&str>,
) -> Result<DryRunRecordView> {
    let mut conn = open_ticket_db(root)?;
    let ticket = load_ticket(root, ticket_key)?.context("ticket not found")?;
    let knowledge_load = create_ticket_knowledge_load(root, ticket_key, None)?;
    if !knowledge_load.gap_domains.is_empty() {
        anyhow::bail!(
            "ticket knowledge gate: missing required knowledge domains for {}: {}",
            ticket_key,
            knowledge_load.gap_domains.join(", ")
        );
    }
    let (label_assignment, bundle, effective_control) = resolve_ticket_control(root, ticket_key)?;
    let now = now_iso_string();
    let case_id = format!("case:{}:{}", ticket_key, stable_digest(&now));
    let state = initial_case_state_for_approval_mode(&effective_control.approval_mode);
    let risk_level = risk_level_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(bundle.default_risk_level.as_str())
        .to_string();
    enforce_ticket_case_create_transition(
        &conn,
        &case_id,
        ticket_key,
        state,
        &label_assignment.label,
        &bundle.support_mode,
        "ctox-ticket",
        "create_dry_run",
    )?;
    conn.execute(
        r#"
        INSERT INTO ticket_cases (
            case_id, ticket_key, label, bundle_label, bundle_version, state, approval_mode,
            autonomy_level, support_mode, risk_level, opened_at, updated_at, closed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL)
        "#,
        params![
            case_id,
            ticket_key,
            label_assignment.label,
            bundle.label,
            bundle.bundle_version,
            state,
            effective_control.approval_mode,
            effective_control.autonomy_level,
            bundle.support_mode,
            risk_level,
            now,
        ],
    )?;
    let artifact = build_dry_run_artifact(
        &ticket,
        &label_assignment,
        &bundle,
        &effective_control,
        &knowledge_load,
        understanding,
    );
    let dry_run_id = format!("dry-run:{}:{}", case_id, stable_digest(&now));
    conn.execute(
        r#"
        INSERT INTO ticket_dry_runs (
            dry_run_id, case_id, ticket_key, label, bundle_label, bundle_version, artifact_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            dry_run_id,
            case_id,
            ticket_key,
            label_assignment.label,
            bundle.label,
            bundle.bundle_version,
            serde_json::to_string(&artifact)?,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key,
            case_id: Some(&case_id),
            actor_type: "control_plane",
            action_type: "label_contract_resolution",
            label: Some(&label_assignment.label),
            bundle_label: Some(&bundle.label),
            bundle_version: Some(bundle.bundle_version),
            details: json!({
                "runbook_id": bundle.runbook_id,
                "runbook_version": bundle.runbook_version,
                "policy_id": bundle.policy_id,
                "policy_version": bundle.policy_version,
                "requested_approval_mode": bundle.approval_mode,
                "requested_autonomy_level": bundle.autonomy_level,
                "effective_approval_mode": effective_control.approval_mode,
                "effective_autonomy_level": effective_control.autonomy_level,
                "grant": effective_control.grant.as_ref().map(|grant| {
                    json!({
                        "label": grant.label,
                        "grant_version": grant.grant_version,
                        "bundle_version": grant.bundle_version,
                        "approval_mode": grant.approval_mode,
                        "autonomy_level": grant.autonomy_level,
                        "approved_by": grant.approved_by,
                        "source_candidate_id": grant.source_candidate_id,
                    })
                }),
            }),
        },
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key,
            case_id: Some(&case_id),
            actor_type: "dry_run_engine",
            action_type: "dry_run_record",
            label: Some(&label_assignment.label),
            bundle_label: Some(&bundle.label),
            bundle_version: Some(bundle.bundle_version),
            details: artifact.clone(),
        },
    )?;
    load_latest_dry_run_for_case(root, &case_id)?.context("failed to load dry run after creation")
}

fn build_dry_run_artifact(
    ticket: &TicketItemView,
    label_assignment: &TicketLabelAssignmentView,
    bundle: &ControlBundleView,
    effective_control: &EffectiveControlResolution,
    knowledge_load: &TicketKnowledgeLoadView,
    understanding: Option<&str>,
) -> Value {
    let actions = bundle
        .execution_actions
        .iter()
        .map(|action| {
            let execution_mode = if matches!(action.as_str(), "observe" | "analyze") {
                "executed_in_dry_run"
            } else {
                "simulated_only"
            };
            json!({
                "action_class": action,
                "execution_mode": execution_mode,
                "rationale": action_rationale(action),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "ticket_understanding": understanding
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{} [{}]", ticket.title.trim(), ticket.remote_status.trim())),
        "ticket_key": ticket.ticket_key,
        "knowledge_load": {
            "load_id": knowledge_load.load_id,
            "status": knowledge_load.status,
            "domains": knowledge_load.domains,
            "gap_domains": knowledge_load.gap_domains,
            "entries": knowledge_load.loaded_entries.iter().map(|entry| {
                json!({
                    "domain": entry.domain,
                    "knowledge_key": entry.knowledge_key,
                    "title": entry.title,
                    "summary": entry.summary,
                    "status": entry.status,
                })
            }).collect::<Vec<_>>(),
        },
        "bound_label": label_assignment.label,
        "runbook": {
            "id": bundle.runbook_id,
            "version": bundle.runbook_version,
        },
        "policy": {
            "id": bundle.policy_id,
            "version": bundle.policy_version,
            "approval_mode": effective_control.approval_mode,
            "autonomy_level": effective_control.autonomy_level,
            "support_mode": bundle.support_mode,
            "verification_profile_id": bundle.verification_profile_id,
            "writeback_profile_id": bundle.writeback_profile_id,
        },
        "requested_control": {
            "approval_mode": bundle.approval_mode,
            "autonomy_level": bundle.autonomy_level,
        },
        "autonomy_grant": effective_control.grant.as_ref().map(|grant| {
            json!({
                "grant_version": grant.grant_version,
                "bundle_version": grant.bundle_version,
                "approval_mode": grant.approval_mode,
                "autonomy_level": grant.autonomy_level,
                "approved_by": grant.approved_by,
                "source_candidate_id": grant.source_candidate_id,
            })
        }),
        "planned_actions": actions,
        "executed_now": ["observe", "analyze"],
        "simulated_only": bundle.execution_actions.iter().filter(|item| !matches!(item.as_str(), "observe" | "analyze")).cloned().collect::<Vec<_>>(),
        "missing_approvals": effective_control.missing_approvals,
        "required_evidence": required_evidence_for_bundle(bundle),
    })
}

pub fn list_cases(
    root: &Path,
    ticket_key: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketCaseView>> {
    let conn = open_ticket_db(root)?;
    list_cases_on_conn(&conn, ticket_key, limit)
}

fn list_cases_on_conn(
    conn: &Connection,
    ticket_key: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketCaseView>> {
    let sql = if ticket_key.is_some() {
        r#"
        SELECT case_id, ticket_key, label, bundle_label, bundle_version, state, approval_mode,
               autonomy_level, support_mode, risk_level, opened_at, updated_at, closed_at
        FROM ticket_cases
        WHERE ticket_key = ?1
        ORDER BY updated_at DESC
        LIMIT ?2
        "#
    } else {
        r#"
        SELECT case_id, ticket_key, label, bundle_label, bundle_version, state, approval_mode,
               autonomy_level, support_mode, risk_level, opened_at, updated_at, closed_at
        FROM ticket_cases
        ORDER BY updated_at DESC
        LIMIT ?1
        "#
    };
    let mut statement = conn.prepare(sql)?;
    let rows = if let Some(ticket_key) = ticket_key {
        statement.query_map(params![ticket_key, limit as i64], map_case_row)?
    } else {
        statement.query_map(params![limit as i64], map_case_row)?
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_case(root: &Path, case_id: &str) -> Result<Option<TicketCaseView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT case_id, ticket_key, label, bundle_label, bundle_version, state, approval_mode,
               autonomy_level, support_mode, risk_level, opened_at, updated_at, closed_at
        FROM ticket_cases
        WHERE case_id = ?1
        LIMIT 1
        "#,
        params![case_id],
        map_case_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_ticket_clarification_request(
    root: &Path,
    clarification_id: &str,
) -> Result<Option<TicketClarificationRequestView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT clarification_id, ticket_key, case_id, work_id, target_type, target_channel,
               question, missing_inputs_json, unblock_criteria, status, outbound_message_key,
               inbound_response_key, inbound_response_body, resume_state, created_by,
               created_at, updated_at, sent_at, resolved_at, metadata_json
        FROM ticket_clarification_requests
        WHERE clarification_id = ?1
        LIMIT 1
        "#,
        params![clarification_id],
        map_ticket_clarification_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn load_latest_dry_run_for_case(root: &Path, case_id: &str) -> Result<Option<DryRunRecordView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT dry_run_id, case_id, ticket_key, label, bundle_label, bundle_version, artifact_json, created_at
        FROM ticket_dry_runs
        WHERE case_id = ?1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params![case_id],
        |row| {
            Ok(DryRunRecordView {
                dry_run_id: row.get(0)?,
                case_id: row.get(1)?,
                ticket_key: row.get(2)?,
                label: row.get(3)?,
                bundle_label: row.get(4)?,
                bundle_version: row.get(5)?,
                artifact: parse_json_column(row.get::<_, String>(6)?),
                created_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn decide_case_approval(
    root: &Path,
    case_id: &str,
    status: &str,
    decided_by: &str,
    rationale: Option<&str>,
) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    let canonical_status = canonical_approval_status(status)?;
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_approvals (approval_id, case_id, status, decided_by, rationale, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            format!("approval:{}:{}", case_id, stable_digest(&now)),
            case_id,
            canonical_status,
            decided_by.trim(),
            rationale.map(str::trim),
            now,
        ],
    )?;
    let next_state = if canonical_status == "approved" {
        "executable"
    } else {
        "blocked"
    };
    enforce_ticket_case_state_transition(
        &conn,
        &case,
        next_state,
        "approver",
        "approval_decision",
    )?;
    conn.execute(
        "UPDATE ticket_cases SET state = ?2, updated_at = ?3 WHERE case_id = ?1",
        params![case_id, next_state, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "approver",
            action_type: "approval_decision",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "status": canonical_status,
                "decided_by": decided_by.trim(),
                "rationale": rationale.map(str::trim),
            }),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after approval decision")
}

fn record_execution_action(root: &Path, case_id: &str, summary: &str) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    ensure_case_is_executable(&case)?;
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_execution_actions (
            action_id, case_id, ticket_key, summary, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            format!("execution:{}:{}", case_id, stable_digest(&now)),
            case_id,
            case.ticket_key,
            summary.trim(),
            now,
        ],
    )?;
    enforce_ticket_case_state_transition(&conn, &case, "executing", "agent", "execution_case")?;
    conn.execute(
        "UPDATE ticket_cases SET state = 'executing', updated_at = ?2 WHERE case_id = ?1",
        params![case_id, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "agent",
            action_type: "execution_case",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({"summary": summary.trim()}),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after execution action")
}

fn record_verification(
    root: &Path,
    case_id: &str,
    status: &str,
    summary: Option<&str>,
) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    let canonical_status = canonical_verification_status(status)?;
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_verifications (
            verification_id, case_id, status, summary, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            format!("verification:{}:{}", case_id, stable_digest(&now)),
            case_id,
            canonical_status,
            summary.map(str::trim),
            now,
        ],
    )?;
    let next_state = if canonical_status == "passed" {
        "writeback_pending"
    } else {
        "blocked"
    };
    enforce_ticket_case_state_transition(
        &conn,
        &case,
        next_state,
        "verification_engine",
        "verification_record",
    )?;
    conn.execute(
        "UPDATE ticket_cases SET state = ?2, updated_at = ?3 WHERE case_id = ?1",
        params![case_id, next_state, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "verification_engine",
            action_type: "verification_record",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "status": canonical_status,
                "summary": summary.map(str::trim),
            }),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after verification")
}

fn create_ticket_clarification_request(
    root: &Path,
    input: TicketClarificationRequestInput,
) -> Result<TicketClarificationRequestView> {
    let mut conn = open_ticket_db(root)?;
    let case = match input.case_id.as_deref() {
        Some(case_id) => Some(load_case(root, case_id)?.context("ticket case not found")?),
        None => None,
    };
    let ticket_key = match (&case, input.ticket_key.as_deref()) {
        (Some(case), Some(ticket_key)) if case.ticket_key != ticket_key => {
            anyhow::bail!(
                "clarification ticket_key {} does not match case {} ticket_key {}",
                ticket_key,
                case.case_id,
                case.ticket_key
            );
        }
        (Some(case), _) => case.ticket_key.clone(),
        (None, Some(ticket_key)) => ticket_key.to_string(),
        (None, None) => anyhow::bail!("case_id or ticket_key is required for clarification"),
    };
    let ticket = load_ticket(root, &ticket_key)?.context("ticket not found for clarification")?;
    let target_type = canonical_clarification_target_type(&input.target_type)?;
    let target_channel = canonical_clarification_target_channel(&input.target_channel)?;
    let resume_state = canonical_clarification_resume_state(&input.resume_state)?;
    let question = input.question.trim();
    anyhow::ensure!(!question.is_empty(), "clarification question is required");
    let missing_inputs = normalize_clarification_inputs(input.missing_inputs);
    let now = now_iso_string();
    let clarification_id = format!(
        "clarification:{}:{}",
        ticket_key,
        stable_digest(&(question.to_string() + now.as_str()))
    );
    if let Some(case) = case.as_ref() {
        ensure_case_can_request_clarification(case)?;
        enforce_ticket_case_state_transition(
            &conn,
            case,
            "blocked_needs_clarification",
            "clarification_engine",
            "missing_info_request",
        )?;
    }
    conn.execute(
        r#"
        INSERT INTO ticket_clarification_requests (
            clarification_id, ticket_key, case_id, work_id, target_type, target_channel,
            question, missing_inputs_json, unblock_criteria, status, outbound_message_key,
            inbound_response_key, inbound_response_body, resume_state, created_by,
            metadata_json, created_at, updated_at, sent_at, resolved_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'draft', NULL, NULL, NULL, ?10, ?11, ?12, ?13, ?13, NULL, NULL)
        "#,
        params![
            clarification_id,
            ticket_key,
            case.as_ref().map(|case| case.case_id.as_str()),
            input.work_id.as_deref().map(str::trim),
            &target_type,
            &target_channel,
            question,
            serde_json::to_string(&missing_inputs)?,
            input.unblock_criteria.as_deref().map(str::trim),
            &resume_state,
            input.created_by.trim(),
            serde_json::to_string(&input.metadata)?,
            now,
        ],
    )?;
    if let Some(case) = case.as_ref() {
        conn.execute(
            "UPDATE ticket_cases SET state = 'blocked_needs_clarification', updated_at = ?2 WHERE case_id = ?1",
            params![case.case_id, now],
        )?;
    }
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &ticket.ticket_key,
            case_id: case.as_ref().map(|case| case.case_id.as_str()),
            actor_type: "clarification_engine",
            action_type: "clarification_requested",
            label: case.as_ref().map(|case| case.label.as_str()),
            bundle_label: case.as_ref().map(|case| case.bundle_label.as_str()),
            bundle_version: case.as_ref().map(|case| case.bundle_version),
            details: json!({
                "clarification_id": clarification_id,
                "target_type": target_type,
                "target_channel": target_channel,
                "question": question,
                "missing_inputs": missing_inputs,
                "unblock_criteria": input.unblock_criteria.as_deref().map(str::trim),
            }),
        },
    )?;
    load_ticket_clarification_request(root, &clarification_id)?
        .context("failed to load ticket clarification after create")
}

fn publish_ticket_clarification_request(
    root: &Path,
    clarification_id: &str,
    reviewed_by: &str,
    review_summary: &str,
) -> Result<TicketClarificationRequestView> {
    let mut conn = open_ticket_db(root)?;
    let clarification = load_ticket_clarification_request(root, clarification_id)?
        .context("ticket clarification not found")?;
    anyhow::ensure!(
        matches!(clarification.status.as_str(), "draft" | "send_failed"),
        "clarification {} cannot be published from status {}",
        clarification.clarification_id,
        clarification.status
    );
    anyhow::ensure!(
        !reviewed_by.trim().is_empty() && !review_summary.trim().is_empty(),
        "publishing a clarification requires reviewed_by and review_summary"
    );
    anyhow::ensure!(
        clarification.target_type == "requester" && clarification.target_channel == "ticket",
        "automatic clarification publish currently supports requester ticket comments only"
    );
    let ticket = load_ticket(root, &clarification.ticket_key)?
        .context("ticket not found for clarification")?;
    let Some(adapter) = ticket_adapters::adapter_for_system(&ticket.source_system) else {
        anyhow::bail!(
            "unsupported ticket system for clarification publish: {}",
            ticket.source_system
        );
    };
    let capabilities = adapter.capabilities();
    anyhow::ensure!(
        capabilities.can_comment_writeback && capabilities.can_public_comments,
        "ticket system {} does not support public clarification comments",
        ticket.source_system
    );
    let result = match adapter.writeback_comment(
        root,
        ticket_protocol::TicketCommentWritebackRequest {
            remote_ticket_id: &ticket.remote_ticket_id,
            body: &clarification.question,
            internal: false,
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            let error = err.to_string();
            let now = now_iso_string();
            conn.execute(
                r#"
                UPDATE ticket_clarification_requests
                SET status = 'send_failed',
                    metadata_json = ?2,
                    updated_at = ?3
                WHERE clarification_id = ?1
                "#,
                params![
                    clarification_id,
                    serde_json::to_string(&json!({
                        "previous": clarification.metadata,
                        "send_error": collapse_inline(&error, 1000),
                    }))?,
                    now,
                ],
            )?;
            anyhow::bail!("{}", error);
        }
    };
    mark_remote_events_outbound(root, &ticket.source_system, &result.remote_event_ids)?;
    if let Err(err) = sync_ticket_system(root, &ticket.source_system) {
        let _ = record_ticket_sync_failure(root, &ticket.source_system, &err.to_string());
    }
    let response_baseline_event_keys =
        list_inbound_ticket_event_keys(&conn, &clarification.ticket_key)?;
    let now = now_iso_string();
    let writeback_id = format!(
        "clarification-writeback:{}:{}",
        clarification_id,
        stable_digest(&now)
    );
    conn.execute(
        r#"
        INSERT INTO ticket_writebacks (
            writeback_id, case_id, ticket_key, operation, payload_json, status, created_at
        ) VALUES (?1, ?2, ?3, 'clarification_request', ?4, 'ok', ?5)
        "#,
        params![
            writeback_id,
            clarification.case_id.as_deref().unwrap_or(""),
            &clarification.ticket_key,
            serde_json::to_string(&json!({
                "clarification_id": clarification.clarification_id,
                "body": clarification.question,
                "reviewed_by": reviewed_by.trim(),
                "review_summary": review_summary.trim(),
                "remote_event_ids": result.remote_event_ids.clone(),
            }))?,
            now,
        ],
    )?;
    let outbound_message_key = result
        .remote_event_ids
        .first()
        .cloned()
        .unwrap_or_else(|| writeback_id.clone());
    conn.execute(
        r#"
        UPDATE ticket_clarification_requests
        SET status = 'waiting_for_response',
            outbound_message_key = ?2,
            metadata_json = ?3,
            updated_at = ?4,
            sent_at = ?4
        WHERE clarification_id = ?1
        "#,
        params![
            clarification_id,
            outbound_message_key,
            serde_json::to_string(&json!({
                "previous": clarification.metadata.clone(),
                "reviewed_by": reviewed_by.trim(),
                "review_summary": review_summary.trim(),
                "writeback_id": writeback_id,
                "response_baseline_event_keys": response_baseline_event_keys,
            }))?,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &ticket.ticket_key,
            case_id: clarification.case_id.as_deref(),
            actor_type: "clarification_engine",
            action_type: "clarification_published",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "clarification_id": clarification_id,
                "reviewed_by": reviewed_by.trim(),
                "review_summary": review_summary.trim(),
                "outbound_message_key": outbound_message_key,
            }),
        },
    )?;
    load_ticket_clarification_request(root, clarification_id)?
        .context("failed to load ticket clarification after publish")
}

fn resolve_ticket_clarification_request(
    root: &Path,
    clarification_id: &str,
    response_key: &str,
    response_body: Option<&str>,
    resolved_by: &str,
) -> Result<TicketClarificationRequestView> {
    let mut conn = open_ticket_db(root)?;
    let clarification = load_ticket_clarification_request(root, clarification_id)?
        .context("ticket clarification not found")?;
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE ticket_clarification_requests
        SET status = 'resolved',
            inbound_response_key = ?2,
            inbound_response_body = ?3,
            updated_at = ?4,
            resolved_at = ?4,
            metadata_json = ?5
        WHERE clarification_id = ?1
        "#,
        params![
            clarification_id,
            response_key.trim(),
            response_body.map(str::trim),
            now,
            serde_json::to_string(&json!({
                "previous": clarification.metadata.clone(),
                "resolved_by": resolved_by.trim(),
            }))?,
        ],
    )?;
    if let Some(case_id) = clarification.case_id.as_deref() {
        if let Some(case) = load_case(root, case_id)? {
            if matches!(
                case.state.as_str(),
                "blocked" | "blocked_needs_clarification"
            ) {
                enforce_ticket_case_state_transition(
                    &conn,
                    &case,
                    &clarification.resume_state,
                    "clarification_engine",
                    "clarification_resolved",
                )?;
                conn.execute(
                    "UPDATE ticket_cases SET state = ?2, updated_at = ?3 WHERE case_id = ?1",
                    params![case_id, clarification.resume_state, now],
                )?;
            }
        }
    }
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &clarification.ticket_key,
            case_id: clarification.case_id.as_deref(),
            actor_type: "clarification_engine",
            action_type: "clarification_resolved",
            label: None,
            bundle_label: None,
            bundle_version: None,
            details: json!({
                "clarification_id": clarification_id,
                "response_key": response_key.trim(),
                "resolved_by": resolved_by.trim(),
            }),
        },
    )?;
    load_ticket_clarification_request(root, clarification_id)?
        .context("failed to load ticket clarification after resolve")
}

fn resolve_waiting_clarifications_from_inbound_events(root: &Path, system: &str) -> Result<usize> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT c.clarification_id, c.ticket_key, COALESCE(c.sent_at, c.created_at), c.metadata_json
        FROM ticket_clarification_requests c
        JOIN ticket_items t ON t.ticket_key = c.ticket_key
        WHERE t.source_system = ?1
          AND c.status = 'waiting_for_response'
        ORDER BY c.created_at ASC
        "#,
    )?;
    let rows = statement.query_map(params![system], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let waiting = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)?;
    drop(statement);
    drop(conn);

    let mut resolved = 0usize;
    for (clarification_id, ticket_key, since, metadata_raw) in waiting {
        let response_baseline_event_keys =
            clarification_response_baseline_event_keys(&parse_json_or_empty(&metadata_raw));
        let conn = open_ticket_db(root)?;
        let mut response_statement = conn.prepare(
            r#"
                SELECT event_key, body_text
                FROM ticket_events
                WHERE ticket_key = ?1
                  AND direction = 'inbound'
                  AND observed_at >= ?2
                  AND trim(body_text) <> ''
                ORDER BY external_created_at ASC, observed_at ASC, event_key ASC
                "#,
        )?;
        let response_rows = response_statement.query_map(params![ticket_key, since], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let response = response_rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)?
            .into_iter()
            .find(|(event_key, _)| !response_baseline_event_keys.contains(event_key));
        drop(response_statement);
        drop(conn);
        if let Some((event_key, body_text)) = response {
            resolve_ticket_clarification_request(
                root,
                &clarification_id,
                &event_key,
                Some(&body_text),
                "ticket-sync",
            )?;
            resolved += 1;
        }
    }
    Ok(resolved)
}

fn list_inbound_ticket_event_keys(conn: &Connection, ticket_key: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        r#"
        SELECT event_key
        FROM ticket_events
        WHERE ticket_key = ?1
          AND direction = 'inbound'
        ORDER BY external_created_at ASC, observed_at ASC, event_key ASC
        "#,
    )?;
    let rows = statement.query_map(params![ticket_key], |row| row.get::<_, String>(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn clarification_response_baseline_event_keys(metadata: &Value) -> BTreeSet<String> {
    metadata
        .get("response_baseline_event_keys")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn writeback_comment(
    root: &Path,
    case_id: &str,
    body: &str,
    internal: bool,
) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    ensure_case_ready_for_writeback(&case)?;
    let ticket = load_ticket(root, &case.ticket_key)?.context("ticket not found for case")?;
    let Some(adapter) = ticket_adapters::adapter_for_system(&ticket.source_system) else {
        anyhow::bail!(
            "unsupported ticket system for writeback: {}",
            ticket.source_system
        );
    };
    let capabilities = adapter.capabilities();
    if !capabilities.can_comment_writeback {
        anyhow::bail!(
            "ticket system {} does not support comment writeback",
            ticket.source_system
        );
    }
    if internal && !capabilities.can_internal_comments {
        anyhow::bail!(
            "ticket system {} does not support internal comments",
            ticket.source_system
        );
    }
    if !internal && !capabilities.can_public_comments {
        anyhow::bail!(
            "ticket system {} does not support public comments",
            ticket.source_system
        );
    }
    let result = match adapter.writeback_comment(
        root,
        ticket_protocol::TicketCommentWritebackRequest {
            remote_ticket_id: &ticket.remote_ticket_id,
            body,
            internal,
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            let error = err.to_string();
            record_failed_writeback(
                &mut conn,
                &case,
                "comment",
                json!({
                    "body": body.trim(),
                    "internal": internal,
                    "remote_ticket_id": ticket.remote_ticket_id.clone(),
                    "source_system": ticket.source_system.clone(),
                }),
                &error,
            )?;
            anyhow::bail!("{}", error);
        }
    };
    mark_remote_events_outbound(root, &ticket.source_system, &result.remote_event_ids)?;
    if let Err(err) = sync_ticket_system(root, &ticket.source_system) {
        let _ = record_ticket_sync_failure(root, &ticket.source_system, &err.to_string());
    }
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_writebacks (
            writeback_id, case_id, ticket_key, operation, payload_json, status, created_at
        ) VALUES (?1, ?2, ?3, 'comment', ?4, 'ok', ?5)
        "#,
        params![
            format!("writeback:{}:{}", case_id, stable_digest(&now)),
            case_id,
            case.ticket_key,
            serde_json::to_string(&json!({
                "body": body.trim(),
                "internal": internal,
                "remote_event_ids": result.remote_event_ids,
            }))?,
            now,
        ],
    )?;
    conn.execute(
        "UPDATE ticket_cases SET updated_at = ?2 WHERE case_id = ?1",
        params![case_id, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "writeback_engine",
            action_type: "writeback_record",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "operation": "comment",
                "body": body.trim(),
                "internal": internal,
            }),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after writeback")
}

fn writeback_transition(
    root: &Path,
    case_id: &str,
    state: &str,
    note_body: Option<&str>,
    internal_note: bool,
) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    ensure_case_ready_for_writeback(&case)?;
    let ticket = load_ticket(root, &case.ticket_key)?.context("ticket not found for case")?;
    let Some(adapter) = ticket_adapters::adapter_for_system(&ticket.source_system) else {
        anyhow::bail!(
            "unsupported ticket system for writeback: {}",
            ticket.source_system
        );
    };
    let capabilities = adapter.capabilities();
    if !capabilities.can_transition_writeback {
        anyhow::bail!(
            "ticket system {} does not support state transitions",
            ticket.source_system
        );
    }
    if internal_note && !capabilities.can_internal_comments {
        anyhow::bail!(
            "ticket system {} does not support internal notes on transitions",
            ticket.source_system
        );
    }
    if note_body.is_some() && !internal_note && !capabilities.can_public_comments {
        anyhow::bail!(
            "ticket system {} does not support public transition notes",
            ticket.source_system
        );
    }
    enforce_ticket_case_close_transition(&conn, &case, "writeback_engine")?;
    let result = match adapter.writeback_transition(
        root,
        ticket_protocol::TicketTransitionWritebackRequest {
            remote_ticket_id: &ticket.remote_ticket_id,
            state,
            note_body,
            internal_note,
            control_note: None,
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            let error = err.to_string();
            record_failed_writeback(
                &mut conn,
                &case,
                "transition",
                json!({
                    "state": state.trim(),
                    "note_body": note_body.map(str::trim),
                    "internal_note": internal_note,
                    "remote_ticket_id": ticket.remote_ticket_id.clone(),
                    "source_system": ticket.source_system.clone(),
                }),
                &error,
            )?;
            anyhow::bail!("{}", error);
        }
    };
    mark_remote_events_outbound(root, &ticket.source_system, &result.remote_event_ids)?;
    if let Err(err) = sync_ticket_system(root, &ticket.source_system) {
        let _ = record_ticket_sync_failure(root, &ticket.source_system, &err.to_string());
    }
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_writebacks (
            writeback_id, case_id, ticket_key, operation, payload_json, status, created_at
        ) VALUES (?1, ?2, ?3, 'transition', ?4, 'ok', ?5)
        "#,
        params![
            format!(
                "writeback:{}:{}",
                case_id,
                stable_digest(&(state.to_string() + now.as_str()))
            ),
            case_id,
            case.ticket_key,
            serde_json::to_string(&json!({
                "state": state.trim(),
                "note_body": note_body.map(str::trim),
                "internal_note": internal_note,
                "remote_event_ids": result.remote_event_ids,
            }))?,
            now,
        ],
    )?;
    enforce_ticket_case_close_transition(&conn, &case, "writeback_engine")?;
    conn.execute(
        "UPDATE ticket_cases SET state = 'closed', updated_at = ?2, closed_at = ?2 WHERE case_id = ?1",
        params![case_id, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "writeback_engine",
            action_type: "writeback_record",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "operation": "transition",
                "state": state.trim(),
                "note_body": note_body.map(str::trim),
                "internal_note": internal_note,
            }),
        },
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "control_plane",
            action_type: "case_closed",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({"reason": "writeback transition completed"}),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after transition writeback")
}

fn close_case(root: &Path, case_id: &str, summary: Option<&str>) -> Result<TicketCaseView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    enforce_ticket_case_close_transition(&conn, &case, "control_plane")?;
    let now = now_iso_string();
    conn.execute(
        "UPDATE ticket_cases SET state = 'closed', updated_at = ?2, closed_at = ?2 WHERE case_id = ?1",
        params![case_id, now],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "control_plane",
            action_type: "case_closed",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({"summary": summary.map(str::trim)}),
        },
    )?;
    load_case(root, case_id)?.context("failed to load case after close")
}

pub(crate) fn run_business_os_ticket_command(
    root: &Path,
    command_type: &str,
    payload: &Value,
) -> Result<Value> {
    match command_type {
        "ctox.ticket.local.create" => {
            let title = required_payload_string(payload, "title")?;
            let body = payload_string(payload, "body")
                .or_else(|| payload_string(payload, "body_text"))
                .unwrap_or_default();
            let record = crate::mission::ticket_local_native::create_local_ticket(
                root,
                &title,
                &body,
                payload_string(payload, "status").as_deref(),
                payload_string(payload, "priority").as_deref(),
            )?;
            sync_ticket_system(root, "local")?;
            Ok(json!({
                "ticket": record,
                "ticket_key": record.ticket_id,
                "source_system": "local",
            }))
        }
        "ctox.ticket.local.comment" => {
            let ticket_id = required_payload_string(payload, "ticket_id")
                .or_else(|_| required_payload_string(payload, "ticket_key"))?;
            let body = required_payload_string(payload, "body")?;
            let event =
                crate::mission::ticket_local_native::add_local_comment(root, &ticket_id, &body)?;
            sync_ticket_system(root, "local")?;
            Ok(json!({
                "event": event,
                "ticket_key": ticket_id,
                "source_system": "local",
            }))
        }
        "ctox.ticket.local.transition" => {
            let ticket_id = required_payload_string(payload, "ticket_id")
                .or_else(|_| required_payload_string(payload, "ticket_key"))?;
            let status = required_payload_string(payload, "status")
                .or_else(|_| required_payload_string(payload, "state"))?;
            let record = crate::mission::ticket_local_native::transition_local_ticket(
                root, &ticket_id, &status,
            )?;
            sync_ticket_system(root, "local")?;
            Ok(json!({
                "ticket": record,
                "ticket_key": ticket_id,
                "source_system": "local",
            }))
        }
        "ctox.ticket.approve" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let status = required_payload_string(payload, "status")?;
            let decided_by =
                payload_string(payload, "decided_by").unwrap_or_else(|| "owner".to_string());
            let case = decide_case_approval(
                root,
                &case_id,
                &status,
                &decided_by,
                payload_string(payload, "rationale").as_deref(),
            )?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        "ctox.ticket.execute" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let summary = required_payload_string(payload, "summary")?;
            let case = record_execution_action(root, &case_id, &summary)?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        "ctox.ticket.verify" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let status = required_payload_string(payload, "status")?;
            let case = record_verification(
                root,
                &case_id,
                &status,
                payload_string(payload, "summary").as_deref(),
            )?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        "ctox.ticket.request_clarification" => {
            let case_id = payload_string(payload, "case_id");
            let ticket_key = payload_string(payload, "ticket_key");
            let question = required_payload_string(payload, "question")?;
            let missing_inputs = payload
                .get("missing_inputs")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    payload_string(payload, "missing_inputs_csv")
                        .map(|value| parse_domain_csv(&value))
                        .unwrap_or_default()
                });
            let clarification = create_ticket_clarification_request(
                root,
                TicketClarificationRequestInput {
                    case_id,
                    ticket_key,
                    work_id: payload_string(payload, "work_id"),
                    target_type: payload_string(payload, "target_type")
                        .unwrap_or_else(|| "requester".to_string()),
                    target_channel: payload_string(payload, "target_channel")
                        .unwrap_or_else(|| "ticket".to_string()),
                    question,
                    missing_inputs,
                    unblock_criteria: payload_string(payload, "unblock_criteria"),
                    resume_state: payload_string(payload, "resume_state")
                        .unwrap_or_else(|| "executable".to_string()),
                    created_by: payload_string(payload, "created_by")
                        .unwrap_or_else(|| "business-os".to_string()),
                    metadata: payload
                        .get("metadata")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                },
            )?;
            let clarification_id = clarification.clarification_id.clone();
            let case_id = clarification.case_id.clone();
            let ticket_key = clarification.ticket_key.clone();
            Ok(json!({
                "clarification": clarification,
                "clarification_id": clarification_id,
                "case_id": case_id,
                "ticket_key": ticket_key
            }))
        }
        "ctox.ticket.publish_clarification" => {
            let clarification_id = required_payload_string(payload, "clarification_id")?;
            let reviewed_by =
                payload_string(payload, "reviewed_by").unwrap_or_else(|| "business-os".to_string());
            let review_summary = payload_string(payload, "review_summary")
                .unwrap_or_else(|| "Clarification question reviewed for this ticket.".to_string());
            let clarification = publish_ticket_clarification_request(
                root,
                &clarification_id,
                &reviewed_by,
                &review_summary,
            )?;
            let case_id = clarification.case_id.clone();
            let ticket_key = clarification.ticket_key.clone();
            Ok(json!({
                "clarification": clarification,
                "clarification_id": clarification_id,
                "case_id": case_id,
                "ticket_key": ticket_key
            }))
        }
        "ctox.ticket.resolve_clarification" => {
            let clarification_id = required_payload_string(payload, "clarification_id")?;
            let response_key = required_payload_string(payload, "response_key")?;
            let clarification = resolve_ticket_clarification_request(
                root,
                &clarification_id,
                &response_key,
                payload_string(payload, "body").as_deref(),
                payload_string(payload, "resolved_by")
                    .as_deref()
                    .unwrap_or("business-os"),
            )?;
            let case_id = clarification.case_id.clone();
            let ticket_key = clarification.ticket_key.clone();
            Ok(json!({
                "clarification": clarification,
                "clarification_id": clarification_id,
                "case_id": case_id,
                "ticket_key": ticket_key
            }))
        }
        "ctox.ticket.writeback_comment" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let body = required_payload_string(payload, "body")?;
            let case = writeback_comment(
                root,
                &case_id,
                &body,
                payload_bool(payload, "internal").unwrap_or(false),
            )?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        "ctox.ticket.writeback_transition" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let state = required_payload_string(payload, "state")?;
            let case = writeback_transition(
                root,
                &case_id,
                &state,
                payload_string(payload, "body").as_deref(),
                payload_bool(payload, "internal").unwrap_or(false),
            )?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        "ctox.ticket.close" => {
            let case_id = required_payload_string(payload, "case_id")?;
            let case = close_case(
                root,
                &case_id,
                payload_string(payload, "summary").as_deref(),
            )?;
            Ok(json!({ "case": case, "case_id": case.case_id, "ticket_key": case.ticket_key }))
        }
        other => anyhow::bail!("unsupported Business OS ticket command: {other}"),
    }
}

fn required_payload_string(payload: &Value, key: &str) -> Result<String> {
    payload_string(payload, key)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("{key} is required"))
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn payload_bool(payload: &Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

fn enforce_ticket_case_close_transition(
    conn: &Connection,
    case: &TicketCaseView,
    actor: &str,
) -> Result<()> {
    let verification_id = latest_passed_ticket_verification_id(conn, &case.case_id)?;
    let from_state = ticket_case_core_state(&case.state)?;
    let mut metadata = BTreeMap::new();
    metadata.insert("ticket_key".to_string(), case.ticket_key.clone());
    metadata.insert("label".to_string(), case.label.clone());
    metadata.insert("support_mode".to_string(), case.support_mode.clone());
    metadata.insert("owner_visible_completion".to_string(), "true".to_string());
    metadata.insert("completion_review_required".to_string(), "true".to_string());
    metadata.insert("completion_review_verdict".to_string(), "pass".to_string());
    metadata.insert(
        "reviewed_work_terminal_success".to_string(),
        "true".to_string(),
    );

    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::Ticket,
            entity_id: case.case_id.clone(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state,
            to_state: CoreState::Closed,
            event: CoreEvent::Close,
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs {
                verification_id,
                review_audit_key: latest_ticket_review_audit_key(conn, &case.case_id)?,
                ..CoreEvidenceRefs::default()
            },
            metadata,
        },
    )?;
    Ok(())
}

fn enforce_ticket_case_create_transition(
    conn: &Connection,
    case_id: &str,
    ticket_key: &str,
    state: &str,
    label: &str,
    support_mode: &str,
    actor: &str,
    reason: &str,
) -> Result<()> {
    let to_core_state = ticket_case_core_state(state)?;
    let mut metadata = BTreeMap::new();
    metadata.insert("ticket_key".to_string(), ticket_key.to_string());
    metadata.insert("label".to_string(), label.to_string());
    metadata.insert("support_mode".to_string(), support_mode.to_string());
    metadata.insert("from_case_state".to_string(), "created".to_string());
    metadata.insert("to_case_state".to_string(), state.to_string());
    metadata.insert("reason".to_string(), reason.to_string());
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::Ticket,
            entity_id: case_id.to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Created,
            to_state: to_core_state,
            event: ticket_case_core_event(state),
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        },
    )?;
    Ok(())
}

fn enforce_ticket_case_state_transition(
    conn: &Connection,
    case: &TicketCaseView,
    to_state: &str,
    actor: &str,
    reason: &str,
) -> Result<()> {
    let from_state = ticket_case_core_state(&case.state)?;
    let to_core_state = ticket_case_core_state(to_state)?;
    let mut metadata = BTreeMap::new();
    metadata.insert("ticket_key".to_string(), case.ticket_key.clone());
    metadata.insert("label".to_string(), case.label.clone());
    metadata.insert("support_mode".to_string(), case.support_mode.clone());
    metadata.insert("from_case_state".to_string(), case.state.clone());
    metadata.insert("to_case_state".to_string(), to_state.to_string());
    metadata.insert("reason".to_string(), reason.to_string());
    enforce_core_transition(
        conn,
        &CoreTransitionRequest {
            entity_type: CoreEntityType::Ticket,
            entity_id: case.case_id.clone(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state,
            to_state: to_core_state,
            event: ticket_case_core_event(to_state),
            actor: actor.to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        },
    )?;
    Ok(())
}

fn latest_passed_ticket_verification_id(
    conn: &Connection,
    case_id: &str,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        SELECT verification_id
        FROM ticket_verifications
        WHERE case_id = ?1 AND status = 'passed'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params![case_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn latest_ticket_review_audit_key(conn: &Connection, case_id: &str) -> Result<Option<String>> {
    conn.query_row(
        r#"
        SELECT audit_id
        FROM ticket_audit_log
        WHERE case_id = ?1
          AND action_type IN ('source_skill_review_note', 'approval_decision', 'verification_record')
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        params![case_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn ticket_case_core_state(raw: &str) -> Result<CoreState> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "created" | "open" | "queued" => Ok(CoreState::Created),
        "classified" => Ok(CoreState::Classified),
        "planned" | "ready" | "executable" => Ok(CoreState::Planned),
        "executing" | "in_progress" | "running" => Ok(CoreState::Executing),
        "approval_pending" | "awaiting_review" | "review" | "reviewing" => {
            Ok(CoreState::AwaitingReview)
        }
        "rework_required" | "rework" => Ok(CoreState::ReworkRequired),
        "awaiting_verification" | "verification" => Ok(CoreState::AwaitingVerification),
        "verified" | "writeback_pending" => Ok(CoreState::Verified),
        "closed" | "done" | "completed" => Ok(CoreState::Closed),
        "blocked" | "blocked_needs_clarification" => Ok(CoreState::Blocked),
        other => anyhow::bail!("ticket case state is not mapped to core state machine: {other}"),
    }
}

fn ticket_case_core_event(state: &str) -> CoreEvent {
    match state.trim().to_ascii_lowercase().as_str() {
        "classified" => CoreEvent::Classify,
        "planned" | "ready" | "executable" => CoreEvent::Plan,
        "executing" | "in_progress" | "running" => CoreEvent::Execute,
        "approval_pending" | "awaiting_review" | "review" | "reviewing" => CoreEvent::RequestReview,
        "rework_required" | "rework" => CoreEvent::RequireRework,
        "awaiting_verification" | "verification" => CoreEvent::Verify,
        "verified" | "writeback_pending" => CoreEvent::Verify,
        "closed" | "done" | "completed" => CoreEvent::Close,
        "blocked" | "blocked_needs_clarification" => CoreEvent::Block,
        _ => CoreEvent::CreateTicket,
    }
}

fn create_learning_candidate(
    root: &Path,
    case_id: &str,
    summary: &str,
    proposed_actions_override: Option<&[String]>,
    evidence_override: Option<Value>,
) -> Result<LearningCandidateView> {
    let mut conn = open_ticket_db(root)?;
    let case = load_case(root, case_id)?.context("ticket case not found")?;
    let dry_run = load_latest_dry_run_for_case(root, case_id)?
        .context("dry run is required before creating a learning candidate")?;
    let proposed_actions = proposed_actions_override
        .map(|items| items.to_vec())
        .unwrap_or_else(|| {
            dry_run
                .artifact
                .get("planned_actions")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("action_class").and_then(Value::as_str))
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .filter(|items| !items.is_empty())
                .unwrap_or_else(default_execution_actions)
        });
    let evidence = evidence_override.unwrap_or_else(|| {
        json!({
            "case_state": case.state,
            "dry_run_id": dry_run.dry_run_id,
            "dry_run_artifact": dry_run.artifact,
        })
    });
    let now = now_iso_string();
    let candidate_id = format!("candidate:{}:{}", case_id, stable_digest(&now));
    conn.execute(
        r#"
        INSERT INTO ticket_learning_candidates (
            candidate_id, case_id, ticket_key, label, bundle_label, bundle_version,
            summary, proposed_actions_json, evidence_json, status, proposed_at,
            decided_at, decided_by, decision_notes, promoted_autonomy_level
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'proposed', ?10, NULL, NULL, NULL, NULL)
        "#,
        params![
            candidate_id,
            case_id,
            case.ticket_key,
            case.label,
            case.bundle_label,
            case.bundle_version,
            summary.trim(),
            serde_json::to_string(&proposed_actions)?,
            serde_json::to_string(&evidence)?,
            now,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(case_id),
            actor_type: "learning_engine",
            action_type: "learning_candidate",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "candidate_id": candidate_id,
                "summary": summary.trim(),
                "proposed_actions": proposed_actions,
            }),
        },
    )?;
    load_learning_candidate(root, &candidate_id)?
        .context("failed to load learning candidate after create")
}

fn list_learning_candidates(
    root: &Path,
    label: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<LearningCandidateView>> {
    let conn = open_ticket_db(root)?;
    let mut statement = conn.prepare(
        r#"
        SELECT candidate_id, case_id, ticket_key, label, bundle_label, bundle_version, summary,
               proposed_actions_json, evidence_json, status, proposed_at, decided_at, decided_by,
               decision_notes, promoted_autonomy_level
        FROM ticket_learning_candidates
        WHERE (?1 IS NULL OR label = ?1)
          AND (?2 IS NULL OR status = ?2)
        ORDER BY proposed_at DESC
        LIMIT ?3
        "#,
    )?;
    let rows = statement.query_map(
        params![label, status, limit as i64],
        map_learning_candidate_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_learning_candidate(
    root: &Path,
    candidate_id: &str,
) -> Result<Option<LearningCandidateView>> {
    let conn = open_ticket_db(root)?;
    conn.query_row(
        r#"
        SELECT candidate_id, case_id, ticket_key, label, bundle_label, bundle_version, summary,
               proposed_actions_json, evidence_json, status, proposed_at, decided_at, decided_by,
               decision_notes, promoted_autonomy_level
        FROM ticket_learning_candidates
        WHERE candidate_id = ?1
        LIMIT 1
        "#,
        params![candidate_id],
        map_learning_candidate_row,
    )
    .optional()
    .map_err(anyhow::Error::from)
}

fn decide_learning_candidate(
    root: &Path,
    candidate_id: &str,
    status: &str,
    decided_by: &str,
    notes: Option<&str>,
    promoted_autonomy_level: Option<&str>,
) -> Result<LearningCandidateView> {
    let mut conn = open_ticket_db(root)?;
    let candidate =
        load_learning_candidate(root, candidate_id)?.context("learning candidate not found")?;
    let canonical_status = canonical_learning_candidate_status(status)?;
    let promoted_autonomy_level = promoted_autonomy_level
        .map(canonical_autonomy_level)
        .transpose()?
        .map(ToOwned::to_owned);
    let now = now_iso_string();
    conn.execute(
        r#"
        UPDATE ticket_learning_candidates
        SET status = ?2,
            decided_at = ?3,
            decided_by = ?4,
            decision_notes = ?5,
            promoted_autonomy_level = ?6
        WHERE candidate_id = ?1
        "#,
        params![
            candidate_id,
            canonical_status,
            now,
            decided_by.trim(),
            notes.map(str::trim),
            promoted_autonomy_level,
        ],
    )?;
    record_audit(
        &mut conn,
        AuditRequest {
            ticket_key: &candidate.ticket_key,
            case_id: Some(&candidate.case_id),
            actor_type: "approver",
            action_type: "learning_candidate_decision",
            label: Some(&candidate.label),
            bundle_label: Some(&candidate.bundle_label),
            bundle_version: Some(candidate.bundle_version),
            details: json!({
                "candidate_id": candidate_id,
                "status": canonical_status,
                "decided_by": decided_by.trim(),
                "notes": notes.map(str::trim),
                "promoted_autonomy_level": promoted_autonomy_level,
            }),
        },
    )?;
    load_learning_candidate(root, candidate_id)?
        .context("failed to load learning candidate after decision")
}

fn list_audit_records(
    root: &Path,
    ticket_key: Option<&str>,
    limit: usize,
) -> Result<Vec<TicketAuditRecord>> {
    let conn = open_ticket_db(root)?;
    let sql = if ticket_key.is_some() {
        r#"
        SELECT audit_id, ticket_key, case_id, actor_type, action_type, label, bundle_label,
               bundle_version, details_json, created_at
        FROM ticket_audit_log
        WHERE ticket_key = ?1
        ORDER BY created_at DESC
        LIMIT ?2
        "#
    } else {
        r#"
        SELECT audit_id, ticket_key, case_id, actor_type, action_type, label, bundle_label,
               bundle_version, details_json, created_at
        FROM ticket_audit_log
        ORDER BY created_at DESC
        LIMIT ?1
        "#
    };
    let mut statement = conn.prepare(sql)?;
    let rows = if let Some(ticket_key) = ticket_key {
        statement.query_map(params![ticket_key, limit as i64], map_audit_row)?
    } else {
        statement.query_map(params![limit as i64], map_audit_row)?
    };
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn ensure_case_is_executable(case: &TicketCaseView) -> Result<()> {
    match case.state.as_str() {
        "executable" | "executing" => Ok(()),
        other => anyhow::bail!(
            "case {} is not executable; current state is {}",
            case.case_id,
            other
        ),
    }
}

fn ensure_case_can_request_clarification(case: &TicketCaseView) -> Result<()> {
    match case.state.as_str() {
        "closed" | "done" | "completed" | "verified" | "writeback_pending" => anyhow::bail!(
            "case {} cannot request clarification from terminal/writeback state {}",
            case.case_id,
            case.state
        ),
        _ => Ok(()),
    }
}

fn ensure_case_ready_for_writeback(case: &TicketCaseView) -> Result<()> {
    match case.state.as_str() {
        "writeback_pending" | "verifying" => Ok(()),
        other => anyhow::bail!(
            "case {} is not ready for writeback; current state is {}",
            case.case_id,
            other
        ),
    }
}

fn canonical_clarification_target_type(raw: &str) -> Result<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "requester" | "customer" | "ticket_requester" => Ok("requester".to_string()),
        "owner" | "founder" | "admin" => Ok("owner".to_string()),
        "internal" | "team" | "operator" => Ok("internal".to_string()),
        other => anyhow::bail!(
            "unsupported clarification target_type '{other}' (expected requester|owner|internal)"
        ),
    }
}

fn canonical_clarification_target_channel(raw: &str) -> Result<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ticket" | "email" | "jami" | "tui" | "teams" | "whatsapp" => {
            Ok(raw.trim().to_ascii_lowercase())
        }
        other => anyhow::bail!(
            "unsupported clarification target_channel '{other}' (expected ticket|email|jami|tui|teams|whatsapp)"
        ),
    }
}

fn canonical_clarification_resume_state(raw: &str) -> Result<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "planned" | "ready" | "executable" => Ok("executable".to_string()),
        other => {
            anyhow::bail!("unsupported clarification resume_state '{other}' (expected executable)")
        }
    }
}

fn normalize_clarification_inputs(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn record_failed_writeback(
    conn: &mut Connection,
    case: &TicketCaseView,
    operation: &str,
    payload: Value,
    error: &str,
) -> Result<()> {
    let now = now_iso_string();
    conn.execute(
        r#"
        INSERT INTO ticket_writebacks (
            writeback_id, case_id, ticket_key, operation, payload_json, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'failed', ?6)
        "#,
        params![
            format!(
                "writeback-failed:{}:{}",
                case.case_id,
                stable_digest(&(operation.to_string() + error + now.as_str()))
            ),
            case.case_id,
            case.ticket_key,
            operation,
            serde_json::to_string(&json!({
                "payload": payload,
                "error": collapse_inline(error, 1000),
            }))?,
            now,
        ],
    )?;
    record_audit(
        conn,
        AuditRequest {
            ticket_key: &case.ticket_key,
            case_id: Some(&case.case_id),
            actor_type: "writeback_engine",
            action_type: "writeback_failed",
            label: Some(&case.label),
            bundle_label: Some(&case.bundle_label),
            bundle_version: Some(case.bundle_version),
            details: json!({
                "operation": operation,
                "error": collapse_inline(error, 1000),
            }),
        },
    )
}

struct AuditRequest<'a> {
    ticket_key: &'a str,
    case_id: Option<&'a str>,
    actor_type: &'a str,
    action_type: &'a str,
    label: Option<&'a str>,
    bundle_label: Option<&'a str>,
    bundle_version: Option<i64>,
    details: Value,
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
        force_ticket_event_routed_state_at(conn, &event_key, &route_status, &observed_at)?;
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
    match raw.trim() {
        "pending" => Ok("pending"),
        "leased" => Ok("leased"),
        "observed" => Ok("observed"),
        "handled" => Ok("handled"),
        "failed" => Ok("failed"),
        "duplicate" => Ok("duplicate"),
        "blocked" => Ok("blocked"),
        other => anyhow::bail!("unsupported ticket event route status: {other}"),
    }
}

fn canonical_control_approval_mode(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "dry_run_only" => Ok("dry_run_only"),
        "human_approval_required" => Ok("human_approval_required"),
        "bounded_auto_execute" => Ok("bounded_auto_execute"),
        "direct_execute_allowed" => Ok("direct_execute_allowed"),
        other => anyhow::bail!("unsupported approval mode: {other}"),
    }
}

fn approval_mode_rank(mode: &str) -> Result<u8> {
    match canonical_control_approval_mode(mode)? {
        "dry_run_only" => Ok(0),
        "human_approval_required" => Ok(1),
        "bounded_auto_execute" => Ok(2),
        "direct_execute_allowed" => Ok(3),
        _ => unreachable!(),
    }
}

fn more_restrictive_approval_mode<'a>(left: &'a str, right: &'a str) -> &'a str {
    let left_rank = approval_mode_rank(left).unwrap_or(0);
    let right_rank = approval_mode_rank(right).unwrap_or(0);
    if left_rank <= right_rank {
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

fn missing_approvals_for_mode(mode: &str) -> Vec<String> {
    match mode {
        "dry_run_only" => vec!["execution is disabled for this bundle".to_string()],
        "human_approval_required" => vec!["owner or designated approver".to_string()],
        "bounded_auto_execute" | "direct_execute_allowed" => Vec::new(),
        _ => vec!["approval mode not recognized; require manual confirmation".to_string()],
    }
}

fn required_evidence_for_bundle(bundle: &ControlBundleView) -> Vec<String> {
    vec![
        format!("verification profile: {}", bundle.verification_profile_id),
        format!("writeback profile: {}", bundle.writeback_profile_id),
        format!("policy: {} {}", bundle.policy_id, bundle.policy_version),
    ]
}

fn initial_case_state_for_approval_mode(mode: &str) -> &'static str {
    match mode {
        "dry_run_only" => "blocked",
        "human_approval_required" => "approval_pending",
        "bounded_auto_execute" | "direct_execute_allowed" => "executable",
        _ => "approval_pending",
    }
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
    let metadata = parse_json_column(row.get::<_, String>(6)?);
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
        state: row.get(5)?,
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
mod tests {
    // ctox-allow-direct-state-write: migration tests below construct historical
    // table layouts directly and verify lossless schema conversion/rollback.
    use super::*;
    use crate::mission::ticket_local_native;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ctox-ticket-test-{}-{}",
            label,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn ticket_self_work_migration_preserves_rows_and_drops_unique() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r#"
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
                updated_at TEXT NOT NULL,
                UNIQUE(source_system, kind)
            );
            INSERT INTO ticket_self_work_items
                (work_id, source_system, kind, title, body_text, state, metadata_json, created_at, updated_at)
            VALUES
                ('w1', 'local', 'triage', 't', 'b', 'open', '{}', 'now', 'now'),
                ('w2', 'local', 'review', 't', 'b', 'open', '{}', 'now', 'now');
            "#,
        )?;

        migrate_ticket_self_work_items_schema(&conn)?;

        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
                row.get(0)
            })?;
        assert_eq!(count, 2, "all rows must survive the migration");
        let sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            !sql.contains("UNIQUE(source_system, kind)"),
            "the UNIQUE(source_system, kind) constraint must be gone after migration"
        );
        let legacy: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items_legacy_unique'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            legacy, 0,
            "the legacy table must be dropped after a successful migration"
        );
        Ok(())
    }

    #[test]
    fn ticket_self_work_migration_rolls_back_without_data_loss() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        // Fixture old schema with a NULLABLE title so the copy into the NOT NULL new column
        // fails after the rename, exercising the rollback path.
        conn.execute_batch(
            r#"
            CREATE TABLE ticket_self_work_items (
                work_id TEXT PRIMARY KEY,
                source_system TEXT NOT NULL,
                kind TEXT NOT NULL,
                title TEXT,
                body_text TEXT NOT NULL,
                state TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                remote_ticket_id TEXT,
                remote_locator TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(source_system, kind)
            );
            INSERT INTO ticket_self_work_items
                (work_id, source_system, kind, body_text, state, metadata_json, created_at, updated_at)
            VALUES
                ('w1', 'local', 'triage', 'b', 'open', '{}', 'now', 'now');
            "#,
        )?;

        let result = migrate_ticket_self_work_items_schema(&conn);
        assert!(
            result.is_err(),
            "migration must fail when a row cannot be copied into the new NOT NULL schema"
        );

        // The original table and its row must be intact: the failed migration rolled back.
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_self_work_items", [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            count, 1,
            "the original row must survive a rolled-back migration"
        );
        let sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ticket_self_work_items'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            sql.contains("UNIQUE(source_system, kind)"),
            "the original schema must be restored after rollback"
        );
        Ok(())
    }

    fn is_terminology_firewall_text_file(path: &std::path::Path) -> bool {
        matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("md" | "py" | "rs" | "sh" | "js" | "mjs" | "ts" | "tsx")
        )
    }

    fn strict_internal_work_legacy_term(line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        [
            "self-work",
            "self_work",
            "self work",
            "ticketselfwork",
            "requeueselfwork",
            "requeue-self-work",
        ]
        .iter()
        .any(|term| lower.contains(term))
    }

    fn user_visible_internal_work_legacy_phrase(line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        [
            "self-work item",
            "self-work items",
            "self-work backlog",
            "self-work closure",
            "ticket/self-work",
            "ctox self work",
            "self work",
            "self_work_open",
            "requeueselfwork",
            "requeue-self-work",
        ]
        .iter()
        .any(|term| lower.contains(term))
    }

    fn internal_work_firewall_legacy_line_allowed(relative: &str, line: &str) -> bool {
        matches!(relative, "HARNESS.md" | "docs/harness-operating-model.md")
            && (line.contains("`ticket_self_work_items`")
                || line.contains("`self-work:*`")
                || line.contains("`self-work-queue-task`"))
    }

    fn scan_internal_work_firewall_file(
        path: &std::path::Path,
        relative: &str,
        strict: bool,
        violations: &mut Vec<String>,
    ) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read terminology firewall file {relative}"))?;
        for (index, line) in content.lines().enumerate() {
            let has_violation = if strict {
                strict_internal_work_legacy_term(line)
            } else {
                user_visible_internal_work_legacy_phrase(line)
            };
            if has_violation && !internal_work_firewall_legacy_line_allowed(relative, line) {
                violations.push(format!("{relative}:{}: {}", index + 1, line.trim()));
            }
        }
        Ok(())
    }

    fn scan_internal_work_firewall_dir(
        root: &std::path::Path,
        relative_dir: &str,
        strict: bool,
        violations: &mut Vec<String>,
    ) -> Result<()> {
        let dir = root.join(relative_dir);
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("failed to read terminology firewall dir {relative_dir}"))?
        {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                scan_internal_work_firewall_dir(root, &relative, strict, violations)?;
            } else if is_terminology_firewall_text_file(&path) {
                scan_internal_work_firewall_file(&path, &relative, strict, violations)?;
            }
        }
        Ok(())
    }

    #[test]
    fn internal_work_terminology_firewall_keeps_self_work_legacy_only() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut violations = Vec::new();

        scan_internal_work_firewall_dir(&root, "src/skills/system", true, &mut violations)?;
        for relative in [
            "HARNESS.md",
            "docs/harness-operating-model.md",
            "src/core/context/live_context.rs",
            "src/core/harness/core/templates/collab/experimental_prompt.md",
            "src/core/mission/review.rs",
            "src/core/mission/plan.rs",
            "src/core/autonomy.rs",
            "src/core/business_os/mcp_channel.rs",
        ] {
            scan_internal_work_firewall_file(
                &root.join(relative),
                relative,
                true,
                &mut violations,
            )?;
        }
        for relative in [
            "src/core/service/service.rs",
            "src/core/service/core_state_machine.rs",
            "src/core/service/harness_flow.rs",
            "src/core/service/process_mining.rs",
            "src/core/mission/channels.rs",
            "src/core/mission/ticket_zammad_native.rs",
        ] {
            scan_internal_work_firewall_file(
                &root.join(relative),
                relative,
                false,
                &mut violations,
            )?;
        }

        if !violations.is_empty() {
            anyhow::bail!(
                "legacy self-work terminology leaked across the internal-work firewall:\n{}",
                violations.join("\n")
            );
        }
        Ok(())
    }

    #[test]
    fn ticket_preflight_reports_missing_zammad_runtime() {
        let root = temp_root("preflight-zammad-missing");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTOX_TICKET_SYSTEMS".to_string(),
            "local,zammad".to_string(),
        );

        let issues = preflight_configured_ticket_systems(&root, &settings);

        assert!(issues
            .iter()
            .any(|issue| issue.system == "zammad" && issue.code == "missing_zammad_base_url"));
        assert!(issues
            .iter()
            .any(|issue| issue.system == "zammad" && issue.code == "missing_zammad_auth"));
        assert!(!issues.iter().any(|issue| issue.system == "local"));
    }

    #[test]
    fn stale_ticket_event_lease_releases_to_pending() -> Result<()> {
        let root = temp_root("stale-ticket-lease");
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Lease me",
            "Initial baseline",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
        sync_ticket_system(&root, "local")?;

        let leased = lease_pending_ticket_events(&root, 1, "ctox-service")?;
        assert_eq!(leased.len(), 1);
        let conn = open_ticket_db(&root)?;
        conn.execute(
            "UPDATE ticket_event_routing_state SET lease_expires_at='2000-01-01T00:00:00Z' WHERE event_key=?1",
            params![leased[0].event_key],
        )?;
        drop(conn);
        let released = release_stale_ticket_event_leases(&root, "ctox-service", &HashSet::new())?;

        assert_eq!(released, vec![leased[0].event_key.clone()]);
        let leased_again = lease_pending_ticket_events(&root, 1, "ctox-service")?;
        assert_eq!(leased_again[0].event_key, leased[0].event_key);
        Ok(())
    }

    #[test]
    fn retryable_ticket_event_terminalizes_after_three_failures() -> Result<()> {
        let root = temp_root("ticket-event-retry-budget");
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Retry me",
            "Initial baseline",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
        sync_ticket_system(&root, "local")?;

        let event_key = lease_pending_ticket_events(&root, 1, "ctox-service")?[0]
            .event_key
            .clone();
        for attempt in 1..=3 {
            fail_ticket_events(
                &root,
                &[event_key.clone()],
                TicketEventFailureClass::Retryable,
                "transient runtime failure",
            )?;
            let conn = open_ticket_db(&root)?;
            let row: (String, i64, Option<String>, Option<String>) = conn.query_row(
                "SELECT route_status, failure_attempt_count, retry_not_before, failure_proof FROM ticket_event_routing_state WHERE event_key=?1",
                params![event_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
            assert_eq!(row.1, attempt);
            if attempt < 3 {
                assert_eq!(row.0, "pending");
                assert!(row.2.is_some());
                conn.execute(
                    "UPDATE ticket_event_routing_state SET retry_not_before='2000-01-01T00:00:00Z' WHERE event_key=?1",
                    params![event_key],
                )?;
                drop(conn);
                assert_eq!(
                    lease_pending_ticket_events(&root, 1, "ctox-service")?[0].event_key,
                    event_key
                );
            } else {
                assert_eq!(row.0, "failed");
                assert!(row.2.is_none());
                assert!(row.3.is_some());
            }
        }
        Ok(())
    }

    #[test]
    fn waiting_external_ticket_event_only_wakes_for_matching_reference() -> Result<()> {
        let root = temp_root("ticket-event-wait-ref");
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Wait for approval",
            "Initial baseline",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Approval needed")?;
        sync_ticket_system(&root, "local")?;

        let event_key = lease_pending_ticket_events(&root, 1, "ctox-service")?[0]
            .event_key
            .clone();
        block_ticket_events_for_wait(
            &root,
            std::slice::from_ref(&event_key),
            &crate::mission::plan::WaitRef {
                entity_type: "approval-gate".to_string(),
                entity_id: "work-release".to_string(),
            },
            "release approval is open",
        )?;

        assert!(lease_pending_ticket_events(&root, 1, "ctox-service")?.is_empty());
        assert_eq!(
            wake_ticket_events_waiting_for(&root, "approval-gate", "other-work")?,
            0
        );
        assert_eq!(
            wake_ticket_events_waiting_for(&root, "approval-gate", "work-release")?,
            1
        );
        assert_eq!(
            lease_pending_ticket_events(&root, 1, "ctox-service")?[0].event_key,
            event_key
        );
        Ok(())
    }

    #[test]
    fn blocked_ticket_event_releases_after_knowledge_and_control_are_ready() -> Result<()> {
        let root = temp_root("blocked-ticket-release");
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Blocked until controls exist",
            "Initial baseline",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        ticket_local_native::add_local_comment(&root, &remote.ticket_id, "Fresh update")?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);
        let leased = lease_pending_ticket_events(&root, 1, "ctox-service")?;
        assert_eq!(leased.len(), 1);
        ack_leased_ticket_events(&root, &[leased[0].event_key.clone()], "blocked")?;

        let still_blocked = release_ready_blocked_ticket_events(&root, 10)?;
        assert!(still_blocked.is_empty());

        refresh_observed_ticket_knowledge(&root, "local")?;
        set_ticket_label(
            &root,
            &ticket_key,
            "support/general",
            "test",
            None,
            json!({}),
        )?;
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/general".to_string(),
                runbook_id: "rb-general".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-general".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "human_approval_required".to_string(),
                autonomy_level: "A0".to_string(),
                verification_profile_id: "verify-general".to_string(),
                writeback_profile_id: "writeback-general".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: None,
            },
        )?;

        let released = release_ready_blocked_ticket_events(&root, 10)?;
        assert_eq!(released, vec![leased[0].event_key.clone()]);
        Ok(())
    }

    fn write_reply_bundle(bundle_dir: &std::path::Path, items: &[Value]) -> Result<()> {
        std::fs::create_dir_all(bundle_dir)?;
        std::fs::write(
            bundle_dir.join("main_skill.json"),
            serde_json::to_string_pretty(&json!({
                "main_skill_id": "eventus.email.support.main.v1",
                "title": "Eventus Email Support Main",
                "primary_channel": "email",
                "entry_action": "resolve_runbook_item",
                "resolver_contract": {"mode": "runbook-item"},
                "execution_contract": {"mode": "reply-only"},
                "resolve_flow": [
                    "resolve the best matching runbook item",
                    "load the linked skillbook",
                    "compose a reply suggestion"
                ],
                "writeback_flow": [
                    "verify reply",
                    "write public comment back to the ticket"
                ],
                "linked_skillbooks": ["eventus.email.support.v1"],
                "linked_runbooks": ["eventus.runbook.registration.v1"]
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("skillbook.json"),
            serde_json::to_string_pretty(&json!({
                "skillbook_id": "eventus.email.support.v1",
                "title": "Eventus Email Support",
                "version": "v1",
                "mission": "Handle incoming support emails safely and clearly.",
                "non_negotiable_rules": [
                    "Never invent product behavior.",
                    "Keep the answer aligned with the manual."
                ],
                "runtime_policy": "Resolve a runbook item first, then draft the reply.",
                "answer_contract": "Give a concise, actionable email answer.",
                "workflow_backbone": [
                    "identify the request",
                    "load the runbook item",
                    "reply only from the runbook facts"
                ],
                "routing_taxonomy": ["registration", "login"],
                "linked_runbooks": ["eventus.runbook.registration.v1"]
            }))?,
        )?;
        let item_labels = items
            .iter()
            .filter_map(|item| item.get("label").and_then(Value::as_str))
            .collect::<Vec<_>>();
        std::fs::write(
            bundle_dir.join("runbook.json"),
            serde_json::to_string_pretty(&json!({
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "title": "Registration issues",
                "version": "v1",
                "status": "active",
                "problem_domain": "registration",
                "item_labels": item_labels
            }))?,
        )?;
        let mut jsonl = String::new();
        for item in items {
            jsonl.push_str(&serde_json::to_string(item)?);
            jsonl.push('\n');
        }
        std::fs::write(bundle_dir.join("runbook_items.jsonl"), jsonl)?;
        Ok(())
    }

    #[test]
    fn source_skill_bundle_imports_every_catalog_runbook_without_orphans() -> Result<()> {
        let root = temp_root("source-skill-runbook-catalog");
        let bundle_dir = root.join("runtime/generated-skills/catalog");
        std::fs::create_dir_all(&bundle_dir)?;
        let runbook_ids = ["catalog.runbook.one.v1", "catalog.runbook.two.v1"];
        std::fs::write(
            bundle_dir.join("main_skill.json"),
            serde_json::to_string_pretty(&json!({
                "main_skill_id": "catalog.main.v1",
                "title": "Catalog Main",
                "primary_channel": "research",
                "entry_action": "resolve_runbook_item",
                "linked_skillbooks": ["catalog.skillbook.v1"],
                "linked_runbooks": runbook_ids,
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("skillbook.json"),
            serde_json::to_string_pretty(&json!({
                "skillbook_id": "catalog.skillbook.v1",
                "title": "Catalog Skillbook",
                "version": "v1",
                "mission": "Test complete catalog imports.",
                "runtime_policy": "Resolve a runbook item.",
                "answer_contract": "Use cited facts only.",
                "linked_runbooks": runbook_ids,
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("runbook.json"),
            serde_json::to_string_pretty(&json!({
                "schema": "ctox.knowledge.runbook_catalog.v2",
                "primary_runbook": runbook_ids[0],
                "runbooks": [
                    {
                        "runbook_id": runbook_ids[0],
                        "skillbook_id": "catalog.skillbook.v1",
                        "title": "First",
                        "version": "v1",
                        "status": "active",
                        "problem_domain": "first",
                        "item_labels": ["ONE"],
                    },
                    {
                        "runbook_id": runbook_ids[1],
                        "skillbook_id": "catalog.skillbook.v1",
                        "title": "Second",
                        "version": "v1",
                        "status": "active",
                        "problem_domain": "second",
                        "item_labels": ["TWO"],
                    }
                ],
            }))?,
        )?;
        let items = [
            json!({
                "item_id": "catalog.item.one.v1",
                "runbook_id": runbook_ids[0],
                "skillbook_id": "catalog.skillbook.v1",
                "label": "ONE",
                "title": "First item",
                "problem_class": "first",
                "chunk_text": "first cited fact",
            }),
            json!({
                "item_id": "catalog.item.two.v1",
                "runbook_id": runbook_ids[1],
                "skillbook_id": "catalog.skillbook.v1",
                "label": "TWO",
                "title": "Second item",
                "problem_class": "second",
                "chunk_text": "second cited fact",
            }),
        ];
        let jsonl = items
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n");
        std::fs::write(bundle_dir.join("runbook_items.jsonl"), format!("{jsonl}\n"))?;
        std::fs::write(
            bundle_dir.join("resources.jsonl"),
            format!(
                "{}\n",
                serde_json::to_string(&json!({
                    "resource_id": "catalog.resource.one",
                    "title": "Source receipt",
                    "kind": "evidence_receipt",
                    "source_id": "SRC-001",
                    "role": "evidence",
                    "canonical_url": "https://example.com/source",
                    "snapshot_hash": "abc123",
                    "evidence_eligible": true,
                    "linked_runbook_items": ["catalog.item.one.v1"],
                }))?
            ),
        )?;

        let imported = import_ticket_source_skill_bundle(
            &root,
            "catalog-test",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )?;
        assert_eq!(
            imported.get("runbook_count").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            imported.get("resource_count").and_then(Value::as_u64),
            Some(1)
        );

        let conn = open_ticket_db(&root)?;
        let runbook_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_runbooks WHERE skillbook_id = 'catalog.skillbook.v1'",
            [],
            |row| row.get(0),
        )?;
        let orphan_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM knowledge_runbook_items AS item
            LEFT JOIN knowledge_runbooks AS runbook ON runbook.runbook_id = item.runbook_id
            WHERE item.skillbook_id = 'catalog.skillbook.v1' AND runbook.runbook_id IS NULL
            "#,
            [],
            |row| row.get(0),
        )?;
        let resource_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_resources WHERE skillbook_id = 'catalog.skillbook.v1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(runbook_count, 2);
        assert_eq!(orphan_count, 0);
        assert_eq!(resource_count, 1);
        conn.execute(
            "INSERT INTO knowledge_embeddings (item_id, embedding_model, embedding_json, updated_at)
             VALUES ('catalog.item.one.v1', 'test-model', '[0.5]', '2026-01-01T00:00:00Z')",
            [],
        )?;
        drop(conn);

        std::fs::remove_file(bundle_dir.join("resources.jsonl"))?;
        let legacy_reimport = import_ticket_source_skill_bundle(
            &root,
            "catalog-test",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )?;
        assert!(legacy_reimport
            .get("resource_count")
            .is_some_and(Value::is_null));
        let conn = open_ticket_db(&root)?;
        let preserved_embedding_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_embeddings
              WHERE item_id = 'catalog.item.one.v1' AND embedding_model = 'test-model'",
            [],
            |row| row.get(0),
        )?;
        let preserved_resource_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_resources
              WHERE resource_id = 'catalog.resource.one'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(preserved_embedding_count, 1);
        assert_eq!(preserved_resource_count, 1);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_bundle_rejects_missing_runbook_parent_before_writes() -> Result<()> {
        let root = temp_root("source-skill-runbook-missing-parent");
        let bundle_dir = root.join("runtime/generated-skills/missing-parent");
        write_reply_bundle(
            &bundle_dir,
            &[json!({
                "item_id": "orphan.item.v1",
                "runbook_id": "missing.runbook.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "ORPHAN",
                "title": "Orphan item",
                "problem_class": "invalid",
                "chunk_text": "must not be imported",
            })],
        )?;

        let error = import_ticket_source_skill_bundle(
            &root,
            "invalid-test",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )
        .expect_err("missing runbook parent must fail");
        assert!(error.to_string().contains("references missing runbook"));

        let conn = open_ticket_db(&root)?;
        let main_skill_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM knowledge_main_skills", [], |row| {
                row.get(0)
            })?;
        assert_eq!(main_skill_count, 0);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_bundle_rejects_ids_owned_by_another_skillbook() -> Result<()> {
        let root = temp_root("source-skill-foreign-owner");
        create_or_update_runbook(
            &root,
            "eventus.runbook.registration.v1",
            "foreign.skillbook.v1",
            "Foreign runbook",
            "v1",
            "active",
            "foreign",
            Vec::new(),
        )?;
        let bundle_dir = root.join("runtime/generated-skills/foreign-owner");
        write_reply_bundle(
            &bundle_dir,
            &[json!({
                "item_id": "eventus.registration.v1",
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "REG-01",
                "title": "Registration",
                "problem_class": "registration",
                "chunk_text": "source-backed guidance",
            })],
        )?;

        let error = import_ticket_source_skill_bundle(
            &root,
            "eventus",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )
        .expect_err("foreign runbook ownership must fail");
        assert!(error.to_string().contains("already owned by skillbook"));

        let conn = open_ticket_db(&root)?;
        let owner: String = conn.query_row(
            "SELECT skillbook_id FROM knowledge_runbooks
              WHERE runbook_id = 'eventus.runbook.registration.v1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(owner, "foreign.skillbook.v1");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_local_sync_dry_run_and_audit_flow_round_trips() -> Result<()> {
        let root = temp_root("lifecycle");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "VPN outage",
            "Users cannot reach the VPN gateway.",
            Some("open"),
            Some("high"),
        )?;
        ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Customer impact confirmed",
        )?;
        let sync = sync_ticket_system(&root, "local")?;
        assert_eq!(sync.get("ok").and_then(Value::as_bool), Some(true));

        let ticket_key = format!("local:{}", remote.ticket_id);
        let ticket = load_ticket(&root, &ticket_key)?.context("ticket missing after sync")?;
        assert_eq!(ticket.title, "VPN outage");

        let bundle = put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/vpn".to_string(),
                runbook_id: "rb-vpn".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-vpn".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "human_approval_required".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "verify-vpn".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "incident".to_string(),
                default_risk_level: "high".to_string(),
                execution_actions: vec![
                    "observe".to_string(),
                    "analyze".to_string(),
                    "draft_communication".to_string(),
                ],
                notes: Some("VPN incident starter bundle".to_string()),
            },
        )?;
        assert_eq!(bundle.bundle_version, 1);

        let assignment = set_ticket_label(
            &root,
            &ticket_key,
            "support/vpn",
            "manual",
            Some("support queue routing"),
            json!({"signal": "vpn"}),
        )?;
        assert_eq!(assignment.label, "support/vpn");

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("VPN outage appears reproducible"),
            None,
        )?;
        assert_eq!(dry_run.label, "support/vpn");
        let case = load_case(&root, &dry_run.case_id)?.context("case missing after dry run")?;
        assert_eq!(case.state, "approval_pending");

        let case = decide_case_approval(
            &root,
            &case.case_id,
            "approved",
            "owner",
            Some("Proceed with bounded investigation"),
        )?;
        assert_eq!(case.state, "executable");

        let case =
            record_execution_action(&root, &case.case_id, "Reviewed VPN endpoint configuration")?;
        assert_eq!(case.state, "executing");

        let case = record_verification(
            &root,
            &case.case_id,
            "passed",
            Some("Dry verification complete"),
        )?;
        assert_eq!(case.state, "writeback_pending");

        let case = writeback_comment(
            &root,
            &case.case_id,
            "CTOX dry run complete; ready for controlled execution.",
            false,
        )?;
        assert_eq!(case.state, "writeback_pending");
        let leased_after_writeback = lease_pending_ticket_events(&root, 20, "ticket-test")?;
        assert!(
            leased_after_writeback.iter().all(|event| {
                event.metadata.get("origin").and_then(Value::as_str) != Some("ctox-writeback")
            }),
            "writeback-generated outbound events must not re-enter the inbound lease queue"
        );

        let audit = list_audit_records(&root, Some(&ticket_key), 20)?;
        assert!(audit
            .iter()
            .any(|item| item.action_type == "ticket_label_assignment"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "dry_run_record"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "approval_decision"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "writeback_record"));
        assert!(!audit.iter().any(|item| item.action_type == "case_closed"));

        let history = list_ticket_history(&root, &ticket_key, 20)?;
        assert!(history.iter().any(|event| event.event_type == "comment"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn business_os_ticket_commands_drive_full_case_lifecycle() -> Result<()> {
        let root = temp_root("business-os-ticket-lifecycle");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Business OS lifecycle",
            "Exercise the command adapter for every visible Ticket action.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/business-os".to_string(),
                runbook_id: "rb-business-os".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-business-os".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "human_approval_required".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "verify-business-os".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Business OS command adapter lifecycle coverage".to_string()),
            },
        )?;
        set_ticket_label(
            &root,
            &ticket_key,
            "support/business-os",
            "test",
            Some("route to Business OS support controls"),
            json!({"source": "business-os-command-test"}),
        )?;

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Business OS command lifecycle dry run"),
            None,
        )?;
        let case_id = dry_run.case_id;

        let approved = run_business_os_ticket_command(
            &root,
            "ctox.ticket.approve",
            &json!({
                "case_id": case_id,
                "status": "approved",
                "decided_by": "business-os-test",
                "rationale": "approve command adapter path",
            }),
        )?;
        assert_eq!(
            approved.pointer("/case/state").and_then(Value::as_str),
            Some("executable")
        );

        let executed = run_business_os_ticket_command(
            &root,
            "ctox.ticket.execute",
            &json!({
                "case_id": case_id,
                "summary": "Executed through Business OS command adapter",
            }),
        )?;
        assert_eq!(
            executed.pointer("/case/state").and_then(Value::as_str),
            Some("executing")
        );

        let verified = run_business_os_ticket_command(
            &root,
            "ctox.ticket.verify",
            &json!({
                "case_id": case_id,
                "status": "passed",
                "summary": "Verified through Business OS command adapter",
            }),
        )?;
        assert_eq!(
            verified.pointer("/case/state").and_then(Value::as_str),
            Some("writeback_pending")
        );

        let written_back = run_business_os_ticket_command(
            &root,
            "ctox.ticket.writeback_comment",
            &json!({
                "case_id": case_id,
                "body": "Business OS command adapter writeback smoke.",
                "internal": false,
            }),
        )?;
        assert_eq!(
            written_back.pointer("/case/state").and_then(Value::as_str),
            Some("writeback_pending")
        );

        let closed = run_business_os_ticket_command(
            &root,
            "ctox.ticket.close",
            &json!({
                "case_id": case_id,
                "summary": "Closed through Business OS command adapter",
            }),
        )?;
        assert_eq!(
            closed.pointer("/case/state").and_then(Value::as_str),
            Some("closed")
        );

        let audit = list_audit_records(&root, Some(&ticket_key), 30)?;
        assert!(audit
            .iter()
            .any(|item| item.action_type == "approval_decision"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "execution_case"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "verification_record"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "writeback_record"));
        assert!(audit.iter().any(|item| item.action_type == "case_closed"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn clarification_request_round_trips_through_business_os_projection() -> Result<()> {
        let root = temp_root("ticket-clarification-business-os");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Missing VPN target",
            "Please connect me, but the VPN endpoint is not included.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/vpn".to_string(),
                runbook_id: "rb-vpn".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-vpn".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "verify-vpn".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Clarification workflow test bundle".to_string()),
            },
        )?;
        set_ticket_label(
            &root,
            &ticket_key,
            "support/vpn",
            "test",
            Some("route to VPN support"),
            json!({}),
        )?;
        let dry_run = create_dry_run(&root, &ticket_key, Some("VPN request needs endpoint"), None)?;

        let requested = run_business_os_ticket_command(
            &root,
            "ctox.ticket.request_clarification",
            &json!({
                "case_id": dry_run.case_id,
                "question": "Which VPN endpoint should CTOX use?",
                "missing_inputs": ["vpn_endpoint"],
                "unblock_criteria": "Requester supplies the exact VPN endpoint.",
            }),
        )?;
        let clarification_id = requested
            .get("clarification_id")
            .and_then(Value::as_str)
            .context("clarification id missing")?
            .to_string();
        assert_eq!(
            requested
                .pointer("/clarification/status")
                .and_then(Value::as_str),
            Some("draft")
        );
        let blocked_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
        assert_eq!(blocked_case.state, "blocked_needs_clarification");

        let projection = business_os_ticket_projection_documents(&root, 50)?;
        let clarification_docs = projection
            .get("ctox_ticket_clarification_requests")
            .context("clarification projection missing")?;
        assert!(clarification_docs.iter().any(|doc| {
            doc.get("clarification_id").and_then(Value::as_str) == Some(clarification_id.as_str())
                && doc.get("status").and_then(Value::as_str) == Some("draft")
        }));

        let resolved = run_business_os_ticket_command(
            &root,
            "ctox.ticket.resolve_clarification",
            &json!({
                "clarification_id": clarification_id,
                "response_key": "manual:test-response",
                "body": "Use vpn.example.test.",
            }),
        )?;
        assert_eq!(
            resolved
                .pointer("/clarification/status")
                .and_then(Value::as_str),
            Some("resolved")
        );
        let resumed_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
        assert_eq!(resumed_case.state, "executable");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn business_os_ticket_projection_reuses_one_ticket_db_connection() -> Result<()> {
        let root = temp_root("ticket-projection-single-db-open");
        std::fs::create_dir_all(&root)?;
        let db_path = resolve_db_path(&root);

        ticket_local_native::create_local_ticket(
            &root,
            "Projection connection reuse",
            "Project ticket state without reopening the ticket database per bucket.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/projection".to_string(),
                runbook_id: "rb-projection".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-projection".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "verify-projection".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Projection connection reuse guard".to_string()),
            },
        )?;

        let before = ticket_db_open_call_count_for_tests(&db_path);
        let projection = business_os_ticket_projection_documents(&root, 50)?;
        assert!(
            projection
                .get("ctox_ticket_control_bundles")
                .is_some_and(|docs| !docs.is_empty()),
            "control bundles must still be projected"
        );
        assert_eq!(
            ticket_db_open_call_count_for_tests(&db_path) - before,
            1,
            "Business OS ticket projection must reuse its single ticket DB connection across buckets"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn published_clarification_resolves_from_later_inbound_ticket_comment() -> Result<()> {
        let root = temp_root("ticket-clarification-inbound");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Printer setup missing model",
            "Please configure the printer.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/printer".to_string(),
                runbook_id: "rb-printer".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "pol-printer".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "verify-printer".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Clarification auto-resolver test bundle".to_string()),
            },
        )?;
        set_ticket_label(
            &root,
            &ticket_key,
            "support/printer",
            "test",
            Some("route to printer support"),
            json!({}),
        )?;
        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Printer request needs model"),
            None,
        )?;
        let clarification = create_ticket_clarification_request(
            &root,
            TicketClarificationRequestInput {
                case_id: Some(dry_run.case_id.clone()),
                ticket_key: None,
                work_id: None,
                target_type: "requester".to_string(),
                target_channel: "ticket".to_string(),
                question: "Which printer model should CTOX configure?".to_string(),
                missing_inputs: vec!["printer_model".to_string()],
                unblock_criteria: Some("Requester supplies the printer model.".to_string()),
                resume_state: "executable".to_string(),
                created_by: "ctox-test".to_string(),
                metadata: json!({}),
            },
        )?;
        let waiting = publish_ticket_clarification_request(
            &root,
            &clarification.clarification_id,
            "review-test",
            "Question is bounded and safe for requester.",
        )?;
        assert_eq!(waiting.status, "waiting_for_response");

        ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "The model is LaserJet 4100.",
        )?;
        let sync = sync_ticket_system(&root, "local")?;
        assert_eq!(
            sync.get("resolved_clarification_count")
                .and_then(Value::as_u64),
            Some(1)
        );
        let resolved = load_ticket_clarification_request(&root, &clarification.clarification_id)?
            .context("clarification missing")?;
        assert_eq!(resolved.status, "resolved");
        assert!(resolved
            .inbound_response_body
            .as_deref()
            .unwrap_or_default()
            .contains("LaserJet 4100"));
        let resumed_case = load_case(&root, &dry_run.case_id)?.context("case missing")?;
        assert_eq!(resumed_case.state, "executable");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn autonomy_grant_controls_effective_ticket_execution_mode() -> Result<()> {
        let root = temp_root("autonomy");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Password reset request",
            "User requests a bounded password reset workflow.",
            Some("open"),
            Some("medium"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/password-reset".to_string(),
                runbook_id: "rb-password-reset".to_string(),
                runbook_version: "v2".to_string(),
                policy_id: "pol-password-reset".to_string(),
                policy_version: "v2".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A4".to_string(),
                verification_profile_id: "verify-password-reset".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "service_request".to_string(),
                default_risk_level: "medium".to_string(),
                execution_actions: vec![
                    "observe".to_string(),
                    "analyze".to_string(),
                    "draft_communication".to_string(),
                    "remote_write".to_string(),
                ],
                notes: Some("Password reset bundle wants broad autonomy".to_string()),
            },
        )?;
        set_ticket_label(
            &root,
            &ticket_key,
            "support/password-reset",
            "manual",
            Some("service desk triage"),
            json!({"queue": "identity"}),
        )?;

        let first_dry_run =
            create_dry_run(&root, &ticket_key, Some("Bounded reset request"), None)?;
        let first_case = load_case(&root, &first_dry_run.case_id)?
            .context("first case missing after dry run")?;
        assert_eq!(first_case.state, "approval_pending");
        assert_eq!(first_case.approval_mode, "human_approval_required");
        assert_eq!(first_case.autonomy_level, "A0");
        assert_eq!(
            first_dry_run
                .artifact
                .get("autonomy_grant")
                .cloned()
                .unwrap_or(Value::Null),
            Value::Null
        );

        let first_case = decide_case_approval(
            &root,
            &first_case.case_id,
            "approved",
            "owner",
            Some("Initial supervised execution"),
        )?;
        let first_case = record_execution_action(
            &root,
            &first_case.case_id,
            "Prepared reset checklist and bounded operator plan",
        )?;
        let first_case = record_verification(
            &root,
            &first_case.case_id,
            "passed",
            Some("Checklist and verification evidence captured"),
        )?;

        let candidate = create_learning_candidate(
            &root,
            &first_case.case_id,
            "Observed password reset flow is stable and bounded",
            None,
            None,
        )?;
        assert_eq!(candidate.status, "proposed");
        let candidate = decide_learning_candidate(
            &root,
            &candidate.candidate_id,
            "approved",
            "owner",
            Some("Promote this runbook pattern"),
            Some("A3"),
        )?;
        assert_eq!(candidate.status, "approved");
        assert_eq!(candidate.promoted_autonomy_level.as_deref(), Some("A3"));

        let grant = put_autonomy_grant(
            &root,
            AutonomyGrantInput {
                label: "support/password-reset".to_string(),
                bundle_version: None,
                approval_mode: "bounded_auto_execute".to_string(),
                autonomy_level: "A3".to_string(),
                approved_by: "owner".to_string(),
                source_candidate_id: Some(candidate.candidate_id.clone()),
                rationale: Some("Approved bounded automation for this runbook".to_string()),
            },
        )?;
        assert_eq!(grant.approval_mode, "bounded_auto_execute");
        assert_eq!(grant.autonomy_level, "A3");

        let second_dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Second identical request after grant"),
            None,
        )?;
        let second_case = load_case(&root, &second_dry_run.case_id)?
            .context("second case missing after dry run")?;
        assert_eq!(second_case.state, "executable");
        assert_eq!(second_case.approval_mode, "bounded_auto_execute");
        assert_eq!(second_case.autonomy_level, "A3");
        assert_eq!(
            second_dry_run
                .artifact
                .get("autonomy_grant")
                .and_then(|item| item.get("approved_by"))
                .and_then(Value::as_str),
            Some("owner")
        );

        let grants = list_autonomy_grants(&root)?;
        assert_eq!(grants.len(), 1);
        let candidates = list_learning_candidates(&root, Some("support/password-reset"), None, 8)?;
        assert_eq!(candidates.len(), 1);

        let audit = list_audit_records(&root, Some(&ticket_key), 40)?;
        assert!(audit
            .iter()
            .any(|item| item.action_type == "learning_candidate"));
        assert!(audit
            .iter()
            .any(|item| item.action_type == "learning_candidate_decision"));
        let control_audit = list_audit_records(&root, Some("*autonomy-grant*"), 20)?;
        assert!(control_audit
            .iter()
            .any(|item| item.action_type == "autonomy_grant_change"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn first_attach_baselines_existing_ticket_events_but_routes_new_ones() -> Result<()> {
        let root = temp_root("attach-baseline");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Existing helpdesk backlog item",
            "This ticket existed before CTOX was attached.",
            Some("open"),
            Some("medium"),
        )?;
        ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Historic conversation before CTOX attach",
        )?;

        let first_sync = sync_ticket_system(&root, "local")?;
        assert_eq!(first_sync.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            first_sync
                .get("source_control")
                .and_then(|item| item.get("adoption_mode"))
                .and_then(Value::as_str),
            Some("baseline_observe_only")
        );

        let source_controls = list_ticket_source_controls(&root)?;
        assert_eq!(source_controls.len(), 1);
        assert_eq!(source_controls[0].source_system, "local");

        let initially_leased = lease_pending_ticket_events(&root, 20, "attach-test")?;
        assert!(
            initially_leased.is_empty(),
            "existing backlog must be baselined on first attach instead of entering active routing"
        );

        ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Fresh update after CTOX attach",
        )?;
        sync_ticket_system(&root, "local")?;

        let leased_after_new_comment = lease_pending_ticket_events(&root, 20, "attach-test")?;
        assert_eq!(leased_after_new_comment.len(), 1);
        assert_eq!(leased_after_new_comment[0].event_type, "comment");
        assert_eq!(
            leased_after_new_comment[0].body_text,
            "Fresh update after CTOX attach"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn sync_bootstraps_knowledge_but_not_self_work_for_ticket_sources() -> Result<()> {
        let root = temp_root("knowledge");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "[VPN] host vpn-gateway-01 unreachable",
            "Users cannot reach vpn-gateway-01 after the overnight maintenance window.",
            Some("open"),
            Some("high"),
        )?;
        let sync = sync_ticket_system(&root, "local")?;
        assert_eq!(sync.get("self_work_count").and_then(Value::as_u64), Some(0));

        let knowledge = list_ticket_knowledge_entries(&root, Some("local"), None, None, 20)?;
        assert!(knowledge
            .iter()
            .any(|entry| entry.domain == "source_profile"));
        assert!(knowledge.iter().any(|entry| entry.domain == "glossary"));
        assert!(knowledge.iter().any(|entry| entry.domain == "access_model"));
        assert!(knowledge
            .iter()
            .any(|entry| entry.domain == "monitoring_landscape"));

        let load =
            create_ticket_knowledge_load(&root, &format!("local:{}", remote.ticket_id), None)?;
        assert_eq!(load.status, "ready");
        assert!(load.gap_domains.is_empty());

        let item = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "system-onboarding".to_string(),
                title: "Review current helpdesk working model".to_string(),
                body_text:
                    "Review the observed operating model and propose the next adoption steps."
                        .to_string(),
                state: "open".to_string(),
                metadata: json!({
                    "skill": "system-onboarding",
                    "phase": "observe",
                }),
            },
            true,
        )?;
        assert_eq!(item.kind, "system-onboarding");
        assert_eq!(item.state, "published");
        assert!(item.remote_ticket_id.is_some());

        let listed = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(listed.len(), 1);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn monitoring_ingest_persists_generic_monitoring_knowledge() -> Result<()> {
        let root = temp_root("monitoring");
        std::fs::create_dir_all(&root)?;

        let entry = put_ticket_knowledge_entry(
            &root,
            TicketKnowledgeUpsertInput {
                source_system: "local".to_string(),
                domain: "monitoring_landscape".to_string(),
                knowledge_key: "prometheus".to_string(),
                title: "Prometheus overview".to_string(),
                summary: summarize_monitoring_snapshot(&json!({
                    "sources": [{"name": "prometheus"}],
                    "services": [{"name": "vpn"}],
                    "alerts": [{"name": "vpn-down"}],
                })),
                status: "observed".to_string(),
                content: json!({
                    "sources": [{"name": "prometheus"}],
                    "services": [{"name": "vpn"}],
                    "alerts": [{"name": "vpn-down"}],
                }),
            },
        )?;
        assert_eq!(entry.domain, "monitoring_landscape");
        assert_eq!(entry.knowledge_key, "prometheus");
        assert!(entry.summary.contains("1 sources"));

        let loaded =
            load_ticket_knowledge_entry(&root, "local", "monitoring_landscape", "prometheus")?
                .context("monitoring entry missing")?;
        assert_eq!(loaded.status, "observed");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn attached_source_without_active_binding_defaults_to_onboarding_skill() -> Result<()> {
        let root = temp_root("ticket-onboarding-default-skill");
        std::fs::create_dir_all(&root)?;

        let _remote = crate::mission::ticket_local_native::create_local_ticket(
            &root,
            "Erste Desk-Anbindung",
            "Der lokale Desk ist frisch verbunden und noch ohne aktive Desk-Skill-Bindung.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;

        assert_eq!(
            preferred_skill_for_ticket_source(&root, "local")?,
            Some("system-onboarding".to_string())
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn access_request_self_work_keeps_secret_refs_outside_ticket_truth() -> Result<()> {
        let root = temp_root("access-request");
        std::fs::create_dir_all(&root)?;

        let item = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "access-request".to_string(),
                title: "Need monitoring access for onboarding".to_string(),
                body_text: "Please grant read access to monitoring and provide references to the required tokens."
                    .to_string(),
                state: "open".to_string(),
                metadata: json!({
                    "skill": "ticket-access-and-secrets",
                    "required_scopes": ["monitoring.read", "ticket.transition"],
                    "secret_refs": ["secret:monitoring/prometheus-api-token"],
                    "channels": ["mail", "jami"],
                }),
            },
            false,
        )?;
        assert_eq!(item.kind, "access-request");
        assert_eq!(
            item.suggested_skill.as_deref(),
            Some("ticket-access-and-secrets")
        );
        assert_eq!(
            item.metadata
                .get("secret_refs")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(1)
        );
        assert!(item.remote_ticket_id.is_none());

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_self_work_list_cache_reuses_idle_reads_until_store_changes() -> Result<()> {
        let root = temp_root("self-work-list-cache");
        std::fs::create_dir_all(&root)?;
        let db_path = resolve_db_path(&root);
        let _ = open_ticket_db(&root)?;

        let first = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert!(first.is_empty());
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            1,
            "first self-work list must hit SQLite"
        );

        let second = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert!(second.is_empty());
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            1,
            "unchanged idle self-work list must reuse the cached snapshot"
        );

        let created = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "cache-test".to_string(),
                title: "Refresh self-work snapshot after write".to_string(),
                body_text: "The idle list cache must notice self-work writes.".to_string(),
                state: "open".to_string(),
                metadata: json!({
                    "skill": "cache-test",
                    "dedupe_key": "self-work-list-cache",
                }),
            },
            false,
        )?;

        let after_write = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(after_write.len(), 1);
        assert_eq!(after_write[0].work_id, created.work_id);
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            2,
            "self-work writes must invalidate the cached snapshot"
        );

        let after_idle = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(after_idle.len(), 1);
        assert_eq!(after_idle[0].work_id, created.work_id);
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            2,
            "unchanged post-write self-work list must reuse the refreshed snapshot"
        );

        assign_ticket_self_work_item(
            &root,
            &created.work_id,
            "ctox-core",
            "cache-test",
            Some("exercise assignment hydration invalidation"),
        )?;

        let after_assignment = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(after_assignment.len(), 1);
        assert_eq!(
            after_assignment[0].assigned_to.as_deref(),
            Some("ctox-core")
        );
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            3,
            "assignment writes must invalidate hydrated self-work snapshots"
        );

        let after_assignment_idle = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(after_assignment_idle.len(), 1);
        assert_eq!(
            after_assignment_idle[0].assigned_to.as_deref(),
            Some("ctox-core")
        );
        assert_eq!(
            ticket_self_work_list_cache_miss_count_for_tests(&db_path, Some("local"), None, 10),
            3,
            "unchanged assigned self-work list must stay cached"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_self_work_list_batches_latest_assignment_hydration() -> Result<()> {
        let root = temp_root("self-work-assignment-batch-hydration");
        std::fs::create_dir_all(&root)?;
        let before = ticket_self_work_assignment_batch_hydration_call_count_for_tests();

        let mut expected = BTreeMap::new();
        for index in 0..5 {
            let item = put_ticket_self_work_item(
                &root,
                TicketSelfWorkUpsertInput {
                    source_system: "local".to_string(),
                    kind: "batch-hydration".to_string(),
                    title: format!("Batch hydrate {index}"),
                    body_text: "Hydrate assignment in one set-based pass.".to_string(),
                    state: "open".to_string(),
                    metadata: json!({
                        "skill": "batch-hydration",
                        "dedupe_key": format!("self-work-assignment-batch-{index}"),
                    }),
                },
                false,
            )?;
            let assignee = format!("ctox-agent-{index}");
            assign_ticket_self_work_item(
                &root,
                &item.work_id,
                &assignee,
                "batch-test",
                Some("exercise batch assignment hydration"),
            )?;
            expected.insert(item.work_id, assignee);
        }

        let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(items.len(), 5);
        for item in &items {
            assert_eq!(
                item.assigned_to.as_deref(),
                expected.get(&item.work_id).map(String::as_str)
            );
        }
        assert_eq!(
            ticket_self_work_assignment_batch_hydration_call_count_for_tests() - before,
            1,
            "self-work list assignment hydration must be one batch query, not one query per item"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_workflow_materialize_cache_reuses_idle_noop_until_store_changes() -> Result<()> {
        let root = temp_root("workflow-materialize-cache");
        std::fs::create_dir_all(&root)?;
        let db_path = resolve_db_path(&root);
        let _ = open_ticket_db(&root)?;

        let first = materialize_ready_workflow_steps(&root, 8)?;
        assert_eq!(first.materialized_count, 0);
        assert_eq!(
            ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
            1,
            "first workflow materialize pass must inspect SQLite"
        );

        let second = materialize_ready_workflow_steps(&root, 8)?;
        assert_eq!(second.materialized_count, 0);
        assert_eq!(
            ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
            1,
            "unchanged idle workflow materialize pass must reuse the no-op cache"
        );

        let workflow = start_ticket_workflow(
            &root,
            TicketWorkflowStartInput {
                source_system: "internal".to_string(),
                title: "Exercise workflow materialize cache".to_string(),
                goal: "Create a ready step so the cache must refresh after writes.".to_string(),
                thread_key: Some("workflow/cache".to_string()),
                workspace_root: None,
                skill: None,
                priority: Some("normal".to_string()),
                first_phase: "plan".to_string(),
                first_phase_goal: None,
                first_exit_gate: None,
                first_step_title: None,
                first_step_prompt: None,
                queue_now: false,
            },
        )?;
        assert_eq!(workflow.ready_steps, vec!["phase-0-reducer".to_string()]);

        let materialized = materialize_ready_workflow_steps(&root, 8)?;
        assert_eq!(materialized.materialized_count, 1);
        assert_eq!(
            ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
            2,
            "workflow writes must invalidate the previous no-op cache"
        );

        let after_materialized = materialize_ready_workflow_steps(&root, 8)?;
        assert_eq!(after_materialized.materialized_count, 0);
        assert_eq!(
            ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
            3,
            "first post-materialization no-op pass must refresh the cache"
        );

        let after_idle = materialize_ready_workflow_steps(&root, 8)?;
        assert_eq!(after_idle.materialized_count, 0);
        assert_eq!(
            ticket_workflow_materialize_cache_miss_count_for_tests(&db_path, None, 8),
            3,
            "unchanged post-materialization no-op pass must stay cached"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn access_request_command_defaults_to_access_and_secrets_skill() -> Result<()> {
        let root = temp_root("access-request-command");
        std::fs::create_dir_all(&root)?;

        handle_ticket_command(
            &root,
            &[
                "access-request-put".to_string(),
                "--system".to_string(),
                "local".to_string(),
                "--title".to_string(),
                "Need admin approval for access request".to_string(),
                "--body".to_string(),
                "Please confirm whether CTOX may handle password reset tickets autonomously."
                    .to_string(),
            ],
        )?;

        let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "access-request");
        assert_eq!(
            items[0].suggested_skill.as_deref(),
            Some("ticket-access-and-secrets")
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn self_work_put_accepts_explicit_skill_hint() -> Result<()> {
        let root = temp_root("self-work-skill");
        std::fs::create_dir_all(&root)?;

        handle_ticket_command(
            &root,
            &[
                "self-work-put".to_string(),
                "--system".to_string(),
                "local".to_string(),
                "--kind".to_string(),
                "secret-hygiene".to_string(),
                "--title".to_string(),
                "Protect leaked API token".to_string(),
                "--body".to_string(),
                "Move the pasted API token into the encrypted store and rewrite memory."
                    .to_string(),
                "--skill".to_string(),
                "secret-hygiene".to_string(),
            ],
        )?;

        let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "secret-hygiene");
        assert_eq!(items[0].suggested_skill.as_deref(), Some("secret-hygiene"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn internal_work_put_alias_accepts_explicit_skill_hint() -> Result<()> {
        let root = temp_root("internal-work-skill");
        std::fs::create_dir_all(&root)?;

        handle_ticket_command(
            &root,
            &[
                "internal-work-put".to_string(),
                "--system".to_string(),
                "local".to_string(),
                "--kind".to_string(),
                "secret-hygiene".to_string(),
                "--title".to_string(),
                "Protect leaked API token".to_string(),
                "--body".to_string(),
                "Move the pasted API token into the encrypted store and rewrite memory."
                    .to_string(),
                "--skill".to_string(),
                "secret-hygiene".to_string(),
            ],
        )?;

        let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "secret-hygiene");
        assert_eq!(items[0].suggested_skill.as_deref(), Some("secret-hygiene"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_workflow_delta_materializes_successor_after_verified_predecessor() -> Result<()> {
        let root = temp_root("ticket-workflow-delta");
        std::fs::create_dir_all(&root)?;

        let workflow = start_ticket_workflow(
            &root,
            TicketWorkflowStartInput {
                source_system: "internal".to_string(),
                title: "Stabilize long task handling".to_string(),
                goal: "Break a hard implementation into verifiable CTOX ticket phases.".to_string(),
                thread_key: Some("workflow/test".to_string()),
                workspace_root: None,
                skill: None,
                priority: Some("normal".to_string()),
                first_phase: "plan".to_string(),
                first_phase_goal: Some("Create the first executable phase".to_string()),
                first_exit_gate: Some("Reducer produces a bounded implementation step".to_string()),
                first_step_title: None,
                first_step_prompt: None,
                queue_now: false,
            },
        )?;
        assert_eq!(workflow.steps.len(), 1);
        assert_eq!(workflow.ready_steps, vec!["phase-0-reducer".to_string()]);

        let delta = json!({
            "phase_decision": "advance",
            "update_steps": [{"step_id": "phase-0-reducer", "workflow_step_status": "verified", "evidence": {"summary": "planning complete"}}],
            "create_steps": [{"step_id": "implement-one-slice", "phase": "implementation", "role": "leaf", "title": "Implement one bounded slice", "prompt": "Make one scoped code change and report evidence.", "predecessor_steps": ["phase-0-reducer"], "exit_gate": "Focused check passes or blocker is recorded", "priority": "normal"}]
        });
        let _ = apply_ticket_workflow_delta(&root, &workflow.workflow_id, delta, false)?;
        let view =
            load_ticket_workflow(&root, &workflow.workflow_id)?.context("workflow missing")?;
        assert!(view
            .ready_steps
            .contains(&"implement-one-slice".to_string()));

        let materialized =
            materialize_ready_workflow_steps_for_workflow(&root, Some(&workflow.workflow_id), 8)?;
        assert_eq!(materialized.materialized_count, 1);
        assert_eq!(
            materialized.materialized[0].assigned_to.as_deref(),
            Some("self")
        );
        assert_eq!(materialized.materialized[0].state, "queued");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn self_work_items_allow_multiple_entries_for_same_kind_when_not_deduped() -> Result<()> {
        let root = temp_root("self-work-multi-kind");
        std::fs::create_dir_all(&root)?;

        let first = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "internal".to_string(),
                kind: "queue-overflow".to_string(),
                title: "Queue spill: monitoring drift".to_string(),
                body_text: "First queue spill body".to_string(),
                state: "spilled".to_string(),
                metadata: json!({
                    "queue_message_key": "queue:one",
                }),
            },
            false,
        )?;
        let second = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "internal".to_string(),
                kind: "queue-overflow".to_string(),
                title: "Queue spill: alert storm".to_string(),
                body_text: "Second queue spill body".to_string(),
                state: "spilled".to_string(),
                metadata: json!({
                    "queue_message_key": "queue:two",
                }),
            },
            false,
        )?;

        assert_ne!(first.work_id, second.work_id);
        let conn = open_ticket_db(&root)?;
        let first_spawn_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_core_spawn_edges
            WHERE child_entity_type = 'WorkItem'
              AND child_entity_id = ?1
              AND spawn_kind = 'self-work:queue-overflow'
              AND parent_entity_type = 'QueueTask'
              AND accepted = 1
            "#,
            params![&first.work_id],
            |row| row.get(0),
        )?;
        let second_spawn_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_core_spawn_edges
            WHERE child_entity_type = 'WorkItem'
              AND child_entity_id = ?1
              AND spawn_kind = 'self-work:queue-overflow'
              AND parent_entity_type = 'QueueTask'
              AND accepted = 1
            "#,
            params![&second.work_id],
            |row| row.get(0),
        )?;
        assert_eq!(first_spawn_count, 1);
        assert_eq!(second_spawn_count, 1);
        let listed = list_ticket_self_work_items(&root, Some("internal"), None, 10)?;
        let overflow_count = listed
            .iter()
            .filter(|item| item.kind == "queue-overflow")
            .count();
        assert_eq!(overflow_count, 2);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn review_spawn_budget_is_scoped_to_parent_work_episode() {
        let first = json!({
            "thread_key": "email/shared-thread",
            "parent_work_id": "work-episode-1"
        });
        let second = json!({
            "thread_key": "email/shared-thread",
            "parent_work_id": "work-episode-2"
        });
        let (first_key, first_budget) = ticket_self_work_spawn_budget(
            "completion-review-rework",
            "email/shared-thread",
            &first,
        );
        let (second_key, second_budget) = ticket_self_work_spawn_budget(
            "completion-review-rework",
            "email/shared-thread",
            &second,
        );

        assert_ne!(first_key, second_key);
        assert!(first_key.ends_with("episode:work-episode-1"));
        assert!(second_key.ends_with("episode:work-episode-2"));
        assert_eq!(first_budget, 5);
        assert_eq!(second_budget, 5);
    }

    #[test]
    fn queue_parent_cannot_spawn_strategy_direction_self_work() -> Result<()> {
        let root = temp_root("queue-strategy-self-work-core-rejected");
        std::fs::create_dir_all(&root)?;

        let err = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "strategic-direction-pass".to_string(),
                title: "Strategic direction setup".to_string(),
                body_text: "Establish strategy before benchmark work.".to_string(),
                state: "open".to_string(),
                metadata: json!({
                    "thread_key": "queue/normal-work",
                    "source_label": "queue",
                    "workspace_root": "/tmp/ctox-workspace",
                    "dedupe_key": "strategy-direction:queue/normal-work",
                }),
            },
            false,
        )
        .expect_err("queue execution must not spawn strategic-direction self-work");
        assert!(err.to_string().contains("core spawn gate rejected"));

        let conn = open_ticket_db(&root)?;
        let rejected_edges: i64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM ctox_core_spawn_edges
            WHERE spawn_kind = 'self-work:strategic-direction-pass'
              AND accepted = 0
            "#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rejected_edges, 1);
        let violation_codes_json: String = conn.query_row(
            r#"
            SELECT violation_codes_json
            FROM ctox_core_spawn_edges
            WHERE spawn_kind = 'self-work:strategic-direction-pass'
              AND accepted = 0
            LIMIT 1
            "#,
            [],
            |row| row.get(0),
        )?;
        assert!(violation_codes_json.contains("strategy_direction_spawn_for_queue_execution"));

        let items = list_ticket_self_work_items(&root, Some("local"), None, 10)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].state, "blocked");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_binding_can_be_listed_and_guides_live_ticket_skill_selection() -> Result<()> {
        let root = temp_root("source-skill-binding");
        std::fs::create_dir_all(&root)?;

        let binding = put_ticket_source_skill_binding(
            &root,
            "local",
            "roller-ticket-desk-operator-v4",
            "operating-model",
            "active",
            "ticket-onboarding",
            Some("runtime/generated-skills/roller-ticket-desk-operator-v4"),
            Some("Use the generated desk skill for live local ticket routing."),
        )?;
        assert_eq!(binding.source_system, "local");
        assert_eq!(binding.skill_name, "roller-ticket-desk-operator-v4");

        let listed = list_ticket_source_skill_bindings(&root, Some("local"))?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].skill_name, "roller-ticket-desk-operator-v4");

        let suggested = suggested_skill_for_live_ticket_source(
            &root,
            &RoutedTicketEvent {
                event_key: "evt-1".to_string(),
                ticket_key: "local:123".to_string(),
                source_system: "local".to_string(),
                remote_event_id: "comment-1".to_string(),
                event_type: "comment".to_string(),
                summary: "Please continue with the MHS lock investigation.".to_string(),
                body_text: "The user is still locked after the password reset.".to_string(),
                title: "Sperrung MHS Benutzer".to_string(),
                remote_status: "open".to_string(),
                label: "support/access".to_string(),
                bundle_label: "support/access".to_string(),
                bundle_version: 1,
                case_id: "case-1".to_string(),
                dry_run_id: "dry-1".to_string(),
                dry_run_artifact: json!({}),
                support_mode: "support_case".to_string(),
                approval_mode: "human_approval_required".to_string(),
                autonomy_level: "A0".to_string(),
                risk_level: "unknown".to_string(),
                thread_key: "ticket:local:123".to_string(),
            },
        )?;
        assert_eq!(suggested.as_deref(), Some("roller-ticket-desk-operator-v4"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn history_export_writes_canonical_jsonl_from_mirrored_tickets() -> Result<()> {
        let root = temp_root("history-export");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "[VPN] host vpn-gateway-01 unreachable",
            "Users cannot reach vpn-gateway-01 after the overnight maintenance window.",
            Some("open"),
            Some("high"),
        )?;
        ticket_local_native::add_local_comment(
            &root,
            &remote.ticket_id,
            "Please verify whether the tunnel service restarted cleanly.",
        )?;
        sync_ticket_system(&root, "local")?;

        let output = root.join("runtime/history/local-history.jsonl");
        let result = export_ticket_history_dataset(&root, "local", &output)?;
        assert_eq!(result.get("record_count").and_then(Value::as_u64), Some(1));
        let content = std::fs::read_to_string(&output)?;
        let first_line = content.lines().next().context("missing exported row")?;
        let row: Value = serde_json::from_str(first_line)?;
        assert_eq!(
            row.get("ticket_id").and_then(Value::as_str),
            Some(remote.ticket_id.as_str())
        );
        assert_eq!(
            row.get("title").and_then(Value::as_str),
            Some("[VPN] host vpn-gateway-01 unreachable")
        );
        assert_eq!(
            row.get("request_type").and_then(Value::as_str),
            Some("ticket")
        );
        assert_eq!(row.get("category").and_then(Value::as_str), Some("general"));
        assert!(row
            .get("request_text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("vpn-gateway-01"));
        assert!(row
            .get("action_text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("Please verify"));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn history_export_skips_ctox_self_work_and_legacy_internal_tickets() -> Result<()> {
        let root = temp_root("history-export-filters-self-work");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "VPN Benutzer kann sich nicht anmelden",
            "Benutzer kann sich nach Passwortwechsel nicht am VPN anmelden.",
            Some("open"),
            Some("high"),
        )?;

        let _work = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "system-onboarding".to_string(),
                title: "CTOX: Ticket system onboarding".to_string(),
                body_text: "Visible onboarding work item for routing validation.".to_string(),
                state: "open".to_string(),
                metadata: json!({"skill": "system-onboarding"}),
            },
            true,
        )?;

        ticket_local_native::create_local_ticket(
            &root,
            "CTOX: legacy onboarding note",
            "Review the attached ticket system and generate onboarding work.",
            Some("closed"),
            Some("normal"),
        )?;

        sync_ticket_system(&root, "local")?;

        let output = root.join("runtime/history/local-history-filtered.jsonl");
        let result = export_ticket_history_dataset(&root, "local", &output)?;
        assert_eq!(result.get("record_count").and_then(Value::as_u64), Some(1));
        let content = std::fs::read_to_string(&output)?;
        let exported_rows: Vec<Value> = content
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        assert_eq!(exported_rows.len(), 1);
        assert_eq!(
            exported_rows[0].get("ticket_id").and_then(Value::as_str),
            Some(remote.ticket_id.as_str())
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_show_and_query_use_bound_operating_model_artifact() -> Result<()> {
        let root = temp_root("source-skill-query");
        std::fs::create_dir_all(&root)?;
        let skill_dir = root.join("runtime/generated-skills/demo-skill");
        let generated_dir = skill_dir.join("references/generated");
        std::fs::create_dir_all(&generated_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Demo Desk Skill\n\nUse this for desk work.\n\n## How To Handle A New Ticket\n\nQuery historical families first.\n",
        )?;
        std::fs::write(
            generated_dir.join("family_playbooks.json"),
            serde_json::to_string_pretty(&vec![json!({
                "family_key": "access :: identity :: mhs",
                "signals": {
                    "token_signals": ["MHS", "Sperrung"],
                    "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
                },
                "usual_handling": {
                    "dominant_channels": [["email", 4]],
                    "dominant_states": [["open", 4]],
                    "actions_seen": ["entsperrt"],
                    "closure_tendency": 0.75
                },
                "decision_support": {
                    "mode": "access_change",
                    "operator_summary": "This desk handles MHS user locks as access work.",
                    "triage_focus": ["identify the locked user"],
                    "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                    "close_when": "Close when the user can sign in again.",
                    "caution_signals": ["do not unlock the wrong account"],
                    "note_guidance": "Record the affected identity and whether retry worked."
                },
                "historical_examples": {
                    "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer", "why": "Representative historical case."}]
                }
            })])?
                + "\n",
        )?;
        std::fs::write(
            generated_dir.join("retrieval_index.jsonl"),
            serde_json::to_string(&json!({
                "card_id": "family:1",
                "card_type": "family_playbook",
                "family_key": "access :: identity :: mhs",
                "request_type": "access",
                "category": "identity",
                "subcategory": "mhs",
                "text": "access identity mhs benutzer sperrung entsperrt"
            }))? + "\n",
        )?;
        put_ticket_source_skill_binding(
            &root,
            "local",
            "demo-skill",
            "operating-model",
            "active",
            "test",
            Some("runtime/generated-skills/demo-skill"),
            Some("test binding"),
        )?;

        let shown = show_ticket_source_skill(&root, "local")?;
        assert_eq!(shown.binding.skill_name, "demo-skill");
        assert!(shown
            .skill_preview
            .unwrap_or_default()
            .contains("Demo Desk Skill"));

        let queried = query_ticket_source_skill(
            &root,
            "local",
            "Benutzer ist im MHS gesperrt und braucht Entsperrung.",
            1,
        )?;
        let top_family = queried
            .get("result")
            .and_then(|value| value.get("families"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("family_key"))
            .and_then(Value::as_str);
        assert_eq!(top_family, Some("access :: identity :: mhs"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn skillbook_runbook_bundle_can_drive_reply_flow_for_ticket_case() -> Result<()> {
        let root = temp_root("source-skill-runbook-reply");
        std::fs::create_dir_all(&root)?;

        let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
        std::fs::create_dir_all(&bundle_dir)?;
        std::fs::write(
            bundle_dir.join("main_skill.json"),
            serde_json::to_string_pretty(&json!({
                "main_skill_id": "eventus.email.support.main.v1",
                "title": "Eventus Email Support Main",
                "primary_channel": "email",
                "entry_action": "resolve_runbook_item",
                "resolver_contract": {"mode": "runbook-item"},
                "execution_contract": {"mode": "reply-only"},
                "resolve_flow": [
                    "resolve the best matching runbook item",
                    "load the linked skillbook",
                    "compose a reply suggestion"
                ],
                "writeback_flow": [
                    "verify reply",
                    "write public comment back to the ticket"
                ],
                "linked_skillbooks": ["eventus.email.support.v1"],
                "linked_runbooks": ["eventus.runbook.registration.v1"]
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("skillbook.json"),
            serde_json::to_string_pretty(&json!({
                "skillbook_id": "eventus.email.support.v1",
                "title": "Eventus Email Support",
                "version": "v1",
                "mission": "Handle incoming support emails safely and clearly.",
                "non_negotiable_rules": [
                    "Never invent product behavior.",
                    "Keep the answer aligned with the manual."
                ],
                "runtime_policy": "Resolve a runbook item first, then draft the reply.",
                "answer_contract": "Give a concise, actionable email answer.",
                "workflow_backbone": [
                    "identify the request",
                    "load the runbook item",
                    "reply only from the runbook facts"
                ],
                "routing_taxonomy": ["registration", "login"],
                "linked_runbooks": ["eventus.runbook.registration.v1"]
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("runbook.json"),
            serde_json::to_string_pretty(&json!({
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "title": "Registration issues",
                "version": "v1",
                "status": "active",
                "problem_domain": "registration",
                "item_labels": ["REG-03"]
            }))?,
        )?;
        std::fs::write(
            bundle_dir.join("runbook_items.jsonl"),
            serde_json::to_string(&json!({
                "item_id": "eventus.runbook.reg.03.v1",
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "REG-03",
                "title": "Password is rejected during registration",
                "problem_class": "registration.password_policy",
                "trigger_phrases": [
                    "password is not accepted",
                    "registration password",
                    "what password rules apply"
                ],
                "entry_conditions": [
                    "user is in the registration flow"
                ],
                "earliest_blocker": "Password does not satisfy the registration password policy.",
                "expected_guidance": "Please check whether your password has at least 6 characters and contains one uppercase letter, one lowercase letter and one digit. Avoid easily guessable personal data. If the password still gets rejected although it matches these rules, reply to this email and we will investigate further.",
                "tool_actions": {
                    "kind": "reply_only",
                    "tools": []
                },
                "verification": [
                    "reply references the documented password rules"
                ],
                "writeback_policy": {
                    "channel": "public_reply"
                },
                "escalate_when": [
                    "a formally valid password is still rejected"
                ],
                "sources": {
                    "manual": "Supplier manual - E.VENT.US_en (demo manual)"
                },
                "pages": ["8"],
                "chunk_text": "REG-03 registration password rejected password policy one uppercase one lowercase one digit minimum 6 characters"
            }))? + "\n",
        )?;

        let imported = import_ticket_source_skill_bundle(
            &root,
            "local",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )?;
        assert_eq!(
            imported.get("embeddings_indexed").and_then(Value::as_bool),
            Some(false)
        );

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Registration password rejected",
            "Hello, during registration my password is not accepted. Which password rules apply?",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        let queried = query_ticket_source_skill(
            &root,
            "local",
            "During registration my password is not accepted. Which password rules apply?",
            1,
        )?;
        assert_eq!(
            queried
                .get("result")
                .and_then(|value| value.get("retrieval_mode"))
                .and_then(Value::as_str),
            Some("lexical_fallback")
        );
        assert_eq!(
            queried
                .get("result")
                .and_then(|value| value.get("matches"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("label"))
                .and_then(Value::as_str),
            Some("REG-03")
        );

        set_ticket_label(
            &root,
            &ticket_key,
            "support/registration",
            "test",
            Some("Bind this ticket to the registration reply flow."),
            json!({}),
        )?;
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/registration".to_string(),
                runbook_id: "eventus.runbook.registration.v1".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "eventus.reply.policy".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "reply-verification".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
            },
        )?;

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Prepare a registration reply"),
            None,
        )?;
        let case = load_case(&root, &dry_run.case_id)?.context("case missing after dry run")?;
        assert_eq!(case.state, "approval_pending");
        decide_case_approval(
            &root,
            &dry_run.case_id,
            "approved",
            "owner",
            Some("Approved public reply for FAQ-style registration request."),
        )?;

        let reply = compose_ticket_source_skill_reply(
            &root,
            None,
            Some(&dry_run.case_id),
            "suggestion",
            None,
            false,
        )?;
        assert_eq!(
            reply.get("matched_label").and_then(Value::as_str),
            Some("REG-03")
        );
        let reply_body = reply
            .get("reply_body")
            .and_then(Value::as_str)
            .context("reply body missing")?
            .to_string();
        assert!(reply_body.contains("at least 6 characters"));
        assert!(reply_body.contains("one uppercase letter"));

        record_execution_action(&root, &dry_run.case_id, "Prepared public reply from REG-03")?;
        record_verification(
            &root,
            &dry_run.case_id,
            "passed",
            Some("Reply follows REG-03 and references the documented password rules."),
        )?;
        writeback_comment(&root, &dry_run.case_id, &reply_body, false)?;

        let history = list_ticket_history(&root, &ticket_key, 12)?;
        assert!(history.iter().any(|event| {
            event.direction == "outbound"
                && event.body_text.contains("at least 6 characters")
                && event.body_text.contains("one uppercase letter")
        }));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn skillbook_runbook_reply_requires_review_for_ambiguous_lexical_match() -> Result<()> {
        let root = temp_root("source-skill-runbook-ambiguous");
        std::fs::create_dir_all(&root)?;

        let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
        write_reply_bundle(
            &bundle_dir,
            &[
                json!({
                    "item_id": "eventus.runbook.reg.03.v1",
                    "runbook_id": "eventus.runbook.registration.v1",
                    "skillbook_id": "eventus.email.support.v1",
                    "label": "REG-03",
                    "title": "Password is rejected during registration",
                    "problem_class": "registration.password_policy",
                    "trigger_phrases": [
                        "password is not accepted",
                        "registration password",
                        "what password rules apply"
                    ],
                    "entry_conditions": ["user is in the registration flow"],
                    "earliest_blocker": "Password does not satisfy the registration password policy.",
                    "expected_guidance": "Reply with the documented password policy.",
                    "tool_actions": { "kind": "reply_only", "tools": [] },
                    "verification": ["reply references the documented password rules"],
                    "writeback_policy": { "channel": "public_reply" },
                    "escalate_when": ["a formally valid password is still rejected"],
                    "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
                    "pages": ["8"],
                    "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
                }),
                json!({
                    "item_id": "eventus.runbook.reg.08.v1",
                    "runbook_id": "eventus.runbook.registration.v1",
                    "skillbook_id": "eventus.email.support.v1",
                    "label": "REG-08",
                    "title": "Registration password policy reminder",
                    "problem_class": "registration.password_policy_repeat",
                    "trigger_phrases": [
                        "password rules",
                        "registration password",
                        "password policy"
                    ],
                    "entry_conditions": ["user asks for password rules during registration"],
                    "earliest_blocker": "Password policy reminder is still too generic for direct send.",
                    "expected_guidance": "Reply with a manual-backed password policy reminder.",
                    "tool_actions": { "kind": "reply_only", "tools": [] },
                    "verification": ["reply references the documented password rules"],
                    "writeback_policy": { "channel": "public_reply" },
                    "escalate_when": ["the right rule set is still unclear"],
                    "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
                    "pages": ["8"],
                    "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
                }),
            ],
        )?;

        import_ticket_source_skill_bundle(
            &root,
            "local",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Registration password rules",
            "During registration my password is not accepted. Which password rules apply?",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;

        let reply = compose_ticket_source_skill_reply(
            &root,
            Some(&format!("local:{}", remote.ticket_id)),
            None,
            "suggestion",
            None,
            false,
        )?;
        assert_eq!(
            reply.get("decision").and_then(Value::as_str),
            Some("needs_review")
        );
        assert_eq!(
            reply.get("retrieval_mode").and_then(Value::as_str),
            Some("lexical_fallback")
        );
        assert_eq!(
            reply
                .get("matches")
                .and_then(Value::as_array)
                .map(|items| items.len()),
            Some(2)
        );
        assert!(reply.get("reply_body").is_none());

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn skillbook_runbook_flow_stays_generic_until_adapter_writeback_boundary() -> Result<()> {
        let root = temp_root("source-skill-runbook-generic-adapter");
        std::fs::create_dir_all(&root)?;

        let bundle_dir = root.join("runtime/generated-skills/eventus-email-main");
        write_reply_bundle(
            &bundle_dir,
            &[json!({
                "item_id": "eventus.runbook.reg.03.v1",
                "runbook_id": "eventus.runbook.registration.v1",
                "skillbook_id": "eventus.email.support.v1",
                "label": "REG-03",
                "title": "Password is rejected during registration",
                "problem_class": "registration.password_policy",
                "trigger_phrases": [
                    "password is not accepted",
                    "registration password",
                    "what password rules apply"
                ],
                "entry_conditions": ["user is in the registration flow"],
                "earliest_blocker": "Password does not satisfy the registration password policy.",
                "expected_guidance": "Please check whether your password has at least 6 characters and contains one uppercase letter, one lowercase letter and one digit.",
                "tool_actions": { "kind": "reply_only", "tools": [] },
                "verification": ["reply references the documented password rules"],
                "writeback_policy": { "channel": "public_reply" },
                "escalate_when": ["a formally valid password is still rejected"],
                "sources": { "manual": "Supplier manual - E.VENT.US_en (demo manual)" },
                "pages": ["8"],
                "chunk_text": "registration password rejected password rules one uppercase one lowercase one digit minimum 6 characters"
            })],
        )?;

        import_ticket_source_skill_bundle(
            &root,
            "mockdesk",
            bundle_dir.to_str().context("bundle path utf-8")?,
            None,
            true,
        )?;

        let now = now_iso_string();
        let ticket_key = upsert_ticket_from_adapter(
            &root,
            AdapterTicketMirrorRequest {
                system: "mockdesk",
                remote_ticket_id: "T-42",
                title: "Registration password rejected",
                body_text: "Hello, during registration my password is not accepted. Which password rules apply?",
                remote_status: "open",
                priority: Some("normal"),
                requester: Some("test@example.com"),
                metadata: json!({"channel": "email"}),
                external_created_at: &now,
                external_updated_at: &now,
            },
        )?
        .key;
        upsert_ticket_event_from_adapter(
            &root,
            AdapterTicketEventRequest {
                system: "mockdesk",
                remote_ticket_id: "T-42",
                remote_event_id: "E-1",
                direction: "inbound",
                event_type: "email",
                summary: "Customer asks for password rules",
                body_text:
                    "During registration my password is not accepted. Which password rules apply?",
                metadata: json!({}),
                external_created_at: &now,
            },
        )?;

        let resolved = resolve_ticket_source_skill_for_target(&root, Some(&ticket_key), None, 1)?;
        assert_eq!(
            resolved
                .get("resolution")
                .and_then(|value| value.get("matches"))
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("label"))
                .and_then(Value::as_str),
            Some("REG-03")
        );

        set_ticket_label(
            &root,
            &ticket_key,
            "support/registration",
            "test",
            Some("Bind this ticket to the registration reply flow."),
            json!({}),
        )?;
        for domain in REQUIRED_KNOWLEDGE_DOMAINS {
            put_ticket_knowledge_entry(
                &root,
                TicketKnowledgeUpsertInput {
                    source_system: "mockdesk".to_string(),
                    domain: (*domain).to_string(),
                    knowledge_key: format!("baseline::{domain}"),
                    title: format!("Mockdesk {domain}"),
                    summary: format!("Baseline knowledge for required domain {domain}."),
                    status: "active".to_string(),
                    content: json!({
                        "source": "test",
                        "domain": domain,
                    }),
                },
            )?;
        }
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/registration".to_string(),
                runbook_id: "eventus.runbook.registration.v1".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "eventus.reply.policy".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "reply-verification".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
            },
        )?;

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Prepare a registration reply"),
            None,
        )?;
        decide_case_approval(
            &root,
            &dry_run.case_id,
            "approved",
            "owner",
            Some("Approved public reply for FAQ-style registration request."),
        )?;
        let reply = compose_ticket_source_skill_reply(
            &root,
            None,
            Some(&dry_run.case_id),
            "suggestion",
            None,
            false,
        )?;
        let reply_body = reply
            .get("reply_body")
            .and_then(Value::as_str)
            .context("reply body missing")?
            .to_string();
        assert!(reply_body.contains("at least 6 characters"));
        record_execution_action(&root, &dry_run.case_id, "Prepared public reply from REG-03")?;
        let case = record_verification(
            &root,
            &dry_run.case_id,
            "passed",
            Some("Reply follows REG-03 and references the documented password rules."),
        )?;
        assert_eq!(case.state, "writeback_pending");

        let err = writeback_comment(&root, &dry_run.case_id, &reply_body, false)
            .expect_err("mockdesk should only fail at the adapter writeback boundary");
        assert!(err
            .to_string()
            .contains("unsupported ticket system for writeback: mockdesk"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn public_writeback_requires_verified_case_state() -> Result<()> {
        let root = temp_root("ticket-writeback-gate");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Registration password rejected",
            "Hello, during registration my password is not accepted. Which password rules apply?",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        set_ticket_label(
            &root,
            &ticket_key,
            "support/registration",
            "test",
            Some("Bind this ticket to the registration reply flow."),
            json!({}),
        )?;
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/registration".to_string(),
                runbook_id: "eventus.runbook.registration.v1".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "eventus.reply.policy".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "reply-verification".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
            },
        )?;

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Prepare a registration reply"),
            None,
        )?;
        decide_case_approval(
            &root,
            &dry_run.case_id,
            "approved",
            "owner",
            Some("Approved public reply for FAQ-style registration request."),
        )?;
        record_execution_action(&root, &dry_run.case_id, "Prepared public reply draft")?;

        let err = writeback_comment(
            &root,
            &dry_run.case_id,
            "Hello, please check the documented password rules.",
            false,
        )
        .expect_err("public writeback before verification should fail");
        assert!(err
            .to_string()
            .contains("is not ready for writeback; current state is executing"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn ticket_close_is_blocked_without_verified_guard_proof() -> Result<()> {
        let root = temp_root("ticket-close-guard");
        std::fs::create_dir_all(&root)?;

        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Registration password rejected",
            "Hello, during registration my password is not accepted.",
            Some("open"),
            Some("normal"),
        )?;
        sync_ticket_system(&root, "local")?;
        let ticket_key = format!("local:{}", remote.ticket_id);

        set_ticket_label(
            &root,
            &ticket_key,
            "support/registration",
            "test",
            Some("Bind this ticket to the registration reply flow."),
            json!({}),
        )?;
        put_control_bundle(
            &root,
            ControlBundleInput {
                label: "support/registration".to_string(),
                runbook_id: "eventus.runbook.registration.v1".to_string(),
                runbook_version: "v1".to_string(),
                policy_id: "eventus.reply.policy".to_string(),
                policy_version: "v1".to_string(),
                approval_mode: "direct_execute_allowed".to_string(),
                autonomy_level: "A1".to_string(),
                verification_profile_id: "reply-verification".to_string(),
                writeback_profile_id: "writeback-comment".to_string(),
                support_mode: "support_case".to_string(),
                default_risk_level: "low".to_string(),
                execution_actions: default_execution_actions(),
                notes: Some("Public reply flow for registration FAQ-style tickets.".to_string()),
            },
        )?;

        let dry_run = create_dry_run(
            &root,
            &ticket_key,
            Some("Prepare a registration reply"),
            None,
        )?;
        decide_case_approval(
            &root,
            &dry_run.case_id,
            "approved",
            "owner",
            Some("Approved bounded reply work."),
        )?;
        record_execution_action(&root, &dry_run.case_id, "Prepared reply draft")?;

        let err = close_case(&root, &dry_run.case_id, Some("premature close"))
            .expect_err("close without verification must be rejected by the core guard");
        assert!(err.to_string().contains("closure_requires_verification"));

        let case = record_verification(
            &root,
            &dry_run.case_id,
            "passed",
            Some("Reply was verified against source-skill evidence."),
        )?;
        assert_eq!(case.state, "writeback_pending");
        let case = close_case(&root, &dry_run.case_id, Some("verified close"))?;
        assert_eq!(case.state, "closed");

        let conn = open_ticket_db(&root)?;
        let accepted_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE entity_id = ?1 AND accepted = 1",
            params![dry_run.case_id],
            |row| row.get(0),
        )?;
        let rejected_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ctox_core_transition_proofs WHERE entity_id = ?1 AND accepted = 0",
            params![dry_run.case_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted_proofs, 1);
        assert_eq!(rejected_proofs, 1);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_review_note_accepts_plain_grounded_internal_note() -> Result<()> {
        let root = temp_root("source-skill-note-review-good");
        std::fs::create_dir_all(&root)?;
        let skill_dir = root.join("runtime/generated-skills/demo-skill");
        let generated_dir = skill_dir.join("references/generated");
        std::fs::create_dir_all(&generated_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Demo Desk Skill\n\n## How To Handle A New Ticket\n\nUse desk language.\n",
        )?;
        std::fs::write(
            generated_dir.join("family_playbooks.json"),
            serde_json::to_string_pretty(&vec![json!({
                "family_key": "access :: identity :: mhs",
                "signals": {
                    "token_signals": ["MHS", "Sperrung", "Benutzer"],
                    "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
                },
                "usual_handling": {
                    "dominant_channels": [["email", 4]],
                    "dominant_states": [["open", 4]],
                    "actions_seen": ["entsperrt"],
                    "closure_tendency": 0.75
                },
                "decision_support": {
                    "mode": "access_change",
                    "operator_summary": "This desk handles MHS user locks as access work.",
                    "triage_focus": ["identify the locked user"],
                    "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                    "close_when": "Close when the user can sign in again.",
                    "caution_signals": ["do not unlock the wrong account"],
                    "note_guidance": "Record the affected identity and whether retry worked."
                },
                "historical_examples": {
                    "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer GAJ", "why": "Representative historical case."}]
                }
            })])?
                + "\n",
        )?;
        std::fs::write(
            generated_dir.join("retrieval_index.jsonl"),
            serde_json::to_string(&json!({
                "card_id": "family:1",
                "card_type": "family_playbook",
                "family_key": "access :: identity :: mhs",
                "request_type": "access",
                "category": "identity",
                "subcategory": "mhs",
                "text": "access identity mhs benutzer sperrung kurzzeichen login entsperrt"
            }))? + "\n",
        )?;
        put_ticket_source_skill_binding(
            &root,
            "local",
            "demo-skill",
            "operating-model",
            "active",
            "test",
            Some("runtime/generated-skills/demo-skill"),
            Some("test binding"),
        )?;
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Sperrung MHS Benutzer GAJ",
            "Benutzer GAJ ist in MHS gesperrt und kann sich nicht mehr anmelden.",
            Some("open"),
            Some("high"),
        )?;
        sync_ticket_system(&root, "local")?;
        let review = review_ticket_note_with_source_skill(
            &root,
            &format!("local:{}", remote.ticket_id),
            "Benutzer GAJ ist in MHS gesperrt. Ich prüfe zuerst das betroffene Kurzzeichen und teste danach den erneuten Login nach der Entsperrung.",
            1,
        )?;
        assert!(review.desk_ready);
        assert!(review.language_clean);
        assert!(review.copy_safe);
        assert!(review.grounded_in_ticket);
        assert_eq!(
            review.matched_family.as_deref(),
            Some("access :: identity :: mhs")
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn source_skill_review_note_flags_leaky_or_copied_notes() -> Result<()> {
        let root = temp_root("source-skill-note-review-bad");
        std::fs::create_dir_all(&root)?;
        let skill_dir = root.join("runtime/generated-skills/demo-skill");
        let generated_dir = skill_dir.join("references/generated");
        std::fs::create_dir_all(&generated_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Demo Desk Skill\n\n## How To Handle A New Ticket\n\nUse desk language.\n",
        )?;
        std::fs::write(
            generated_dir.join("family_playbooks.json"),
            serde_json::to_string_pretty(&vec![json!({
                "family_key": "access :: identity :: mhs",
                "signals": {
                    "token_signals": ["MHS", "Sperrung"],
                    "common_phrases": ["mhs benutzer", "benutzer gesperrt"]
                },
                "usual_handling": {
                    "dominant_channels": [["email", 4]],
                    "dominant_states": [["open", 4]],
                    "actions_seen": ["entsperrt"],
                    "closure_tendency": 0.75
                },
                "decision_support": {
                    "mode": "access_change",
                    "operator_summary": "This desk handles MHS user locks as access work.",
                    "triage_focus": ["identify the locked user"],
                    "handling_steps": ["confirm the affected MHS identity", "unlock only after identity is clear"],
                    "close_when": "Close when the user can sign in again.",
                    "caution_signals": ["do not unlock the wrong account"],
                    "note_guidance": "Record the affected identity and whether retry worked."
                },
                "historical_examples": {
                    "canonical": [{"ticket_id": "100", "title": "Sperrung MHS Benutzer", "why": "Representative historical case."}]
                }
            })])?
                + "\n",
        )?;
        std::fs::write(
            generated_dir.join("retrieval_index.jsonl"),
            serde_json::to_string(&json!({
                "card_id": "family:1",
                "card_type": "family_playbook",
                "family_key": "access :: identity :: mhs",
                "request_type": "access",
                "category": "identity",
                "subcategory": "mhs",
                "text": "access identity mhs benutzer sperrung entsperrt"
            }))? + "\n",
        )?;
        put_ticket_source_skill_binding(
            &root,
            "local",
            "demo-skill",
            "operating-model",
            "active",
            "test",
            Some("runtime/generated-skills/demo-skill"),
            Some("test binding"),
        )?;
        let remote = ticket_local_native::create_local_ticket(
            &root,
            "Sperrung MHS Benutzer",
            "MHS account is locked.",
            Some("open"),
            Some("high"),
        )?;
        sync_ticket_system(&root, "local")?;
        let review = review_ticket_note_with_source_skill(
            &root,
            &format!("local:{}", remote.ticket_id),
            "This desk handles MHS user locks as access work. Use `note_guidance` from sqlite before writeback.",
            1,
        )?;
        assert!(!review.desk_ready);
        assert!(!review.language_clean);
        assert!(!review.copy_safe);
        assert!(review
            .findings
            .iter()
            .any(|item| item.kind == "internal_field_names" || item.kind == "tooling_terms"));
        assert!(review
            .findings
            .iter()
            .any(|item| item.kind == "copied_skill_language"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn self_work_lifecycle_supports_assign_notes_and_transition() -> Result<()> {
        let root = temp_root("self-work-lifecycle");
        std::fs::create_dir_all(&root)?;

        let item = put_ticket_self_work_item(
            &root,
            TicketSelfWorkUpsertInput {
                source_system: "local".to_string(),
                kind: "onboarding-gap".to_string(),
                title: "Review access gaps for monitoring".to_string(),
                body_text: "Investigate which monitoring systems still need access.".to_string(),
                state: "open".to_string(),
                metadata: json!({"skill": "system-onboarding"}),
            },
            true,
        )?;
        assert_eq!(item.state, "published");
        assert!(item.remote_ticket_id.is_some());

        let item = assign_ticket_self_work_item(
            &root,
            &item.work_id,
            "ctox-agent",
            "ctox",
            Some("CTOX should own onboarding work by default"),
        )?;
        assert_eq!(item.assigned_to.as_deref(), Some("ctox-agent"));
        assert_eq!(item.assigned_by.as_deref(), Some("ctox"));

        let note = append_ticket_self_work_note(
            &root,
            &item.work_id,
            "Observed that monitoring access is still missing for two systems.",
            "ctox",
            "internal",
        )?;
        assert_eq!(note.authored_by, "ctox");
        assert_eq!(note.visibility, "internal");
        assert!(note.remote_event_id.is_some());

        let item = transition_ticket_self_work_item(
            &root,
            &item.work_id,
            "blocked",
            "ctox",
            Some("Blocked until monitoring credentials are provided."),
            "internal",
        )?;
        assert_eq!(item.state, "blocked");

        let shown = load_ticket_self_work_item(&root, &item.work_id)?
            .context("internal work item missing after lifecycle")?;
        assert_eq!(shown.assigned_to.as_deref(), Some("ctox-agent"));

        let assignments = list_ticket_self_work_assignments(&root, &item.work_id, 10)?;
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].assigned_to, "ctox-agent");

        let notes = list_ticket_self_work_notes(&root, &item.work_id, 10)?;
        assert_eq!(notes.len(), 2);
        assert!(notes
            .iter()
            .any(|entry| entry.body_text.contains("two systems")));
        assert!(notes
            .iter()
            .any(|entry| entry.body_text.contains("credentials are provided")));

        let local_ticket = ticket_local_native::load_local_ticket(
            &root,
            item.remote_ticket_id
                .as_deref()
                .context("missing remote ticket id")?,
        )?
        .context("published local ticket missing")?;
        assert_eq!(
            local_ticket
                .metadata
                .get("assigned_to")
                .and_then(Value::as_str),
            Some("ctox-agent")
        );
        let local_events = ticket_local_native::list_local_ticket_events(
            &root,
            item.remote_ticket_id.as_deref().unwrap(),
            20,
        )?;
        assert!(local_events
            .iter()
            .any(|event| event.event_type == "assignment_changed"));
        assert!(local_events
            .iter()
            .any(|event| event.body_text.contains("two systems")));
        assert!(local_events
            .iter()
            .any(|event| event.event_type == "status_changed"));

        let audit = list_audit_records(
            &root,
            Some(&format!("*self-work:{}*", shown.source_system)),
            20,
        )?;
        assert!(audit
            .iter()
            .any(|entry| entry.action_type == "self_work_assigned"));
        assert!(audit
            .iter()
            .any(|entry| entry.action_type == "self_work_note_appended"));
        assert!(audit
            .iter()
            .any(|entry| entry.action_type == "self_work_transitioned"));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
