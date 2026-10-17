//! Adapt committed P9 documents to the existing native dispatcher. No V3
//! intake, command-ledger insert, projection write or acceptance RPC is used.
#[cfg(test)]
#[path = "command_plane_e49_fence_tests.rs"]
mod e49_native_fence_tests;
use super::*;
use ctox_store::change_log::subscription::write_contract::{
    routes::{self, NativeCommand},
    CommandError,
};
use rxdb::plugins::replication_webrtc::index_mod::NativeCommandAuthority;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(crate) fn execute(
    root: &Path,
    command: NativeCommand,
    peer: &str,
    capability: &str,
    authority: Arc<dyn NativeCommandAuthority>,
) -> Result<Value, CommandError> {
    let route = command.validate()?;
    if !routes::is_command_bus(route) {
        return Err(routes::error("unknown_command", "unknown_command"));
    }
    let mut result = None;
    authority
        .with_current(&command, peer, capability, &mut || {
            if result.is_some() {
                return Err(rxdb::rx_error::new_rx_error("E25_DUPLICATE_EFFECT", None));
            }
            result = Some(dispatch(root, &command, capability));
            Ok(())
        })
        .map_err(|_| routes::error("revoked", "command_authority_changed"))?;
    result.ok_or_else(|| routes::error("forbidden", "command_not_admitted"))?
}
fn dispatch(root: &Path, native: &NativeCommand, capability: &str) -> Result<Value, CommandError> {
    let command = BusinessCommand {
        id: Some(native.command_id.clone()),
        module: "ctox".into(),
        command_type: native.kind.clone(),
        record_id: ["record_id", "project_id", "session_id", "id"]
            .iter()
            .find_map(|k| native.payload[*k].as_str())
            .map(str::to_owned),
        payload: native.payload.clone(),
        client_context: serde_json::json!({"capability_token":capability}),
        origin: CommandOrigin::ReplicatedPeer,
    };
    // Resolve user identity from the official admitted token; never construct a
    // privileged BusinessOsSession from app payload or a service user.
    let session = if matches!(
        CommandAuthorizationStage::for_command(&command),
        CommandAuthorizationStage::CommandSession
    ) {
        rxdb_command_session(root, &command)
    } else {
        rxdb_authenticated_session(root, &command)
    }
    .map_err(|_| routes::error("forbidden", "native_identity_unavailable"))?;
    if session_user_id(&session) != Some(native.actor_id.as_str()) {
        return Err(routes::error("forbidden", "native_actor_mismatch"));
    }
    dispatch_authorized(root, native, &command, &session)
}
fn dispatch_authorized(
    root: &Path,
    native: &NativeCommand,
    command: &BusinessCommand,
    session: &BusinessOsSession,
) -> Result<Value, CommandError> {
    if let Some(requirement) = CentralCommandPolicyRequirement::for_command(&command) {
        let requirement = requirement
            .resolve(root, command, session)
            .map_err(|_| routes::error("forbidden", "native_policy_unavailable"))?;
        use super::super::store_policy::{
            module_policy_decision, scoped_policy_decision, workspace_policy_decision,
        };
        let decision = match requirement {
            CommandPolicyRequirement::Module {
                permission,
                module_id,
            } => module_policy_decision(root, session, permission, &module_id),
            CommandPolicyRequirement::Workspace { permission } => {
                workspace_policy_decision(root, session, permission)
            }
            CommandPolicyRequirement::Scoped { permission, scope } => {
                scoped_policy_decision(root, session, permission, scope)
            }
        }
        .map_err(|_| routes::error("forbidden", "native_policy_unavailable"))?;
        if !decision.allowed {
            return Err(routes::error("forbidden", "native_permission_denied"));
        }
    }
    if matches!(
        native.kind.as_str(),
        "ctox.crew.execution.list"
            | "ctox.crew.execution.claim"
            | "ctox.crew.execution.plan.update"
            | "ctox.crew.execution.report"
            | "ctox.task.create"
    ) {
        // Both callers enter under the actual retained native authority/fence.
        return super::super::mcp_channel::native_tasks::execute(
            root,
            &native.command_id,
            &native.kind,
            &native.payload,
            session,
        )
        .map_err(|e| routes::native_error(&e.to_string()));
    }
    if matches!(
        native.kind.as_str(),
        "ctox.secret.request.list"
            | "ctox.secret.request.fulfill"
            | "ctox.secret.request.decline"
            | "ctox.workjet.remote_worker.admit"
            | "ctox.maintenance.restart"
            | "instance.providers.models.check"
    ) {
        // E49 uses the canonical Platform writer, not a legacy command ledger.
        return Err(routes::error(
            "unavailable",
            "canonical_platform_feed_required",
        ));
    }
    let mut prepared = PreparedBusinessCommand::from_command(command)
        .map_err(|_| routes::error("invalid", "native_payload_invalid"))?;
    if domain_effect::supports_command(&command.command_type) {
        // Admission is minted only INSIDE the retained real P9 execution guard.
        // Existing domain receipts still enforce their native transaction and
        // recovery contract. A handler requiring an unmigrated legacy Core
        // claim fails closed; this adapter never fabricates that second ledger.
        let intent =
            serde_json::json!([native.kind, native.app_id, native.actor_id, native.payload]);
        let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&intent).unwrap()));
        prepared.domain_effect_admission = Some(
            DomainEffectAdmission::newly_claimed(&native.command_id, &hash, &native.actor_id)
                .map_err(|_| routes::error("invalid", "native_domain_identity"))?,
        );
    }
    match dispatch_business_command(root, &native.command_id, command, prepared, Some(session))
        .map_err(|_| routes::error("unavailable", "native_handler_failed"))?
    {
        BusinessCommandDispatchOutcome::Returned(value) => Ok(value),
        BusinessCommandDispatchOutcome::Control { result, .. } => Ok(result),
        BusinessCommandDispatchOutcome::FailedControl { error, .. } => {
            Err(routes::native_error(&error.to_string()))
        }
    }
}

