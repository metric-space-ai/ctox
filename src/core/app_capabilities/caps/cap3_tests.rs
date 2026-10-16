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
/// Deliberately unscoped store: returns any record for any app, so the
/// capability's own owner check is what keeps apps apart.
#[derive(Default)]
struct Files {
    records: std::sync::Mutex<Vec<(files::FileMeta, Vec<u8>)>>,
    corrupt_ref: std::sync::atomic::AtomicBool,
}
impl files::FileStore for Files {
    fn put(
        &self,
        owner_app: &str,
        _: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<files::FileMeta, CtoxError> {
        let mut records = self.records.lock().unwrap();
        let reference = if self.corrupt_ref.load(std::sync::atomic::Ordering::SeqCst) {
            "0".repeat(64)
        } else {
            sha256_hex(bytes)
        };
        let meta = files::FileMeta {
            id: format!("file-{}", records.len() + 1),
            reference,
            name: name.into(),
            mime: mime.into(),
            size: bytes.len() as u64,
            owner_app: owner_app.into(),
            created_at_ms: 1,
        };
        records.push((meta.clone(), bytes.to_vec()));
        Ok(meta)
    }
    fn get(&self, _: &str, key: &str) -> Result<Option<(files::FileMeta, Vec<u8>)>, CtoxError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .find(|(m, _)| m.id == key || m.reference == key)
            .cloned())
    }
    fn list(&self, _: &str, limit: usize) -> Result<Vec<files::FileMeta>, CtoxError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .map(|(m, _)| m.clone())
            .collect())
    }
    fn delete(&self, owner_app: &str, id: &str) -> Result<bool, CtoxError> {
        let mut records = self.records.lock().unwrap();
        let before = records.len();
        records.retain(|(m, _)| !(m.id == id && m.owner_app == owner_app));
        Ok(records.len() != before)
    }
}
struct Fixture {
    root: tempfile::TempDir,
    files: Files,
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
            files: Files::default(),
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
            files: Some(&self.files),
        };
        invoke(host, &backend, &host.ctx, name, function, args)
    }
}
fn code(result: Result<Value, CtoxError>) -> ErrorCode {
    result.expect_err("capability must reject").code
}

/// The capability layer is a synchronous Core caller. Under the injected-owner
/// contract it runs only with the daemon's owners installed for its root (Core,
/// Runtime, Secrets), with the current Store rights bound as the policy
/// authority, and on the bounded `run_core_sync` executor: exactly how a
/// migrated synchronous caller reaches Core. Nothing here opens a second owner.
fn with_core_owners(body: impl FnOnce(&Fixture) + Send + 'static) {
    let f = Fixture::new();
    let root = f.root.path().to_path_buf();
    // The fixture (and its root) outlives the body: owners close first.
    let f = run_with_core_owners(root, move || {
        body(&f);
        f
    });
    drop(f);
}

/// Root-based form for sibling capability tests that bring their own root.
pub(crate) fn run_with_core_owners<T: Send + 'static>(
    root: std::path::PathBuf,
    body: impl FnOnce() -> T + Send + 'static,
) -> T {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let core = crate::inference::runtime_env::RuntimeOwnerFixture::open(&root)
            .await
            .unwrap();
        let secrets = SecretOwner::open(&root).await;
        let rights = BoundRights::open(&root).await;
        let result = run_core_sync_admitted(move || Ok(body())).await;
        let failed = result.is_err();
        rights.close(failed).await;
        secrets.close(failed).await;
        let closed = core.close().await;
        let value = result.unwrap();
        closed.unwrap();
        value
    })
}

/// `run_core_sync` admits at most four waiting compatibility callers and
/// rejects the fifth as Busy without running it. Parallel tests wait for a
/// slot instead of failing; an admitted operation is never run twice.
async fn run_core_sync_admitted<T: Send + 'static>(
    operation: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let pending = std::sync::Arc::new(std::sync::Mutex::new(Some(operation)));
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    loop {
        let slot = pending.clone();
        let result = crate::persistence::run_core_sync(move || {
            let operation = slot.lock().unwrap().take().expect("admitted once");
            operation()
        })
        .await;
        if result.is_err()
            && pending.lock().unwrap().is_some()
            && std::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }
        return result;
    }
}

