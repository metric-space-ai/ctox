use super::*;
use crate::app_capabilities::registry::AuditEntry;
use crate::app_capabilities::{Actor, CapabilityGrant, ExecutorFence};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

struct Host {
    ctx: CapCallContext,
    guard: Cell<bool>,
}
impl Host {
    fn new(app_id: &str, role: &str) -> Self {
        Self::call(app_id, role, "command", "call")
    }
    fn call(app_id: &str, role: &str, command: &str, call: &str) -> Self {
        Self {
            ctx: CapCallContext {
                app_id: app_id.into(),
                app_ver: "1".into(),
                actor: Actor {
                    id: "actor".into(),
                    role: role.into(),
                },
                command_id: command.into(),
                call_id: call.into(),
                fence: ExecutorFence {
                    run_id: "run".into(),
                    participant: "core".into(),
                    fence: 3,
                    expires_at_core_ms: u64::MAX,
                },
            },
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
        if ctx != &self.ctx {
            return Err(CtoxError::new(ErrorCode::Forbidden, "denied"));
        }
        Ok(CapabilityGrant {
            app_id: ctx.app_id.clone(),
            app_ver: ctx.app_ver.clone(),
            actor: ctx.actor.clone(),
            command_id: ctx.command_id.clone(),
            capabilities_required: all()
                .map(|r| r.capability.to_owned())
                .collect::<BTreeSet<_>>(),
            allowed_roles: [self.ctx.actor.role.clone()].into_iter().collect(),
        })
    }
    fn now_ms(&self) -> u64 {
        1000
    }
    fn with_fence(
        &self,
        _: &CapCallContext,
        _: &str,
        _: &str,
        _: &Value,
        effect: &mut dyn FnMut() -> Result<Value, CtoxError>,
    ) -> Result<Value, CtoxError> {
        self.guard.set(true);
        let result = effect();
        self.guard.set(false);
        result
    }
    fn audit(&self, _: &AuditEntry) -> Result<(), CtoxError> {
        Ok(())
    }
}

#[derive(Default)]
struct Blobs {
    bytes: RefCell<BTreeMap<String, Vec<u8>>>,
}
impl cap2::blob::BlobAccess for Blobs {
    fn read(&self, _: &CapCallContext, r: &cap2::blob::BlobRef) -> Result<Vec<u8>, CtoxError> {
        self.bytes
            .borrow()
            .get(&r.sha256)
            .cloned()
            .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "missing"))
    }
    fn publish(
        &self,
        _: &CapCallContext,
        bytes: &[u8],
        _: &str,
    ) -> Result<cap2::blob::BlobRef, CtoxError> {
        let hash = sha256_hex(bytes);
        self.bytes.borrow_mut().insert(hash.clone(), bytes.to_vec());
        Ok(cap2::blob::BlobRef {
            sha256: hash.clone(),
            size: bytes.len() as u64,
            transfer_id: format!("internal-transfer-{hash}"),
        })
    }
}
#[derive(Default)]
struct Intake {
    values: RefCell<BTreeMap<(String, String), String>>,
}
impl Intake {
    fn stash(&self, command: &str, field: &str, value: &str) {
        self.values
            .borrow_mut()
            .insert((command.into(), field.into()), value.into());
    }
}
impl secrets::SecretIntake for Intake {
    fn take(&self, ctx: &CapCallContext, field: &str) -> Option<String> {
        self.values
            .borrow_mut()
            .remove(&(ctx.command_id.clone(), field.into()))
    }
}
#[derive(Default)]
struct Engine {
    ops: RefCell<Vec<(String, Value)>>,
}
impl browser::BrowserEngine for Engine {
    fn op(
        &self,
        _: &CapCallContext,
        _: &str,
        _: Option<&str>,
        op: &str,
        params: &Value,
    ) -> Result<Value, CtoxError> {
        self.ops.borrow_mut().push((op.into(), params.clone()));
        Ok(json!({"ok":true,"nav":{"url":"https://example.test/"},"passwordValue":"leak"}))
    }
    fn session_stop(&self, _: &CapCallContext, _: &str) -> Result<Value, CtoxError> {
        self.ops
            .borrow_mut()
            .push(("session_stop".into(), Value::Null));
        Ok(json!({"ok":true}))
    }
    fn auth_assist_complete(&self, _: &CapCallContext, _: &str) -> Result<Value, CtoxError> {
        self.ops
            .borrow_mut()
            .push(("auth_assist_complete".into(), Value::Null));
        Ok(json!({"ok":true}))
    }
    fn capture(
        &self,
        _: &CapCallContext,
        url: &str,
        _: u64,
    ) -> Result<browser::CapturedPage, CtoxError> {
        self.ops.borrow_mut().push(("capture".into(), json!(url)));
        Ok(browser::CapturedPage {
            final_url: url.into(),
            status: 200,
            mime: "text/html".into(),
            bytes: format!("<html>{url}</html>").into_bytes(),
        })
    }
}
struct Runtime {
    origin: Option<String>,
    handlers: BTreeSet<(String, String)>,
    /// Handler answer and the delay before the handler answers.
    answer: RefCell<Option<(Value, Duration)>>,
    dispatched: RefCell<Vec<(String, String, Value)>>,
    /// (owner app, files.ref) → bytes.
    files: RefCell<BTreeMap<(String, String), Vec<u8>>>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            origin: Some("https://core.example.test".into()),
            handlers: [
                ("outbound".to_owned(), "outbound.tracking.event".to_owned()),
                ("iot".to_owned(), "ctox.iot.attribute.write".to_owned()),
            ]
            .into_iter()
            .collect(),
            answer: RefCell::new(None),
            dispatched: RefCell::new(Vec::new()),
            files: RefCell::new(BTreeMap::new()),
        }
    }
}
impl ingest::IngestRuntime for Runtime {
    fn public_origin(&self) -> Option<String> {
        self.origin.clone()
    }
    fn has_handler(&self, app_id: &str, command_type: &str) -> Result<bool, CtoxError> {
        Ok(self
            .handlers
            .contains(&(app_id.to_owned(), command_type.to_owned())))
    }
    fn dispatch(
        &self,
        app_id: &str,
        command_type: &str,
        payload: Value,
    ) -> Result<std::sync::mpsc::Receiver<Value>, CtoxError> {
        self.dispatched
            .borrow_mut()
            .push((app_id.into(), command_type.into(), payload));
        let (sender, receiver) = std::sync::mpsc::channel();
        if let Some((answer, delay)) = self.answer.borrow().clone() {
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                let _ = sender.send(answer);
            });
        }
        Ok(receiver)
    }
    fn store_body(&self, _: &str, bytes: &[u8], _: &str) -> Result<String, CtoxError> {
        Ok(sha256_hex(bytes))
    }
    fn load_body(&self, app_id: &str, reference: &str) -> Result<Vec<u8>, CtoxError> {
        self.files
            .borrow()
            .get(&(app_id.to_owned(), reference.to_owned()))
            .cloned()
            .ok_or_else(|| CtoxError::new(ErrorCode::NotFound, "missing"))
    }
}
struct Fixture {
    root: tempfile::TempDir,
    blobs: Blobs,
    intake: Intake,
    engine: Engine,
    runtime: Runtime,
    clock: Cell<u64>,
}
impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
            blobs: Blobs::default(),
            intake: Intake::default(),
            engine: Engine::default(),
            runtime: Runtime::default(),
            clock: Cell::new(1_000_000),
        }
    }
    fn call(
        &self,
        host: &Host,
        name: &str,
        function: &str,
        args: Value,
    ) -> Result<Value, CtoxError> {
        let now = || self.clock.get();
        let backend = Cap3Backend {
            root: self.root.path(),
            blobs: &self.blobs,
            intake: &self.intake,
            browser: &self.engine,
            ingest: &self.runtime,
            now_ms: &now,
        };
        invoke(host, &backend, &host.ctx, name, function, args)
    }
}
fn code(result: Result<Value, CtoxError>) -> ErrorCode {
    result.expect_err("capability must reject").code
}