/// Productive Core feed adapter. The authority is the already-retained native
/// invocation, not a grant deserialized from the binding. No legacy token is
/// minted, copied from payload or substituted with a service user.
pub(crate) fn execute_core(
    root: &Path,
    binding: &Value,
    authority: &dyn ctox_app_runtime::core_host::CoreAuthority,
) -> anyhow::Result<Value> {
    let doc = &binding["command"];
    let text = |k: &str| {
        doc[k]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("invalid: native command {k}"))
    };
    let native = NativeCommand {
        command_id: text("id")?,
        kind: text("type")?,
        app_id: text("app_id")?,
        actor_id: text("actor_id")?,
        payload: doc["payload"].clone(),
    };
    anyhow::ensure!(
        routes::is_command_bus(
            native
                .validate()
                .map_err(|e| anyhow::anyhow!("{}: {}", e.code, e.reason))?
        ),
        "unknown_command: native Core route"
    );
    // admit checks canonical running row, original actor, current manifest/rights
    // and retained invocation. with_fence excludes teardown during the effect.
    authority.admit(&binding["manifest"], binding)?;
    authority.with_fence(binding, &mut || {
        let actor = &doc["actor"];
        anyhow::ensure!(
            actor["id"] == native.actor_id && doc["status"] == "running",
            "forbidden: native actor binding"
        );
        let role = actor["role"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("forbidden: native role"))?;
        anyhow::ensure!(
            ["admin", "chef", "founder", "user"].contains(&role),
            "forbidden: native role"
        );
        let session = BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(super::super::session::BusinessOsSessionUser {
                id: native.actor_id.clone(),
                display_name: native.actor_id.clone(),
                role: role.into(),
                is_admin: role == "admin",
            }),
            login_url: None,
            reason: None,
        };
        let command = BusinessCommand {
            id: Some(native.command_id.clone()),
            module: "ctox".into(),
            command_type: native.kind.clone(),
            record_id: ["record_id", "project_id", "session_id", "id"]
                .iter()
                .find_map(|k| native.payload[*k].as_str())
                .map(str::to_owned),
            payload: native.payload.clone(),
            client_context: serde_json::json!({}),
            origin: CommandOrigin::ReplicatedPeer,
        };
        dispatch_authorized(root, &native, &command, &session).map_err(|e| {
            ctox_app_runtime::HandlerError {
                uncertain: e.code == "unavailable",
                code: e.code,
                message: e.reason,
                path: None,
            }
            .into()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business_os::store;
    struct RetainedTestAuthority;
    impl NativeCommandAuthority for RetainedTestAuthority {
        fn with_current(
            &self,
            _: &NativeCommand,
            _: &str,
            _: &str,
            effect: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
        ) -> rxdb::rx_error::RxResult<()> {
            effect()
        }
        fn with_completion(
            &self,
            _: &rusqlite::Transaction<'_>,
            _: &NativeCommand,
            _: &str,
            _: &str,
            effect: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>,
        ) -> rxdb::rx_error::RxResult<()> {
            effect()
        }
    }
    #[test]
    fn e25_native_command_bus_preserves_user_and_skips_legacy_intake() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "e25-owner",
            "E25 owner",
            "chef",
            chrono::Utc::now().timestamp_millis(),
        )?;
        fn commands_at(root: &Path) -> anyhow::Result<i64> {
            let conn = store::open_store(root)?;
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='business_commands')",
                [],
                |r| r.get(0),
            )?;
            if exists {
                Ok(conn.query_row("SELECT COUNT(*) FROM business_commands", [], |r| r.get(0))?)
            } else {
                Ok(0)
            }
        }
        let before = commands_at(root.path())?;
        for kind in [
            "ctox.workjet.project.list",
            "ctox.workjet.computer.list",
            "ctox.workjet.session.list",
        ] {
            let command = NativeCommand {
                command_id: uuid::Uuid::new_v4().to_string(),
                kind: kind.into(),
                app_id: "ctox".into(),
                actor_id: "e25-owner".into(),
                payload: serde_json::json!({}),
            };
            let result = execute(
                root.path(),
                command.clone(),
                "admitted-browser",
                &token,
                Arc::new(RetainedTestAuthority),
            )
            .map_err(|e| anyhow::anyhow!("{}: {} {}", kind, e.code, e.reason))?;
            assert!(result.is_object());
            assert_ne!(result["ok"], serde_json::json!(false));
            let mut forged = command;
            forged.actor_id = "foreign-owner".into();
            assert_eq!(
                execute(
                    root.path(),
                    forged,
                    "admitted-browser",
                    &token,
                    Arc::new(RetainedTestAuthority)
                )
                .unwrap_err()
                .reason,
                "native_actor_mismatch"
            );
        }
        assert_eq!(
            commands_at(root.path())?,
            before,
            "V4 native dispatch must not use the V3 command intake or duplicate its ledger"
        );
        Ok(())
    }
}