/// The Secrets file owner for a fixture root (schema as at daemon boot).
struct SecretOwner {
    _store: crate::secrets::SecretStore,
    writer: ctox_store::writer::WriterHandle,
    readers: ctox_store::read_pool::ReadPool,
}
impl SecretOwner {
    async fn open(root: &std::path::Path) -> Self {
        let path = crate::secrets::resolve_db_path(root);
        assert!(path.starts_with(root), "isolated Secrets fixture required");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let (writer, readers) =
            ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path).unwrap())
                .await
                .unwrap()
                .into_parts();
        writer
            .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
                crate::secrets::initialize_owned_secret_schema(tx)
                    .map_err(|_| ctox_store::StoreError::Invalid("secret fixture schema".into()))?;
                Ok(())
            }))
            .unwrap()
            .await
            .unwrap();
        let store = crate::secrets::SecretStore::attach(root, writer.clone(), readers.clone())
            .await
            .unwrap();
        Self {
            _store: store,
            writer,
            readers,
        }
    }
    async fn close(self, body_failed: bool) {
        let Self {
            _store,
            writer,
            readers,
        } = self;
        drop(_store);
        let closed = (readers.shutdown().await, writer.shutdown().await);
        if !body_failed {
            closed.0.unwrap();
            closed.1.unwrap();
        }
    }
}

/// Base-release rights as current Store rows, bound as the process policy
/// authority (one binding per process: wait while another test holds it).
struct BoundRights {
    registration: crate::business_os::policy::PolicyAuthorityRegistration,
    authority: std::sync::Arc<crate::business_os::policy::sync_rights::StoreRightsAuthority>,
}
impl BoundRights {
    async fn open(root: &std::path::Path) -> Self {
        use ctox_store::{registry::Registry, writer::WriterOptions, DbId};
        let writer = Registry
            .open(
                DbId::new(root.join("rights-fixture.sqlite3")).unwrap(),
                WriterOptions {
                    configure: Some(std::sync::Arc::new(|db| {
                        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                        Ok(())
                    })),
                    head_seq: Some(std::sync::Arc::new(|db| {
                        db.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))
                            .map_err(Into::into)
                    })),
                },
            )
            .unwrap();
        let authority = crate::business_os::policy::sync_rights::StoreRightsAuthority::attach(
            writer,
            "cap3-fixture".into(),
        )
        .await
        .unwrap();
        authority
            .replace(crate::business_os::policy::base_release_rights(&[]), vec![])
            .await
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(300);
        let registration = loop {
            match crate::business_os::policy::bind_policy_authority(authority.clone()) {
                Ok(registration) => break registration,
                Err(error) if std::time::Instant::now() < deadline => {
                    let _ = error;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => panic!("policy authority: {error}"),
            }
        };
        Self {
            registration,
            authority,
        }
    }
    async fn close(self, body_failed: bool) {
        let Self {
            registration,
            authority,
        } = self;
        drop(registration);
        let closed = authority.writer().shutdown().await;
        if !body_failed {
            closed.unwrap();
        }
    }
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
    with_core_owners(|f| {
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
    });
}

