// Generated from src/core/rxdb/tests/fixtures/business-command-lifecycle-v2.json.
// Capability source: src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json.
// Run: node src/core/rxdb/tools/build_business_command_lifecycle_contract.mjs

export const CTOX_COMMAND_CONTRACT_VERSION = 2;
export const CTOX_COMMAND_LIFECYCLE_CAPABILITY = "ctox-command-lifecycle-v2";
export const CTOX_COMMAND_REPLICATION_PHASES = Object.freeze([
  "local",
  "pushed",
  "native_observed"
]);
export const CTOX_COMMAND_EXECUTION_MODES = Object.freeze([
  "control",
  "queue"
]);
export const CTOX_COMMAND_EXECUTION_PHASES = Object.freeze([
  "waiting_dependencies",
  "accepted",
  "queued",
  "leased",
  "running",
  "awaiting_review",
  "validating",
  "retry_wait",
  "blocked",
  "terminal"
]);
export const CTOX_COMMAND_TERMINAL_STATUSES = Object.freeze([
  "none",
  "completed",
  "failed",
  "cancelled"
]);
export const CTOX_COMMAND_ERROR_CODES = Object.freeze([
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
  "cancelled"
]);
export const CTOX_COMMAND_AUTHORIZATION = Object.freeze({
  defaultRequirement: "capability",
  offlineIntentAllowed: false
});
export const CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS = Object.freeze([
  "command_id",
  "idempotency_key",
  "payload_hash",
  "module",
  "command_type",
  "record_id",
  "payload",
  "client_context",
  "created_at_ms"
]);
export const CTOX_COMMAND_NATIVE_OWNED_FIELDS = Object.freeze([
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
  "retryable"
]);
export const CTOX_COMMAND_ALLOWED_EXECUTION_TRANSITIONS = Object.freeze({
  waiting_dependencies: [
    "waiting_dependencies",
    "accepted",
    "blocked",
    "terminal"
  ],
  accepted: [
    "accepted",
    "waiting_dependencies",
    "queued",
    "running",
    "blocked",
    "terminal"
  ],
  queued: [
    "queued",
    "leased",
    "blocked",
    "terminal"
  ],
  leased: [
    "leased",
    "running",
    "retry_wait",
    "blocked",
    "terminal"
  ],
  running: [
    "running",
    "awaiting_review",
    "retry_wait",
    "blocked",
    "terminal"
  ],
  awaiting_review: [
    "awaiting_review",
    "validating",
    "retry_wait",
    "blocked",
    "terminal"
  ],
  validating: [
    "validating",
    "retry_wait",
    "blocked",
    "terminal"
  ],
  retry_wait: [
    "retry_wait",
    "queued",
    "leased",
    "running",
    "blocked",
    "terminal"
  ],
  blocked: [
    "blocked",
    "waiting_dependencies",
    "queued",
    "retry_wait",
    "terminal"
  ],
  terminal: [
    "terminal"
  ]
});
export const CTOX_COMMAND_RESULT_ENVELOPE = Object.freeze({
  required: [
    "command_id",
    "attempt"
  ],
  fields: [
    "command_id",
    "execution_task_id",
    "attempt",
    "user_message",
    "structured_output",
    "artifacts",
    "writebacks",
    "verification_claims",
    "retry",
    "error"
  ]
});
