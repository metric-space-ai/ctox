// Origin: CTOX
// License: AGPL-3.0-only

//! Resolve an explicit project Luma only inside its actual native Supervisor
//! lease. Eligibility is not a Claude Code holding permit or execution proof.
use super::super::{provider_federation, worker_profile_bindings};
use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde_json::json;

#[derive(Debug)]
pub(crate) struct SupervisorLumaUnavailable {
    pub code: &'static str,
    detail: String,
}
impl std::fmt::Display for SupervisorLumaUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for SupervisorLumaUnavailable {}
fn unavailable(code: &'static str, detail: impl Into<String>) -> anyhow::Error {
    SupervisorLumaUnavailable {
        code,
        detail: detail.into(),
    }
    .into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeAccountReference {
    account_id: String,
    holder_instance_id: String,
    account_revision: i64,
}

/// Requested selection only. No private selector, credential, or claimed
/// actual producer/model is serializable from this value.
#[derive(Debug, PartialEq, Serialize)]
struct RequestedRoute {
    project_id: String,
    supervisor_thread_id: String,
    luma_id: String,
    configuration_revision: u64,
    computer_id: String,
    harness: String,
    route_id: String,
    model: String,
    native_account: NativeAccountReference,
    catalog_checked_at_ms: i64,
}

fn required_text(value: &Value, key: &str, limit: usize) -> anyhow::Result<String> {
    value[key]
        .as_str()
        .filter(|v| !v.trim().is_empty() && v == &v.trim() && v.len() <= limit)
        .map(str::to_owned)
        .ok_or_else(|| {
            unavailable(
                "invalid_supervisor_luma_configuration",
                format!("invalid {key}"),
            )
        })
}
fn unique<'a>(configuration: &'a Value, list: &str, id: &str) -> anyhow::Result<&'a Value> {
    let mut found = configuration[list]
        .as_array()
        .filter(|entries| entries.len() <= 512)
        .ok_or_else(|| {
            unavailable(
                "invalid_supervisor_luma_configuration",
                format!("invalid {list}"),
            )
        })?
        .iter()
        .filter(|entry| entry["id"].as_str() == Some(id));
    let entry = found.next().ok_or_else(|| {
        unavailable(
            "supervisor_luma_reference_unavailable",
            format!("missing {list} reference"),
        )
    })?;
    anyhow::ensure!(
        found.next().is_none(),
        unavailable(
            "ambiguous_supervisor_luma_reference",
            format!("duplicate {list} reference")
        )
    );
    Ok(entry)
}