#[test]
fn app_capabilities_cap3_secrets_scope_contract() {
    with_core_owners(|f| {
        let olg = Host::new("outbound-lead-generation", "staff");
        let canary = "synthetic-cap3-secret-canary";
        f.intake.stash("command", "value", canary);
        let put =
            json!({"command_id":"command","name":"PORTAL_LOGIN","value":{"secret_intake":true}});
        let stored = f.call(&olg, "secret", "put", put.clone()).unwrap();
        assert_eq!(stored["scope"], "app:outbound-lead-generation");
        // The intake is consumed once; a replay has no value to store.
        assert_eq!(code(f.call(&olg, "secret", "put", put)), ErrorCode::Invalid);
        let scope = f.call(&olg, "secret", "scope", Value::Null).unwrap()["scope"].clone();
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
    });
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
    let turn = json!({"app_id":"news","draft_id":"draft-1","base_draft_rev":"3-abc","prompt":"fix","model":"ctox","turn_id":"t1"});
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
        "cancel",
        json!({"turn_id":"t1"}),
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
            ("ctox.coding.cancel".to_owned(), json!({"turn_id":"t1"})),
            ("ctox.coding.models".to_owned(), json!({"module_id":"news"}))
        ]
    );
    for args in [
        // V3 shape, alone and smuggled next to the §16.44 fields.
        json!({"module_id":"news","prompt":"x","faux":true,"preset_id":"ctox"}),
        json!({"app_id":"news","draft_id":"d","base_draft_rev":"1","prompt":"x","module_id":"news"}),
        json!({"app_id":"news","prompt":"x"}),
        json!({"payload":{"module_id":"news","prompt":"x"}}),
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
        3,
        "a refused coding turn reached the engine"
    );
    // Production dispatch: unbound draft engine is unavailable, never the V3 path.
    let f = Fixture::new();
    let turn = json!({"app_id":"news","draft_id":"d","base_draft_rev":"1","prompt":"x"});
    assert_eq!(
        code(f.call(&host, "agent.coding", "turn", turn)),
        ErrorCode::Unavailable
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

// ------------------------------------------------------------------ CAP-3b

#[test]
fn app_capabilities_cap3_secret_intake_live_contract() {
    // The production intake reads the value the native intake detached from
    // exactly this command (replicated-document sanitizer path), once.
    let mut document = json!({"id":"cmd-secret-1","type":"ctox.secret.put","payload":{"name":"X","value":"synthetic-intake"}});
    crate::business_os::store::detach_secret_from_replicated_document(
        "business_commands",
        &mut document,
    );
    assert!(document["payload"].get("value").is_none());
    let owner = Host::call("credentials", "admin", "cmd-secret-1", "c");
    let other = Host::call("credentials", "admin", "cmd-secret-2", "c");
    let intake = secrets::CoreSecretIntake;
    use secrets::SecretIntake as _;
    assert_eq!(intake.take(&other.ctx, "value"), None);
    assert_eq!(intake.take(&owner.ctx, "password"), None);
    assert_eq!(
        intake.take(&owner.ctx, "value").as_deref(),
        Some("synthetic-intake")
    );
    assert_eq!(
        intake.take(&owner.ctx, "value"),
        None,
        "an intake value must be consumed once"
    );
}

#[test]
fn app_capabilities_cap3_platform_origin_contract() {
    with_core_owners(|f| {
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            config::PUBLIC_ORIGIN_KEY,
            "https://core.example.test/",
        )
        .unwrap();
        let key = json!({"key":config::PUBLIC_ORIGIN_KEY});
        assert_eq!(
            code(f.call(
                &Host::new("outbound", "admin"),
                "config",
                "get",
                key.clone()
            )),
            ErrorCode::Forbidden
        );
        let platform = f
            .call(&Host::new("ctox-platform", "admin"), "config", "get", key)
            .unwrap();
        assert_eq!(platform["value"], "https://core.example.test");
        // The production runtime uses this origin, and refuses while no app
        // runtime ingress is bound (never a fallback dispatch).
        let runtime = ingest::CoreIngestRuntime {
            root: f.root.path(),
        };
        use ingest::IngestRuntime as _;
        assert_eq!(
            runtime.public_origin().as_deref(),
            Some("https://core.example.test")
        );
        assert_eq!(
            runtime.has_handler("outbound", "x").unwrap_err().code,
            ErrorCode::Unavailable
        );
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            config::PUBLIC_ORIGIN_KEY,
            "https://core.example.test/path",
        )
        .unwrap();
        assert_eq!(config::public_origin(f.root.path()), None);
        // HTTP routing: /ingest always, a legacy alias only once registered.
        assert!(ingest::serves(f.root.path(), "/ingest/outbound/tracking"));
        assert!(!ingest::serves(f.root.path(), "/mail/track/o/abc.gif"));
        f.call(
            &Host::new("outbound", "staff"),
            "net.ingest.webhook",
            "register",
            json!({"path":"tracking","command":"outbound.tracking.event","aliases":["/mail/track/o/"]}),
        )
        .unwrap();
        assert!(ingest::serves(f.root.path(), "/mail/track/o/abc.gif"));
        assert!(!ingest::serves(f.root.path(), "/mail/track/c/abc"));
    });
}

