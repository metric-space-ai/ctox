// Generated from src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json.
// Capability source: src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_business_command_lifecycle_contract.mjs

#![allow(dead_code)]

pub(crate) const CTOX_COMMAND_CONTRACT_VERSION: u32 = 2;
pub(crate) const CTOX_COMMAND_LIFECYCLE_CAPABILITY: &str = "ctox-command-lifecycle-v2";
pub(crate) const CTOX_COMMAND_REPLICATION_PHASES: &[&str] = &["local", "pushed", "native_observed"];
pub(crate) const CTOX_COMMAND_EXECUTION_MODES: &[&str] = &["control", "queue"];
pub(crate) const CTOX_COMMAND_EXECUTION_PHASES: &[&str] = &[
    "waiting_dependencies",
    "accepted",
    "queued",
    "leased",
    "running",
    "awaiting_review",
    "validating",
    "retry_wait",
    "blocked",
    "terminal",
];
pub(crate) const CTOX_COMMAND_TERMINAL_STATUSES: &[&str] =
    &["none", "completed", "failed", "cancelled"];
pub(crate) const CTOX_COMMAND_ERROR_CODES: &[&str] = &[
    "sync_unavailable",
    "schema_mismatch",
    "auth_required",
    "native_unavailable",
    "projection_delayed",
    "command_terminal_failure",
    "dependency_missing",
    "idempotency_conflict",
    "invalid_transition",
    "deadline_exceeded",
    "cancelled",
];
pub(crate) const CTOX_COMMAND_DEFAULT_AUTHORIZATION_REQUIREMENT: &str = "capability";
pub(crate) const CTOX_COMMAND_OFFLINE_INTENT_ALLOWED: bool = false;
pub(crate) const CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS: &[&str] = &[
    "command_id",
    "idempotency_key",
    "payload_hash",
    "module",
    "command_type",
    "record_id",
    "payload",
    "client_context",
    "created_at_ms",
];
pub(crate) const CTOX_COMMAND_NATIVE_OWNED_FIELDS: &[&str] = &[
    "execution_mode",
    "execution_task_id",
    "target_task_id",
    "target_record_id",
    "replication_phase",
    "execution_phase",
    "terminal_status",
    "projection_version",
    "attempt",
    "result",
    "result_ref",
    "error_code",
    "error_message",
    "retryable",
];
pub(crate) const CTOX_COMMAND_RESULT_REQUIRED_FIELDS: &[&str] = &["command_id", "attempt"];
pub(crate) const CTOX_COMMAND_RESULT_FIELDS: &[&str] = &[
    "command_id",
    "execution_task_id",
    "attempt",
    "user_message",
    "structured_output",
    "artifacts",
    "writebacks",
    "verification_claims",
    "retry",
    "error",
];
pub(crate) fn execution_transition_allowed(from: &str, to: &str) -> bool {
    match from {
        "waiting_dependencies" => matches!(
            to,
            "waiting_dependencies" | "accepted" | "blocked" | "terminal"
        ),
        "accepted" => matches!(
            to,
            "accepted" | "waiting_dependencies" | "queued" | "running" | "blocked" | "terminal"
        ),
        "queued" => matches!(to, "queued" | "leased" | "blocked" | "terminal"),
        "leased" => matches!(
            to,
            "leased" | "running" | "retry_wait" | "blocked" | "terminal"
        ),
        "running" => matches!(
            to,
            "running" | "awaiting_review" | "retry_wait" | "blocked" | "terminal"
        ),
        "awaiting_review" => matches!(
            to,
            "awaiting_review" | "validating" | "retry_wait" | "blocked" | "terminal"
        ),
        "validating" => matches!(to, "validating" | "retry_wait" | "blocked" | "terminal"),
        "retry_wait" => matches!(
            to,
            "retry_wait" | "queued" | "leased" | "running" | "blocked" | "terminal"
        ),
        "blocked" => matches!(
            to,
            "blocked" | "waiting_dependencies" | "queued" | "retry_wait" | "terminal"
        ),
        "terminal" => matches!(to, "terminal"),
        _ => false,
    }
}
