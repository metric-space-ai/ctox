// Origin: CTOX
// License: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The core runtime state machine guards harness behavior that must not be
/// left to prompt discipline: founder communication, deadlines, queue repair,
/// ticket closure, review gates, and knowledge capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreEntityType {
    Service,
    Mission,
    Context,
    QueueItem,
    WorkItem,
    Ticket,
    Review,
    FounderCommunication,
    Commitment,
    Schedule,
    Knowledge,
    Repair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLane {
    P0FounderCommunication,
    P0CommitmentBacking,
    P1RuntimeSafety,
    P1QueueRepair,
    P2MissionDelivery,
    P3Housekeeping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreState {
    Booting,
    Ready,
    Processing,
    Degraded,
    Repairing,
    Stopped,

    Empty,
    Ingesting,
    Rebuilding,
    MissionReady,
    MissionRunning,
    WaitingOnExternal,
    MissionBlocked,
    MissionClosed,

    Cold,
    Hydrating,
    Fresh,
    CompactionDue,
    Compacted,
    Stale,

    Pending,
    Leased,
    Running,
    Blocked,
    Failed,
    Completed,
    Superseded,

    Created,
    Classified,
    TicketBacked,
    Planned,
    Executing,
    AwaitingReview,
    ReworkRequired,
    AwaitingVerification,
    Verified,
    Closed,

    Drafting,
    DraftReady,
    Reviewing,
    Approved,
    Rejected,
    SentBackForRework,

    InboundObserved,
    ContextBuilt,
    ReplyNeeded,
    NoResponseNeeded,
    Sending,
    Sent,
    SendFailed,
    DeliveryRepair,
    AwaitingAcknowledgement,
    Done,
    Escalated,

    Proposed,
    Reviewed,
    Committed,
    BackingScheduled,
    DueSoon,
    InProgress,
    Delivered,
    AtRisk,
    CancelledWithNotice,

    Enabled,
    Due,
    Emitted,
    BackingWorkQueued,
    Acknowledged,
    Paused,
    Expired,
    DisabledByPolicy,

    IncidentObserved,
    LessonDrafted,
    EvidenceAttached,
    Active,

    Healthy,
    PressureDetected,
    RepairPlanning,
    RepairPlanReviewed,
    ApplyingDeterministicActions,
    RepairVerification,
    Restored,
    StillDegraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreEvent {
    Boot,
    HealthCheckPassed,
    HealthCheckFailed,
    StartProcessing,
    StopProcessing,
    StartRepair,
    StopService,

    IngestMission,
    HydrateContext,
    Lease,
    Release,
    Retry,
    Complete,
    Block,
    Fail,
    Supersede,

    Classify,
    CreateTicket,
    Plan,
    Execute,
    RequestReview,
    Approve,
    Reject,
    RequireRework,
    Verify,
    Close,

    ObserveInbound,
    BuildContext,
    DecideNoResponseNeeded,
    DraftReply,
    Send,
    ConfirmDelivery,
    Escalate,

    ProposeCommitment,
    Commit,
    ScheduleBackingTask,
    MarkDueSoon,
    Deliver,
    MarkAtRisk,
    CancelWithNotice,

    EnableSchedule,
    EmitSchedule,
    AcknowledgeSchedule,
    PauseSchedule,
    ExpireSchedule,
    DisableSchedule,

    CaptureIncident,
    DraftLesson,
    AttachEvidence,
    ActivateKnowledge,

    DetectPressure,
    PlanRepair,
    ReviewRepairPlan,
    ApplyRepairActions,
    VerifyRepair,
    MarkRestored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    OutboundEmail,
    OutboundCommunication,
    WorkspaceFile,
    TicketClosure,
    KnowledgeEntry,
    VerificationRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    pub primary_key: String,
    pub expected_terminal_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreEvidenceRefs {
    pub review_audit_key: Option<String>,
    pub approved_body_sha256: Option<String>,
    pub outgoing_body_sha256: Option<String>,
    pub approved_recipient_set_sha256: Option<String>,
    pub outgoing_recipient_set_sha256: Option<String>,
    pub verification_id: Option<String>,
    pub schedule_task_id: Option<String>,
    pub replacement_schedule_task_id: Option<String>,
    pub escalation_id: Option<String>,
    pub knowledge_entry_id: Option<String>,
    pub incident_id: Option<String>,
    pub canonical_hot_path: Vec<String>,
    pub expected_artifact_refs: Vec<ArtifactRef>,
    pub delivered_artifact_refs: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTransitionRequest {
    pub entity_type: CoreEntityType,
    pub entity_id: String,
    pub lane: RuntimeLane,
    pub from_state: CoreState,
    pub to_state: CoreState,
    pub event: CoreEvent,
    pub actor: String,
    pub evidence: CoreEvidenceRefs,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTransitionViolation {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTransitionReport {
    pub accepted: bool,
    pub violations: Vec<CoreTransitionViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreLivenessEntityReport {
    pub entity_type: CoreEntityType,
    pub start_state: CoreState,
    pub terminal_states: Vec<CoreState>,
    pub transition_count: usize,
    pub state_count: usize,
    pub unreachable_states: Vec<CoreState>,
    pub nonterminal_dead_end_states: Vec<CoreState>,
    pub states_without_terminal_path: Vec<CoreState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreLivenessReport {
    pub ok: bool,
    pub entities: Vec<CoreLivenessEntityReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewHarnessState {
    Queued,
    Leased,
    Running,
    AwaitingReview,
    ReviewQueued,
    Reviewing,
    ReviewPassed,
    ReviewRejected,
    ReviewUnavailable,
    ReviewRetry,
    ReworkRequired,
    AwaitingValidation,
    Validating,
    Passed,
    ModelFailed,
    InfraFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewHarnessProof {
    NoProof,
    WorkerFinished,
    WorkerFailed,
    InfraError,
    StartReview,
    SpawnReviewer,
    ReviewPass,
    ReviewReject,
    ReviewUnavailable,
    RetryReview,
    ReviewRetriesExhausted,
    RequeueSameMainWork,
    ReviewRoundsExhausted,
    RunValidator,
    ValidatorPass,
    ValidatorFail,
    ValidatorReworkExhausted,
    ValidatorInfraError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewHarnessBudget {
    NoBudget,
    ReviewRounds,
    ReviewerUnavailableRetries,
    ValidatorRework,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewHarnessEdge {
    pub from: ReviewHarnessState,
    pub to: ReviewHarnessState,
    pub proof: ReviewHarnessProof,
    pub consumes_budget: ReviewHarnessBudget,
    pub terminal_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewHarnessModelReport {
    pub ok: bool,
    pub violations: Vec<String>,
    pub proof: String,
}

impl CoreTransitionReport {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            violations: Vec::new(),
        }
    }

    pub fn rejected(violations: Vec<CoreTransitionViolation>) -> Self {
        Self {
            accepted: false,
            violations,
        }
    }
}

pub fn validate_transition(request: &CoreTransitionRequest) -> CoreTransitionReport {
    let mut violations = Vec::new();

    if !is_allowed_transition(request.entity_type, request.from_state, request.to_state) {
        violations.push(violation(
            "invalid_transition",
            format!(
                "{:?} cannot move from {:?} to {:?}",
                request.entity_type, request.from_state, request.to_state
            ),
        ));
    }

    validate_founder_communication(request, &mut violations);
    validate_review_gate(request, &mut violations);
    validate_rework_required_gate(request, &mut violations);
    validate_review_harness_static_model(&mut violations);
    validate_work_terminal_success_gate(request, &mut violations);
    validate_ticket_closure(request, &mut violations);
    validate_commitment_backing(request, &mut violations);
    validate_schedule_backing(request, &mut violations);
    validate_repair(request, &mut violations);
    validate_knowledge_capture(request, &mut violations);
    validate_outcome_witness(request, &mut violations);

    if violations.is_empty() {
        CoreTransitionReport::accepted()
    } else {
        CoreTransitionReport::rejected(violations)
    }
}

fn validate_review_harness_static_model(violations: &mut Vec<CoreTransitionViolation>) {
    let report = analyze_review_harness_model();
    if report.ok {
        return;
    }
    for violation_text in report.violations {
        violations.push(violation(
            "review_harness_static_model_invalid",
            format!("review harness static model violation: {violation_text}"),
        ));
    }
}

fn validate_work_terminal_success_gate(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if !matches!(
        request.entity_type,
        CoreEntityType::QueueItem | CoreEntityType::WorkItem | CoreEntityType::Ticket
    ) {
        return;
    }
    if !matches!(request.to_state, CoreState::Completed | CoreState::Closed) {
        return;
    }
    if metadata_bool(request, "completion_review_required") {
        return;
    }
    if request
        .metadata
        .get("terminal_policy_proof")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return;
    }

    violations.push(violation(
        "work_terminal_success_requires_review_or_policy",
        "work terminal success requires completion review plus validation proof, or an explicit terminal policy proof",
    ));
}

fn validate_outcome_witness(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request.evidence.expected_artifact_refs.is_empty() {
        return;
    }
    if !is_outcome_terminal_transition(request) {
        return;
    }

    for expected in &request.evidence.expected_artifact_refs {
        let matching_delivery = request
            .evidence
            .delivered_artifact_refs
            .iter()
            .find(|delivered| artifact_ref_satisfies(expected, delivered));
        if matching_delivery.is_none() {
            violations.push(violation(
                "WP-Outcome-Missing",
                format!(
                    "terminal work transition requires delivered {:?} artifact `{}` in `{}`",
                    expected.kind, expected.primary_key, expected.expected_terminal_state
                ),
            ));
        }
    }
}

fn artifact_ref_satisfies(expected: &ArtifactRef, delivered: &ArtifactRef) -> bool {
    expected.kind == delivered.kind
        && expected.expected_terminal_state == delivered.expected_terminal_state
        && (expected.primary_key == delivered.primary_key
            || expected.primary_key == "*"
            || expected.primary_key.starts_with("thread:"))
}

fn is_outcome_terminal_transition(request: &CoreTransitionRequest) -> bool {
    matches!(
        request.to_state,
        CoreState::Completed | CoreState::Closed | CoreState::Sent | CoreState::Done
    )
}

fn validate_founder_communication(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    let protected = is_founder_protected(request);

    if protected && request.lane != RuntimeLane::P0FounderCommunication {
        violations.push(violation(
            "founder_lane_required",
            "founder/owner/admin communication must run in the P0 founder communication lane",
        ));
    }

    if protected && request.to_state == CoreState::Superseded {
        violations.push(violation(
            "founder_work_cannot_spill",
            "founder/owner/admin communication cannot be superseded by lower-priority queue work",
        ));
    }

    if protected && matches!(request.to_state, CoreState::Sending | CoreState::Sent) {
        require_reviewed_outbound(request, violations);
    }
}

fn validate_review_gate(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    let owner_visible_completion = request
        .metadata
        .get("owner_visible_completion")
        .map(|value| value == "true")
        .unwrap_or(false);

    if owner_visible_completion
        && matches!(request.to_state, CoreState::Closed | CoreState::Delivered)
    {
        if request
            .evidence
            .review_audit_key
            .as_deref()
            .unwrap_or("")
            .is_empty()
        {
            violations.push(violation(
                "owner_visible_completion_requires_review",
                "owner-visible completion claims require a durable review audit key",
            ));
        }
    }

    validate_required_completion_review(request, violations);
    validate_review_checkpoint(request, violations);
}

fn validate_required_completion_review(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if !metadata_bool(request, "completion_review_required") {
        return;
    }
    if !is_outcome_terminal_transition(request) {
        return;
    }

    let review_audit_key = request
        .evidence
        .review_audit_key
        .as_deref()
        .unwrap_or("")
        .trim();
    if review_audit_key.is_empty() {
        violations.push(violation(
            "completion_review_required_requires_audit",
            "review-required terminal success requires a durable review audit key",
        ));
    }

    let verdict = request
        .metadata
        .get("completion_review_verdict")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if verdict != "pass" {
        violations.push(violation(
            "completion_review_required_requires_pass",
            "review-required terminal success requires completion_review_verdict=pass",
        ));
    }
    if verdict == "unavailable" {
        violations.push(violation(
            "completion_review_unavailable_cannot_complete",
            "an unavailable reviewer verdict is not a completion proof",
        ));
    }

    let validation_not_required = request
        .metadata
        .get("validation_not_required_policy_proof")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_validation_proof = request
        .evidence
        .verification_id
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !has_validation_proof && !validation_not_required {
        violations.push(violation(
            "completion_review_required_requires_validation",
            "review-required terminal success requires validator evidence or an explicit validation-not-required policy proof",
        ));
    }
}

fn validate_review_checkpoint(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if !metadata_bool(request, "review_checkpoint") {
        return;
    }

    if request
        .evidence
        .review_audit_key
        .as_deref()
        .unwrap_or("")
        .is_empty()
    {
        violations.push(violation(
            "review_checkpoint_requires_audit",
            "review checkpoints require a durable review audit key",
        ));
    }

    if request.to_state != CoreState::ReworkRequired {
        return;
    }

    let valid_feedback_transition = match request.entity_type {
        CoreEntityType::Ticket | CoreEntityType::WorkItem => {
            request.from_state == CoreState::AwaitingReview
                && request.event == CoreEvent::RequireRework
        }
        CoreEntityType::QueueItem => {
            matches!(
                request.from_state,
                CoreState::Leased | CoreState::Running | CoreState::Blocked | CoreState::Failed
            ) && request.event == CoreEvent::RequireRework
        }
        _ => false,
    };
    if !valid_feedback_transition {
        violations.push(violation(
            "review_checkpoint_invalid_feedback_transition",
            "review checkpoint feedback must move the reviewed main work item to ReworkRequired through the review gate",
        ));
    }

    if metadata_bool(request, "spawns_review_owned_work")
        || request
            .metadata
            .get("spawned_work_kind")
            .map(|kind| kind.starts_with("review-"))
            .unwrap_or(false)
    {
        violations.push(violation(
            "review_checkpoint_cannot_spawn_rework",
            "review checkpoints may feed findings back to the main work item but may not spawn review-owned rework",
        ));
    }

    let feedback_target = request
        .metadata
        .get("feedback_target_entity_id")
        .map(|value| value.trim())
        .unwrap_or("");
    if feedback_target != request.entity_id {
        violations.push(violation(
            "review_checkpoint_feedback_target_mismatch",
            "review checkpoint feedback must target the same entity that was reviewed",
        ));
    }

    let feedback_owner = request
        .metadata
        .get("feedback_owner")
        .map(|value| value.trim())
        .unwrap_or("");
    if feedback_owner != "main_agent" {
        violations.push(violation(
            "review_checkpoint_requires_main_agent_feedback",
            "review checkpoint feedback must resume the main agent instead of becoming review-owned work",
        ));
    }
}

fn validate_rework_required_gate(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request.to_state != CoreState::ReworkRequired {
        return;
    }
    if !matches!(
        request.entity_type,
        CoreEntityType::QueueItem | CoreEntityType::WorkItem | CoreEntityType::Ticket
    ) {
        return;
    }
    if request.lane == RuntimeLane::P3Housekeeping {
        return;
    }
    let review_checkpoint = metadata_bool(request, "review_checkpoint");
    let validator_rework = metadata_bool(request, "validator_rework");
    let has_review = request
        .evidence
        .review_audit_key
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_validation = request
        .evidence
        .verification_id
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if review_checkpoint && has_review {
        return;
    }
    if validator_rework && (has_review || has_validation) {
        return;
    }
    violations.push(violation(
        "rework_required_requires_review_or_validator_witness",
        "ReworkRequired requires a durable review rejection/checkpoint or validator rework witness",
    ));
}

fn validate_ticket_closure(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if matches!(
        request.entity_type,
        CoreEntityType::Ticket | CoreEntityType::WorkItem
    ) && request.to_state == CoreState::Closed
        && request
            .evidence
            .verification_id
            .as_deref()
            .unwrap_or("")
            .is_empty()
    {
        violations.push(violation(
            "closure_requires_verification",
            "ticket/self-work closure requires durable verification evidence",
        ));
    }
}

fn validate_commitment_backing(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request.entity_type != CoreEntityType::Commitment {
        return;
    }

    if matches!(
        request.to_state,
        CoreState::Committed | CoreState::BackingScheduled | CoreState::DueSoon
    ) && request
        .evidence
        .schedule_task_id
        .as_deref()
        .unwrap_or("")
        .is_empty()
    {
        violations.push(violation(
            "commitment_requires_schedule",
            "deadline commitments require a backing schedule task before they become active",
        ));
    }
}

fn validate_schedule_backing(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request.entity_type != CoreEntityType::Schedule {
        return;
    }

    let backs_commitment = request
        .metadata
        .get("backs_commitment")
        .map(|value| value == "true")
        .unwrap_or(false);

    if backs_commitment
        && matches!(
            request.to_state,
            CoreState::Paused | CoreState::DisabledByPolicy
        )
    {
        let has_replacement = !request
            .evidence
            .replacement_schedule_task_id
            .as_deref()
            .unwrap_or("")
            .is_empty();
        let has_escalation = !request
            .evidence
            .escalation_id
            .as_deref()
            .unwrap_or("")
            .is_empty();

        if !has_replacement && !has_escalation {
            violations.push(violation(
                "commitment_schedule_disable_requires_replacement",
                "a schedule backing a commitment cannot be paused or disabled without replacement or escalation",
            ));
        }
    }
}

fn validate_repair(request: &CoreTransitionRequest, violations: &mut Vec<CoreTransitionViolation>) {
    if request.entity_type != CoreEntityType::Repair {
        return;
    }

    if matches!(
        request.to_state,
        CoreState::RepairPlanReviewed
            | CoreState::ApplyingDeterministicActions
            | CoreState::RepairVerification
            | CoreState::Restored
    ) && request.evidence.canonical_hot_path.is_empty()
    {
        violations.push(violation(
            "repair_requires_canonical_hot_path",
            "repair transitions must name the protected hot path being repaired",
        ));
    }
}

fn validate_knowledge_capture(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request.entity_type != CoreEntityType::Knowledge {
        return;
    }

    if matches!(request.to_state, CoreState::Active | CoreState::Closed)
        && request
            .evidence
            .incident_id
            .as_deref()
            .unwrap_or("")
            .is_empty()
    {
        violations.push(violation(
            "knowledge_requires_incident",
            "failure-shield knowledge must be linked to the incident it prevents from recurring",
        ));
    }
}

fn require_reviewed_outbound(
    request: &CoreTransitionRequest,
    violations: &mut Vec<CoreTransitionViolation>,
) {
    if request
        .evidence
        .review_audit_key
        .as_deref()
        .unwrap_or("")
        .is_empty()
    {
        violations.push(violation(
            "founder_send_requires_review_audit",
            "founder/owner/admin outbound send requires a durable review audit key",
        ));
    }

    if request
        .evidence
        .approved_body_sha256
        .as_deref()
        .unwrap_or("")
        != request
            .evidence
            .outgoing_body_sha256
            .as_deref()
            .unwrap_or("")
    {
        violations.push(violation(
            "founder_send_body_hash_mismatch",
            "reviewed body hash must match the outgoing body hash",
        ));
    }

    if request
        .evidence
        .approved_recipient_set_sha256
        .as_deref()
        .unwrap_or("")
        != request
            .evidence
            .outgoing_recipient_set_sha256
            .as_deref()
            .unwrap_or("")
    {
        violations.push(violation(
            "founder_send_recipient_hash_mismatch",
            "reviewed recipient set must match the outgoing recipient set",
        ));
    }
}

fn is_founder_protected(request: &CoreTransitionRequest) -> bool {
    request.entity_type == CoreEntityType::FounderCommunication
        || request.lane == RuntimeLane::P0FounderCommunication
        || request
            .metadata
            .get("protected_party")
            .map(|value| matches!(value.as_str(), "founder" | "owner" | "admin"))
            .unwrap_or(false)
}

fn metadata_bool(request: &CoreTransitionRequest, key: &str) -> bool {
    request
        .metadata
        .get(key)
        .map(|value| value == "true")
        .unwrap_or(false)
}

fn is_allowed_transition(
    entity_type: CoreEntityType,
    from_state: CoreState,
    to_state: CoreState,
) -> bool {
    allowed_transition_catalog(entity_type).contains(&(from_state, to_state))
}

pub fn core_entity_types() -> &'static [CoreEntityType] {
    use CoreEntityType::*;
    &[
        Service,
        Mission,
        Context,
        QueueItem,
        WorkItem,
        Ticket,
        Review,
        FounderCommunication,
        Commitment,
        Schedule,
        Knowledge,
        Repair,
    ]
}

pub fn allowed_transition_catalog(
    entity_type: CoreEntityType,
) -> &'static [(CoreState, CoreState)] {
    use CoreEntityType::*;
    use CoreState::*;

    match entity_type {
        Service => &[
            (Booting, Ready),
            (Booting, Degraded),
            (Ready, Processing),
            (Processing, Ready),
            (Processing, Degraded),
            (Degraded, Repairing),
            (Repairing, Ready),
            (Repairing, Degraded),
            (Booting, Stopped),
            (Ready, Stopped),
            (Processing, Stopped),
            (Degraded, Stopped),
            (Repairing, Stopped),
        ],
        Mission => &[
            (Empty, Ingesting),
            (Ingesting, Rebuilding),
            (Rebuilding, MissionReady),
            (MissionReady, MissionRunning),
            (MissionRunning, WaitingOnExternal),
            (MissionRunning, MissionBlocked),
            (WaitingOnExternal, MissionRunning),
            (MissionBlocked, Repairing),
            (Repairing, MissionRunning),
            (MissionRunning, MissionClosed),
        ],
        Context => &[
            (Cold, Hydrating),
            (Hydrating, Fresh),
            (Fresh, CompactionDue),
            (CompactionDue, Compacted),
            (Compacted, Fresh),
            (Fresh, Stale),
            (Stale, Hydrating),
        ],
        QueueItem => &[
            (Pending, Completed),
            (Pending, Blocked),
            (Pending, Failed),
            (Pending, Leased),
            (Pending, ReworkRequired),
            (Leased, Pending),
            (Leased, Running),
            (Leased, Completed),
            (Leased, Blocked),
            (Leased, Failed),
            (Leased, ReworkRequired),
            (Running, Completed),
            (Running, Blocked),
            (Running, Failed),
            (Running, ReworkRequired),
            (Running, Superseded),
            (Blocked, Pending),
            (Blocked, Completed),
            (Blocked, Failed),
            (Blocked, ReworkRequired),
            (Blocked, Superseded),
            (Failed, Pending),
            (Failed, Blocked),
            (Failed, Completed),
            (Failed, ReworkRequired),
            (Failed, Superseded),
            (ReworkRequired, Pending),
            (ReworkRequired, Failed),
            (ReworkRequired, Superseded),
            (Pending, Superseded),
            (Leased, Superseded),
        ],
        WorkItem => &[
            (Created, Classified),
            (Created, Planned),
            (Created, Superseded),
            (Classified, TicketBacked),
            (Classified, Superseded),
            (TicketBacked, Planned),
            (TicketBacked, Superseded),
            (Planned, Closed),
            (Planned, Executing),
            (Planned, Blocked),
            (Planned, Failed),
            (Planned, Superseded),
            (Executing, Closed),
            (Executing, AwaitingReview),
            (Executing, Failed),
            (AwaitingReview, ReworkRequired),
            (AwaitingReview, AwaitingVerification),
            (AwaitingReview, Closed),
            (AwaitingReview, Failed),
            (AwaitingReview, Superseded),
            (ReworkRequired, Closed),
            (ReworkRequired, Executing),
            (ReworkRequired, Failed),
            (ReworkRequired, Superseded),
            (AwaitingVerification, Verified),
            (Verified, Closed),
            (Executing, Blocked),
            (Executing, Superseded),
            (Blocked, Closed),
            (Blocked, Failed),
            (Blocked, Planned),
            (Blocked, Superseded),
        ],
        Ticket => &[
            (Created, Classified),
            (Created, AwaitingReview),
            (Created, Blocked),
            (Created, Planned),
            (Created, Superseded),
            (Classified, Planned),
            (Classified, Superseded),
            (Planned, Blocked),
            (Planned, Executing),
            (Planned, Superseded),
            (Executing, Verified),
            (Executing, AwaitingReview),
            (AwaitingReview, ReworkRequired),
            (AwaitingReview, AwaitingVerification),
            (AwaitingReview, Planned),
            (AwaitingReview, Blocked),
            (AwaitingReview, Superseded),
            (ReworkRequired, Executing),
            (ReworkRequired, Superseded),
            (AwaitingVerification, Verified),
            (Verified, Closed),
            (Executing, Blocked),
            (Executing, Superseded),
            (Blocked, Planned),
            (Blocked, Superseded),
        ],
        Review => &[
            (Drafting, DraftReady),
            (DraftReady, Reviewing),
            (Reviewing, Approved),
            (Reviewing, Rejected),
            (Reviewing, SentBackForRework),
            (SentBackForRework, Drafting),
        ],
        FounderCommunication => &[
            (InboundObserved, InboundObserved),
            (InboundObserved, ContextBuilt),
            (ContextBuilt, ReplyNeeded),
            (ContextBuilt, NoResponseNeeded),
            (ReplyNeeded, Drafting),
            (Drafting, DraftReady),
            (DraftReady, Reviewing),
            (Reviewing, Approved),
            (Reviewing, ReworkRequired),
            (ReworkRequired, ContextBuilt),
            (Approved, Sending),
            (Sending, Sent),
            (Sending, SendFailed),
            (SendFailed, DeliveryRepair),
            (DeliveryRepair, Sending),
            (Sent, AwaitingAcknowledgement),
            (AwaitingAcknowledgement, Done),
            (NoResponseNeeded, Done),
            (ReplyNeeded, Escalated),
        ],
        Commitment => &[
            (Proposed, Reviewed),
            (Reviewed, Committed),
            (Committed, BackingScheduled),
            (BackingScheduled, DueSoon),
            (DueSoon, InProgress),
            (InProgress, Delivered),
            (DueSoon, AtRisk),
            (InProgress, AtRisk),
            (AtRisk, InProgress),
            (AtRisk, Escalated),
            (Committed, CancelledWithNotice),
            (BackingScheduled, CancelledWithNotice),
        ],
        Schedule => &[
            (Created, Enabled),
            (Enabled, Due),
            (Due, Emitted),
            (Emitted, BackingWorkQueued),
            (BackingWorkQueued, Acknowledged),
            (Enabled, Paused),
            (Paused, Enabled),
            (Enabled, Expired),
            (Paused, DisabledByPolicy),
            (Enabled, DisabledByPolicy),
        ],
        Knowledge => &[
            (IncidentObserved, LessonDrafted),
            (LessonDrafted, AwaitingReview),
            (AwaitingReview, EvidenceAttached),
            (EvidenceAttached, Active),
            (Active, Superseded),
        ],
        Repair => &[
            (Healthy, PressureDetected),
            (PressureDetected, RepairPlanning),
            (RepairPlanning, RepairPlanReviewed),
            (RepairPlanReviewed, ApplyingDeterministicActions),
            (ApplyingDeterministicActions, RepairVerification),
            (RepairVerification, Restored),
            (RepairVerification, StillDegraded),
            (StillDegraded, RepairPlanning),
        ],
    }
}

pub fn core_start_state(entity_type: CoreEntityType) -> CoreState {
    use CoreEntityType::*;
    use CoreState::*;

    match entity_type {
        Service => Booting,
        Mission => Empty,
        Context => Cold,
        QueueItem => Pending,
        WorkItem | Ticket => Created,
        Review => Drafting,
        FounderCommunication => InboundObserved,
        Commitment => Proposed,
        Schedule => Created,
        Knowledge => IncidentObserved,
        Repair => Healthy,
    }
}

pub fn core_terminal_states(entity_type: CoreEntityType) -> &'static [CoreState] {
    use CoreEntityType::*;
    use CoreState::*;

    match entity_type {
        Service => &[Ready, Stopped],
        Mission => &[MissionReady, MissionClosed],
        Context => &[Fresh],
        QueueItem => &[Completed, Superseded],
        WorkItem => &[Closed, Failed, Superseded],
        Ticket => &[Closed, Superseded],
        Review => &[Approved, Rejected],
        FounderCommunication => &[Done, Escalated],
        Commitment => &[Delivered, Escalated, CancelledWithNotice],
        Schedule => &[Acknowledged, Expired, DisabledByPolicy],
        Knowledge => &[Active, Superseded],
        Repair => &[Restored],
    }
}

pub fn analyze_core_liveness() -> CoreLivenessReport {
    use std::collections::BTreeSet;

    let mut entities = Vec::new();
    for entity_type in core_entity_types() {
        let transitions = allowed_transition_catalog(*entity_type);
        let start_state = core_start_state(*entity_type);
        let terminal_states = core_terminal_states(*entity_type).to_vec();
        let terminal_set = terminal_states.iter().copied().collect::<BTreeSet<_>>();
        let mut states = BTreeSet::new();
        let mut outgoing: BTreeMap<CoreState, BTreeSet<CoreState>> = BTreeMap::new();
        let mut incoming: BTreeMap<CoreState, BTreeSet<CoreState>> = BTreeMap::new();

        states.insert(start_state);
        for terminal in &terminal_states {
            states.insert(*terminal);
        }
        for (from, to) in transitions {
            states.insert(*from);
            states.insert(*to);
            outgoing.entry(*from).or_default().insert(*to);
            incoming.entry(*to).or_default().insert(*from);
        }

        let reachable = graph_reachable(start_state, &outgoing);
        let unreachable_states = states
            .iter()
            .filter(|state| !reachable.contains(state))
            .copied()
            .collect::<Vec<_>>();
        let nonterminal_dead_end_states = states
            .iter()
            .filter(|state| !terminal_set.contains(state))
            .filter(|state| {
                outgoing
                    .get(state)
                    .map(|next| next.is_empty())
                    .unwrap_or(true)
            })
            .copied()
            .collect::<Vec<_>>();

        let mut reverse_reachable = BTreeSet::new();
        let mut stack = terminal_states.clone();
        while let Some(state) = stack.pop() {
            if !reverse_reachable.insert(state) {
                continue;
            }
            if let Some(prev_states) = incoming.get(&state) {
                stack.extend(prev_states.iter().copied());
            }
        }
        let states_without_terminal_path = states
            .iter()
            .filter(|state| !reverse_reachable.contains(state))
            .copied()
            .collect::<Vec<_>>();

        entities.push(CoreLivenessEntityReport {
            entity_type: *entity_type,
            start_state,
            terminal_states,
            transition_count: transitions.len(),
            state_count: states.len(),
            unreachable_states,
            nonterminal_dead_end_states,
            states_without_terminal_path,
        });
    }

    let ok = entities.iter().all(|entity| {
        entity.unreachable_states.is_empty()
            && entity.nonterminal_dead_end_states.is_empty()
            && entity.states_without_terminal_path.is_empty()
    });
    CoreLivenessReport { ok, entities }
}

pub fn review_harness_transition_catalog() -> &'static [ReviewHarnessEdge] {
    use ReviewHarnessBudget::*;
    use ReviewHarnessProof::*;
    use ReviewHarnessState::*;

    &[
        ReviewHarnessEdge {
            from: Queued,
            to: Leased,
            proof: NoProof,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Leased,
            to: Running,
            proof: NoProof,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Running,
            to: AwaitingReview,
            proof: WorkerFinished,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Running,
            to: ModelFailed,
            proof: WorkerFailed,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Running,
            to: InfraFailed,
            proof: InfraError,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: AwaitingReview,
            to: ReviewQueued,
            proof: StartReview,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewQueued,
            to: Reviewing,
            proof: SpawnReviewer,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Reviewing,
            to: ReviewPassed,
            proof: ReviewPass,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Reviewing,
            to: ReviewRejected,
            proof: ReviewReject,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Reviewing,
            to: ReviewHarnessState::ReviewUnavailable,
            proof: ReviewHarnessProof::ReviewUnavailable,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewPassed,
            to: AwaitingValidation,
            proof: ReviewPass,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewRejected,
            to: ReworkRequired,
            proof: ReviewReject,
            consumes_budget: ReviewRounds,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewHarnessState::ReviewUnavailable,
            to: ReviewRetry,
            proof: ReviewHarnessProof::ReviewUnavailable,
            consumes_budget: ReviewerUnavailableRetries,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewHarnessState::ReviewUnavailable,
            to: InfraFailed,
            proof: ReviewRetriesExhausted,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReviewRetry,
            to: AwaitingReview,
            proof: RetryReview,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReworkRequired,
            to: Queued,
            proof: RequeueSameMainWork,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: ReworkRequired,
            to: ModelFailed,
            proof: ReviewRoundsExhausted,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: AwaitingValidation,
            to: Validating,
            proof: RunValidator,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Validating,
            to: Passed,
            proof: ValidatorPass,
            consumes_budget: NoBudget,
            terminal_success: true,
        },
        ReviewHarnessEdge {
            from: Validating,
            to: ReworkRequired,
            proof: ValidatorFail,
            consumes_budget: ValidatorRework,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Validating,
            to: ModelFailed,
            proof: ValidatorReworkExhausted,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
        ReviewHarnessEdge {
            from: Validating,
            to: InfraFailed,
            proof: ValidatorInfraError,
            consumes_budget: NoBudget,
            terminal_success: false,
        },
    ]
}

pub fn analyze_review_harness_model() -> ReviewHarnessModelReport {
    use ReviewHarnessProof::*;
    use ReviewHarnessState::*;

    let edges = review_harness_transition_catalog();
    let terminals = [Passed, ModelFailed, InfraFailed];
    let mut violations = Vec::new();
    let mut states = std::collections::BTreeSet::new();
    let mut outgoing: BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>> = BTreeMap::new();
    let mut incoming: BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>> = BTreeMap::new();

    states.insert(Queued);
    states.extend(terminals);
    for edge in edges {
        states.insert(edge.from);
        states.insert(edge.to);
        outgoing.entry(edge.from).or_default().push(edge);
        incoming.entry(edge.to).or_default().push(edge);

        if edge.terminal_success && edge.to != Passed {
            violations.push(format!(
                "terminal_success_edge_must_end_in_passed:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.to == Passed && edge.proof != ValidatorPass {
            violations.push(format!(
                "passed_requires_validator_pass:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.terminal_success && edge.proof != ValidatorPass {
            violations.push(format!(
                "terminal_success_requires_validator_pass:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.to == Passed && path_exists_without_review_pass(Queued, edge.from, &outgoing) {
            violations.push(format!(
                "passed_path_missing_review_pass:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.proof == ReviewHarnessProof::ReviewUnavailable
            && matches!(edge.to, AwaitingValidation | Validating | Passed)
        {
            violations.push(format!(
                "review_unavailable_cannot_advance_success:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.from == ReviewPassed && edge.to != AwaitingValidation {
            violations.push(format!(
                "review_passed_can_only_enter_validation_gate:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.from == ReviewRejected && edge.to != ReworkRequired {
            violations.push(format!(
                "review_rejected_can_only_enter_rework_required:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.from == ReviewHarnessState::ReviewUnavailable
            && !matches!(edge.to, ReviewRetry | InfraFailed)
        {
            violations.push(format!(
                "review_unavailable_can_only_retry_or_fail_infra:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.from == ReworkRequired && !matches!(edge.to, Queued | ModelFailed) {
            violations.push(format!(
                "rework_required_can_only_requeue_main_or_model_fail:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
        if edge.to == Queued && edge.from == ReworkRequired && edge.proof != RequeueSameMainWork {
            violations.push(format!(
                "rework_requeue_requires_same_main_work_proof:{:?}->{:?}",
                edge.from, edge.to
            ));
        }
    }

    let reachable = review_harness_reachable(Queued, &outgoing);
    for state in &states {
        if !reachable.contains(state) {
            violations.push(format!("unreachable_state:{state:?}"));
        }
        if !terminals.contains(state) && outgoing.get(state).map(Vec::is_empty).unwrap_or(true) {
            violations.push(format!("nonterminal_dead_end:{state:?}"));
        }
    }

    let terminal_reachable = reverse_review_harness_reachable(&terminals, &incoming);
    for state in &states {
        if !terminal_reachable.contains(state) {
            violations.push(format!("state_without_terminal_path:{state:?}"));
        }
    }

    for cycle in review_harness_simple_cycles(&states, &outgoing) {
        if !cycle
            .iter()
            .any(|edge| edge.consumes_budget != ReviewHarnessBudget::NoBudget)
        {
            violations.push(format!("cycle_without_budget_decrease:{cycle:?}"));
        }
    }

    let ok = violations.is_empty();
    ReviewHarnessModelReport {
        ok,
        violations,
        proof: "The review harness is a finite graph with terminal states Passed, ModelFailed, and InfraFailed. The only terminal-success state is Passed. Every path to Passed must cross Reviewing->ReviewPassed with ReviewPass, then ReviewPassed->AwaitingValidation, then Validating->Passed with ValidatorPass. ReviewUnavailable has no edge to AwaitingValidation, Validating, or Passed. ReviewRejected can only enter ReworkRequired, and ReworkRequired can only requeue the same main work or end as ModelFailed. Every cycle contains an edge that consumes a finite budget, so the ranking function review_rounds_left + reviewer_unavailable_retries_left + validator_rework_left strictly decreases on cyclic progress and must eventually force Passed, ModelFailed, or InfraFailed.".to_string(),
    }
}

fn review_harness_reachable(
    start: ReviewHarnessState,
    outgoing: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
) -> std::collections::BTreeSet<ReviewHarnessState> {
    let mut reachable = std::collections::BTreeSet::new();
    let mut stack = vec![start];
    while let Some(state) = stack.pop() {
        if !reachable.insert(state) {
            continue;
        }
        if let Some(edges) = outgoing.get(&state) {
            stack.extend(edges.iter().map(|edge| edge.to));
        }
    }
    reachable
}

fn reverse_review_harness_reachable(
    starts: &[ReviewHarnessState],
    incoming: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
) -> std::collections::BTreeSet<ReviewHarnessState> {
    let mut reachable = std::collections::BTreeSet::new();
    let mut stack = starts.to_vec();
    while let Some(state) = stack.pop() {
        if !reachable.insert(state) {
            continue;
        }
        if let Some(edges) = incoming.get(&state) {
            stack.extend(edges.iter().map(|edge| edge.from));
        }
    }
    reachable
}

fn review_harness_simple_cycles(
    states: &std::collections::BTreeSet<ReviewHarnessState>,
    outgoing: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
) -> Vec<Vec<ReviewHarnessEdge>> {
    let mut cycles = Vec::new();
    for start in states {
        let mut path = Vec::new();
        review_harness_cycles_from(*start, *start, outgoing, &mut path, &mut cycles);
    }
    cycles
}

fn review_harness_cycles_from(
    start: ReviewHarnessState,
    current: ReviewHarnessState,
    outgoing: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
    path: &mut Vec<ReviewHarnessEdge>,
    cycles: &mut Vec<Vec<ReviewHarnessEdge>>,
) {
    if path.len() > review_harness_transition_catalog().len() {
        return;
    }
    let Some(edges) = outgoing.get(&current) else {
        return;
    };
    for edge in edges {
        if edge.to == start {
            let mut cycle = path.clone();
            cycle.push(**edge);
            cycles.push(cycle);
            continue;
        }
        if path.iter().any(|path_edge| path_edge.from == edge.to) {
            continue;
        }
        path.push(**edge);
        review_harness_cycles_from(start, edge.to, outgoing, path, cycles);
        path.pop();
    }
}

fn path_exists_without_review_pass(
    start: ReviewHarnessState,
    target: ReviewHarnessState,
    outgoing: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
) -> bool {
    path_exists_without_review_pass_from(start, target, outgoing, false, &mut Vec::new())
}

fn path_exists_without_review_pass_from(
    start: ReviewHarnessState,
    target: ReviewHarnessState,
    outgoing: &BTreeMap<ReviewHarnessState, Vec<&ReviewHarnessEdge>>,
    has_review_pass: bool,
    path: &mut Vec<ReviewHarnessState>,
) -> bool {
    if start == target {
        return !has_review_pass;
    }
    if path.contains(&start) {
        return false;
    }
    path.push(start);
    let Some(edges) = outgoing.get(&start) else {
        path.pop();
        return false;
    };
    for edge in edges {
        let next_has_review_pass = has_review_pass || edge.proof == ReviewHarnessProof::ReviewPass;
        if path_exists_without_review_pass_from(
            edge.to,
            target,
            outgoing,
            next_has_review_pass,
            path,
        ) {
            path.pop();
            return true;
        }
    }
    path.pop();
    false
}

fn graph_reachable(
    start: CoreState,
    outgoing: &BTreeMap<CoreState, std::collections::BTreeSet<CoreState>>,
) -> std::collections::BTreeSet<CoreState> {
    let mut reachable = std::collections::BTreeSet::new();
    let mut stack = vec![start];
    while let Some(state) = stack.pop() {
        if !reachable.insert(state) {
            continue;
        }
        if let Some(next_states) = outgoing.get(&state) {
            stack.extend(next_states.iter().copied());
        }
    }
    reachable
}

fn violation(code: &'static str, message: impl Into<String>) -> CoreTransitionViolation {
    CoreTransitionViolation {
        code: code.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn founder_send_request() -> CoreTransitionRequest {
        CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: "email/thread-founder".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Approved,
            to_state: CoreState::Sending,
            event: CoreEvent::Send,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn blocks_founder_send_without_review_audit() {
        let report = validate_transition(&founder_send_request());

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "founder_send_requires_review_audit"));
    }

    #[test]
    fn blocks_founder_send_body_hash_mismatch() {
        let mut request = founder_send_request();
        request.evidence.review_audit_key = Some("review-1".to_string());
        request.evidence.approved_body_sha256 = Some("approved".to_string());
        request.evidence.outgoing_body_sha256 = Some("changed".to_string());
        request.evidence.approved_recipient_set_sha256 = Some("recipients".to_string());
        request.evidence.outgoing_recipient_set_sha256 = Some("recipients".to_string());

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "founder_send_body_hash_mismatch"));
    }

    #[test]
    fn allows_reviewed_founder_send_when_hashes_match() {
        let mut request = founder_send_request();
        request.evidence.review_audit_key = Some("review-1".to_string());
        request.evidence.approved_body_sha256 = Some("body".to_string());
        request.evidence.outgoing_body_sha256 = Some("body".to_string());
        request.evidence.approved_recipient_set_sha256 = Some("recipients".to_string());
        request.evidence.outgoing_recipient_set_sha256 = Some("recipients".to_string());

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    fn review_checkpoint_feedback_request() -> CoreTransitionRequest {
        let mut metadata = BTreeMap::new();
        metadata.insert("review_checkpoint".to_string(), "true".to_string());
        metadata.insert("feedback_owner".to_string(), "main_agent".to_string());
        metadata.insert(
            "feedback_target_entity_id".to_string(),
            "self-work:1".to_string(),
        );
        metadata.insert("spawns_review_owned_work".to_string(), "false".to_string());

        CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: "self-work:1".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::AwaitingReview,
            to_state: CoreState::ReworkRequired,
            event: CoreEvent::RequireRework,
            actor: "ctox-completion-review".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-checkpoint-1".to_string()),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        }
    }

    #[test]
    fn work_item_terminal_transition_rejects_missing_outcome_witness() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: "self-work:local:send-mail".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Verified,
            to_state: CoreState::Closed,
            event: CoreEvent::Close,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                verification_id: Some("verification-1".to_string()),
                expected_artifact_refs: vec![ArtifactRef {
                    kind: ArtifactKind::OutboundEmail,
                    primary_key: "thread:founder-mail".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::new(),
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "WP-Outcome-Missing"));
    }

    #[test]
    fn work_item_without_expected_artifact_can_close_with_review_and_policy_verification() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::WorkItem,
            entity_id: "self-work:local:research".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Verified,
            to_state: CoreState::Closed,
            event: CoreEvent::Close,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-audit-pass-1".to_string()),
                verification_id: Some("validation-not-required:research".to_string()),
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::from([
                ("completion_review_required".to_string(), "true".to_string()),
                ("completion_review_verdict".to_string(), "pass".to_string()),
                (
                    "reviewed_work_terminal_success".to_string(),
                    "true".to_string(),
                ),
                (
                    "validation_not_required_policy_proof".to_string(),
                    "validation-not-required:research".to_string(),
                ),
            ]),
        };

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn queue_item_terminal_transition_accepts_delivered_outcome_witness() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue:mail".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Running,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-service".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-audit-pass-1".to_string()),
                verification_id: Some("outcome-witness:queue-mail".to_string()),
                expected_artifact_refs: vec![ArtifactRef {
                    kind: ArtifactKind::OutboundEmail,
                    primary_key: "thread:founder-mail".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                delivered_artifact_refs: vec![ArtifactRef {
                    kind: ArtifactKind::OutboundEmail,
                    primary_key: "email:cto@example.test::pending_send::abc".to_string(),
                    expected_terminal_state: "accepted".to_string(),
                }],
                ..CoreEvidenceRefs::default()
            },
            metadata: BTreeMap::from([
                ("completion_review_required".to_string(), "true".to_string()),
                ("completion_review_verdict".to_string(), "pass".to_string()),
                (
                    "reviewed_work_terminal_success".to_string(),
                    "true".to_string(),
                ),
                ("outcome_witness".to_string(), "true".to_string()),
            ]),
        };

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn allows_review_checkpoint_feedback_to_same_main_work_item() {
        let report = validate_transition(&review_checkpoint_feedback_request());

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn blocks_review_checkpoint_without_audit_key() {
        let mut request = review_checkpoint_feedback_request();
        request.evidence.review_audit_key = None;

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "review_checkpoint_requires_audit"));
    }

    #[test]
    fn blocks_review_checkpoint_that_targets_review_owned_rework() {
        let mut request = review_checkpoint_feedback_request();
        request
            .metadata
            .insert("spawns_review_owned_work".to_string(), "true".to_string());
        request
            .metadata
            .insert("spawned_work_kind".to_string(), "review-rework".to_string());
        request
            .metadata
            .insert("feedback_owner".to_string(), "review_agent".to_string());

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "review_checkpoint_cannot_spawn_rework"));
        assert!(report.violations.iter().any(|violation| {
            violation.code == "review_checkpoint_requires_main_agent_feedback"
        }));
    }

    #[test]
    fn blocks_review_checkpoint_feedback_to_different_work_item() {
        let mut request = review_checkpoint_feedback_request();
        request.metadata.insert(
            "feedback_target_entity_id".to_string(),
            "self-work:other".to_string(),
        );

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| { violation.code == "review_checkpoint_feedback_target_mismatch" }));
    }

    #[test]
    fn blocks_founder_lane_spill() {
        let mut request = founder_send_request();
        request.from_state = CoreState::Pending;
        request.to_state = CoreState::Superseded;
        request.entity_type = CoreEntityType::QueueItem;
        request
            .metadata
            .insert("protected_party".to_string(), "founder".to_string());

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "founder_work_cannot_spill"));
    }

    #[test]
    fn queue_ack_from_lease_to_completed_requires_review_or_policy() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-1".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report.violations.iter().any(|violation| {
            violation.code == "work_terminal_success_requires_review_or_policy"
        }));
    }

    #[test]
    fn queue_ack_from_lease_to_completed_accepts_terminal_policy_proof() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-1".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::from([(
                "terminal_policy_proof".to_string(),
                "policy:system-non-work".to_string(),
            )]),
        };

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn core_liveness_graph_has_no_unreachable_or_dead_end_states() {
        let report = analyze_core_liveness();

        assert!(report.ok, "{report:#?}");
    }

    #[test]
    fn review_harness_model_is_safe_and_live() {
        let report = analyze_review_harness_model();

        assert!(report.ok, "{report:#?}");
    }

    #[test]
    fn review_harness_transition_catalog_matches_required_flow() {
        use ReviewHarnessBudget::*;
        use ReviewHarnessProof::*;
        use ReviewHarnessState::*;

        let actual: std::collections::BTreeSet<_> = review_harness_transition_catalog()
            .iter()
            .map(|edge| {
                (
                    edge.from,
                    edge.to,
                    edge.proof,
                    edge.consumes_budget,
                    edge.terminal_success,
                )
            })
            .collect();
        let expected = std::collections::BTreeSet::from([
            (Queued, Leased, NoProof, NoBudget, false),
            (Leased, Running, NoProof, NoBudget, false),
            (Running, AwaitingReview, WorkerFinished, NoBudget, false),
            (Running, ModelFailed, WorkerFailed, NoBudget, false),
            (Running, InfraFailed, InfraError, NoBudget, false),
            (AwaitingReview, ReviewQueued, StartReview, NoBudget, false),
            (ReviewQueued, Reviewing, SpawnReviewer, NoBudget, false),
            (Reviewing, ReviewPassed, ReviewPass, NoBudget, false),
            (Reviewing, ReviewRejected, ReviewReject, NoBudget, false),
            (
                Reviewing,
                ReviewHarnessState::ReviewUnavailable,
                ReviewHarnessProof::ReviewUnavailable,
                NoBudget,
                false,
            ),
            (
                ReviewPassed,
                AwaitingValidation,
                ReviewPass,
                NoBudget,
                false,
            ),
            (
                ReviewRejected,
                ReworkRequired,
                ReviewReject,
                ReviewRounds,
                false,
            ),
            (
                ReviewHarnessState::ReviewUnavailable,
                ReviewRetry,
                ReviewHarnessProof::ReviewUnavailable,
                ReviewerUnavailableRetries,
                false,
            ),
            (
                ReviewHarnessState::ReviewUnavailable,
                InfraFailed,
                ReviewRetriesExhausted,
                NoBudget,
                false,
            ),
            (ReviewRetry, AwaitingReview, RetryReview, NoBudget, false),
            (ReworkRequired, Queued, RequeueSameMainWork, NoBudget, false),
            (
                ReworkRequired,
                ModelFailed,
                ReviewRoundsExhausted,
                NoBudget,
                false,
            ),
            (
                AwaitingValidation,
                Validating,
                RunValidator,
                NoBudget,
                false,
            ),
            (Validating, Passed, ValidatorPass, NoBudget, true),
            (
                Validating,
                ReworkRequired,
                ValidatorFail,
                ValidatorRework,
                false,
            ),
            (
                Validating,
                ModelFailed,
                ValidatorReworkExhausted,
                NoBudget,
                false,
            ),
            (
                Validating,
                InfraFailed,
                ValidatorInfraError,
                NoBudget,
                false,
            ),
        ]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn review_required_terminal_completion_rejects_missing_review_proof() {
        let mut metadata = BTreeMap::new();
        metadata.insert("completion_review_required".to_string(), "true".to_string());
        metadata.insert(
            "completion_review_verdict".to_string(),
            "unavailable".to_string(),
        );
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-review-required".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| { violation.code == "completion_review_required_requires_audit" }));
        assert!(report
            .violations
            .iter()
            .any(|violation| { violation.code == "completion_review_required_requires_pass" }));
        assert!(report.violations.iter().any(|violation| {
            violation.code == "completion_review_unavailable_cannot_complete"
        }));
    }

    #[test]
    fn review_required_terminal_completion_accepts_pass_proof() {
        let mut metadata = BTreeMap::new();
        metadata.insert("completion_review_required".to_string(), "true".to_string());
        metadata.insert("completion_review_verdict".to_string(), "pass".to_string());
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-review-required".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-proof-1".to_string()),
                verification_id: Some("validator-proof-1".to_string()),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        };

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn review_required_terminal_completion_rejects_missing_validation_proof() {
        let mut metadata = BTreeMap::new();
        metadata.insert("completion_review_required".to_string(), "true".to_string());
        metadata.insert("completion_review_verdict".to_string(), "pass".to_string());
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-review-required".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-proof-1".to_string()),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report.violations.iter().any(|violation| {
            violation.code == "completion_review_required_requires_validation"
        }));
    }

    #[test]
    fn review_required_terminal_completion_accepts_policy_no_validation_proof() {
        let mut metadata = BTreeMap::new();
        metadata.insert("completion_review_required".to_string(), "true".to_string());
        metadata.insert("completion_review_verdict".to_string(), "pass".to_string());
        metadata.insert(
            "validation_not_required_policy_proof".to_string(),
            "policy:unit-test".to_string(),
        );
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::QueueItem,
            entity_id: "queue-review-required".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Leased,
            to_state: CoreState::Completed,
            event: CoreEvent::Complete,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs {
                review_audit_key: Some("review-proof-1".to_string()),
                ..CoreEvidenceRefs::default()
            },
            metadata,
        };

        let report = validate_transition(&request);

        assert!(report.accepted, "{:?}", report.violations);
    }

    #[test]
    fn transition_catalog_edges_are_accepted_by_validator() {
        for entity_type in core_entity_types() {
            for (from_state, to_state) in allowed_transition_catalog(*entity_type) {
                let request = CoreTransitionRequest {
                    entity_type: *entity_type,
                    entity_id: "catalog-edge".to_string(),
                    lane: RuntimeLane::P3Housekeeping,
                    from_state: *from_state,
                    to_state: *to_state,
                    event: CoreEvent::Execute,
                    actor: "ctox-runtime".to_string(),
                    evidence: CoreEvidenceRefs {
                        review_audit_key: Some("review-1".to_string()),
                        approved_body_sha256: Some("body".to_string()),
                        outgoing_body_sha256: Some("body".to_string()),
                        approved_recipient_set_sha256: Some("recipients".to_string()),
                        outgoing_recipient_set_sha256: Some("recipients".to_string()),
                        verification_id: Some("verify-1".to_string()),
                        schedule_task_id: Some("schedule-1".to_string()),
                        replacement_schedule_task_id: Some("replacement-1".to_string()),
                        escalation_id: Some("escalation-1".to_string()),
                        knowledge_entry_id: Some("knowledge-1".to_string()),
                        incident_id: Some("incident-1".to_string()),
                        canonical_hot_path: vec!["test".to_string()],
                        ..CoreEvidenceRefs::default()
                    },
                    metadata: if matches!(to_state, CoreState::Completed | CoreState::Closed) {
                        BTreeMap::from([(
                            "terminal_policy_proof".to_string(),
                            "policy:catalog-edge".to_string(),
                        )])
                    } else {
                        BTreeMap::new()
                    },
                };

                let report = validate_transition(&request);

                assert!(
                    !report
                        .violations
                        .iter()
                        .any(|violation| violation.code == "invalid_transition"),
                    "{entity_type:?} {from_state:?}->{to_state:?}: {:?}",
                    report.violations
                );
            }
        }
    }

    #[test]
    fn blocks_ticket_close_without_verification() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::Ticket,
            entity_id: "ticket-1".to_string(),
            lane: RuntimeLane::P2MissionDelivery,
            from_state: CoreState::Verified,
            to_state: CoreState::Closed,
            event: CoreEvent::Close,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "closure_requires_verification"));
    }

    #[test]
    fn blocks_commitment_without_backing_schedule() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::Commitment,
            entity_id: "commitment-1".to_string(),
            lane: RuntimeLane::P0CommitmentBacking,
            from_state: CoreState::Reviewed,
            to_state: CoreState::Committed,
            event: CoreEvent::Commit,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "commitment_requires_schedule"));
    }

    #[test]
    fn blocks_disabling_commitment_schedule_without_replacement_or_escalation() {
        let mut metadata = BTreeMap::new();
        metadata.insert("backs_commitment".to_string(), "true".to_string());
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::Schedule,
            entity_id: "schedule-1".to_string(),
            lane: RuntimeLane::P0CommitmentBacking,
            from_state: CoreState::Enabled,
            to_state: CoreState::DisabledByPolicy,
            event: CoreEvent::DisableSchedule,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata,
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report.violations.iter().any(|violation| {
            violation.code == "commitment_schedule_disable_requires_replacement"
        }));
    }

    #[test]
    fn blocks_repair_without_canonical_hot_path() {
        let request = CoreTransitionRequest {
            entity_type: CoreEntityType::Repair,
            entity_id: "repair-1".to_string(),
            lane: RuntimeLane::P1QueueRepair,
            from_state: CoreState::RepairPlanning,
            to_state: CoreState::RepairPlanReviewed,
            event: CoreEvent::ReviewRepairPlan,
            actor: "ctox-runtime".to_string(),
            evidence: CoreEvidenceRefs::default(),
            metadata: BTreeMap::new(),
        };

        let report = validate_transition(&request);

        assert!(!report.accepted);
        assert!(report
            .violations
            .iter()
            .any(|violation| violation.code == "repair_requires_canonical_hot_path"));
    }
}
