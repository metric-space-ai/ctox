// Outcome classification for scrape runs: maps payload markers, probe
// results, exit codes and record counts onto a ScrapeRunStatus.

use super::{CommandExecution, ProbeResult};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScrapeRunStatus {
    /// An accepted provider job still needs bounded native continuation.
    AwaitingProvider,
    Succeeded,
    /// A current provider query completed with a validated empty-result receipt.
    CompletedEmpty,
    TemporaryUnreachable,
    PortalDrift,
    Blocked,
    PartialOutput,
    /// Session expired/invalid on a credential-protected source: the run
    /// landed on the source's own login page during an authenticated
    /// capture. Distinct from `PortalDrift` (genuine layout/domain drift)
    /// and from `Blocked` (upstream challenge/verification).
    AuthorizationRequired,
}

#[derive(Debug)]
pub(super) struct Classification {
    pub(super) status: ScrapeRunStatus,
    pub(super) should_queue_repair: bool,
    pub(super) reason: String,
}

pub(super) fn classify_outcome(
    payload: &Value,
    probe: &ProbeResult,
    execution: &CommandExecution,
    records_found: i64,
    expected_min_records: i64,
) -> Classification {
    let explicit_failure = payload
        .get("failure_mode")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if explicit_failure == "temporary_unreachable" {
        return Classification {
            status: ScrapeRunStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: "explicit_failure_mode_temporary_unreachable".to_string(),
        };
    }
    if explicit_failure == "portal_drift" {
        return Classification {
            status: ScrapeRunStatus::PortalDrift,
            should_queue_repair: true,
            reason: "explicit_failure_mode_portal_drift".to_string(),
        };
    }
    if explicit_failure == "blocked" {
        return Classification {
            status: ScrapeRunStatus::Blocked,
            should_queue_repair: true,
            reason: "explicit_failure_mode_blocked".to_string(),
        };
    }
    if explicit_failure == "auth_required" {
        return Classification {
            status: ScrapeRunStatus::Blocked,
            should_queue_repair: true,
            reason: "explicit_failure_mode_auth_required".to_string(),
        };
    }
    if explicit_failure == "authorization_required" {
        return Classification {
            status: ScrapeRunStatus::AuthorizationRequired,
            should_queue_repair: true,
            reason: "explicit_failure_mode_authorization_required".to_string(),
        };
    }
    if explicit_failure == "partial_output"
        || payload.get("partial_output") == Some(&Value::Bool(true))
    {
        return Classification {
            status: ScrapeRunStatus::PartialOutput,
            should_queue_repair: true,
            reason: "payload_marked_partial_output".to_string(),
        };
    }

    let lower = format!(
        "{}\n{}",
        execution.stderr_text,
        probe.error.as_deref().unwrap_or("")
    )
    .to_lowercase();
    if probe.human_verification || matches!(probe.status_code, Some(401 | 403)) {
        return Classification {
            status: ScrapeRunStatus::Blocked,
            should_queue_repair: true,
            reason: probe
                .error
                .clone()
                .unwrap_or_else(|| format!("http_{}", probe.status_code.unwrap_or_default())),
        };
    }
    if probe.status_code == Some(404) {
        return Classification {
            status: ScrapeRunStatus::PortalDrift,
            should_queue_repair: true,
            reason: "http_404".to_string(),
        };
    }
    if matches!(probe.status_code, Some(429))
        || probe.status_code.map(|code| code >= 500).unwrap_or(false)
    {
        return Classification {
            status: ScrapeRunStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: format!("http_{}", probe.status_code.unwrap_or_default()),
        };
    }
    if !probe.reachable {
        return Classification {
            status: ScrapeRunStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: probe
                .error
                .clone()
                .unwrap_or_else(|| "portal_unreachable".to_string()),
        };
    }
    // A run that exited 0 and delivered the expected records succeeded no
    // matter what its stderr chattered about ("timeout", "429", ...). The
    // transient classification must never outrank that: records only
    // materialize on Succeeded, so misclassifying here silently drops them.
    let full_success = execution.exit_code.unwrap_or(0) == 0
        && records_found > 0
        && records_found >= expected_min_records;
    if execution.timed_out || (!full_success && contains_transient_hint(&lower)) {
        return Classification {
            status: ScrapeRunStatus::TemporaryUnreachable,
            should_queue_repair: false,
            reason: if execution.timed_out {
                "command_timed_out".to_string()
            } else {
                "transient_error_hint".to_string()
            },
        };
    }
    if expected_min_records > 0 && records_found > 0 && records_found < expected_min_records {
        return Classification {
            status: ScrapeRunStatus::PartialOutput,
            should_queue_repair: true,
            reason: format!(
                "records_found_below_expected_min:{}<{}",
                records_found, expected_min_records
            ),
        };
    }
    if execution.exit_code.unwrap_or(0) != 0 {
        return Classification {
            status: ScrapeRunStatus::PortalDrift,
            should_queue_repair: true,
            reason: format!("command_failed_exit_{:?}", execution.exit_code),
        };
    }
    if records_found == 0 {
        return Classification {
            status: ScrapeRunStatus::PortalDrift,
            should_queue_repair: true,
            reason: "empty_record_set_on_reachable_portal".to_string(),
        };
    }
    Classification {
        status: ScrapeRunStatus::Succeeded,
        should_queue_repair: false,
        reason: "ok".to_string(),
    }
}

pub(super) fn contains_transient_hint(text: &str) -> bool {
    [
        "timeout",
        "timed out",
        "temporary",
        "temporarily",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "name or service not known",
        "429",
        "502",
        "503",
        "504",
        "ssl",
        "proxyerror",
        "net::err_",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}
