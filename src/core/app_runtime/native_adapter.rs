//! Compiled by the daemon, not the isolated runtime crate. Consume the real
//! CAP-1/2 catalog/backend without inventing routes or a second native executor.
use crate::app_capabilities as native;
use anyhow::{bail, Result};
use ctox_app_runtime::{
    cap_contract as contract,
    capability_host::CoreCapabilityHost,
    core_host::{CapabilityRouter, CoreAuthority},
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

fn convert<A: Serialize, B: DeserializeOwned>(value: &A) -> Result<B> {
    Ok(serde_json::from_value(serde_json::to_value(value)?)?)
}
fn to_native(error: contract::CtoxError) -> native::CtoxError {
    convert(&error).unwrap_or_else(|_| native::CtoxError::uncertain())
}
fn to_contract(error: native::CtoxError) -> contract::CtoxError {
    convert(&error).unwrap_or_else(|_| {
        let mut error = contract::CtoxError::new(
            contract::ErrorCode::Unavailable,
            "native outcome requires reconciliation",
        );
        error.uncertain = true;
        error
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;
    use contract::{
        protocol::{CapMessage, CapOutcome},
        Actor, CapCallContext, ExecutorFence,
    };
    use serde_json::json;
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    struct Authority {
        live: AtomicBool,
    }
    fn context(call: &str) -> CapCallContext {
        CapCallContext {
            app_id: "app".into(),
            app_ver: "v1".into(),
            actor: Actor {
                id: "core-user".into(),
                role: "writer".into(),
            },
            command_id: "parent".into(),
            call_id: call.into(),
            fence: ExecutorFence {
                run_id: "run".into(),
                participant: "core".into(),
                fence: 7,
                expires_at_core_ms: 1000,
            },
        }
    }
    impl CoreAuthority for Authority {
        fn admit(&self, _: &Value, b: &Value) -> Result<()> {
            ensure!(
                b["command"]["actor"]["id"] == "core-user",
                "forbidden: Core actor"
            );
            Ok(())
        }
        fn authorize(&self, _: &str, actor: &Value, _: &Value) -> Result<()> {
            ensure!(actor["id"] == "core-user", "forbidden: Core actor");
            Ok(())
        }
        fn exported(&self, _: &str, _: &str, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn capability_available(&self, name: &str) -> bool {
            name == "doc.html.query"
        }
        fn with_fence(
            &self,
            _: &Value,
            effect: &mut dyn FnMut() -> Result<Value>,
        ) -> Result<Value> {
            ensure!(
                self.live.load(Ordering::SeqCst),
                "revoked: retained Core fence"
            );
            effect()
        }
        fn capability_context(&self, _: &Value, call: &str) -> Result<CapCallContext> {
            Ok(context(call))
        }
        fn grants(&self, _: &str, _: &str) -> Result<BTreeSet<String>> {
            Ok(BTreeSet::from(["doc.html".into()]))
        }
        fn event_binding(&self, _: &str, _: &str, _: &Value) -> Result<Value> {
            bail!("forbidden: not a fixture event")
        }
        fn now_ms(&self) -> u64 {
            1
        }
        fn log(&self, _: &Value, _: &Value) -> Result<Value> {
            Ok(Value::Null)
        }
    }
    struct Blobs(AtomicUsize);
    const HTML: &[u8] = b"<h1>actual native DOM</h1>";
    impl native::cap2::blob::BlobAccess for Blobs {
        fn read(
            &self,
            _: &native::CapCallContext,
            _: &native::cap2::blob::BlobRef,
        ) -> Result<Vec<u8>, native::CtoxError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HTML.to_vec())
        }
        fn publish(
            &self,
            _: &native::CapCallContext,
            _: &[u8],
            _: &str,
        ) -> Result<native::cap2::blob::BlobRef, native::CtoxError> {
            Err(native::CtoxError::new(
                native::ErrorCode::Forbidden,
                "read-only fixture",
            ))
        }
    }
    #[test]
    fn native_registry_bridge_preserves_core_fence_and_executes_real_cap2() {
        use sha2::Digest;
        let authority = Arc::new(Authority {
            live: AtomicBool::new(true),
        });
        let blobs = Arc::new(Blobs(AtomicUsize::new(0)));
        let router = NativeCapabilityRouter {
            root: PathBuf::from("unused-fixture-root"),
            authority: authority.clone(),
            blobs: blobs.clone(),
        };
        let binding = json!({"command":{"id":"parent","app_id":"app","app_ver":"v1","actor":{"id":"core-user","role":"writer"}},
            "invocation_id":"core-retained","callback_id":1,"manifest":{"app_id":"app","app_ver":"v1","handlers":{},"capabilities_required":["doc.html"]}});
        let (name,function,args)=router.prepare("doc.html",&json!({"blob":{"sha256":format!("{:x}",sha2::Sha256::digest(HTML)),"size":HTML.len(),"transfer_id":"fixture"},"selector":"h1"})).unwrap();
        let call = ctox_app_runtime::core_host::capability_call_id(&binding).unwrap();
        let bytes = CapMessage::Invoke {
            request_id: call.clone(),
            ctx: context(&call),
            name,
            function,
            args,
        }
        .encode()
        .unwrap();
        let result = CapMessage::decode(&router.exchange_bound(&binding, &bytes).unwrap()).unwrap();
        assert!(
            matches!(result,CapMessage::Result{outcome:CapOutcome::Ok{value},..} if value["matches"]==json!(["actual native DOM"]))
        );
        authority.live.store(false, Ordering::SeqCst);
        let result = CapMessage::decode(&router.exchange_bound(&binding, &bytes).unwrap()).unwrap();
        assert!(
            matches!(result,CapMessage::Result{outcome:CapOutcome::Error{error},..} if error.code==contract::ErrorCode::Revoked)
        );
        assert_eq!(blobs.0.load(Ordering::SeqCst), 1);
        assert!(router.exchange(&bytes).is_err());
    }
}
struct Host<'a>(CoreCapabilityHost<'a>);
impl native::CapabilityHost for Host<'_> {
    fn authorize(
        &self,
        ctx: &native::CapCallContext,
        name: &str,
        function: &str,
        args: &Value,
    ) -> Result<native::CapabilityGrant, native::CtoxError> {
        use contract::CapabilityHost;
        let ctx = convert(ctx).map_err(|_| native::CtoxError::uncertain())?;
        let grant = self
            .0
            .authorize(&ctx, name, function, args)
            .map_err(to_native)?;
        Ok(native::CapabilityGrant {
            app_id: grant.app_id,
            app_ver: grant.app_ver,
            actor: convert(&grant.actor).map_err(|_| native::CtoxError::uncertain())?,
            command_id: grant.command_id,
            capabilities_required: grant.capabilities_required,
            allowed_roles: grant.allowed_roles,
        })
    }
    fn now_ms(&self) -> u64 {
        contract::CapabilityHost::now_ms(&self.0)
    }
    fn with_fence(
        &self,
        ctx: &native::CapCallContext,
        name: &str,
        function: &str,
        args: &Value,
        effect: &mut dyn FnMut() -> Result<Value, native::CtoxError>,
    ) -> Result<Value, native::CtoxError> {
        let ctx = convert(ctx).map_err(|_| native::CtoxError::uncertain())?;
        contract::CapabilityHost::with_fence(&self.0, &ctx, name, function, args, &mut || {
            effect().map_err(to_contract)
        })
        .map_err(to_native)
    }
    fn audit(&self, entry: &native::registry::AuditEntry) -> Result<(), native::CtoxError> {
        let stage = match entry.stage {
            native::registry::AuditStage::Requested => contract::registry::AuditStage::Requested,
            native::registry::AuditStage::Completed => contract::registry::AuditStage::Completed,
            native::registry::AuditStage::Rejected => contract::registry::AuditStage::Rejected,
            native::registry::AuditStage::Uncertain => contract::registry::AuditStage::Uncertain,
        };
        let audit = contract::registry::AuditEntry {
            app_id: entry.app_id.clone(),
            app_ver: entry.app_ver.clone(),
            actor_id: entry.actor_id.clone(),
            actor_verified: entry.actor_verified,
            command_id: entry.command_id.clone(),
            call_id: entry.call_id.clone(),
            capability: entry.capability.clone(),
            function: entry.function.clone(),
            fence: entry.fence,
            at_core_ms: entry.at_core_ms,
            stage,
            code: entry
                .code
                .as_ref()
                .map(convert)
                .transpose()
                .map_err(|_| native::CtoxError::uncertain())?,
        };
        contract::CapabilityHost::audit(&self.0, &audit).map_err(to_native)
    }
}