#[test]
fn app_capabilities_cap3_files_owner_contract() {
    let f = Fixture::new();
    let docs = Host::new("documents", "staff");
    let put = f
        .call(
            &docs,
            "files",
            "put",
            json!({"name":"a.txt","mime":"text/plain","text":"hello"}),
        )
        .unwrap();
    assert_eq!(put["ref"], sha256_hex(b"hello"));
    let binary = f
        .call(
            &docs,
            "files",
            "put",
            json!({"name":"b.bin","base64":"//4="}),
        )
        .unwrap();
    let got = f
        .call(&docs, "files", "get", json!({"ref":put["ref"]}))
        .unwrap();
    assert_eq!(got["text"], "hello");
    let raw = f
        .call(&docs, "files", "get", json!({"id":binary["id"]}))
        .unwrap();
    assert!(
        raw.get("text").is_none() && raw.get("base64").is_none(),
        "binary content must not be inlined"
    );
    // Another app never reads, lists or deletes these records.
    let other = Host::new("news", "staff");
    assert_eq!(
        code(f.call(&other, "files", "get", json!({"id":put["id"]}))),
        ErrorCode::NotFound
    );
    assert_eq!(
        f.call(&other, "files", "list", json!({})).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        f.call(&other, "files", "delete", json!({"id":put["id"]}))
            .unwrap()["deleted"],
        false
    );
    assert_eq!(
        f.call(&docs, "files", "list", json!({})).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    f.files
        .corrupt_ref
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let wrong = f
        .call(&docs, "files", "put", json!({"name":"c","text":"x"}))
        .unwrap_err();
    assert!(
        wrong.uncertain,
        "a store addressing other content is not a successful put"
    );
}

#[test]
fn app_capabilities_cap3_process_env_contract() {
    with_core_owners(|f| {
        let host = Host::new("outbound-lead-generation", "staff");
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            process::ALLOWLIST_KEY,
            r#"{"sh":"/bin/sh"}"#,
        )
        .unwrap();
        let run = f
            .call(&host, "process", "run", json!({"program":"sh","args":["-c","printf '[%s][%s]' \"$HOME\" \"$CTOX_SCRAPE_TARGET\"; pwd"],
                "env":{"CTOX_SCRAPE_TARGET":"t1"},"cwd_ref":"scrape/t1","timeout_ms":10000}))
            .unwrap();
        let stdout = run["stdout"].as_str().unwrap();
        assert!(
            stdout.starts_with("[][t1]"),
            "environment was inherited: {stdout}"
        );
        assert!(
            stdout
                .trim_end()
                .ends_with("app-workspaces/outbound-lead-generation/scrape/t1"),
            "{stdout}"
        );
        assert_eq!(run["exit"], 0);
        let slow = f
            .call(
                &host,
                "process",
                "run",
                json!({"program":"sh","args":["-c","while :; do :; done"],"timeout_ms":200}),
            )
            .unwrap();
        assert_eq!(slow["timed_out"], true);
        for args in [
            json!({"program":"sh","env":{"LD_PRELOAD":"/x.so"}}),
            json!({"program":"sh","env":{"OPENAI_API_KEY":"k"}}),
            json!({"program":"sh","cwd_ref":"../other-app"}),
            json!({"program":"sh","cwd_ref":"/etc"}),
        ] {
            assert_eq!(
                code(f.call(&host, "process", "run", args)),
                ErrorCode::Invalid
            );
        }
        assert_eq!(
            code(f.call(&host, "process", "run", json!({"program":"/bin/sh"}))),
            ErrorCode::Forbidden
        );
    });
}

