// Origin: CTOX
// License: Apache-2.0
//
// Command-bus handlers for agent secret requests. They reuse the redaction
// contract of `ctox.secret.put`: the `value` field of
// `ctox.secret.request.fulfill` is detached at intake (see
// `secret_payload_field` in store.rs), and every persisted copy of the command
// carries only the request id. The outcome returns the reference, never the
// value.

use super::command_plane::write_rxdb_control_command_outcome;
use super::policy::BusinessOsPermission;
use super::store::{
    rxdb_authenticated_session, secret_command_safe_command, take_secret_intake_value,
    BusinessCommand,
};
use super::store_policy::{
    enforce_command_policy, reject_command_if_policy_denied, workspace_policy_decision,
    CommandPolicyRequirement,
};
use serde_json::{json, Value};
use std::path::Path;
use zeroize::Zeroizing;

pub(super) fn handle_secret_request_command(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    if command.command_type == "ctox.secret.request.list" {
        return enforce_command_policy(
            root,
            command,
            |_| {
                Ok(CommandPolicyRequirement::workspace(
                    BusinessOsPermission::SecretsManage,
                ))
            },
            |_session| {
                let status = command.payload.get("status").and_then(Value::as_str);
                let thread = command.payload.get("thread_key").and_then(Value::as_str);
                let outcome = match crate::secret_requests::list_requests(root, status, thread) {
                    Ok(requests) => json!({ "outcome": { "ok": true, "requests": requests } }),
                    Err(error) => json!({ "outcome": { "ok": false, "error": error.to_string() } }),
                };
                write_rxdb_control_command_outcome(
                    root,
                    command,
                    "completed",
                    None,
                    Some("completed"),
                    outcome,
                )
            },
        )?
        .into_outcome();
    }
    let session = rxdb_authenticated_session(root, command)?;
    let request_id = command
        .payload
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let value = Zeroizing::new(take_secret_intake_value(command).unwrap_or_default());
    let safe_command =
        secret_command_safe_command(command, &session, json!({ "request_id": request_id }));
    let decision = workspace_policy_decision(root, &session, BusinessOsPermission::SecretsManage)?;
    if let Some(outcome) = reject_command_if_policy_denied(root, &safe_command, &decision)? {
        return Ok(outcome);
    }
    let result = match command.command_type.as_str() {
        "ctox.secret.request.fulfill" => {
            crate::secret_requests::fulfill_request(root, &request_id, value.as_str())
        }
        "ctox.secret.request.decline" => crate::secret_requests::decline_request(root, &request_id),
        other => anyhow::bail!("unsupported secret request command {other}"),
    };
    match result {
        Ok(request) => write_rxdb_control_command_outcome(
            root,
            &safe_command,
            "completed",
            None,
            Some("completed"),
            json!({ "outcome": {
                "ok": true,
                "request_id": request.request_id,
                "status": request.status,
                "ref": request.secret_ref,
            } }),
        ),
        Err(error) => write_rxdb_control_command_outcome(
            root,
            &safe_command,
            "failed",
            None,
            Some("failed"),
            json!({ "outcome": { "ok": false, "error": error.to_string() } }),
        ),
    }
}