fn resolve(
    policy: &Connection,
    owner: &str,
    project_id: &str,
    thread_id: &str,
) -> anyhow::Result<Option<RequestedRoute>> {
    let project = store::outbound_load_record(policy, "workjet_projects", project_id)?
        .context("registered Supervisor project missing")?;
    // Deliberately do not consult Luma/account configuration for the default.
    let Some(id) = project.get("supervisor_luma_id").filter(|id| !id.is_null()) else {
        return Ok(None);
    };
    let id = id
        .as_str()
        .filter(|v| !v.trim().is_empty() && v.len() <= 160)
        .ok_or_else(|| {
            unavailable(
                "invalid_supervisor_luma_configuration",
                "invalid project Luma reference",
            )
        })?;
    let record = store::outbound_load_record(policy, "workjet_luma_configuration", "instance")?
        .ok_or_else(|| {
            unavailable(
                "supervisor_luma_configuration_unavailable",
                "no instance Luma configuration",
            )
        })?;
    let revision = record["revision"]
        .as_u64()
        .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
        .ok_or_else(|| {
            unavailable(
                "invalid_supervisor_luma_configuration",
                "invalid configuration revision",
            )
        })?;
    anyhow::ensure!(
        record["schema_version"] == 1 && record["is_deleted"] != true,
        unavailable(
            "invalid_supervisor_luma_configuration",
            "unsupported or retired configuration"
        )
    );
    let config = &record["configuration"];
    let profile = unique(config, "workerProfiles", id)?;
    let computer_id = required_text(profile, "computerId", 256)?;
    let binding = worker_profile_bindings::require_active(policy, owner, id)?;
    anyhow::ensure!(
        binding["computer_id"] == computer_id,
        unavailable(
            "supervisor_luma_computer_binding_changed",
            "profile and native computer assignment differ"
        )
    );
    let harness = required_text(profile, "harness", 32)?;
    anyhow::ensure!(
        matches!(
            harness.as_str(),
            "claude-code"
                | "codex-cli"
                | "opencode"
                | "grok-cli"
                | "cursor-agent"
                | "greppy"
                | "minimax-code"
                | "pi-code"
        ),
        unavailable(
            "unsupported_supervisor_luma_harness",
            "unknown configured harness"
        )
    );
    let route_id = required_text(profile, "llmRouteId", 160)?;
    let model = required_text(profile, "modelId", 256)?;
    let route = unique(config, "llmRoutes", &route_id)?;
    let reference: NativeAccountReference = serde_json::from_value(
        route
            .get("nativeAccountReference")
            .ok_or_else(|| {
                unavailable(
                    "missing_native_account_binding",
                    "route has no authoritative native account reference",
                )
            })?
            .clone(),
    )
    .map_err(|_| {
        unavailable(
            "invalid_native_account_binding",
            "invalid native account reference",
        )
    })?;
    anyhow::ensure!(
        !reference.account_id.is_empty()
            && reference.account_id.len() <= 256
            && !reference.holder_instance_id.is_empty()
            && reference.holder_instance_id.len() <= 256
            && (1..=9_007_199_254_740_991).contains(&reference.account_revision),
        unavailable(
            "invalid_native_account_binding",
            "invalid native account identity or revision"
        )
    );
    let eligibility = provider_federation::resolve_supervisor_model(
        policy,
        owner,
        &reference.account_id,
        reference.account_revision,
        &model,
    )
    .map_err(|error| unavailable("supervisor_account_model_unavailable", error.to_string()))?;
    anyhow::ensure!(
        eligibility.account().holder_instance_id == reference.holder_instance_id,
        unavailable(
            "supervisor_account_holder_changed",
            "native account holder differs"
        )
    );
    eligibility.revalidate(policy, owner)?;
    Ok(Some(RequestedRoute {
        project_id: project_id.to_owned(),
        supervisor_thread_id: thread_id.to_owned(),
        luma_id: id.to_owned(),
        configuration_revision: revision,
        computer_id,
        harness,
        route_id,
        model: eligibility.model().to_owned(),
        native_account: reference,
        catalog_checked_at_ms: eligibility.catalog_checked_at_ms(),
    }))
}

fn lease_record(trusted: &Value) -> anyhow::Result<(String, String, String)> {
    let execution_key = workjet_jour_fixe::execution_key(trusted)?;
    let lease = json!({"command":trusted["workjet_supervisor_lease"],"plan":trusted["workjet_confirmed_plan"],
        "epoch":trusted["workjet_supervisor_epoch"]});
    let lease_json = serde_json::to_string(&lease)?;
    let lease_hash =
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, lease_json.as_bytes()).as_ref());
    Ok((execution_key, lease_json, lease_hash))
}

/// Clearing a project selection affects a new execution, not an already
/// sealed native lease. This default-path read creates no schema/writer.
fn require_unsealed_default(root: &Path, trusted: &Value, owner: &str) -> anyhow::Result<()> {
    let mut core = Connection::open_with_flags(
        crate::paths::core_db(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let core = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let exists: bool = core.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table'
         AND name='workjet_supervisor_route_attempts')",
        [],
        |row| row.get(0),
    )?;
    if exists {
        let (execution_key, _, lease_hash) = lease_record(trusted)?;
        let sealed: bool = core.query_row(
            "SELECT EXISTS(SELECT 1 FROM workjet_supervisor_route_attempts
             WHERE execution_key=?1 AND lease_hash=?2 AND owner_user_id=?3)",
            params![execution_key, lease_hash, owner],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            !sealed,
            unavailable(
                "supervisor_selection_changed_during_lease",
                "a sealed project Luma was cleared in this execution lease"
            )
        );
    }
    core.commit()?;
    Ok(())
}