#[test]
fn app_capabilities_cap3_directory_policy_contract() {
    with_core_owners(|f| {
        let decide = json!({"permission":"apps.modify","module":"news"});
        let admin = f
            .call(
                &Host::new("threads", "admin"),
                "policy",
                "module_decision",
                decide.clone(),
            )
            .unwrap();
        let user = f
            .call(
                &Host::new("threads", "user"),
                "policy",
                "module_decision",
                decide,
            )
            .unwrap();
        assert_eq!(
            (admin["allowed"].clone(), user["allowed"].clone()),
            (json!(true), json!(false))
        );
        let forged = json!({"permission":"apps.modify","module":"news","actor":{"role":"admin"}});
        assert_eq!(
            code(f.call(
                &Host::new("threads", "user"),
                "policy",
                "module_decision",
                forged
            )),
            ErrorCode::Invalid
        );
        assert_eq!(
            f.call(
                &Host::new("threads", "user"),
                "directory",
                "user",
                json!({"user_id":"nobody"})
            )
            .unwrap()["found"],
            false
        );
        assert!(f.call(&Host::new("kundenpipeline", "user"), "identity.users", "list", json!({"active":true,"roles":["chef","admin"],"order":["created_at_ms:asc","user_id:asc"]})).unwrap()["users"].is_array());
        // command.get: own app's command intent only.
        let conn = crate::business_os::store::open_store(f.root.path()).unwrap();
        conn.execute("INSERT INTO business_commands (command_id,module,command_type,record_id,status,payload_json,client_context_json,observed_at_ms)
                      VALUES ('cmd-r1','outbound-lead-generation','web_stack.person_research','lead-1','completed','{\"company\":\"ACME\"}','{}',1)", []).unwrap();
        let own = f
            .call(
                &Host::new("outbound-lead-generation", "staff"),
                "command",
                "get",
                json!({"command_id":"cmd-r1"}),
            )
            .unwrap();
        assert_eq!(
            (own["found"].clone(), own["payload"]["company"].clone()),
            (json!(true), json!("ACME"))
        );
        let foreign = f
            .call(
                &Host::new("news", "staff"),
                "command",
                "get",
                json!({"command_id":"cmd-r1"}),
            )
            .unwrap();
        assert_eq!(
            foreign["found"], false,
            "command.get crossed the app boundary"
        );
    });
}

#[test]
fn app_capabilities_cap3_auth_assist_contract() {
    let f = Fixture::new();
    let host = Host::new("news", "staff");
    for (args, expected) in [
        (
            json!({"source_id":"s1","source_module":"outbound"}),
            ErrorCode::Forbidden,
        ),
        (
            json!({"source_id":"s1","credential_ref":"plain-password"}),
            ErrorCode::Invalid,
        ),
        (
            json!({"source_id":"s1","credential_ref":"ctox-secret://app:other/X"}),
            ErrorCode::Invalid,
        ),
        (
            json!({"source_id":"s1","target_url":"file:///etc/passwd"}),
            ErrorCode::Invalid,
        ),
    ] {
        assert_eq!(
            code(f.call(&host, "browser.auth_assist", "request", args)),
            expected
        );
    }
}

// ------------------------------------------------------------------ CAP-3c (§16.46)

#[test]
fn app_capabilities_cap3_browser_core_entry_contract() {
    with_core_owners(|f| {
        let root = f.root.path();
        let binding = |app: &'static str, user: &'static str, role: &'static str| {
            crate::business_os::CoreBrowserBinding {
                app_id: app,
                user_id: user,
                role,
                command_id: "cmd",
                call_id: "call",
            }
        };
        let gate =
            |b: &crate::business_os::CoreBrowserBinding<'_>, command: &str, session: &str| {
                crate::business_os::core_browser_gate(root, b, command, session)
                    .map_err(|e| format!("{e:#}"))
            };
        gate(
            &binding("outbound", "u1", "admin"),
            "browser.navigate",
            "s1",
        )
        .unwrap();
        // Same session again for the same (app, user): still allowed.
        gate(&binding("outbound", "u1", "admin"), "browser.reload", "s1").unwrap();
        let foreign_app =
            gate(&binding("news", "u1", "admin"), "browser.navigate", "s1").unwrap_err();
        assert!(foreign_app.contains("another app"), "{foreign_app}");
        let foreign_user = gate(
            &binding("outbound", "u2", "admin"),
            "browser.navigate",
            "s1",
        )
        .unwrap_err();
        assert!(foreign_user.contains("another user"), "{foreign_user}");
        let no_right =
            gate(&binding("outbound", "u1", "user"), "browser.navigate", "s2").unwrap_err();
        assert!(no_right.contains("policy"), "{no_right}");
        let token_gated = gate(
            &binding("outbound", "u1", "admin"),
            "browser.credential.fill",
            "s1",
        )
        .unwrap_err();
        assert!(token_gated.contains("command"), "{token_gated}");
        // Every use is audited, without payload contents.
        let conn = crate::business_os::store::open_store(root).unwrap();
        let count = |kind: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM business_events WHERE command_type=?1",
                [kind],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            count("browser.core_entry.allowed"),
            2,
            "allowed use not audited"
        );
        assert_eq!(count("browser.core_entry.denied"), 4, "refusal not audited");
        let bodies: Vec<String> = conn
            .prepare("SELECT payload_json FROM business_events WHERE command_type LIKE 'browser.core_entry.%'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(bodies
            .iter()
            .all(|b| b.contains("\"app_id\"") && !b.contains("url")));
        // The production engine refuses a foreign session before any engine effect.
        let engine = browser::PeerBrowserEngine { root };
        use browser::BrowserEngine as _;
        let news = Host::new("news", "admin");
        assert_eq!(
            engine.session_stop(&news.ctx, "s1").unwrap_err().code,
            ErrorCode::Forbidden
        );
        assert_eq!(
            engine
                .op(
                    &news.ctx,
                    "s1",
                    None,
                    "credential_fill",
                    &json!({"credential_ref":"X"})
                )
                .unwrap_err()
                .code,
            ErrorCode::Forbidden
        );
    });
}

