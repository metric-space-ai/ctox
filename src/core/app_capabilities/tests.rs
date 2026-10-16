use super::caps::NativeCall;
use super::protocol::{CapMessage, CapOutcome, MAX_MESSAGE_BYTES};
use super::registry::{AuditEntry, AuditStage};
use super::*;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

fn context() -> CapCallContext {
    CapCallContext {
        app_id: "test-app".into(),
        app_ver: "1".into(),
        actor: Actor {
            id: "actor".into(),
            role: "owner".into(),
        },
        command_id: "command".into(),
        call_id: "call".into(),
        fence: ExecutorFence {
            run_id: "run".into(),
            participant: "core".into(),
            fence: 7,
            expires_at_core_ms: 30000,
        },
    }
}
struct Host {
    ctx: CapCallContext,
    manifest: BTreeSet<String>,
    roles: BTreeSet<String>,
    revoked: Cell<bool>,
    expire_on_callback: Cell<bool>,
    revoke_on_callback: Cell<bool>,
    audits: RefCell<Vec<AuditEntry>>,
    fail_audit: Cell<Option<usize>>,
    guard: Cell<bool>,
}
impl Host {
    fn new() -> Self {
        Self {
            ctx: context(),
            manifest: caps::all_routes().map(|r| r.capability.into()).collect(),
            roles: ["owner".into()].into_iter().collect(),
            revoked: Cell::new(false),
            expire_on_callback: Cell::new(false),
            revoke_on_callback: Cell::new(false),
            audits: RefCell::new(Vec::new()),
            fail_audit: Cell::new(None),
            guard: Cell::new(false),
        }
    }
}
impl CapabilityHost for Host {
    fn authorize(
        &self,
        ctx: &CapCallContext,
        _: &str,
        _: &str,
        _: &Value,
    ) -> Result<CapabilityGrant, CtoxError> {
        if self.revoked.get() || ctx != &self.ctx {
            return Err(CtoxError::new(ErrorCode::Revoked, "Core binding changed"));
        }
        Ok(CapabilityGrant {
            app_id: self.ctx.app_id.clone(),
            app_ver: self.ctx.app_ver.clone(),
            actor: self.ctx.actor.clone(),
            command_id: self.ctx.command_id.clone(),
            capabilities_required: self.manifest.clone(),
            allowed_roles: self.roles.clone(),
        })
    }
    fn now_ms(&self) -> u64 {
        if self.expire_on_callback.get() && self.guard.get() {
            30000
        } else {
            1000
        }
    }
    fn with_fence(
        &self,
        ctx: &CapCallContext,
        _: &str,
        _: &str,
        _: &Value,
        effect: &mut dyn FnMut() -> Result<Value, CtoxError>,
    ) -> Result<Value, CtoxError> {
        if self.revoked.get() || ctx.fence != self.ctx.fence {
            return Err(CtoxError::new(ErrorCode::Revoked, "executor fence changed"));
        }
        self.guard.set(true);
        if self.revoke_on_callback.get() {
            self.revoked.set(true);
        }
        let result = effect();
        self.guard.set(false);
        result
    }
    fn audit(&self, entry: &AuditEntry) -> Result<(), CtoxError> {
        if self.fail_audit.get() == Some(self.audits.borrow().len()) {
            return Err(CtoxError::new(ErrorCode::Unavailable, "audit unavailable"));
        }
        self.audits.borrow_mut().push(entry.clone());
        Ok(())
    }
}
struct Backend<'a> {
    host: &'a Host,
    calls: RefCell<Vec<String>>,
    output: Value,
}
impl<'a> Backend<'a> {
    fn new(host: &'a Host) -> Self {
        Self {
            host,
            calls: RefCell::new(Vec::new()),
            output: json!({"native_receipt":"receipt"}),
        }
    }
}
impl CapabilityBackend for Backend<'_> {
    fn execute(&self, _: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        assert!(self.host.guard.get(), "effect outside held Core fence");
        assert_eq!(call.payload()["probe"], "existing-handler");
        assert_eq!(call.record_id(), Some("record"));
        self.calls
            .borrow_mut()
            .push(call.route().command_type().into());
        Ok(self.output.clone())
    }
}
fn args(name: &str) -> Value {
    if name == "knowledge" {
        json!({"payload":{"args":["tables"]}})
    } else {
        json!({"payload":{"probe":"existing-handler"},"record_id":"record"})
    }
}
fn exercise(name: &str, function: &str, command_type: &str) {
    let host = Host::new();
    struct RouteBackend<'a> {
        host: &'a Host,
        expected: &'a str,
        called: Cell<bool>,
    }
    impl CapabilityBackend for RouteBackend<'_> {
        fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
            assert!(self.host.guard.get());
            assert_eq!(ctx, &self.host.ctx);
            assert_eq!(call.route().command_type(), self.expected);
            self.called.set(true);
            Ok(json!({"receipt":"existing-native-path"}))
        }
    }
    let backend = RouteBackend {
        host: &host,
        expected: command_type,
        called: Cell::new(false),
    };
    let result = CapabilityRegistry::cap1()
        .invoke(&host, &backend, &host.ctx, name, function, args(name))
        .unwrap();
    assert_eq!(result["receipt"], "existing-native-path");
    assert!(backend.called.get());
    let audit = host.audits.borrow();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].stage, AuditStage::Requested);
    assert_eq!(audit[1].stage, AuditStage::Completed);
    assert!(!audit[0].actor_verified);
    assert!(audit[1].actor_verified);
}
macro_rules! route_test {
    ($test:ident,$cap:literal,$func:literal,$cmd:literal) => {
        #[test]
        fn $test() {
            exercise($cap, $func, $cmd);
        }
    };
}
route_test!(
    ticket_local_create,
    "ctox.ticket",
    "local.create",
    "ctox.ticket.local.create"
);
route_test!(
    ticket_approve,
    "ctox.ticket",
    "approve",
    "ctox.ticket.approve"
);
route_test!(
    ticket_execute,
    "ctox.ticket",
    "execute",
    "ctox.ticket.execute"
);
route_test!(ticket_close, "ctox.ticket", "close", "ctox.ticket.close");
route_test!(ticket_verify, "ctox.ticket", "verify", "ctox.ticket.verify");
route_test!(
    ticket_publish_clarification,
    "ctox.ticket",
    "publish_clarification",
    "ctox.ticket.publish_clarification"
);
route_test!(
    ticket_request_clarification,
    "ctox.ticket",
    "request_clarification",
    "ctox.ticket.request_clarification"
);
route_test!(
    ticket_resolve_clarification,
    "ctox.ticket",
    "resolve_clarification",
    "ctox.ticket.resolve_clarification"
);
route_test!(
    ticket_writeback_comment,
    "ctox.ticket",
    "writeback_comment",
    "ctox.ticket.writeback_comment"
);
route_test!(
    ticket_create_from_conversation,
    "ctox.ticket",
    "create_from_conversation",
    "support.ticket.create_from_conversation"
);
route_test!(
    queue_abort_turn,
    "agent.queue",
    "abort_turn",
    "ctox.queue.abort_turn"
);
route_test!(queue_block, "agent.queue", "block", "ctox.queue.block");
route_test!(
    queue_capacity,
    "agent.queue",
    "capacity",
    "ctox.queue.capacity"
);
route_test!(queue_pause, "agent.queue", "pause", "ctox.queue.pause");
route_test!(
    queue_release,
    "agent.queue",
    "release",
    "ctox.queue.release"
);
route_test!(queue_retry, "agent.queue", "retry", "ctox.queue.retry");
route_test!(
    queue_task_delete,
    "agent.queue",
    "task.delete",
    "ctox.task.delete"
);
route_test!(
    queue_task_update,
    "agent.queue",
    "task.update",
    "ctox.task.update"
);
route_test!(
    task_delegate,
    "agent.task",
    "delegate",
    "kundenpipeline.delegate"
);
route_test!(
    task_company_research,
    "agent.task",
    "company.research",
    "outbound.company.research"
);
route_test!(
    task_contact_research,
    "agent.task",
    "contact_research",
    "outbound.pipeline.contact_research"
);
route_test!(
    task_lead_qualification,
    "agent.task",
    "lead_qualification",
    "outbound.pipeline.lead_qualification"
);
route_test!(
    task_report_bug,
    "agent.task",
    "report_bug",
    "ctox.report.bug"
);
route_test!(task_request, "agent.task", "request", "threads.ai.request");
route_test!(
    task_approve,
    "agent.task",
    "approve",
    "threads.ctox_approval.approve"
);
route_test!(crew_assign, "agent.crew", "assign", "ctox.crew.assign");
route_test!(
    crew_learning_confirm,
    "agent.crew",
    "learning.confirm",
    "ctox.crew.learning.confirm"
);
route_test!(
    crew_learning_delete,
    "agent.crew",
    "learning.delete",
    "ctox.crew.learning.delete"
);
route_test!(
    crew_learning_update,
    "agent.crew",
    "learning.update",
    "ctox.crew.learning.update"
);
route_test!(
    crew_member_create,
    "agent.crew",
    "member.create",
    "ctox.crew.member.create"
);
route_test!(
    crew_member_update,
    "agent.crew",
    "member.update",
    "ctox.crew.member.update"
);
route_test!(
    crew_memory_update,
    "agent.crew",
    "memory.update",
    "ctox.crew.memory.update"
);
route_test!(
    turn_submit,
    "agent.turn",
    "submit",
    "ctox.workjet.project.supervisor.turn.submit"
);
route_test!(
    turn_watch,
    "agent.turn",
    "watch",
    "ctox.workjet.project.supervisor.turn.watch"
);
route_test!(
    turn_cancel,
    "agent.turn",
    "cancel",
    "ctox.workjet.project.supervisor.turn.cancel"
);
route_test!(
    turn_input,
    "agent.turn",
    "input",
    "ctox.workjet.project.supervisor.turn.input"
);
route_test!(
    turn_capabilities,
    "agent.turn",
    "capabilities",
    "ctox.workjet.project.supervisor.turn.capabilities"
);
route_test!(
    turn_history,
    "agent.turn",
    "history",
    "ctox.workjet.project.supervisor.turn.history"
);
route_test!(
    turn_route_read_v1,
    "agent.turn",
    "route.read.v1",
    "ctox.workjet.project.supervisor.route.read.v1"
);
route_test!(
    turn_route_read_v2,
    "agent.turn",
    "route.read.v2",
    "ctox.workjet.project.supervisor.route.read.v2"
);
route_test!(
    turn_route_capabilities_v1,
    "agent.turn",
    "route.capabilities.v1",
    "ctox.workjet.project.supervisor.route.capabilities.v1"
);
route_test!(
    turn_route_capabilities_v2,
    "agent.turn",
    "route.capabilities.v2",
    "ctox.workjet.project.supervisor.route.capabilities.v2"
);
route_test!(
    coding_models,
    "agent.coding",
    "models",
    "ctox.coding.models"
);
route_test!(coding_turn, "agent.coding", "turn", "ctox.coding.turn");
route_test!(schedule_upsert, "schedule", "upsert", "news.topic.schedule");
route_test!(knowledge_query, "knowledge", "query", "knowledge.command");

