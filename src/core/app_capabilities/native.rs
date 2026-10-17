//! CAP-1 reuses the existing admitted native command pipeline. No free-form
//! command name, module, actor or client_context is accepted from args.
//! Existing control claims, policy, domain receipts and queue/writebacks remain
//! intact. The app runtime must supply a real Core CapabilityHost; no wire field
//! mints an authenticated caller.
use super::{caps::NativeCall, CapCallContext, CapabilityBackend, CtoxError};
use crate::business_os::store::{accept_rxdb_business_command_with_origin, CommandOrigin};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct NativeCommandBackend<'a> {
    root: &'a Path,
}
impl<'a> NativeCommandBackend<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self { root }
    }
}
/// Stable retry identity. Changing payload for an existing call_id is rejected
/// by the existing native immutable-intent claim, rather than minting a new ID.
pub(crate) fn command_document(ctx: &CapCallContext, call: &NativeCall) -> Value {
    let identity = serde_json::to_vec(&(&ctx.app_id, &ctx.app_ver, &ctx.command_id, &ctx.call_id))
        .expect("string tuple serializes");
    let id = format!("cap:{:x}", Sha256::digest(identity));
    json!({
        "id":id, "module":ctx.app_id, "type":call.route().command_type(),
        "record_id":call.record_id(), "payload":call.payload(),
        "client_context":{
            "actor":{"id":ctx.actor.id,"role":ctx.actor.role},
            "owner_user_id":ctx.actor.id,
            "app_version":ctx.app_ver,
            "parent_command_id":ctx.command_id
        }
    })
}
impl CapabilityBackend for NativeCommandBackend<'_> {
    fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        // This boundary is entered by the Core-owned held fence only, after its
        // app/actor/command permission binding; child process claims never enter
        // as TrustedLocal without that host's validation.
        accept_rxdb_business_command_with_origin(
            self.root,
            command_document(ctx, call),
            CommandOrigin::TrustedLocal,
        )
        // Native error strings can contain paths or upstream detail. They are
        // not relayed to untrusted handlers or misclassified by text matching.
        .map_err(|_| CtoxError::uncertain())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_capabilities::{caps, Actor, ExecutorFence};
    #[test]
    fn app_capabilities_native_envelope_stamps_only_core_binding_and_fixed_route() {
        let ctx = CapCallContext {
            app_id: "runtime-app".into(),
            app_ver: "1.2.3".into(),
            actor: Actor {
                id: "authenticated-actor".into(),
                role: "staff".into(),
            },
            command_id: "parent-command".into(),
            call_id: "native-call".into(),
            fence: ExecutorFence {
                run_id: "actual-run".into(),
                participant: "core".into(),
                fence: 9,
                expires_at_core_ms: 30000,
            },
        };
        let route = caps::all_routes()
            .find(|r| r.command_type() == "ctox.queue.pause")
            .unwrap();
        let first = route
            .call(&json!({"payload":{"paused":true},"record_id":"record"}))
            .unwrap();
        let document = command_document(&ctx, &first);
        assert_eq!(document["module"], ctx.app_id);
        assert_eq!(document["client_context"]["actor"]["id"], ctx.actor.id);
        assert_eq!(document["client_context"]["actor"]["role"], ctx.actor.role);
        assert_eq!(
            document["client_context"]["parent_command_id"],
            ctx.command_id
        );
        assert_eq!(document["type"], "ctox.queue.pause");
        assert_eq!(document["payload"], json!({"paused":true}));
        assert!(document["client_context"]
            .get("business_os_capability")
            .is_none());
        let changed = route.call(&json!({"payload":{"paused":false}})).unwrap();
        assert_eq!(
            document["id"],
            command_document(&ctx, &changed)["id"],
            "retry changes must conflict with native immutable intent"
        );
        let mut next = ctx.clone();
        next.call_id = "other-call".into();
        assert_ne!(document["id"], command_document(&next, &first)["id"]);
        for field in ["actor", "module", "type", "id", "client_context"] {
            let mut forged = json!({"payload":{}});
            forged[field] = json!("forged");
            assert!(route.call(&forged).is_err());
        }
    }
}
