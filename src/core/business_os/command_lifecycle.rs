use serde_json::Value;
use std::fmt;

use super::command_lifecycle_generated::{
    execution_transition_allowed, CTOX_COMMAND_CONTRACT_VERSION, CTOX_COMMAND_ERROR_CODES,
    CTOX_COMMAND_EXECUTION_MODES, CTOX_COMMAND_EXECUTION_PHASES,
    CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS, CTOX_COMMAND_REPLICATION_PHASES,
    CTOX_COMMAND_TERMINAL_STATUSES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandLifecycleError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl fmt::Display for CommandLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandLifecycleError {}

pub(crate) fn validate_document(document: &Value) -> Result<(), CommandLifecycleError> {
    let object = document
        .as_object()
        .ok_or_else(|| invalid("command lifecycle document must be an object"))?;
    if object.get("contract_version").and_then(Value::as_u64)
        != Some(u64::from(CTOX_COMMAND_CONTRACT_VERSION))
    {
        return Err(invalid(format!(
            "contract_version must be {CTOX_COMMAND_CONTRACT_VERSION}"
        )));
    }
    require_known(
        object.get("replication_phase"),
        CTOX_COMMAND_REPLICATION_PHASES,
        "replication_phase",
    )?;
    require_known(
        object.get("execution_mode"),
        CTOX_COMMAND_EXECUTION_MODES,
        "execution_mode",
    )?;
    require_known(
        object.get("execution_phase"),
        CTOX_COMMAND_EXECUTION_PHASES,
        "execution_phase",
    )?;
    require_known(
        object.get("terminal_status"),
        CTOX_COMMAND_TERMINAL_STATUSES,
        "terminal_status",
    )?;
    if object
        .get("error_code")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
    {
        require_known(
            object.get("error_code"),
            CTOX_COMMAND_ERROR_CODES,
            "error_code",
        )?;
    }
    require_non_negative_integer(object.get("projection_version"), "projection_version")?;
    require_non_negative_integer(object.get("attempt"), "attempt")?;
    validate_terminal_pair(
        object
            .get("execution_phase")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        object
            .get("terminal_status")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

pub(crate) fn validate_transition(
    previous: &Value,
    next: &Value,
) -> Result<(), CommandLifecycleError> {
    validate_document(previous)?;
    validate_document(next)?;
    let previous = previous.as_object().expect("validated object");
    let next = next.as_object().expect("validated object");

    for field in CTOX_COMMAND_IMMUTABLE_INTENT_FIELDS {
        if previous.contains_key(*field) && previous.get(*field) != next.get(*field) {
            return Err(CommandLifecycleError {
                code: "idempotency_conflict",
                message: format!("{field} is immutable"),
            });
        }
    }
    if previous.get("execution_mode") != next.get("execution_mode") {
        return Err(invalid(
            "execution_mode is immutable after native observation",
        ));
    }
    let previous_version = u64_field(previous, "projection_version");
    let next_version = u64_field(next, "projection_version");
    if next_version < previous_version {
        return Err(invalid("projection_version cannot decrease"));
    }
    let previous_attempt = u64_field(previous, "attempt");
    let next_attempt = u64_field(next, "attempt");
    let state_changed = [
        "replication_phase",
        "execution_phase",
        "terminal_status",
        "attempt",
    ]
    .iter()
    .any(|field| previous.get(*field) != next.get(*field));
    if state_changed && next_version == previous_version {
        return Err(invalid("state changes require a newer projection_version"));
    }
    if next_attempt < previous_attempt {
        return Err(invalid("attempt cannot decrease"));
    }
    let from = string_field(previous, "execution_phase");
    let to = string_field(next, "execution_phase");
    if !execution_transition_allowed(from, to) {
        return Err(invalid(format!(
            "execution transition {from} -> {to} is not allowed"
        )));
    }
    if from == "terminal" && previous.get("terminal_status") != next.get("terminal_status") {
        return Err(invalid(
            "terminal_status cannot change after terminalization",
        ));
    }
    Ok(())
}

pub(crate) fn validate_execution_phase_transition(
    from: &str,
    to: &str,
) -> Result<(), CommandLifecycleError> {
    if execution_transition_allowed(from, to) {
        Ok(())
    } else {
        Err(invalid(format!(
            "execution transition {from} -> {to} is not allowed"
        )))
    }
}

fn validate_terminal_pair(
    execution_phase: &str,
    terminal_status: &str,
) -> Result<(), CommandLifecycleError> {
    if execution_phase == "terminal" && terminal_status == "none" {
        return Err(invalid("terminal execution requires a terminal_status"));
    }
    if execution_phase != "terminal" && terminal_status != "none" {
        return Err(invalid(
            "nonterminal execution must use terminal_status=none",
        ));
    }
    Ok(())
}

fn require_known(
    value: Option<&Value>,
    allowed: &[&str],
    field: &str,
) -> Result<(), CommandLifecycleError> {
    let value = value.and_then(Value::as_str).unwrap_or_default();
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!("{field} contains an unknown value")))
    }
}

fn require_non_negative_integer(
    value: Option<&Value>,
    field: &str,
) -> Result<(), CommandLifecycleError> {
    if value.and_then(Value::as_u64).is_some() {
        Ok(())
    } else {
        Err(invalid(format!("{field} must be a non-negative integer")))
    }
}

fn u64_field(object: &serde_json::Map<String, Value>, field: &str) -> u64 {
    object
        .get(field)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, field: &str) -> &'a str {
    object
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn invalid(message: impl Into<String>) -> CommandLifecycleError {
    CommandLifecycleError {
        code: "invalid_transition",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(execution_phase: &str, terminal_status: &str, version: u64, attempt: u64) -> Value {
        serde_json::json!({
            "contract_version": 2,
            "command_id": "cmd-contract",
            "idempotency_key": "cmd-contract",
            "payload_hash": "sha256:fixture",
            "module": "ctox",
            "command_type": "business_os.chat.task",
            "record_id": "",
            "payload": {"instruction": "test"},
            "client_context": {},
            "created_at_ms": 1,
            "replication_phase": "native_observed",
            "execution_mode": "queue",
            "execution_phase": execution_phase,
            "terminal_status": terminal_status,
            "projection_version": version,
            "attempt": attempt
        })
    }

    #[test]
    fn lifecycle_accepts_review_and_bounded_rework_transitions() {
        let running = document("running", "none", 3, 0);
        let review = document("awaiting_review", "none", 4, 0);
        let retry = document("retry_wait", "none", 5, 1);
        let queued = document("queued", "none", 6, 1);
        validate_transition(&running, &review).unwrap();
        validate_transition(&review, &retry).unwrap();
        validate_transition(&retry, &queued).unwrap();
    }

    #[test]
    fn lifecycle_rejects_terminal_regression_and_payload_change() {
        let terminal = document("terminal", "completed", 7, 1);
        let running = document("running", "none", 8, 2);
        assert_eq!(
            validate_transition(&terminal, &running).unwrap_err().code,
            "invalid_transition"
        );

        let previous = document("accepted", "none", 1, 0);
        let mut changed = document("queued", "none", 2, 0);
        changed["payload"] = serde_json::json!({"instruction": "different"});
        assert_eq!(
            validate_transition(&previous, &changed).unwrap_err().code,
            "idempotency_conflict"
        );
    }
}