#[test]
fn app_capabilities_registry_is_exact_and_duplicate_registration_preserves_original() {
    let mut registry = CapabilityRegistry::cap1();
    assert_eq!(caps::all_routes().count(), 46);
    assert_eq!(registry.descriptors().as_array().unwrap().len(), 49);
    let route = *caps::all_routes().next().unwrap();
    assert_eq!(
        registry.register(route).unwrap_err().code,
        ErrorCode::Conflict
    );
    assert_eq!(registry.descriptors().as_array().unwrap().len(), 49);
}
#[test]
fn app_capabilities_manifest_and_role_denials_never_enter_effect() {
    for role_denial in [false, true] {
        let mut host = Host::new();
        if role_denial {
            host.roles.clear()
        } else {
            host.manifest.clear()
        }
        let backend = Backend::new(&host);
        let error = CapabilityRegistry::cap1()
            .invoke(
                &host,
                &backend,
                &host.ctx,
                "agent.queue",
                "pause",
                args("agent.queue"),
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Forbidden);
        assert!(backend.calls.borrow().is_empty());
        assert_eq!(host.audits.borrow()[1].stage, AuditStage::Rejected);
    }
}
#[test]
fn app_capabilities_wire_context_cannot_mint_actor_version_or_fence() {
    for field in [
        "actor",
        "role",
        "app",
        "version",
        "command",
        "call",
        "run",
        "participant",
        "fence",
    ] {
        let host = Host::new();
        let mut ctx = host.ctx.clone();
        match field {
            "actor" => ctx.actor.id = "foreign".into(),
            "role" => ctx.actor.role = "admin".into(),
            "app" => ctx.app_id = "foreign".into(),
            "version" => ctx.app_ver = "2".into(),
            "command" => ctx.command_id = "other".into(),
            "call" => ctx.call_id = "other".into(),
            "run" => ctx.fence.run_id = "other".into(),
            "participant" => ctx.fence.participant = "other".into(),
            _ => ctx.fence.fence += 1,
        }
        let backend = Backend::new(&host);
        let error = CapabilityRegistry::cap1()
            .invoke(
                &host,
                &backend,
                &ctx,
                "agent.queue",
                "pause",
                args("agent.queue"),
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Revoked);
        assert!(backend.calls.borrow().is_empty());
    }
}
#[test]
fn app_capabilities_revoke_between_admission_and_effect_fails_closed() {
    let host = Host::new();
    host.revoke_on_callback.set(true);
    let backend = Backend::new(&host);
    let error = CapabilityRegistry::cap1()
        .invoke(
            &host,
            &backend,
            &host.ctx,
            "agent.queue",
            "pause",
            args("agent.queue"),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Revoked);
    assert!(backend.calls.borrow().is_empty());
}
#[test]
fn app_capabilities_expiry_checked_at_admission_and_effect() {
    for at_effect in [false, true] {
        let mut host = Host::new();
        if at_effect {
            host.expire_on_callback.set(true)
        } else {
            host.ctx.fence.expires_at_core_ms = 1000
        }
        let backend = Backend::new(&host);
        let error = CapabilityRegistry::cap1()
            .invoke(
                &host,
                &backend,
                &host.ctx,
                "agent.queue",
                "pause",
                args("agent.queue"),
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Expired);
        assert!(backend.calls.borrow().is_empty());
    }
}
#[test]
fn app_capabilities_unknown_and_malformed_requests_are_audited_without_dispatch() {
    let host = Host::new();
    let backend = Backend::new(&host);
    let registry = CapabilityRegistry::cap1();
    for (name, function, arg, code) in [
        ("unknown", "pause", args("queue"), ErrorCode::NotFound),
        (
            "agent.queue",
            "unrestricted",
            args("queue"),
            ErrorCode::NotFound,
        ),
        (
            "agent.queue",
            "pause",
            json!({"payload":[]}),
            ErrorCode::Invalid,
        ),
        (
            "agent.queue",
            "pause",
            json!({"payload":{},"client_context":{"actor":{"role":"admin"}}}),
            ErrorCode::Invalid,
        ),
        (
            "agent.queue",
            "pause",
            json!({"payload":{},"record_id":7}),
            ErrorCode::Invalid,
        ),
        (
            "knowledge",
            "query",
            json!({"payload":{"args":["tables",7]}}),
            ErrorCode::Invalid,
        ),
    ] {
        assert_eq!(
            registry
                .invoke(&host, &backend, &host.ctx, name, function, arg)
                .unwrap_err()
                .code,
            code
        );
    }
    assert!(backend.calls.borrow().is_empty());
    assert_eq!(host.audits.borrow().len(), 12);
}
#[test]
fn app_capabilities_start_audit_failure_prevents_effect_and_finish_failure_is_uncertain() {
    for fail_at in [0, 1] {
        let host = Host::new();
        host.fail_audit.set(Some(fail_at));
        let backend = Backend::new(&host);
        let error = CapabilityRegistry::cap1()
            .invoke(
                &host,
                &backend,
                &host.ctx,
                "agent.queue",
                "pause",
                args("agent.queue"),
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert_eq!(error.uncertain, fail_at == 1);
        assert_eq!(backend.calls.borrow().len(), fail_at);
    }
}
#[test]
fn app_capabilities_limits_do_not_replay_completed_effect() {
    let host = Host::new();
    let mut backend = Backend::new(&host);
    backend.output = json!({"value":"x".repeat(MAX_MESSAGE_BYTES)});
    let registry = CapabilityRegistry::cap1();
    let error = registry
        .invoke(
            &host,
            &backend,
            &host.ctx,
            "agent.queue",
            "pause",
            args("agent.queue"),
        )
        .unwrap_err();
    assert!(error.uncertain);
    assert_eq!(backend.calls.borrow().len(), 1);
    let input = json!({"payload":{"value":"x".repeat(MAX_MESSAGE_BYTES)}});
    assert_eq!(
        registry
            .invoke(&host, &backend, &host.ctx, "agent.queue", "pause", input)
            .unwrap_err()
            .code,
        ErrorCode::Quota
    );
    assert_eq!(backend.calls.borrow().len(), 1);
    let audit = serde_json::to_string(&*host.audits.borrow()).unwrap();
    assert!(!audit.contains("existing-handler"));
    assert!(!audit.contains("native_receipt"));
}
#[test]
fn app_capabilities_protocol_roundtrip_and_closed_envelopes() {
    let ctx = context();
    let msg = CapMessage::Invoke {
        request_id: ctx.call_id.clone(),
        ctx,
        name: "agent.queue".into(),
        function: "pause".into(),
        args: args("queue"),
    };
    assert_eq!(CapMessage::decode(&msg.encode().unwrap()).unwrap(), msg);
    let mut raw = serde_json::to_value(&msg).unwrap();
    raw["ctx"]["actor"]["authority"] = json!(true);
    assert_eq!(
        CapMessage::decode(&serde_json::to_vec(&raw).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::Invalid
    );
    raw = serde_json::to_value(&msg).unwrap();
    raw["request_id"] = json!("different");
    assert_eq!(
        CapMessage::decode(&serde_json::to_vec(&raw).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::Invalid
    );
    assert_eq!(
        CapMessage::decode(&vec![b' '; MAX_MESSAGE_BYTES + 1])
            .unwrap_err()
            .code,
        ErrorCode::Quota
    );
    assert_eq!(
        protocol::json_schema()["oneOf"].as_array().unwrap().len(),
        2
    );
    let result = CapMessage::Result {
        request_id: "call".into(),
        outcome: CapOutcome::Error {
            error: CtoxError::new(ErrorCode::Forbidden, "denied"),
        },
    };
    assert_eq!(
        CapMessage::decode(&result.encode().unwrap()).unwrap(),
        result
    );
}
#[test]
fn app_capabilities_handle_correlates_failure_and_success() {
    let host = Host::new();
    let backend = Backend::new(&host);
    let registry = CapabilityRegistry::cap1();
    let request = CapMessage::Invoke {
        request_id: "call".into(),
        ctx: host.ctx.clone(),
        name: "agent.queue".into(),
        function: "pause".into(),
        args: args("queue"),
    };
    let response = registry.handle(&host, &backend, request).unwrap();
    assert!(
        matches!(response,CapMessage::Result{request_id,outcome:CapOutcome::Ok{..}} if request_id=="call")
    );
    let request = CapMessage::Invoke {
        request_id: "call".into(),
        ctx: host.ctx.clone(),
        name: "agent.queue".into(),
        function: "not-registered".into(),
        args: args("queue"),
    };
    assert!(matches!(
        registry.handle(&host, &backend, request).unwrap(),
        CapMessage::Result {
            outcome: CapOutcome::Error {
                error: CtoxError {
                    code: ErrorCode::NotFound,
                    ..
                }
            },
            ..
        }
    ));
}

#[test]
fn app_capabilities_action_aliases_keep_the_closed_native_routes() {
    let registry = CapabilityRegistry::cap1();
    for (name, function) in [
        ("ctox.ticket", "run"),
        ("agent.queue", "control"),
        ("agent.task", "enqueue"),
    ] {
        let descriptor = registry
            .descriptors()
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name && v["fn"] == function)
            .unwrap()
            .clone();
        for action in descriptor["input"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
        {
            let action = action.as_str().unwrap();
            let route = caps::all_routes()
                .find(|r| r.capability == name && r.function == action)
                .unwrap();
            let host = Host::new();
            struct AliasBackend<'a>(&'a Host, &'a str);
            impl CapabilityBackend for AliasBackend<'_> {
                fn execute(
                    &self,
                    _: &CapCallContext,
                    call: &NativeCall,
                ) -> Result<Value, CtoxError> {
                    assert!(self.0.guard.get());
                    assert_eq!(call.route().command_type(), self.1);
                    assert_eq!(call.payload(), &json!({"probe":"existing-handler"}));
                    Ok(json!({"ok":true}))
                }
            }
            let backend = AliasBackend(&host, route.command_type());
            let mut input = args(name);
            input["action"] = json!(action);
            registry
                .invoke(&host, &backend, &host.ctx, name, function, input)
                .unwrap();
        }
    }
    let host = Host::new();
    let backend = Backend::new(&host);
    for input in [
        json!({"payload":{}}),
        json!({"action":"secret.arbitrary","payload":{}}),
    ] {
        assert_eq!(
            registry
                .invoke(&host, &backend, &host.ctx, "ctox.ticket", "run", input)
                .unwrap_err()
                .code,
            ErrorCode::Invalid
        );
    }
    assert!(backend.calls.borrow().is_empty());
}

#[test]
fn app_capabilities_wrong_output_shape_is_uncertain_without_repeating_effect() {
    let host = Host::new();
    let mut backend = Backend::new(&host);
    backend.output = json!(["not a native receipt"]);
    let error = CapabilityRegistry::cap1()
        .invoke(
            &host,
            &backend,
            &host.ctx,
            "agent.queue",
            "pause",
            args("agent.queue"),
        )
        .unwrap_err();
    assert!(error.uncertain);
    assert_eq!(backend.calls.borrow().len(), 1);
}

#[test]
fn app_capabilities_grant_is_bound_to_core_app_actor_and_parent_command() {
    let host = Host::new();
    let grant = host
        .authorize(&host.ctx, "agent.queue", "pause", &args("queue"))
        .unwrap();
    for field in ["app", "version", "actor", "role", "command"] {
        let mut ctx = host.ctx.clone();
        match field {
            "app" => ctx.app_id = "foreign".into(),
            "version" => ctx.app_ver = "other".into(),
            "actor" => ctx.actor.id = "foreign".into(),
            "role" => ctx.actor.role = "admin".into(),
            _ => ctx.command_id = "foreign".into(),
        }
        assert_eq!(
            grant.check(&ctx, "agent.queue").unwrap_err().code,
            ErrorCode::Revoked
        );
    }
    assert_eq!(
        grant.check(&host.ctx, "not.required").unwrap_err().code,
        ErrorCode::Forbidden
    );
}

#[test]
fn app_capabilities_alias_policy_receives_resolved_function_and_scoped_arguments() {
    struct ScopedHost {
        inner: Host,
        checks: RefCell<Vec<(String, Value)>>,
    }
    impl CapabilityHost for ScopedHost {
        fn authorize(
            &self,
            ctx: &CapCallContext,
            cap: &str,
            function: &str,
            args: &Value,
        ) -> Result<CapabilityGrant, CtoxError> {
            self.checks
                .borrow_mut()
                .push((function.into(), args.clone()));
            if function != "retry" || args["record_id"] != "record" {
                return Err(CtoxError::new(ErrorCode::Forbidden, "record scope denied"));
            }
            self.inner.authorize(ctx, cap, function, args)
        }
        fn now_ms(&self) -> u64 {
            self.inner.now_ms()
        }
        fn with_fence(
            &self,
            ctx: &CapCallContext,
            cap: &str,
            function: &str,
            args: &Value,
            effect: &mut dyn FnMut() -> Result<Value, CtoxError>,
        ) -> Result<Value, CtoxError> {
            self.inner.with_fence(ctx, cap, function, args, effect)
        }
        fn audit(&self, entry: &AuditEntry) -> Result<(), CtoxError> {
            self.inner.audit(entry)
        }
    }
    let host = ScopedHost {
        inner: Host::new(),
        checks: RefCell::new(Vec::new()),
    };
    let backend = Backend::new(&host.inner);
    let registry = CapabilityRegistry::cap1();
    let mut input = args("queue");
    input["action"] = json!("retry");
    registry
        .invoke(
            &host,
            &backend,
            &host.inner.ctx,
            "agent.queue",
            "control",
            input.clone(),
        )
        .unwrap();
    assert_eq!(
        *host.checks.borrow(),
        vec![
            ("retry".into(), input.clone()),
            ("retry".into(), input.clone())
        ]
    );
    input["record_id"] = json!("foreign-record");
    assert_eq!(
        registry
            .invoke(
                &host,
                &backend,
                &host.inner.ctx,
                "agent.queue",
                "control",
                input
            )
            .unwrap_err()
            .code,
        ErrorCode::Forbidden
    );
    assert_eq!(backend.calls.borrow().len(), 1);
}
