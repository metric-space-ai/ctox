use super::{CtoxError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub id: String,
    pub role: String,
}

/// Descriptive wire fence, checked against the actual current Core lease by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorFence {
    pub run_id: String,
    pub participant: String,
    pub fence: u64,
    pub expires_at_core_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapCallContext {
    pub app_id: String,
    pub app_ver: String,
    pub actor: Actor,
    pub command_id: String,
    /// Unique within command_id. Same call_id is a retry, never a new effect.
    pub call_id: String,
    pub fence: ExecutorFence,
}

/// Must be loaded by Core from the admitted app version and authenticated actor.
/// Never deserialize this grant from the app runtime.
#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    pub app_id: String,
    pub app_ver: String,
    pub actor: Actor,
    pub capabilities_required: BTreeSet<String>,
    pub allowed_roles: BTreeSet<String>,
    pub command_id: String,
}

impl CapabilityGrant {
    pub(crate) fn check(&self, ctx: &CapCallContext, capability: &str) -> Result<(), CtoxError> {
        if self.app_id != ctx.app_id
            || self.app_ver != ctx.app_ver
            || self.actor != ctx.actor
            || self.command_id != ctx.command_id
        {
            return Err(CtoxError::new(
                ErrorCode::Revoked,
                "runtime binding changed",
            ));
        }
        if !self.capabilities_required.contains(capability)
            || !self.allowed_roles.contains(&ctx.actor.role)
        {
            return Err(CtoxError::new(
                ErrorCode::Forbidden,
                "capability is not granted",
            ));
        }
        Ok(())
    }
}

/// Only the trusted Core runtime implements this interface. There is deliberately
/// no permissive default implementation or constructor from wire claims.
pub trait CapabilityHost {
    /// Resolve current app version, parent command rights (§16.20), authenticated
    /// actor and the per-function native permission. Not just a role-string lookup.
    /// Inspect args for record/owner scope and native subcommands (in particular,
    /// knowledge.command can write; the compatibility name query is not a read grant).
    fn authorize(
        &self,
        ctx: &CapCallContext,
        capability: &str,
        function: &str,
        args: &Value,
    ) -> Result<CapabilityGrant, CtoxError>;
    fn now_ms(&self) -> u64;
    /// Revalidate current lease/fence AND policy, holding authority against
    /// revoke/reclaim for the complete synchronous callback. A timestamp or a
    /// prior check alone is insufficient. Never call effect after failure.
    /// No async callback or authority value may escape this scope.
    fn with_fence(
        &self,
        ctx: &CapCallContext,
        capability: &str,
        function: &str,
        args: &Value,
        effect: &mut dyn FnMut() -> Result<Value, CtoxError>,
    ) -> Result<Value, CtoxError>;
    /// Persist to the Core audit sink. No input, output or credential values.
    /// Failure before dispatch prevents the effect; failure after dispatch yields
    /// unavailable/uncertain and must not cause automatic non-idempotent replay.
    fn audit(&self, entry: &super::registry::AuditEntry) -> Result<(), CtoxError>;
}