/// BlobAccess must come from the live Transfer owner. A CAS hash alone is never
/// a read grant; this adapter provides no filesystem or permissive substitute.
pub(crate) struct NativeCapabilityRouter {
    pub root: PathBuf,
    pub authority: Arc<dyn CoreAuthority>,
    pub blobs: Arc<dyn native::cap2::blob::BlobAccess + Send + Sync>,
}
impl CapabilityRouter for NativeCapabilityRouter {
    fn prepare(&self, name: &str, request: &Value) -> Result<(String, String, Value)> {
        let canonical = native::cap2::ROUTES
            .iter()
            .chain(native::cap2::mail::ROUTES)
            .find(|route| native::cap2::call_path(route) == name)
            .map(|route| format!("{}.{}", route.capability, route.function));
        ctox_app_runtime::core_host::prepare_descriptors(
            &native::cap2::registry().descriptors(),
            canonical.as_deref().unwrap_or(name),
            request,
        )
    }
    fn exchange(&self, _: &[u8]) -> Result<Vec<u8>> {
        bail!("forbidden: native registry requires retained Core invocation")
    }
    fn exchange_bound(&self, binding: &Value, bytes: &[u8]) -> Result<Vec<u8>> {
        let host = Host(CoreCapabilityHost::for_bound_call(
            self.authority.as_ref(),
            binding,
        )?);
        let backend = native::cap2::Cap2Backend {
            root: &self.root,
            blobs: self.blobs.as_ref(),
        };
        // prepare already emitted the closed envelope. Calling cap2::handle
        // here would wrap CAP-2 args a second time; invoke the real registry.
        Ok(native::cap2::registry()
            .handle(
                &host,
                &backend,
                native::protocol::CapMessage::decode(bytes)?,
            )?
            .encode()?)
    }
}
