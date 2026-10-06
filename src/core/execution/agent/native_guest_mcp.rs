//! Actual in-process MCP guest emission, installed only on native-admitted turns.
use anyhow::{ensure, Context, Result};
use ctox_core::native_mcp_dispatch::{
    NativeMcpDispatch, NativeMcpInvocation, NativeMcpRegistration,
};
use ctox_protocol::mcp::CallToolResult;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::business_os::store::{BusinessCommand, CommandOrigin};
use crate::channels::{
    NativeProviderAdmission, NativeProviderCommandEmitter, NativeProviderTurnOwner,
};

struct GuestDispatch {
    emitter: NativeProviderCommandEmitter,
    consumer: Arc<dyn NativeProviderAdmission>,
    binding_id: String,
    thread_id: String,
    actor: String,
    workspace: String,
}

pub(super) async fn register(
    client: &ctox_app_server_client::InProcessAppServerClient,
    owner: &NativeProviderTurnOwner,
    consumer: Arc<dyn NativeProviderAdmission>,
) -> Result<NativeMcpRegistration> {
    let (binding_id, thread_id, actor, workspace) =
        owner.binding().with_live_provider(|facts, _| {
            let provenance = facts
                .command_provenance
                .as_ref()
                .context("native MCP guest emission requires verified command identity")?;
            let actor = provenance
                .get("actor")
                .and_then(Value::as_str)
                .filter(|id| valid_id(id))
                .context("native MCP guest actor is missing")?;
            let workspace = provenance
                .get("workspace")
                .and_then(Value::as_str)
                .filter(|id| valid_id(id))
                .context("native MCP guest workspace is missing")?;
            Ok((
                facts.binding_id.clone(),
                facts.provider_session_id.clone(),
                actor.to_owned(),
                workspace.to_owned(),
            ))
        })?;
    let actual_thread = client
        .thread_manager()
        .get_thread(ctox_protocol::ThreadId::from_string(&thread_id).map_err(anyhow::Error::msg)?)
        .await?;
    actual_thread
        .register_native_mcp_dispatch(Arc::new(GuestDispatch {
            emitter: owner.command_emitter(),
            consumer,
            binding_id,
            thread_id,
            actor,
            workspace,
        }))
        .map_err(anyhow::Error::msg)
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn guest_arguments(arguments: Option<&Value>) -> Result<(&str, Option<String>, Value)> {
    let args = arguments
        .and_then(Value::as_object)
        .context("native MCP guest arguments are not an object")?;
    ensure!(
        args.keys().all(|key| matches!(
            key.as_str(),
            "module_id" | "action_id" | "record_id" | "payload"
        )),
        "native MCP guest emission rejects client identity/extra fields"
    );
    let module = args
        .get("module_id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .context("native MCP guest module is missing")?;
    let record_id = match args.get("record_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if valid_id(id) => Some(id.clone()),
        _ => anyhow::bail!("native MCP guest record identity is invalid"),
    };
    let payload = args
        .get("payload")
        .filter(|p| p.is_object())
        .context("native MCP guest payload is missing")?
        .clone();
    Ok((module, record_id, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_guest_mcp_rejects_client_identity_and_malformed_envelopes() {
        let valid = json!({"module_id": "guest", "action_id": "ctox.guest.observe",
            "record_id": "guest-1", "payload": {"guest_id": "guest-1"}});
        assert_eq!(
            guest_arguments(Some(&valid)).unwrap(),
            (
                "guest",
                Some("guest-1".into()),
                json!({"guest_id":"guest-1"})
            )
        );
        for key in [
            "command_id",
            "client_context",
            "actor",
            "workspace",
            "thread_id",
            "turn_id",
            "provider_session_id",
            "worker_id",
        ] {
            let mut forged = valid.clone();
            forged[key] = json!("forged");
            assert!(
                guest_arguments(Some(&forged)).is_err(),
                "client field {key} must not supply native identity"
            );
        }
        for (key, value) in [
            ("module_id", json!("")),
            ("record_id", json!(false)),
            ("record_id", json!("bad\nrecord")),
            ("payload", json!([])),
        ] {
            let mut malformed = valid.clone();
            malformed[key] = value;
            assert!(guest_arguments(Some(&malformed)).is_err());
        }
        assert!(guest_arguments(None).is_err());
        assert!(guest_arguments(Some(&json!([]))).is_err());
    }
}

impl NativeMcpDispatch for GuestDispatch {
    fn dispatch(
        &self,
        invocation: NativeMcpInvocation<'_>,
    ) -> Option<std::result::Result<CallToolResult, String>> {
        if invocation.server() != super::BUSINESS_OS_MCP_SESSION_SERVER_NAME
            || invocation.tool() != "business_os.execute_action"
        {
            return None;
        }
        let action = invocation
            .arguments()
            .and_then(|a| a.get("action_id"))
            .and_then(Value::as_str)?;
        if !matches!(action, "ctox.guest.observe" | "ctox.guest.input") {
            return None;
        }
        Some(self.execute(&invocation, action).map_err(|e| e.to_string()))
    }
}

impl GuestDispatch {
    fn execute(
        &self,
        invocation: &NativeMcpInvocation<'_>,
        action: &str,
    ) -> Result<CallToolResult> {
        ensure!(
            invocation.thread_id() == self.thread_id
                && valid_id(invocation.turn_id())
                && valid_id(invocation.call_id()),
            "native MCP guest emission has a foreign scope"
        );
        let (module, record_id, payload) = guest_arguments(invocation.arguments())?;
        // IDs are minted from the actual native invocation, never a model's
        // command_id. The same call cannot emit a second command/effect.
        let identity = serde_json::to_vec(&(
            self.binding_id.as_str(),
            invocation.thread_id(),
            invocation.turn_id(),
            invocation.call_id(),
            invocation.server(),
            invocation.tool(),
        ))?;
        let hash = ring::digest::digest(&ring::digest::SHA256, &identity);
        let id = format!(
            "guest-native-{}",
            hash.as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let command = BusinessCommand {
            origin: CommandOrigin::TrustedLocal,
            id: Some(id),
            module: module.into(),
            command_type: action.into(),
            record_id,
            payload,
            client_context: json!({"actor": {"id": self.actor}, "workspace": self.workspace,
                "source": "native_harness_mcp", "mcp_call_id": invocation.call_id(),
                "native_thread_id": invocation.thread_id(), "native_turn_id": invocation.turn_id()}),
        };
        let witness = self.emitter.admit_emitted_guest_command(
            invocation.turn_id(),
            &command,
            |_, facts, turn, _| {
                ensure!(
                    facts.binding_id == self.binding_id
                        && facts.provider_session_id == invocation.thread_id()
                        && turn == invocation.turn_id(),
                    "native MCP emission no longer matches its provider"
                );
                Ok(())
            },
        )?;
        // This private witness is the emission evidence. Client context remains
        // attribution only. The consumer must revalidate current policy and
        // controller/effect in with_current_command_transaction, without nesting
        // NativeGuestExecution::with_current or another provider transaction.
        self.consumer.execute_guest_command(&command, witness)
    }
}
