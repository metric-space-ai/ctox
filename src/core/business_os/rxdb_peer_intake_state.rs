// Origin: CTOX
// License: AGPL-3.0-only

use super::rxdb_peer::now_ms;
use super::store;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingBusinessCommandIntakeOutcome {
    Accepted,
    CanonicalReplayed,
    WaitingForReplication {
        error: String,
    },
    RetryableFailure {
        error: String,
    },
    /// Native intake rejected the replicated command because it carries no
    /// valid capability token. Deterministic: recorded with a small budget and
    /// then terminalized as `auth_required`, never replayed in a loop.
    CapabilityRejected {
        reason: &'static str,
        error: String,
    },
    Terminalized,
}

pub(super) fn business_command_document_is_terminal(document: &Value) -> bool {
    let terminal_status = document
        .get("terminal_status")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let status = document.get("status").and_then(Value::as_str).unwrap_or("");
    document.get("execution_phase").and_then(Value::as_str) == Some("terminal")
        || terminal_status != "none"
        || crate::command_lifecycle::terminal_status_is_outcome(status)
}

pub(super) async fn resolve_business_command_intake_failure_history(root: &Path, command_id: &str) {
    let resolve_root = root.to_path_buf();
    let resolved_command_id = command_id.to_string();
    match tokio::task::spawn_blocking(move || {
        store::resolve_business_command_intake_failures(&resolve_root, &resolved_command_id)
    })
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => eprintln!(
            "[business-os] resolving intake failure history for `{command_id}` failed: {error:#}"
        ),
        Err(error) => eprintln!(
            "[business-os] joining intake failure resolution for `{command_id}` failed: {error}"
        ),
    }
}

pub(super) fn is_transient_business_command_store_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}").to_ascii_lowercase();
    [
        "database is locked",
        "database table is locked",
        "sqlite_busy",
        "cannot promote read transaction",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub(super) fn is_pending_desktop_file_replication_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}").to_ascii_lowercase();
    ["desktop file `", "business os attachment `"]
        .iter()
        .any(|prefix| message.contains(prefix))
        && [
            " is not indexed",
            " content is not available",
            " has no content generation",
            " has no chunks for generation",
            "no complete equivalent desktop file chunks found",
        ]
        .iter()
        .any(|needle| message.contains(needle))
}

pub(super) fn transient_business_command_retry_document(
    document: &Value,
    error_message: &str,
    attempt: u32,
) -> Option<Value> {
    let command_type = document
        .get("command_type")
        .or_else(|| document.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !store::is_recoverable_background_control_command_type(command_type) {
        return None;
    }
    let mut retry = document.clone();
    let object = retry.as_object_mut()?;
    object.insert(
        "status".to_string(),
        Value::String("pending_sync".to_string()),
    );
    object.insert(
        "task_status".to_string(),
        Value::String("pending_sync".to_string()),
    );
    object.insert("retryable".to_string(), Value::Bool(true));
    object.insert("retry_attempt".to_string(), Value::from(attempt));
    object.insert(
        "last_retry_error".to_string(),
        Value::String(error_message.to_string()),
    );
    object.remove("error");
    object.remove("error_code");
    object.insert("updated_at_ms".to_string(), Value::from(now_ms() as u64));
    Some(retry)
}

#[cfg(test)]
mod tests {
    use super::is_pending_desktop_file_replication_error;

    #[test]
    fn classifies_only_incomplete_desktop_file_replication_as_pending() {
        let missing_chunks = anyhow::anyhow!(
            "no complete equivalent desktop file chunks found for `appimport_file`; primary decode failed: Business OS attachment `appimport_file` has no chunks for generation `gen_1`"
        );
        assert!(is_pending_desktop_file_replication_error(&missing_chunks));

        let missing_file = anyhow::anyhow!("desktop file `appimport_file` is not indexed");
        assert!(is_pending_desktop_file_replication_error(&missing_file));

        let hash_mismatch =
            anyhow::anyhow!("Business OS attachment `appimport_file` content hash mismatch");
        assert!(!is_pending_desktop_file_replication_error(&hash_mismatch));
    }
}