#[test]
fn app_capabilities_cap3_browser_internal_only_contract() {
    let root = tempfile::tempdir().unwrap();
    // A replicated document claiming a Core binding or an actor never selects
    // the internal entry: without the user's capability token it is refused.
    let forged = json!({"type":"browser.navigate","payload":{"session_id":"s1"},
        "client_context":{"core_entry":true,"actor":{"id":"u1"},"owner_user_id":"u1"}});
    let refused = crate::business_os::resolve_browser_command_owner(root.path(), &forged, None)
        .map_err(|e| format!("{e:#}"))
        .unwrap_err();
    assert!(
        refused.contains("capability token is required"),
        "{refused}"
    );
    let core = crate::business_os::resolve_browser_command_owner(root.path(), &forged, Some("u7"))
        .unwrap();
    assert_eq!(
        core,
        ("u7".to_owned(), String::new()),
        "the Core binding, not the document, names the owner"
    );
}

// ------------------------------------------------------------------ CAP-3d (§16.47)

#[test]
fn app_capabilities_cap3_process_base_env_contract() {
    with_core_owners(|f| {
        let host = Host::new("outbound-lead-generation", "staff");
        let script = f.root.path().join("platform-script.sh");
        std::fs::write(&script, "printf 'script:%s' \"$1\"").unwrap();
        let allowlist = json!({
            "sh": {"program":"/bin/sh","env":{"PATH":"/usr/bin:/bin","LANG":"C.UTF-8","CTOX_BIN":"/opt/ctox/bin/ctox"}},
            "scrape": {"interpreter":"/bin/sh","script":script.to_string_lossy(),"env":{"PATH":"/usr/bin:/bin"}}
        });
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            process::ALLOWLIST_KEY,
            &allowlist.to_string(),
        )
        .unwrap();
        let env = f
            .call(&host, "process", "run", json!({"program":"sh","args":["-c","printf '[%s][%s][%s][%s][%s]' \"$PATH\" \"$LANG\" \"$CTOX_BIN\" \"$CTOX_SCRAPE_X\" \"$HOME\""],
                "env":{"CTOX_SCRAPE_X":"1"}}))
            .unwrap();
        assert_eq!(
            env["stdout"], "[/usr/bin:/bin][C.UTF-8][/opt/ctox/bin/ctox][1][]",
            "platform base env missing"
        );
        // The app adds variables; it cannot replace the platform base.
        assert_eq!(
            code(f.call(
                &host,
                "process",
                "run",
                json!({"program":"sh","env":{"PATH":"/tmp"}})
            )),
            ErrorCode::Invalid
        );
        // Interpreter + script from the allowlist; the app only supplies args.
        let scripted = f
            .call(
                &host,
                "process",
                "run",
                json!({"program":"scrape","args":["a"]}),
            )
            .unwrap();
        assert_eq!(scripted["stdout"], "script:a");
        // stdin as text or as a files record of the calling app only.
        let text = f
            .call(
                &host,
                "process",
                "run",
                json!({"program":"sh","args":["-c","cat"],"stdin":"inline"}),
            )
            .unwrap();
        assert_eq!(text["stdout"], "inline");
        let own = f
            .call(
                &host,
                "files",
                "put",
                json!({"name":"in.txt","text":"from-file"}),
            )
            .unwrap();
        let piped = f
            .call(
                &host,
                "process",
                "run",
                json!({"program":"sh","args":["-c","cat"],"stdin_ref":{"ref":own["ref"]}}),
            )
            .unwrap();
        assert_eq!(piped["stdout"], "from-file");
        let foreign = f
            .call(
                &Host::new("news", "staff"),
                "files",
                "put",
                json!({"name":"secret.txt","text":"other-app"}),
            )
            .unwrap();
        assert_eq!(
            code(f.call(
                &host,
                "process",
                "run",
                json!({"program":"sh","args":["-c","cat"],"stdin_ref":{"ref":foreign["ref"]}})
            )),
            ErrorCode::NotFound,
            "stdin_ref of another app must be refused"
        );
    });
}

