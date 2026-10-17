use super::caps::{self, NativeCall, NativeRoute};
use super::policy::{CapCallContext, CapabilityHost};
use super::protocol::{self, CapMessage, CapOutcome, CtoxError, ErrorCode, MAX_MESSAGE_BYTES};
use super::schema::{FunctionSchema, PayloadShape};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub trait CapabilityBackend {
    /// Called only inside the Core host's held fence callback.
    fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStage {
    Requested,
    Completed,
    Rejected,
    Uncertain,
}
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub app_id: String,
    pub app_ver: String,
    pub actor_id: String,
    /// Requested entries contain claims until the Core binding is verified.
    pub actor_verified: bool,
    pub command_id: String,
    pub call_id: String,
    pub capability: String,
    pub function: String,
    pub fence: u64,
    pub at_core_ms: u64,
    pub stage: AuditStage,
    pub code: Option<ErrorCode>,
}

#[derive(Clone)]
struct ActionAdapter {
    capability: &'static str,
    function: &'static str,
    actions: &'static [&'static str],
    routes: Vec<NativeRoute>,
}
impl ActionAdapter {
    fn schema(&self) -> FunctionSchema {
        FunctionSchema {
            payload: PayloadShape::Object,
            action: Some(self.actions),
        }
    }
    fn call(&self, args: &Value) -> Result<NativeCall, CtoxError> {
        self.schema().validate(args)?;
        let action = args["action"].as_str().expect("validated action");
        let route = self
            .routes
            .iter()
            .find(|route| route.function == action)
            .ok_or_else(|| CtoxError::invalid("$.args.action"))?;
        let mut native_args = args.clone();
        native_args.as_object_mut().unwrap().remove("action");
        route.call(&native_args)
    }
}

