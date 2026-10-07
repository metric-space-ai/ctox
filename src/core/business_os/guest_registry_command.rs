// Origin: CTOX
// License: AGPL-3.0-only

//! Resolve an enrolled guest from the canonical command, never queue metadata.
//! The same proof is repeated while the real worker and policy guards are held.
use super::*;
use serde_json::Value;

impl std::fmt::Debug for NativeGuestRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeGuestRegistry")
            .finish_non_exhaustive()
    }
}

pub(super) fn validate_command(
    worker: &Connection,
    policy: &Connection,
    context: &Value,
    destination: &GuestRestoreDestination,
) -> Result<()> {
    let command = super::super::mcp_channel::current_guest_crew_command(worker, context)?;
    ensure!(
        command["command_type"] == "business_os.chat.task"
            && command
                .pointer("/payload/thread_id")
                .and_then(Value::as_str)
                == Some(destination.thread_id.as_str())
            && command.pointer("/payload/external_executor").is_none(),
        "native guest command targets another chat or execution owner"
    );
    for (key, expected) in [
        ("project_id", &destination.project_id),
        ("worker_profile_id", &destination.worker_profile_id),
    ] {
        if let Some(claim) = command["payload"].get(key) {
            ensure!(
                claim.as_str() == Some(expected.as_str()),
                "native guest command scope changed"
            );
        }
    }
    let profile = super::super::worker_profile_bindings::require_active(
        policy,
        &destination.human_owner_id,
        &destination.worker_profile_id,
    )?;
    let member = profile["crew_member_id"]
        .as_str()
        .context("guest profile has no Crew identity")?;
    ensure!(
        context
            .pointer("/crew_binding/member_id")
            .and_then(Value::as_str)
            == Some(member)
            && crate::crew::members(worker)?
                .iter()
                .any(|m| m.id == member && !m.archived),
        "native guest profile differs from the actual active Crew attempt"
    );
    Ok(())
}

impl NativeGuestRegistry {
    /// Called only with a freshly verified command-session context. An absent
    /// enrollment leaves the existing route intact; a stale matching assignment
    /// is an error, never permission to fall back to an unguarded model session.
    pub(crate) fn select_command_context(
        &self,
        runtime_root: &Path,
        context: &Value,
    ) -> Result<Option<String>> {
        self.verify_runtime_root(runtime_root)?;
        let command_id = context["command_id"]
            .as_str()
            .context("native command ID missing")?;
        let mut worker = Connection::open_with_flags(
            crate::paths::core_db(runtime_root),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        worker.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
        let worker = worker.transaction()?;
        let command = crate::channels::business_command_projection_from_conn(&worker, command_id)?;
        // External Crew/PI/Workjet execution remains with its existing owner.
        if command["command_type"] != "business_os.chat.task"
            || command.pointer("/payload/external_executor").is_some()
        {
            return Ok(None);
        }
        let Some(thread) = command
            .pointer("/payload/thread_id")
            .and_then(Value::as_str)
        else {
            return Ok(None);
        };
        let owner = context["actor"]
            .as_str()
            .context("native command owner missing")?;
        let candidates: Vec<_> = self
            .guests
            .lock()
            .map_err(|_| anyhow::anyhow!("native guest registry poisoned"))?
            .values()
            .cloned()
            .collect();
        self.with_policy(|policy| {
            let mut selected = None;
            for registration in candidates {
                let entry = registration
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native controller poisoned"))?;
                let d = &entry.assignment.destination;
                if d.human_owner_id != owner || d.thread_id != thread {
                    continue;
                }
                // One actual Crew member resolves group-chat ambiguity too.
                let member = context
                    .pointer("/crew_binding/member_id")
                    .and_then(Value::as_str)
                    .context("enrolled native guest requires an actual Crew attempt")?;
                let profile = super::super::worker_profile_bindings::require_active(
                    policy,
                    owner,
                    &d.worker_profile_id,
                )?;
                if profile["crew_member_id"].as_str() != Some(member) {
                    continue;
                }
                ensure!(
                    selected.is_none(),
                    "native guest command has ambiguous enrollment"
                );
                self.admission_destination(policy, &entry)?;
                super::accounts::require_assignment(policy, d)?;
                ensure!(
                    entry.execution.is_none(),
                    "native guest already owns an execution; reconcile before another session"
                );
                ensure!(
                    context["expires_at_ms"]
                        .as_u64()
                        .is_some_and(|expiry| u128::from(expiry) > super::super::store::now_ms()),
                    "native guest command expired"
                );
                validate_command(&worker, policy, context, d)?;
                selected = Some(d.guest_id.clone());
            }
            Ok(selected)
        })
    }
}