#[test]
fn app_capabilities_cap3_browser_automation_contract() {
    let f = Fixture::new();
    let host = Host::new("outbound", "staff");
    let ok = f
        .call(
            &host,
            "browser.automation",
            "run",
            json!({"session":"s-auto","timeout_ms":10000,"steps":[
            {"op":"navigate","params":{"url":"https://example.test/"}},{"op":"reload"}]}),
        )
        .unwrap();
    assert_eq!(
        (ok["completed"].clone(), ok["session_id"].clone()),
        (json!(2), json!("s-auto"))
    );
    let artifact = ok["artifact"]["ref"].as_str().unwrap().to_owned();
    let stored = f
        .call(&host, "files", "get", json!({"ref":artifact}))
        .unwrap();
    assert!(stored["text"].as_str().unwrap().contains("\"reload\""));
    assert_eq!(f.engine.ops.borrow().len(), 2);
    // A disallowed op anywhere refuses the whole run before its first step.
    for steps in [
        json!([{"op":"navigate","params":{"url":"https://example.test/"}},{"op":"evaluate","params":{"source":"x"}}]),
        json!([{"op":"reload"},{"op":"navigate","params":{"url":"file:///etc/passwd"}}]),
        json!([{"op":"reload"},{"op":"credential_fill","params":{"credential_ref":"X","passwordValue":"pw"}}]),
    ] {
        assert_eq!(
            code(f.call(&host, "browser.automation", "run", json!({"steps":steps}))),
            ErrorCode::Invalid
        );
    }
    assert_eq!(
        f.engine.ops.borrow().len(),
        2,
        "a refused run executed a step"
    );
}

#[test]
fn app_capabilities_cap3_policy_assignment_contract() {
    with_core_owners(|f| {
        let root = f.root.path();
        let founder = crate::business_os::CoreBrowserBinding {
            app_id: "outbound",
            user_id: "u-founder",
            role: "founder",
            command_id: "cmd",
            call_id: "call",
        };
        let denied =
            crate::business_os::core_browser_gate(root, &founder, "browser.navigate", "s-f")
                .map_err(|e| format!("{e:#}"))
                .unwrap_err();
        assert!(denied.contains("policy"), "{denied}");
        let decide = json!({"permission":"data.write","module":"outbound"});
        let host = Host::new("threads", "founder");
        let mut host_founder = host;
        host_founder.ctx.actor.id = "u-founder".into();
        assert_eq!(
            f.call(&host_founder, "policy", "module_decision", decide.clone())
                .unwrap()["allowed"],
            false
        );
        // Founder assignment in business_module_acl grants module rights.
        let conn = crate::business_os::store::open_store(root).unwrap();
        conn.execute(
            "INSERT INTO business_module_acl (module_id,user_id,role,active,created_at_ms,updated_at_ms)
             VALUES ('outbound','u-founder','founder',1,1,1)",
            [],
        )
        .unwrap();
        crate::business_os::core_browser_gate(root, &founder, "browser.navigate", "s-f").unwrap();
        assert_eq!(
            f.call(&host_founder, "policy", "module_decision", decide)
                .unwrap()["allowed"],
            true
        );
    });
}

// ------------------------------------------------------------------ CAP-3e (§16.48)

