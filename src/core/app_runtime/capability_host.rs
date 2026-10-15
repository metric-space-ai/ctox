//! Production CAP host: all identity, policy and fence authority stays in Core.
//! No constructor accepts handler-supplied grants or a wire fence as authority.
use crate::core_host::{capability_call_id, prepare_registry, CapabilityRouter, CoreAuthority};
use anyhow::{bail, Result};
use ctox_app_capabilities_contract::{
    policy::CapabilityHost, protocol::CapMessage, registry::AuditEntry, CapCallContext,
    CapabilityBackend, CapabilityGrant, CapabilityRegistry, CtoxError, ErrorCode,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};

fn denied(error: anyhow::Error) -> CtoxError {
    if let Some(error) = error.downcast_ref::<CtoxError>() {
        return error.clone();
    }
    let code = match crate::error_code(&error.to_string()) {
        "forbidden" => ErrorCode::Forbidden,
        "revoked" => ErrorCode::Revoked,
        "expired" => ErrorCode::Expired,
        "conflict" => ErrorCode::Conflict,
        "invalid" => ErrorCode::Invalid,
        _ => ErrorCode::Unavailable,
    };
    CtoxError::new(code, "Core capability authority denied")
}

/// Only catalog primitives, including the catalog's explicitly named native
/// aliases. App-specific legacy namespaces are never admitted through this host.
fn catalog_function(name: &str, function: &str) -> bool {
    let full = format!("{name}.{function}");
    let namespaces = [
        "files",
        "file",
        "blob",
        "artifact.write",
        "net.http",
        "web.fetch",
        "http.fetch",
        "browser",
        "process.run",
        "mail",
        "model",
        "agent.task",
        "agent.workspace",
        "agent.turn",
        "agent.coding",
        "agent.queue",
        "agent.command",
        "schedule",
        "secrets",
        "directory",
        "audit",
        "config",
        "doc",
        "office",
        "speech",
        "dataframe",
        "knowledge.query",
        "net.ingest",
        "policy",
        "lock",
    ];
    if name == "agent.task"
        && !matches!(
            function,
            "enqueue"
                | "get"
                | "find"
                | "update"
                | "cancel"
                | "set_metadata"
                | "workspace.read"
                | "workspace_file"
        )
    {
        return false; // Legacy action aliases are not the generic task primitive.
    }
    namespaces
        .iter()
        .any(|prefix| full == *prefix || full.starts_with(&format!("{prefix}.")))
}