/// The actual service calls this after issuing its restricted command/plan
/// token, before any model invocation. A configured external harness must not
/// fall through to PersistentSession with the instance default. Until a genuine
/// holding producer is connected, fail explicitly and retain requested facts
/// under this exact native execution lease; actual execution remains NULL.
pub(crate) fn require_executor(root: &Path, token: Option<&str>) -> anyhow::Result<()> {
    let Some(token) = token else { return Ok(()) };
    let trusted = verify_internal_command_session_token(root, token)?;
    if trusted["workjet_supervisor_only"] != true {
        return Ok(());
    }
    let context = context_from_arguments_with_trusted_gateway_context(
        workjet_worker_dispatch::TOOL,
        &json!({}),
        Some(&trusted),
    )?;
    // The default path performs no new writer reservation. This hint can only
    // skip an added restriction; it grants no execution authority. Explicit
    // selections are re-read under the actual lease/project fence below.
    let policy_snapshot = store::open_store(root)?;
    let project_hint = if let Some(project) = trusted
        .pointer("/workjet_confirmed_plan/project_id")
        .and_then(Value::as_str)
    {
        project.to_owned()
    } else {
        store::load_business_command(&policy_snapshot, &required_arg(&trusted, "command_id")?)?
            .record_id
            .context("Supervisor project missing")?
    };
    let project_snapshot =
        store::outbound_load_record(&policy_snapshot, "workjet_projects", &project_hint)?
            .context("Supervisor project missing")?;
    if project_snapshot
        .get("supervisor_luma_id")
        .is_none_or(Value::is_null)
    {
        return require_unsealed_default(root, &trusted, &context.actor);
    }
    drop(policy_snapshot);
    let mut core = Connection::open(crate::paths::core_db(root))?;
    core.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let mut policy = store::open_store(root)?;
    let core = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let policy = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (project, thread, _) =
        workjet_jour_fixe::bound_project(&core, &policy, &context, &trusted)?;
    let Some(route) = resolve(&policy, &context.actor, &project, &thread)? else {
        return Err(unavailable(
            "supervisor_selection_changed_during_lease",
            "project Luma was cleared during execution admission",
        ));
    };
    let (execution_key, lease_json, lease_hash) = lease_record(&trusted)?;
    let code = if route.harness == "claude-code" {
        "claude_code_holding_executor_unavailable"
    } else {
        "project_supervisor_holding_executor_unavailable"
    };
    core.execute_batch(
        "CREATE TABLE IF NOT EXISTS workjet_supervisor_route_attempts (
        execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL, owner_user_id TEXT NOT NULL,
        lease_json TEXT NOT NULL, requested_json TEXT NOT NULL, actual_json TEXT,
        error_code TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
        PRIMARY KEY(execution_key,lease_hash));",
    )?;
    let requested_json = serde_json::to_string(&route)?;
    let prior: Option<String> = core.query_row(
        "SELECT requested_json FROM workjet_supervisor_route_attempts WHERE execution_key=?1 AND lease_hash=?2",
        params![execution_key, lease_hash], |row| row.get(0),
    ).optional()?;
    anyhow::ensure!(
        prior.as_deref().is_none_or(|prior| prior == requested_json),
        unavailable(
            "supervisor_selection_changed_during_lease",
            "requested route changed within the same execution lease"
        )
    );
    core.execute(
        "INSERT INTO workjet_supervisor_route_attempts
        (execution_key,lease_hash,owner_user_id,lease_json,requested_json,error_code,created_at_ms)
        VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(execution_key,lease_hash) DO NOTHING",
        params![
            execution_key,
            lease_hash,
            context.actor,
            lease_json,
            serde_json::to_string(&route)?,
            code,
            now_ms()
        ],
    )?;
    policy.commit()?;
    core.commit()?;
    Err(unavailable(code, "selected project Luma has no admitted holding producer; instance-default fallback was not invoked"))
}

#[cfg(test)]
#[path = "mcp_supervisor_luma_tests.rs"]
mod tests;