#[test]
fn app_capabilities_cap3_browser_extract_contract() {
    let f = Fixture::new();
    let host = Host::new("outbound-lead-generation", "staff");
    let fields = json!({"name":{"selector":":scope h2","text":true},"href":{"selector":":scope a","attr":"href"}});
    let ok = f
        .call(
            &host,
            "browser.automation",
            "run",
            json!({"session":"s-x","steps":[
            {"op":"wait","selector":"ul.results"},
            {"op":"extract","selector":"ul.results > li","fields":fields,"limit":50},
            {"op":"click","selector":"a.next"},
            {"op":"wait","ms":500}]}),
        )
        .unwrap();
    assert_eq!(ok["completed"], 4);
    let ops = f.engine.ops.borrow().clone();
    assert_eq!(
        ops[1],
        (
            "extract".to_owned(),
            json!({"selector":"ul.results > li","fields":fields,"limit":50})
        )
    );
    // Every malformed step refuses the whole run before its first step.
    for bad in [
        json!({"op":"extract","selector":"li","fields":fields}),
        json!({"op":"extract","selector":"li","fields":fields,"limit":0}),
        json!({"op":"extract","selector":"li","fields":{"x":{"selector":"a","attr":"href","text":true}},"limit":5}),
        json!({"op":"extract","selector":"li","fields":{"x":{"selector":"a","script":"alert(1)"}},"limit":5}),
        json!({"op":"wait","ms":30001}),
        json!({"op":"wait","selector":"a","ms":5}),
        json!({"op":"click"}),
        json!({"op":"click","selector":"a","params":{"selector":"b"}}),
    ] {
        let steps = json!([{"op":"reload"}, bad]);
        assert_eq!(
            code(f.call(&host, "browser.automation", "run", json!({"steps":steps}))),
            ErrorCode::Invalid,
            "{bad}"
        );
    }
    assert_eq!(
        f.engine.ops.borrow().len(),
        4,
        "a refused run executed a step"
    );
}

// ------------------------------------------------------------------ E20 sandbox
#[cfg(target_os = "linux")]
#[path = "sandbox_tests.rs"]
mod sandbox_tests;
#[test]
fn app_capabilities_cap3_process_untrusted_contract() {
    with_core_owners(|f| {
        let host = Host::new("outbound-lead-generation", "staff");
        let host_script = f.root.path().join("host.mjs");
        std::fs::write(&host_script, "generated code host").unwrap();
        let allowlist = json!({
            "olg-scrape-js": {"interpreter":"/bin/cat","script":host_script.to_string_lossy(),"untrusted_code":true,"env":{"PATH":"/usr/bin:/bin"}},
            "olg-scrape-sh": {"interpreter":"/bin/bash","script":host_script.to_string_lossy(),"untrusted_code":true},
            "trusted-cat": {"interpreter":"/bin/cat","script":host_script.to_string_lossy()}
        });
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            process::ALLOWLIST_KEY,
            &allowlist.to_string(),
        )
        .unwrap();
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            sandbox::SANDBOX_KEY,
            r#"{"bwrap":"/missing/e20-bwrap","sandbox_exec":"/missing/e20-sandbox"}"#,
        )
        .unwrap();
        // An unavailable E20 sandbox never falls back to ordinary spawning.
        assert_eq!(
            code(f.call(
                &host,
                "process",
                "run",
                json!({"program":"olg-scrape-js","stdin":"console.log(1)"})
            )),
            ErrorCode::Unavailable
        );
        // A shell is never an interpreter for generated code.
        assert_eq!(
            code(f.call(&host, "process", "run", json!({"program":"olg-scrape-sh"}))),
            ErrorCode::Forbidden
        );
        crate::inference::runtime_env::set_runtime_env_value(
            f.root.path(),
            sandbox::SANDBOX_KEY,
            "{}",
        )
        .unwrap();
        // Both domain spellings require the entry's explicit permission.
        for field in ["allowed_domains", "net_domains"] {
            let mut args = json!({"program":"trusted-cat"});
            args[field] = json!(["example.test"]);
            assert_eq!(
                code(f.call(&host, "process", "run", args)),
                ErrorCode::Forbidden
            );
        }
        // The same interpreter without the flag is still sandboxed.
        assert_eq!(
            f.call(&host, "process", "run", json!({"program":"trusted-cat"}))
                .unwrap()["stdout"],
            "generated code host"
        );
    });
}