#[derive(Default)]
pub struct CapabilityRegistry {
    functions: BTreeMap<(String, String), NativeRoute>,
    actions: BTreeMap<(String, String), ActionAdapter>,
}
impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cap1() -> Self {
        let mut registry = Self::new();
        for route in caps::all_routes() {
            registry
                .register(*route)
                .expect("duplicate built-in capability function");
        }
        for (capability, function, actions) in [
            (
                "agent.queue",
                "control",
                &[
                    "abort_turn",
                    "block",
                    "capacity",
                    "pause",
                    "release",
                    "retry",
                ],
            ),
        ] {
            let adapter = ActionAdapter {
                capability,
                function,
                actions,
                routes: caps::all_routes()
                    .filter(|r| r.capability == capability && actions.contains(&r.function))
                    .copied()
                    .collect(),
            };
            registry
                .actions
                .insert((capability.into(), function.into()), adapter);
        }
        registry
    }
    /// Registration is Core-owned, never a message from the app runtime.
    pub fn register(&mut self, route: NativeRoute) -> Result<(), CtoxError> {
        let key = (route.capability.into(), route.function.into());
        if self.functions.contains_key(&key) || self.actions.contains_key(&key) {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "capability function already registered",
            ));
        }
        self.functions.insert(key, route);
        Ok(())
    }
    pub fn descriptors(&self) -> Value {
        Value::Array(self.functions.values().map(|route| json!({
            "name":route.capability, "fn":route.function,
            "input":route.schema.json(),
            // Domain result fields remain owned/validated by the existing handler.
            "output":{"type":"object"}, "max_output_bytes":MAX_MESSAGE_BYTES
        })).chain(self.actions.values().map(|adapter|json!({
            "name":adapter.capability,"fn":adapter.function,"input":adapter.schema().json(),
            "output":{"type":"object"},"max_output_bytes":MAX_MESSAGE_BYTES
        }))).collect())
    }
    pub fn invoke(
        &self,
        host: &dyn CapabilityHost,
        backend: &dyn CapabilityBackend,
        ctx: &CapCallContext,
        name: &str,
        function: &str,
        args: Value,
    ) -> Result<Value, CtoxError> {
        protocol::validate_context(ctx)?;
        protocol::identifier(name, "$.name")?;
        protocol::identifier(function, "$.fn")?;
        let mut audit = AuditEntry {
            app_id: ctx.app_id.clone(),
            app_ver: ctx.app_ver.clone(),
            actor_id: ctx.actor.id.clone(),
            actor_verified: false,
            command_id: ctx.command_id.clone(),
            call_id: ctx.call_id.clone(),
            capability: name.into(),
            function: function.into(),
            fence: ctx.fence.fence,
            at_core_ms: host.now_ms(),
            stage: AuditStage::Requested,
            code: None,
        };
        host.audit(&audit)?;
        let mut entered = false;
        let result = (|| {
            let size = serde_json::to_vec(&args)
                .map_err(|_| CtoxError::invalid("$.args"))?
                .len();
            if size > MAX_MESSAGE_BYTES {
                return Err(CtoxError::new(
                    ErrorCode::Quota,
                    "capability input exceeds 1 MiB",
                ));
            }
            let key = (name.into(), function.into());
            let call = if let Some(route) = self.functions.get(&key) {
                route.call(&args)?
            } else if let Some(adapter) = self.actions.get(&key) {
                adapter.call(&args)?
            } else {
                return Err(CtoxError::new(
                    ErrorCode::NotFound,
                    "unknown capability function",
                ));
            };
            let target_function = call.route().function;
            host.authorize(ctx, name, target_function, &args)?
                .check(ctx, name)?;
            audit.actor_verified = true;
            if ctx.fence.expires_at_core_ms <= host.now_ms() {
                return Err(CtoxError::new(ErrorCode::Expired, "executor lease expired"));
            }
            let mut effect = || {
                if entered {
                    return Err(CtoxError::new(
                        ErrorCode::Conflict,
                        "effect callback already entered",
                    ));
                }
                // Recheck immediately at the effect point; the host must also retain
                // its actual current generation/fence and policy guard across execute.
                host.authorize(ctx, name, target_function, &args)?
                    .check(ctx, name)?;
                if ctx.fence.expires_at_core_ms <= host.now_ms() {
                    return Err(CtoxError::new(ErrorCode::Expired, "executor lease expired"));
                }
                entered = true;
                backend.execute(ctx, &call)
            };
            let output = host.with_fence(ctx, name, target_function, &args, &mut effect)?;
            if !entered {
                return Err(CtoxError::new(
                    ErrorCode::Unavailable,
                    "Core fence did not execute callback",
                ));
            }
            if !output.is_object()
                || serde_json::to_vec(&output)
                    .map_err(|_| CtoxError::uncertain())?
                    .len()
                    > MAX_MESSAGE_BYTES
            {
                return Err(CtoxError::uncertain());
            }
            Ok(output)
        })();
        audit.at_core_ms = host.now_ms();
        audit.stage = match &result {
            Ok(_) => AuditStage::Completed,
            Err(error) if error.uncertain => AuditStage::Uncertain,
            Err(_) => AuditStage::Rejected,
        };
        audit.code = result.as_ref().err().map(|e| e.code);
        if host.audit(&audit).is_err() {
            return Err(if entered {
                CtoxError::uncertain()
            } else {
                CtoxError::new(ErrorCode::Unavailable, "capability audit unavailable")
            });
        }
        result
    }

    /// Harness calls this only on the LocalTransport bound to the admitted app
    /// process; authorize/with_fence must reject a forged ctx from that process.
    pub fn handle(
        &self,
        host: &dyn CapabilityHost,
        backend: &dyn CapabilityBackend,
        message: CapMessage,
    ) -> Result<CapMessage, CtoxError> {
        let CapMessage::Invoke {
            request_id,
            ctx,
            name,
            function,
            args,
        } = message
        else {
            return Err(CtoxError::invalid("$.type"));
        };
        if request_id != ctx.call_id {
            return Err(CtoxError::invalid("$.ctx.call_id"));
        }
        let outcome = match self.invoke(host, backend, &ctx, &name, &function, args) {
            Ok(value) => CapOutcome::Ok { value },
            Err(error) => CapOutcome::Error { error },
        };
        let reply = CapMessage::Result {
            request_id: request_id.clone(),
            outcome,
        };
        if reply.encode().is_err() {
            return Ok(CapMessage::Result {
                request_id,
                outcome: CapOutcome::Error {
                    error: CtoxError::uncertain(),
                },
            });
        }
        Ok(reply)
    }
}