pub struct CoreCapabilityHost<'a> {
    authority: &'a dyn CoreAuthority,
    binding: &'a Value,
    expected: CapCallContext,
}
impl<'a> CoreCapabilityHost<'a> {
    /// binding is the Core-retained invocation, never the decoded cap.invoke.
    pub fn for_bound_call(authority: &'a dyn CoreAuthority, binding: &'a Value) -> Result<Self> {
        authority.admit(&binding["manifest"], binding)?;
        let expected = authority.capability_context(binding, &capability_call_id(binding)?)?;
        Ok(Self {
            authority,
            binding,
            expected,
        })
    }
    fn check(&self, ctx: &CapCallContext) -> std::result::Result<(), CtoxError> {
        if ctx != &self.expected {
            return Err(CtoxError::new(
                ErrorCode::Revoked,
                "Core invocation identity differs",
            ));
        }
        self.authority
            .admit(&self.binding["manifest"], self.binding)
            .map_err(denied)?;
        let current = self
            .authority
            .capability_context(self.binding, &self.expected.call_id)
            .map_err(denied)?;
        if current != self.expected {
            return Err(CtoxError::new(
                ErrorCode::Revoked,
                "Core invocation changed",
            ));
        }
        if ctx.fence.expires_at_core_ms <= self.authority.now_ms() {
            return Err(CtoxError::new(
                ErrorCode::Expired,
                "Core executor lease expired",
            ));
        }
        Ok(())
    }
}
impl CapabilityHost for CoreCapabilityHost<'_> {
    fn authorize(
        &self,
        ctx: &CapCallContext,
        name: &str,
        function: &str,
        args: &Value,
    ) -> std::result::Result<CapabilityGrant, CtoxError> {
        self.check(ctx)?;
        if !catalog_function(name, function) {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "Capability is outside the OS catalog",
            ));
        }
        let full = format!("{name}.{function}");
        if name == "agent.task" && function == "enqueue" {
            let request = crate::core_host::capability_request(args);
            for key in ["on_complete", "on_failed"] {
                if let Some(target) = request.get(key) {
                    if target
                        .as_str()
                        .is_none_or(|t| self.binding["manifest"]["handlers"].get(t).is_none())
                    {
                        return Err(CtoxError::new(
                            ErrorCode::Forbidden,
                            "Completion must name an own-app command type",
                        ));
                    }
                }
            }
        }
        if !self.authority.capability_available(&full) {
            return Err(CtoxError::new(
                ErrorCode::Unavailable,
                "Native catalog function requires update",
            ));
        }
        let manifest = &self.binding["manifest"];
        let declared = manifest["capabilities_required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        let approved = self
            .authority
            .grants(&ctx.app_id, &ctx.app_ver)
            .map_err(denied)?;
        if !crate::declared(declared, &full)
            || !crate::declared(approved.iter().map(String::as_str), &full)
        {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "Capability is not declared and approved",
            ));
        }
        self.authority.authorize(&self.expected.app_id, &self.binding["command"]["actor"],
            &json!({"operation":"capability","capability":name,"function":function,"args":args})).map_err(denied)?;
        Ok(CapabilityGrant {
            app_id: self.expected.app_id.clone(),
            app_ver: self.expected.app_ver.clone(),
            actor: self.expected.actor.clone(),
            command_id: self.expected.command_id.clone(),
            capabilities_required: BTreeSet::from([name.to_owned()]),
            allowed_roles: BTreeSet::from([self.expected.actor.role.clone()]),
        })
    }
    fn now_ms(&self) -> u64 {
        self.authority.now_ms()
    }
    fn with_fence(
        &self,
        ctx: &CapCallContext,
        name: &str,
        function: &str,
        args: &Value,
        effect: &mut dyn FnMut() -> std::result::Result<Value, CtoxError>,
    ) -> std::result::Result<Value, CtoxError> {
        self.check(ctx)?;
        let mut entered = false;
        let result = self.authority.with_fence(self.binding, &mut || {
            self.authorize(ctx, name, function, args)
                .map_err(anyhow::Error::new)?;
            if entered {
                bail!("conflict: capability callback already entered");
            }
            entered = true;
            effect().map_err(anyhow::Error::new)
        });
        match result {
            Ok(value) if entered => Ok(value),
            Ok(_) => Err(CtoxError::new(
                ErrorCode::Unavailable,
                "Core fence omitted the effect",
            )),
            Err(error) => {
                let mut error = denied(error);
                // A failed guard after entering may have committed an effect.
                error.uncertain |= entered;
                Err(error)
            }
        }
    }
    fn audit(&self, entry: &AuditEntry) -> std::result::Result<(), CtoxError> {
        // AuditEntry contains identities/status only, never capability payloads.
        self.authority
            .log(self.binding, &json!(["info", "Capability audit", entry]))
            .map(|_| ())
            .map_err(denied)
    }
}

/// The Core backend is supplied explicitly; there is no mock/default backend.
pub struct CoreRegistryRouter<B> {
    pub registry: CapabilityRegistry,
    pub backend: B,
    pub authority: Arc<dyn CoreAuthority>,
}
impl<B: CapabilityBackend + Send + Sync + 'static> CapabilityRouter for CoreRegistryRouter<B> {
    fn prepare(&self, function: &str, request: &Value) -> Result<(String, String, Value)> {
        prepare_registry(&self.registry, function, request)
    }
    fn exchange(&self, _: &[u8]) -> Result<Vec<u8>> {
        bail!("forbidden: native capability requires retained Core invocation")
    }
    fn exchange_bound(&self, binding: &Value, bytes: &[u8]) -> Result<Vec<u8>> {
        let host = CoreCapabilityHost::for_bound_call(self.authority.as_ref(), binding)?;
        Ok(self
            .registry
            .handle(&host, &self.backend, CapMessage::decode(bytes)?)?
            .encode()?)
    }
}