#[test]
fn app_capabilities_cap3_audit_scope_contract() {
    let f = Fixture::new();
    let consent = Host::new("consent", "owner");
    let other = Host::new("esign", "owner");
    let event = json!({"collection":"business_commands","record_id":"r1","event_type":"business_os.ats.subject_exported",
        "payload":{"summary":{"candidate_id":"c-1"}},"observed_at_ms":5});
    assert_eq!(
        f.call(&consent, "audit", "event", event.clone()).unwrap()["stored"],
        true
    );
    // Same call_id is the same append, never a second event.
    assert_eq!(
        f.call(&consent, "audit", "event", event).unwrap()["stored"],
        false
    );
    let forged = json!({"collection":"c","record_id":"r","event_type":"t","payload":{"_cap":{"app_id":"esign"}}});
    assert_eq!(
        code(f.call(
            &Host::call("consent", "owner", "c", "x"),
            "audit",
            "event",
            forged
        )),
        ErrorCode::Invalid
    );
    let query = json!({"command_type_prefix":"business_os.ats.","summary":{"candidate_id":"c-1"},"order":"observed_at_ms"});
    let own = f.call(&consent, "audit", "query", query.clone()).unwrap();
    assert_eq!(own["events"].as_array().unwrap().len(), 1);
    assert_eq!(own["events"][0]["app_id"], "consent");
    assert!(own["events"][0]["payload"].get("_cap").is_none());
    // Another app never reads consent's journal.
    let foreign = f.call(&other, "audit", "query", query.clone()).unwrap();
    assert_eq!(
        foreign["events"].as_array().unwrap().len(),
        0,
        "audit query crossed the app boundary"
    );
    let mismatch =
        json!({"command_type_prefix":"business_os.ats.","summary":{"candidate_id":"c-2"}});
    assert_eq!(
        f.call(&consent, "audit", "query", mismatch).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let platform = f
        .call(
            &Host::new("ctox-platform", "admin"),
            "audit",
            "query",
            query,
        )
        .unwrap();
    assert_eq!(platform["events"].as_array().unwrap().len(), 1);
}

#[test]
fn app_capabilities_cap3_config_whitelist_contract() {
    let f = Fixture::new();
    let host = Host::new("consent", "owner");
    crate::inference::runtime_env::set_runtime_env_value(
        f.root.path(),
        "CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE",
        "1",
    )
    .unwrap();
    crate::inference::runtime_env::set_runtime_env_value(f.root.path(), "CTOX_CHAT_MODEL", "m")
        .unwrap();
    let value = f
        .call(
            &host,
            "config",
            "get",
            json!({"key":"CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE"}),
        )
        .unwrap();
    assert_eq!(value["value"], "1");
    assert!(f
        .call(&host, "config", "get", json!({"key":"CTO_EMAIL_ADDRESS"}))
        .unwrap()
        .get("value")
        .is_some());
    // A configured, non-whitelisted key stays unreadable.
    assert_eq!(
        code(f.call(&host, "config", "get", json!({"key":"CTOX_CHAT_MODEL"}))),
        ErrorCode::Forbidden
    );
    assert_eq!(
        code(f.call(&host, "config", "get", json!({"key":"OPENAI_API_KEY"}))),
        ErrorCode::Forbidden
    );
    assert_eq!(
        code(f.call(&host, "config", "get", json!({"key":"x","default":"y"}))),
        ErrorCode::Invalid
    );
}

#[test]
fn app_capabilities_cap3_secrets_scope_contract() {
    let f = Fixture::new();
    let olg = Host::new("outbound-lead-generation", "staff");
    let canary = "synthetic-cap3-secret-canary";
    f.intake.stash("command", "value", canary);
    let put = json!({"command_id":"command","name":"PORTAL_LOGIN","value":{"secret_intake":true}});
    let stored = f.call(&olg, "secret", "put", put.clone()).unwrap();
    assert_eq!(stored["scope"], "app:outbound-lead-generation");
    // The intake is consumed once; a replay has no value to store.
    assert_eq!(code(f.call(&olg, "secret", "put", put)), ErrorCode::Invalid);
    let scope = f.call(&olg, "secret", "scope", json!({})).unwrap()["scope"].clone();
    let exists = f
        .call(
            &olg,
            "secret",
            "exists",
            json!({"scope":scope,"name":"PORTAL_LOGIN"}),
        )
        .unwrap();
    assert_eq!(exists["exists"], true);
    let listed = f.call(&olg, "secret", "list", json!({})).unwrap();
    assert_eq!(listed["secrets"][0]["name"], "PORTAL_LOGIN");
    for output in [&stored, &exists, &listed] {
        assert!(
            !output.to_string().contains(canary),
            "secret value left the Core"
        );
    }
    // Another app cannot name, probe or delete this scope.
    let news = Host::new("news", "staff");
    for (function, args) in [
        (
            "exists",
            json!({"scope":"app:outbound-lead-generation","name":"PORTAL_LOGIN"}),
        ),
        (
            "delete",
            json!({"scope":"app:outbound-lead-generation","name":"PORTAL_LOGIN"}),
        ),
        ("list", json!({"scope":"app:outbound-lead-generation"})),
    ] {
        assert_eq!(
            code(f.call(&news, "secret", function, args)),
            ErrorCode::Forbidden
        );
    }
    assert_eq!(
        f.call(&news, "secrets", "exists", json!({"name":"PORTAL_LOGIN"}))
            .unwrap()["exists"],
        false
    );
    // Intake of another command, plaintext values and instance writes without
    // secrets.manage are refused before any effect.
    f.intake.stash("other", "value", canary);
    assert_eq!(
        code(f.call(
            &olg,
            "secret",
            "put",
            json!({"command_id":"other","name":"X","value":{"secret_intake":true}})
        )),
        ErrorCode::Forbidden
    );
    assert_eq!(
        code(f.call(
            &olg,
            "secret",
            "put",
            json!({"command_id":"command","name":"X","value":canary})
        )),
        ErrorCode::Invalid
    );
    f.intake.stash("command", "value", canary);
    assert_eq!(
        code(f.call(&olg, "secret", "put", json!({"command_id":"command","name":"OPENAI_API_KEY","scope":"credentials","value":{"secret_intake":true}}))),
        ErrorCode::Forbidden
    );
    let deleted = f
        .call(&olg, "secret", "delete", json!({"name":"PORTAL_LOGIN"}))
        .unwrap();
    assert_eq!(deleted["deleted"], true);
}

#[test]
fn app_capabilities_cap3_lock_fence_contract() {
    let f = Fixture::new();
    let first = Host::call("outbound-lead-generation", "staff", "research-1", "c1");
    let second = Host::call("outbound-lead-generation", "staff", "research-2", "c1");
    let key = json!({"key":"outbound-research:lead-1","ttl_ms":5_000});
    let a = f.call(&first, "fence", "claim", key.clone()).unwrap();
    assert!(a["token"].is_string());
    // Exactly one holder while the lease is live.
    assert_eq!(
        f.call(&second, "fence", "claim", key.clone()).unwrap()["token"],
        Value::Null
    );
    // The same call retried gets its own lease back, not a second one.
    assert_eq!(
        f.call(&first, "fence", "claim", key.clone()).unwrap()["token"],
        a["token"]
    );
    // Other apps have their own key space.
    assert!(f
        .call(&Host::new("news", "staff"), "lock", "claim", key.clone())
        .unwrap()["token"]
        .is_string());
    // After TTL expiry a new claimant takes over with a higher fence ...
    f.clock.set(f.clock.get() + 5_001);
    let b = f.call(&second, "fence", "claim", key.clone()).unwrap();
    assert!(b["fence"].as_i64().unwrap() > a["fence"].as_i64().unwrap());
    // ... and the previous holder's release is a deterministic stale_fence.
    let stale = f
        .call(&first, "fence", "release", json!({"token":a["token"]}))
        .unwrap_err();
    assert_eq!(
        (stale.code, stale.message.as_str()),
        (ErrorCode::Conflict, lock::STALE_FENCE)
    );
    assert_eq!(
        f.call(&third(), "fence", "claim", key.clone()).unwrap()["token"],
        Value::Null
    );
    assert_eq!(
        f.call(&second, "fence", "release", json!({"token":b["token"]}))
            .unwrap()["released"],
        true
    );
    let again = f
        .call(&second, "fence", "release", json!({"token":b["token"]}))
        .unwrap_err();
    assert_eq!(again.message, lock::STALE_FENCE);
    assert!(f.call(&third(), "fence", "claim", key).unwrap()["token"].is_string());
    assert_eq!(
        code(f.call(&first, "lock", "claim", json!({"key":"k","ttl_ms":1}))),
        ErrorCode::Invalid
    );
}
fn third() -> Host {
    Host::call("outbound-lead-generation", "staff", "research-3", "c1")
}

#[test]
fn app_capabilities_cap3_flow_platform_contract() {
    let f = Fixture::new();
    let record =
        json!({"event_kind":"cockpit.control","title":"ctox.task.pause","metadata":{"task":"t"}});
    assert_eq!(
        code(f.call(
            &Host::new("outbound", "admin"),
            "agent.flow",
            "record",
            record.clone()
        )),
        ErrorCode::Forbidden
    );
    let recorded = f
        .call(&Host::new("ctox", "admin"), "agent.flow", "record", record)
        .unwrap();
    assert!(recorded["event_id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    let forged = json!({"event_kind":"k","title":"t","metadata":{"_cap":{"app_id":"x"}}});
    assert_eq!(
        code(f.call(&Host::new("ctox", "admin"), "agent.flow", "record", forged)),
        ErrorCode::Invalid
    );
}

#[test]
fn app_capabilities_cap3_browser_device_contract() {
    let f = Fixture::new();
    let host = Host::new("browser", "staff");
    let ok = f
        .call(&host, "browser", "op", json!({"session_id":"s1","tab_id":null,"op":"navigate","params":{"url":"https://example.test/","timeoutMs":30000}}))
        .unwrap();
    assert!(
        ok.get("passwordValue").is_none(),
        "engine credential echoed to the app"
    );
    for args in [
        json!({"session_id":"s1","op":"evaluate","params":{"source":"document.cookie"}}),
        json!({"session_id":"s1","op":"navigate","params":{"url":"file:///etc/passwd"}}),
        json!({"session_id":"s1","op":"navigate","params":{"url":"https://user:pw@example.test/"}}),
        json!({"session_id":"s1","op":"credential_fill","params":{"credential_ref":"x","passwordValue":"pw"}}),
        json!({"session_id":"s1","op":"credential_fill","params":{"selector":"#p"}}),
    ] {
        assert_eq!(
            code(f.call(&host, "browser", "op", args)),
            ErrorCode::Invalid
        );
    }
    assert_eq!(
        f.engine.ops.borrow().len(),
        1,
        "a refused op reached the engine"
    );
    f.call(&host, "browser", "session_stop", json!({"session_id":"s1"}))
        .unwrap();
    f.call(
        &host,
        "browser",
        "auth_assist_complete",
        json!({"session_id":"s1"}),
    )
    .unwrap();
    let captured = f
        .call(
            &host,
            "browser",
            "capture",
            json!({"urls":["https://example.test/a"]}),
        )
        .unwrap();
    let blob = &captured["captures"][0]["blob"];
    assert_eq!(
        blob["ref"],
        sha256_hex(b"<html>https://example.test/a</html>")
    );
    assert!(
        !captured.to_string().contains("transfer"),
        "transfer id crossed the app boundary"
    );
    assert_eq!(
        code(f.call(&host, "browser", "capture", json!({"urls":["ftp://x/"]}))),
        ErrorCode::Invalid
    );
}

#[test]
fn app_capabilities_cap3_ingest_register_contract() {
    let f = Fixture::new();
    let outbound = Host::new("outbound", "staff");
    let unknown = json!({"path":"tracking","command":"outbound.not.a.handler"});
    assert_eq!(
        code(f.call(&outbound, "net.ingest.webhook", "register", unknown)),
        ErrorCode::Invalid
    );
    let register = json!({"path":"tracking","command":"outbound.tracking.event",
        "default_response":{"status":200,"headers":{"content-type":"image/gif"},"body":"GIF89a"},
        "aliases":["/mail/track/o/","/mail/track/c/"]});
    let first = f
        .call(
            &outbound,
            "net.ingest",
            "webhook.register",
            register.clone(),
        )
        .unwrap();
    assert_eq!(
        first["ingest_url"],
        "https://core.example.test/ingest/outbound/tracking"
    );
    assert_eq!(
        f.call(&outbound, "net.ingest.webhook", "register", register)
            .unwrap(),
        first
    );
    // Legacy aliases belong to their fixed owner only.
    let iot = Host::new("iot", "staff");
    let stolen =
        json!({"path":"hook","command":"ctox.iot.attribute.write","aliases":["/mail/track/o/"]});
    assert_eq!(
        code(f.call(&iot, "net.ingest.webhook", "register", stolen)),
        ErrorCode::Forbidden
    );
    let legacy_field =
        json!({"path":"hook","command":"ctox.iot.attribute.write","deliver":{"command_type":"x"}});
    assert_eq!(
        code(f.call(&iot, "net.ingest.webhook", "register", legacy_field)),
        ErrorCode::Invalid
    );
    let subscribed = f
        .call(&iot, "net.ingest", "subscribe", json!({"endpoint":"a1","protocol":"mqtt","enabled":true,"config":{"topic":"t","password_ref":"MQTT"}}))
        .unwrap();
    assert_eq!(subscribed["enabled"], true);
    assert_eq!(
        code(f.call(
            &iot,
            "net.ingest",
            "subscribe",
            json!({"endpoint":"a1","protocol":"mqtt","config":{"password":"pw"}})
        )),
        ErrorCode::Invalid
    );
}

fn ingress(f: &Fixture, path: &str, wait: Duration) -> ingest::HttpResponse {
    let request = ingest::IngressRequest {
        method: "GET".into(),
        path: path.into(),
        query: "a=1".into(),
        headers: vec![
            ("Cookie".into(), "session=1".into()),
            ("Authorization".into(), "Bearer x".into()),
            ("X-Webhook-Token".into(), "tok".into()),
            ("User-Agent".into(), "ua".into()),
        ],
        body: Vec::new(),
        received_at_ms: 7,
    };
    ingest::handle_ingress(f.root.path(), &f.runtime, &request, wait)
}
fn registered_fixture() -> Fixture {
    let f = Fixture::new();
    f.call(&Host::new("outbound", "staff"), "net.ingest.webhook", "register",
        json!({"path":"tracking","command":"outbound.tracking.event",
               "default_response":{"status":200,"headers":{"content-type":"image/gif"},"body":"GIF89a"},
               "aliases":["/mail/track/o/"]}))
        .unwrap();
    f.call(&Host::new("iot", "staff"), "net.ingest.webhook", "register",
        json!({"path":"webhook","command":"ctox.iot.attribute.write","pass_headers":["X-Webhook-Token"]}))
        .unwrap();
    f
}

#[test]
fn app_capabilities_cap3_ingest_fallback_contract() {
    assert_eq!(ingest::RESPONSE_WAIT, Duration::from_secs(3));
    let f = registered_fixture();
    let answer = json!({"response":{"status":204}});
    *f.runtime.answer.borrow_mut() = Some((answer.clone(), Duration::from_millis(400)));
    // The handler is slower than the deadline: the registered default answers.
    let late = ingress(
        &f,
        "/ingest/outbound/tracking/o/abc.gif",
        Duration::from_millis(50),
    );
    assert_eq!(late.status, 200, "slow handler blocked the ingress answer");
    assert_eq!(late.body, b"GIF89a".to_vec());
    let dispatched = f.runtime.dispatched.borrow().last().cloned().unwrap();
    assert_eq!(
        (dispatched.0.as_str(), dispatched.1.as_str()),
        ("outbound", "outbound.tracking.event")
    );
    assert_eq!(dispatched.2["sub_path"], "o/abc.gif");
    // In time: the handler's response is the HTTP answer.
    *f.runtime.answer.borrow_mut() = Some((answer, Duration::from_millis(0)));
    assert_eq!(
        ingress(&f, "/mail/track/o/abc.gif", Duration::from_secs(2)).status,
        204
    );
    // No handler answer at all: the default for a registration without one is 202.
    *f.runtime.answer.borrow_mut() = None;
    assert_eq!(
        ingress(&f, "/ingest/iot/webhook/dev-1", Duration::from_millis(20)).status,
        202
    );
    assert_eq!(
        ingress(&f, "/ingest/iot/unknown", Duration::from_millis(20)).status,
        404
    );
}

#[test]
fn app_capabilities_cap3_ingest_header_contract() {
    let f = registered_fixture();
    *f.runtime.answer.borrow_mut() = Some((
        json!({"response":{"status":200,"headers":{"Content-Type":"text/plain","Set-Cookie":"a=b","X-Debug":"1","Cache-Control":"no-store"},"body":"ok"}}),
        Duration::from_millis(0),
    ));
    let response = ingress(&f, "/ingest/outbound/tracking", Duration::from_secs(2));
    assert_eq!(
        response
            .headers
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["cache-control", "content-type"],
        "a non-whitelisted response header left the Core"
    );
    // Request side: Cookie/Authorization/X-Webhook-Token only via pass_headers.
    let outbound = f.runtime.dispatched.borrow().last().cloned().unwrap().2;
    assert!(outbound["headers"].get("cookie").is_none());
    assert!(outbound["headers"].get("authorization").is_none());
    assert!(outbound["headers"].get("x-webhook-token").is_none());
    assert_eq!(outbound["headers"]["user-agent"], "ua");
    ingress(&f, "/ingest/iot/webhook/dev-1", Duration::from_secs(2));
    let iot = f.runtime.dispatched.borrow().last().cloned().unwrap().2;
    assert_eq!(iot["headers"]["x-webhook-token"], "tok");
    assert!(iot["headers"].get("cookie").is_none());
}

#[test]
fn app_capabilities_cap3_ingest_location_contract() {
    for (location, admissible) in [
        ("https://example.test/landing", true),
        ("http://example.test/", true),
        ("/relative", false),
        ("javascript:alert(1)", false),
        ("https://user:pw@example.test/", false),
    ] {
        let response = json!({"status":302,"headers":{"location":location}});
        assert_eq!(
            ingest::admissible_response(&response).is_some(),
            admissible,
            "{location}"
        );
    }
    assert!(ingest::admissible_response(&json!({"status":302})).is_none());
    for status in [100, 300, 309, 411, 500] {
        assert!(
            ingest::admissible_response(&json!({"status":status})).is_none(),
            "{status}"
        );
    }
    let f = registered_fixture();
    *f.runtime.answer.borrow_mut() = Some((
        json!({"response":{"status":302,"headers":{"location":"javascript:alert(1)"}}}),
        Duration::from_millis(0),
    ));
    assert_eq!(
        ingress(&f, "/ingest/outbound/tracking", Duration::from_secs(2)).status,
        502
    );
    let bad_default = json!({"path":"x","command":"outbound.tracking.event","default_response":{"status":301,"headers":{"location":"/x"}}});
    assert_eq!(
        code(f.call(
            &Host::new("outbound", "staff"),
            "net.ingest.webhook",
            "register",
            bad_default
        )),
        ErrorCode::Invalid
    );
}

struct Recording(RefCell<Vec<(String, Value)>>);
impl CapabilityBackend for Recording {
    fn execute(&self, _: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        self.0
            .borrow_mut()
            .push((call.route().command_type().into(), call.payload().clone()));
        Ok(json!({"ok":true}))
    }
}
#[test]
fn app_capabilities_cap3_coding_flat_contract() {
    let host = Host::new("coding-agents", "admin");
    let backend = Recording(RefCell::new(Vec::new()));
    let turn = json!({"module_id":"news","prompt":"fix","faux":true,"preset_id":"ctox"});
    invoke(
        &host,
        &backend,
        &host.ctx,
        "agent.coding",
        "turn",
        turn.clone(),
    )
    .unwrap();
    invoke(
        &host,
        &backend,
        &host.ctx,
        "agent.coding",
        "models",
        json!({"module_id":"news"}),
    )
    .unwrap();
    assert_eq!(
        backend.0.borrow().clone(),
        vec![
            ("ctox.coding.turn".to_owned(), turn),
            ("ctox.coding.models".to_owned(), json!({"module_id":"news"}))
        ]
    );
    for args in [
        json!({"module_id":"news","prompt":"x","model":{"id":"raw"}}),
        json!({"module_id":"../etc","prompt":"x"}),
        json!({"prompt":"x"}),
    ] {
        assert_eq!(
            code(invoke(
                &host,
                &backend,
                &host.ctx,
                "agent.coding",
                "turn",
                args
            )),
            ErrorCode::Invalid
        );
    }
    assert_eq!(
        backend.0.borrow().len(),
        2,
        "a refused coding turn reached the sidecar path"
    );
}

#[test]
fn app_capabilities_cap3_mail_server_intake_contract() {
    let host = Host::new("mail", "admin");
    let intake = Intake::default();
    let save = json!({"command_id":"command","password":{"secret_intake":true},"username":"info@example.test",
        "owner_user_id":"u1","communication_account":{"account_key":"email:info@example.test"}});
    assert_eq!(
        code(cap2::mail::server_payload(
            &intake,
            &host.ctx,
            "user.save",
            &save
        )),
        ErrorCode::Invalid,
        "a user without an intake password must be refused"
    );
    intake.stash("command", "password", "synthetic-mail-pw");
    let native = cap2::mail::server_payload(&intake, &host.ctx, "user.save", &save).unwrap();
    assert_eq!(native["password"], "synthetic-mail-pw");
    let mut foreign = save.clone();
    foreign["command_id"] = json!("other");
    assert_eq!(
        code(cap2::mail::server_payload(
            &intake,
            &host.ctx,
            "user.save",
            &foreign
        )),
        ErrorCode::Forbidden
    );
    let mut plain = save;
    plain["password"] = json!("pw");
    assert_eq!(
        code(cap2::mail::server_payload(
            &intake,
            &host.ctx,
            "user.save",
            &plain
        )),
        ErrorCode::Invalid
    );
    let domain =
        json!({"domain_name":"example.test","dkim_selector":"default","dkim_private_key":null});
    assert!(
        cap2::mail::server_payload(&intake, &host.ctx, "domain.save", &domain)
            .unwrap()
            .get("dkim_private_key")
            .is_none()
    );
    let supplied =
        json!({"domain_name":"example.test","dkim_private_key":"-----BEGIN PRIVATE KEY-----"});
    assert_eq!(
        code(cap2::mail::server_payload(
            &intake,
            &host.ctx,
            "domain.save",
            &supplied
        )),
        ErrorCode::Invalid
    );
    let projection = cap2::mail::redact_secrets(
        json!({"users":[{"username":"u","password_hash":"$argon2","password_set":true}],
        "domains":[{"domain_name":"d","dkim_private_key":"k","dkim_public_key":"p"}],"runtime_config":{"admin_secret":"s"}}),
    );
    let text = projection.to_string();
    for leak in ["$argon2", "\"k\"", "admin_secret"] {
        assert!(!text.contains(leak), "{leak} reached a mail.server result");
    }
    assert_eq!(projection["users"][0]["password_set"], true);
    assert_eq!(projection["domains"][0]["dkim_public_key"], "p");
}

#[test]
fn app_capabilities_cap3_contract_covers_routes() {
    let contract = contract();
    let paths = contract
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    for route in ROUTES {
        assert!(
            paths.contains(&cap2::call_path(route)),
            "{} has no contract entry",
            cap2::call_path(route)
        );
    }
    // Every CAP-3 route is reachable through the flat JS path.
    let registry = registry();
    let descriptors = registry.descriptors().to_string();
    for route in ROUTES {
        assert!(descriptors.contains(route.function));
    }
}

#[test]
fn app_capabilities_cap3_ingest_body_ref_contract() {
    let f = registered_fixture();
    let pixel = b"GIF89a\x01\x00\x01\x00\x00\xff".to_vec();
    let reference = sha256_hex(&pixel);
    f.runtime
        .files
        .borrow_mut()
        .insert(("outbound".into(), reference.clone()), pixel.clone());
    *f.runtime.answer.borrow_mut() = Some((
        json!({"response":{"status":200,"headers":{"content-type":"image/gif"},"body_ref":{"ref":reference}}}),
        Duration::from_millis(0),
    ));
    // The Core delivers the file's bytes, never a base64 or JSON rendering.
    let response = ingress(&f, "/ingest/outbound/tracking", Duration::from_secs(2));
    assert_eq!((response.status, response.body), (200, pixel.clone()));
    // Another app's file is not deliverable through this app's ingress.
    f.runtime.files.borrow_mut().clear();
    f.runtime
        .files
        .borrow_mut()
        .insert(("iot".into(), reference.clone()), pixel);
    assert_eq!(
        ingress(&f, "/ingest/outbound/tracking", Duration::from_secs(2)).status,
        502
    );
    let both = json!({"status":200,"body":"x","body_ref":{"ref":reference}});
    assert!(ingest::admissible_response(&both).is_none());
}
