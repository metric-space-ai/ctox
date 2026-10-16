//! Supervised, app-neutral Node execution. Core owns policy, persistence and grants.
//! App code has no host credentials and receives only an inherited LocalTransport.
use anyhow::{bail, ensure, Context, Result};
#[cfg(feature = "core-store")]
pub use ctox_app_capabilities_contract as cap_contract;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub mod artifact_text;
#[cfg(feature = "core-store")]
pub mod capability_host;
#[cfg(all(test, feature = "core-store"))]
mod completion_fence_tests;
#[cfg(feature = "core-store")]
pub mod core_host;
#[cfg(all(test, feature = "core-store"))]
mod core_tests;
#[cfg(feature = "core-store")]
pub mod daemon;
pub mod delivery;
pub mod ipc;
#[cfg(feature = "legacy-store")]
pub mod legacy_store;
#[path = "../execution/models/local_transport.rs"]
pub mod local_transport;
#[cfg(feature = "core-store")]
pub mod registration;
#[cfg(feature = "core-store")]
pub mod service;
pub mod shared;
#[cfg(feature = "core-store")]
mod state;
use local_transport::{LocalStream, LocalTransport};

const WORKER: &str = include_str!("worker.mjs");
const MANGO: &str = include_str!("handler-kit/mango.mjs");
const CONTEXT: &str = include_str!("handler-kit/context.mjs");
const TRANSACTION: &str = include_str!("handler-kit/transaction.mjs");
const FRAME_LIMIT: usize = 8 * 1024 * 1024;
const RESULT_LIMIT: usize = 1024 * 1024;
const CALL_LIMIT: Duration = Duration::from_secs(30);
const IDLE_LIMIT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HandlerError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub uncertain: bool,
}
impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for HandlerError {}
struct FinishBinding(Arc<dyn RuntimeHost>, Value);
impl Drop for FinishBinding {
    fn drop(&mut self) {
        self.0.finish(&self.1);
    }
}
fn reference_file(reference: &str) -> Result<&str> {
    let mut parts = reference.split('#');
    let file = parts.next().unwrap_or("");
    ensure!(
        safe_path(file) && !file.rsplit('/').next().unwrap_or("").starts_with('_'),
        "invalid: handler/library reference"
    );
    if let Some(export) = parts.next() {
        ensure!(
            !export.is_empty()
                && export
                    .bytes()
                    .enumerate()
                    .all(|(i, c)| c.is_ascii_alphabetic()
                        || c == b'_'
                        || c == b'$'
                        || i > 0 && c.is_ascii_digit()),
            "invalid: handler export"
        );
    }
    ensure!(parts.next().is_none(), "invalid: handler reference");
    Ok(file)
}
/// Implemented by the Core. Never implemented by an app or supplied over IPC.
/// All methods run outside the store writer. The write adapter must enqueue
/// into the existing authoritative writer, not mutate a JS mirror.
pub trait RuntimeHost: Send + Sync + 'static {
    /// Core resolves the active shell once before the handler starts. Empty
    /// means shared imports are unavailable, never a host-filesystem fallback.
    fn shared_modules(&self, _binding: &Value) -> Result<shared::SharedModules> {
        Ok(shared::SharedModules::default())
    }
    fn authorize(&self, app: &str, actor: &Value, operation: &Value) -> Result<()>;
    fn exported(&self, owner: &str, collection: &str, operation: &str) -> Result<bool>;
    fn capability_available(&self, name: &str) -> bool;
    fn invoke(&self, app: &str, actor: &Value, method: &str, args: &Value) -> Result<Value>;
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()> {
        self.authorize(string(manifest, "app_id")?, &binding["command"]["actor"], &json!({"operation":"invoke","command":binding["command"],"command_rights":manifest["command_rights"]}))
    }
    fn invoke_bound(&self, binding: &Value, method: &str, args: &Value) -> Result<Value> {
        self.invoke(
            string(&binding["command"], "app_id")?,
            &binding["command"]["actor"],
            method,
            args,
        )
    }
    /// Revalidate the retained executor before accepting a terminal IPC frame.
    /// Production start_bound uses CoreAuthority's current lease/fence gate;
    /// durable completion still performs its own atomic Store-writer check.
    fn complete_bound(&self, binding: &Value) -> Result<()> {
        self.start_bound(binding, &mut || Ok(()))
    }
    fn finish(&self, _binding: &Value) {}
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn start_bound(&self, _binding: &Value, effect: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        effect()
    }
}

#[derive(Clone)]
pub struct AppBundle {
    manifest: Value,
    sources: BTreeMap<String, String>,
    grants: BTreeSet<String>,
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("invalid: missing {key}"))
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains(':'))
}
fn declared<'a>(names: impl Iterator<Item = &'a str>, name: &str) -> bool {
    names
        .into_iter()
        .any(|prefix| name == prefix || name.starts_with(&format!("{prefix}.")))
}
impl AppBundle {
    /// Source and grants come from the verified installed release and the Core
    /// approval store. This validates hashes; it is not a signature verifier.
    pub fn new(
        manifest: Value,
        sources: BTreeMap<String, String>,
        grants: BTreeSet<String>,
    ) -> Result<Self> {
        string(&manifest, "app_id")?;
        ipc::validate(&manifest)?;
        ensure!(
            manifest["collections"].is_object() && manifest["handlers"].is_object(),
            "invalid: manifest sections"
        );
        let files = manifest["files"]
            .as_object()
            .context("invalid: files are required")?;
        let mut size = 0usize;
        for (path, source) in &sources {
            ensure!(safe_path(path), "invalid: app-relative module path");
            let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
            ensure!(
                files.get(path).and_then(Value::as_str) == Some(hash.as_str()),
                "conflict: module hash mismatch"
            );
            size = size
                .checked_add(source.len())
                .context("quota: source size")?;
        }
        ensure!(
            size <= FRAME_LIMIT / 2,
            "quota: bundled app source exceeds 4 MiB"
        );
        for path in manifest["handlers"].as_object().unwrap().values() {
            ensure!(
                path.as_str().is_some_and(|p| reference_file(p).is_ok_and(|file| sources.contains_key(file))),
                "invalid: handler module missing"
            );
        }
        for spec in manifest["migrations"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
        {
            if let Some(path) = spec.get("migrate_fn") {
                ensure!(
                    path.as_str().is_some_and(
                        |p| reference_file(p).is_ok_and(|file| sources.contains_key(file))
                    ),
                    "invalid: migration module missing"
                );
            }
        }
        if let Some(caps) = manifest.get("capabilities_required") {
            ensure!(
                caps.as_array().is_some_and(|items| items
                    .iter()
                    .all(|name| name.as_str().is_some_and(valid_capability))),
                "invalid: capabilities_required"
            );
        }
        if let Some(foreign) = manifest.get("foreign_collections") {
            for (name, rule) in foreign
                .as_object()
                .context("invalid: foreign_collections")?
            {
                ensure!(
                    manifest["collections"].get(name).is_none(),
                    "invalid: foreign collection overlaps owned collection"
                );
                ensure!(
                    string(rule, "owner_app")? != string(&manifest, "app_id")?,
                    "invalid: foreign owner"
                );
                ensure!(
                    rule["ops"].as_array().is_some_and(|ops| ops
                        .iter()
                        .all(|op| matches!(op.as_str(), Some("read" | "write" | "watch")))),
                    "invalid: foreign operations"
                );
            }
        }
        Ok(Self {
            manifest,
            sources,
            grants,
        })
    }
    pub fn app_id(&self) -> &str {
        self.manifest["app_id"].as_str().unwrap()
    }
    fn capabilities(&self) -> impl Iterator<Item = &str> {
        self.manifest["capabilities_required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }
}
fn valid_capability(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
                && !matches!(part, "constructor" | "prototype" | "__proto__")
        })
}
struct Process {
    child: Arc<Mutex<Child>>,
    stream: BufReader<LocalStream>,
    socket: Option<PathBuf>,
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = &self.socket {
            let _ = std::fs::remove_file(path);
        }
    }
}
fn send(stream: &mut LocalStream, frame: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(frame)?;
    ensure!(bytes.len() <= FRAME_LIMIT, "quota: runtime frame");
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}
fn receive(stream: &mut impl Read, limit: usize) -> Result<Value> {
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    loop {
        ensure!(
            stream
                .read(&mut byte)
                .context("unavailable: app IPC read failed")?
                != 0,
            "unavailable: app process closed IPC"
        );
        if byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
        ensure!(bytes.len() <= limit, "quota: runtime response");
    }
    serde_json::from_slice(&bytes).context("invalid: runtime protocol JSON")
}
/// Restrict inherited authority to the explicitly configured stdio handles.
/// In particular, an inherited file descriptor would bypass Node fs permissions.
fn restrict_inherited_handles(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: only async-signal-safe syscalls are used after fork. Descriptor
        // flags change in the child fd table only; parent handles are untouched.
        unsafe {
            command.pre_exec(|| {
                #[cfg(target_os = "linux")]
                if libc::syscall(
                    libc::SYS_close_range,
                    3u32,
                    u32::MAX,
                    libc::CLOSE_RANGE_CLOEXEC,
                ) == 0
                {
                    return Ok(());
                }
                let mut limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // Fail closed instead of scanning an unbounded descriptor range.
                if limit.rlim_cur > 1_048_576 {
                    return Err(std::io::Error::from_raw_os_error(libc::ENOTSUP));
                }
                for fd in 3..limit.rlim_cur as i32 {
                    let flags = libc::fcntl(fd, libc::F_GETFD);
                    if flags < 0 {
                        let error = std::io::Error::last_os_error();
                        if error.raw_os_error() != Some(libc::EBADF) {
                            return Err(error);
                        }
                        continue;
                    }
                    if libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    let _ = command;
}
struct SocketCleanup(Option<PathBuf>);
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
impl Process {
    fn start(node: &Path, root: &Path) -> Result<Self> {
        let id = Uuid::new_v4();
        let transport = LocalTransport::ipc_for_host(
            root.join(format!("{:016x}.sock", id.as_u128() as u64)),
            format!("ctox-app-{id}"),
        );
        ensure!(
            transport.is_private_ipc(),
            "forbidden: only private LocalTransport is permitted"
        );
        let mut listener = transport.bind()?;
        let mut cleanup = SocketCleanup(transport.unix_socket_path().map(Path::to_path_buf));
        let inherited = transport.connect_blocking(Duration::from_secs(5))?;
        let stream = listener.accept()?;
        let stdout = inherited.try_clone()?.into_child_stdio()?;
        let stdin = inherited.into_child_stdio()?;
        let worker = format!("{MANGO}\n{CONTEXT}\n{TRANSACTION}\n{WORKER}");
        let mut command = Command::new(node);
        command
            .args([
                "--permission",
                "--max-old-space-size=256",
                "--experimental-vm-modules",
                "--input-type=module",
                "--eval",
                &worker,
            ])
            .env_clear()
            .stdin(stdin)
            .stdout(stdout)
            .stderr(Stdio::null());
        restrict_inherited_handles(&mut command);
        let child = command
            .spawn()
            .context("unavailable: start permission-restricted Node app runtime")?;
        let mut process = Self {
            child: Arc::new(Mutex::new(child)),
            stream: BufReader::new(stream),
            socket: cleanup.0.take(),
        };
        process
            .stream
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(5)))?;
        process
            .stream
            .get_ref()
            .set_write_timeout(Some(Duration::from_secs(5)))?;
        let startup_deadline = Deadline::start(process.child.clone(), Duration::from_secs(5))?;
        let ready = receive(&mut process.stream, 4096)
            .context("unavailable: Node >=25 with empty network/filesystem permissions required")?;
        ensure!(
            ready["type"] == "ready" && ready["permissions"] == "empty",
            "forbidden: runtime did not attest empty permissions"
        );
        drop(startup_deadline);
        Ok(process)
    }
    fn alive(&self) -> bool {
        self.child
            .lock()
            .ok()
            .is_some_and(|mut child| matches!(child.try_wait(), Ok(None)))
    }
}
struct Slot {
    interrupt: Arc<std::sync::atomic::AtomicBool>,
    retired: bool,
    bundle: AppBundle,
    process: Option<Process>,
    last_use: Instant,
}
struct Entry {
    state: Mutex<Slot>,
    interrupt: Arc<std::sync::atomic::AtomicBool>,
}
impl Entry {
    fn new(bundle: AppBundle) -> Self {
        let interrupt = Arc::new(std::sync::atomic::AtomicBool::new(false));
        Self {
            state: Mutex::new(Slot {
                interrupt: interrupt.clone(),
                retired: false,
                bundle,
                process: None,
                last_use: Instant::now(),
            }),
            interrupt,
        }
    }
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Slot>> {
        self.state.lock()
    }
    fn try_lock(&self) -> std::sync::TryLockResult<std::sync::MutexGuard<'_, Slot>> {
        self.state.try_lock()
    }
}
struct Inner {
    node: PathBuf,
    root: PathBuf,
    host: Arc<dyn RuntimeHost>,
    slots: Mutex<BTreeMap<String, Arc<Entry>>>,
    lifecycle: Mutex<()>,
    call_limit: Duration,
    idle_limit: Duration,
}
/// One lazy pooled process per registered app; calls to the same app serialize.
/// Maintenance owns idle termination and replaces crashed processes without
/// replaying a failed call. Failed startups await the next authorized call.
pub struct RuntimePool {
    inner: Arc<Inner>,
}
impl RuntimePool {
    pub fn production(root: PathBuf, host: Arc<dyn RuntimeHost>) -> Result<Self> {
        let node = delivery::bundled_node(&root)?;
        Self::new(node, root.join("ipc"), host)
    }
    pub fn new(node: PathBuf, root: PathBuf, host: Arc<dyn RuntimeHost>) -> Result<Self> {
        Self::with_limits(node, root, host, CALL_LIMIT, IDLE_LIMIT)
    }
    fn with_limits(
        node: PathBuf,
        root: PathBuf,
        host: Arc<dyn RuntimeHost>,
        call_limit: Duration,
        idle_limit: Duration,
    ) -> Result<Self> {
        ensure!(
            node.is_absolute() && root.is_absolute(),
            "invalid: absolute runtime paths required"
        );
        std::fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        }
        let inner = Arc::new(Inner {
            node,
            root,
            host,
            slots: Mutex::new(BTreeMap::new()),
            lifecycle: Mutex::new(()),
            call_limit,
            idle_limit,
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("app-runtime-supervisor".into())
            .spawn(move || maintenance(weak))?;
        Ok(Self { inner })
    }
    pub fn register(&self, bundle: AppBundle) -> Result<()> {
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle lock poisoned"))?;
        let old = self
            .inner
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime lock poisoned"))?
            .get(bundle.app_id())
            .cloned();
        // Never hold the registry lock while waiting for an app call. Core
        // callbacks may need registry reads while that call completes.
        if let Some(old) = old {
            // Publication drains the running call before changing generations.
            // Unregister/revocation still interrupts immediately.
            let mut old = old
                .lock()
                .map_err(|_| anyhow::anyhow!("app lock poisoned"))?;
            old.retired = true;
            old.process = None;
        }
        self.inner
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime lock poisoned"))?
            .insert(bundle.app_id().to_owned(), Arc::new(Entry::new(bundle)));
        Ok(())
    }
    pub fn unregister(&self, app: &str) -> Result<()> {
        let _lifecycle = self
            .inner
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("lifecycle lock poisoned"))?;
        let slot = self
            .inner
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime lock poisoned"))?
            .remove(app);
        if let Some(slot) = slot {
            slot.interrupt
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let mut slot = slot
                .lock()
                .map_err(|_| anyhow::anyhow!("app lock poisoned"))?;
            slot.retired = true;
            slot.process = None;
        }
        Ok(())
    }
    pub fn invoke(&self, app: &str, actor: Value, command: &str, input: Value) -> Result<Value> {
        self.call(app, actor, command, input, false, None)
    }
    /// Production entry point: Core supplies the authenticated command and lease.
    pub fn dispatch(&self, command: Value, fence: Value) -> Result<Value> {
        let app = string(&command, "app_id")?.to_owned();
        let target = string(&command, "type")?.to_owned();
        string(&command, "id")?;
        string(&command, "app_ver")?;
        ensure!(
            command["actor"].is_object() && command.get("payload").is_some(),
            "invalid: command envelope"
        );
        let binding = json!({"command":command,"fence":fence});
        self.call(
            &app,
            binding["command"]["actor"].clone(),
            &target,
            binding["command"].clone(),
            false,
            Some(binding),
        )
    }
    pub(crate) fn dispatch_entry(
        &self,
        command: Value,
        fence: Value,
        entry: &str,
        kind: &str,
        input: Value,
    ) -> Result<Value> {
        ensure!(
            matches!(kind, "trigger" | "completion"),
            "invalid: handler kind"
        );
        let app = string(&command, "app_id")?.to_owned();
        let mut binding = json!({"command":command,"fence":fence,"entry":entry,"kind":kind});
        if kind == "trigger" {
            binding["trigger_id"] = binding["command"]["client_context"]["trigger_id"].clone();
        }
        self.call(
            &app,
            binding["command"]["actor"].clone(),
            entry,
            input,
            false,
            Some(binding),
        )
    }
    /// Core-owned migration proposal run. Commit remains a separate writer job
    /// with base_rev/generation checks; no app data is changed by this runner.
    pub fn dispatch_migration(
        &self,
        command: Value,
        fence: Value,
        collection: &str,
        docs: Vec<Value>,
    ) -> Result<Value> {
        let app = string(&command, "app_id")?.to_owned();
        string(&command, "id")?;
        string(&command, "app_ver")?;
        ensure!(command["actor"].is_object(), "invalid: migration actor");
        let mut ids = BTreeSet::new();
        for doc in &docs {
            ensure!(
                ids.insert(string(doc, "id")?.to_owned()),
                "invalid: duplicate migration id"
            );
            string(doc, "rev")?;
        }
        let binding = json!({"command":command,"fence":fence,"kind":"migration"});
        self.call(
            &app,
            binding["command"]["actor"].clone(),
            collection,
            Value::Array(docs),
            true,
            Some(binding),
        )
    }
    pub fn migrate(
        &self,
        app: &str,
        actor: Value,
        collection: &str,
        docs: Vec<Value>,
    ) -> Result<Value> {
        let mut ids = BTreeSet::new();
        for doc in &docs {
            ensure!(
                ids.insert(string(doc, "id")?.to_owned()),
                "invalid: duplicate migration id"
            );
            string(doc, "rev")?;
        }
        self.call(app, actor, collection, Value::Array(docs), true, None)
    }
    fn call(
        &self,
        app: &str,
        actor: Value,
        target: &str,
        input: Value,
        migration: bool,
        mut binding: Option<Value>,
    ) -> Result<Value> {
        let slot = self
            .inner
            .slots
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime lock poisoned"))?
            .get(app)
            .cloned()
            .context("not_found: app is not registered")?;
        let mut slot = slot
            .lock()
            .map_err(|_| anyhow::anyhow!("app lock poisoned"))?;
        ensure!(
            !slot.retired,
            "revoked: app release was unregistered or replaced"
        );
        let interrupt = slot.interrupt.clone();
        ensure!(
            !interrupt.load(std::sync::atomic::Ordering::SeqCst),
            "revoked: app was stopped"
        );
        let bundle = slot.bundle.clone();
        let operation = if migration { "migration" } else { "invoke" };
        if let Some(binding) = &mut binding {
            ensure!(
                binding["command"]["app_ver"] == bundle.manifest["app_ver"],
                "conflict: app version changed"
            );
            binding["invocation_id"] = json!(Uuid::new_v4().to_string());
            binding["manifest"] = bundle.manifest.clone();
            self.inner.host.admit(&bundle.manifest, binding)?;
        }
        let _finish = binding
            .as_ref()
            .map(|value| FinishBinding(self.inner.host.clone(), value.clone()));
        let shared = self
            .inner
            .host
            .shared_modules(binding.as_ref().unwrap_or(&Value::Null))?;
        self.inner.host.authorize(
            app,
            &actor,
            &json!({"operation":operation, "target":target}),
        )?;
        let path = if migration {
            ensure!(
                bundle.manifest["collections"].get(target).is_some(),
                "forbidden: migration collection is not owned"
            );
            string(&bundle.manifest["migrations"][target], "migrate_fn")?
        } else if binding.as_ref().is_some_and(|b| b["entry"].is_string()) {
            target
        } else {
            bundle.manifest["handlers"][target]
                .as_str()
                .context("forbidden: command has no declared handler")?
        };
        if !migration {
            for name in bundle.capabilities() {
                ensure!(
                    declared(bundle.grants.iter().map(String::as_str), name),
                    "forbidden: capability not approved"
                );
                ensure!(
                    self.inner.host.capability_available(name),
                    "unavailable: native capability requires update"
                );
            }
        }
        ensure!(
            serde_json::to_vec(&input)?.len() <= RESULT_LIMIT,
            "quota: handler input exceeds 1 MiB"
        );
        if !slot.process.as_ref().is_some_and(Process::alive) {
            slot.process = None;
            let mut start = || {
                slot.process = Some(Process::start(&self.inner.node, &self.inner.root)?);
                Ok(())
            };
            if let Some(binding) = &binding {
                self.inner.host.start_bound(binding, &mut start)?;
            } else {
                start()?;
            }
        }
        let process = slot.process.as_mut().unwrap();
        let call_id = Uuid::new_v4().to_string();
        let guard = Deadline::cpu(
            process.child.clone(),
            self.inner.call_limit,
            interrupt.clone(),
        )?;
        let result = (|| {
            send(
                process.stream.get_mut(),
                &json!({"type":"call", "id":call_id, "kind":operation,
                "path":path, "sources":bundle.sources, "shared":shared, "input":input,"core_now_ms":self.inner.host.now_ms()}),
            )?;
            let deadline = Instant::now() + Duration::from_secs(600);
            loop {
                ensure!(
                    !interrupt.load(std::sync::atomic::Ordering::SeqCst),
                    "revoked: app was stopped"
                );
                let remaining = deadline.saturating_duration_since(Instant::now());
                ensure!(!remaining.is_zero(), "quota: app call exceeded time budget");
                process.stream.get_ref().set_read_timeout(Some(remaining))?;
                let frame = receive(&mut process.stream, RESULT_LIMIT + 8192)?;
                ensure!(
                    !interrupt.load(std::sync::atomic::Ordering::SeqCst),
                    "revoked: app was stopped"
                );
                match frame["type"].as_str() {
                    Some("ctx") => {
                        ensure!(
                            frame["call_id"] == call_id && frame["callback_id"].is_u64(),
                            "invalid: callback correlation"
                        );
                        let method = string(&frame, "method")?;
                        let response = callback(
                            self.inner.host.as_ref(),
                            &bundle,
                            &actor,
                            method,
                            &frame["args"],
                            migration,
                            binding.as_ref().map(|b| {
                                let mut b = b.clone();
                                b["callback_id"] = frame["callback_id"].clone();
                                b
                            }),
                        );
                        let response = match response {
                            Ok(value) => {
                                json!({"type":"ctx_result","call_id":call_id,"callback_id":frame["callback_id"],"value":value})
                            }
                            Err(error) => {
                                json!({"type":"ctx_result","call_id":call_id,"callback_id":frame["callback_id"],
                                "error":error.downcast_ref::<HandlerError>().map(|e| serde_json::to_value(e).unwrap()).unwrap_or_else(|| json!({"code":error_code(&error.to_string()),"message":error.to_string()}))})
                            }
                        };
                        send(process.stream.get_mut(), &response)?;
                    }
                    Some("result") => {
                        ensure!(frame["id"] == call_id, "invalid: result correlation");
                        if let Some(binding) = &binding {
                            self.inner.host.complete_bound(binding)?;
                        }
                        ensure!(
                            serde_json::to_vec(&frame["value"])?.len() <= RESULT_LIMIT,
                            "quota: handler result"
                        );
                        return Ok(frame["value"].clone());
                    }
                    Some("error") => {
                        ensure!(frame["id"] == call_id, "invalid: error correlation");
                        if let Some(binding) = &binding {
                            self.inner.host.complete_bound(binding)?;
                        }
                        return Err(serde_json::from_value::<HandlerError>(
                            frame["error"].clone(),
                        )?
                        .into());
                    }
                    _ => bail!("invalid: unexpected runtime frame"),
                }
            }
        })();
        let exceeded = guard.exceeded.load(std::sync::atomic::Ordering::SeqCst);
        drop(guard);
        let result = if interrupt.load(std::sync::atomic::Ordering::SeqCst) {
            Err(anyhow::anyhow!("revoked: app was stopped"))
        } else if exceeded {
            Err(anyhow::anyhow!("quota: app exceeded CPU/time budget"))
        } else {
            result
        };
        slot.last_use = Instant::now();
        // Errors never replay writes. Recreate process on the next call, which
        // must pass the current policy again.
        if result.is_err() {
            slot.process = None;
        }
        result
    }
    pub fn process_id(&self, app: &str) -> Option<u32> {
        let slot = self.inner.slots.lock().ok()?.get(app)?.clone();
        let slot = slot.lock().ok()?;
        let child = slot.process.as_ref()?.child.lock().ok()?;
        Some(child.id())
    }
}
impl Drop for RuntimePool {
    fn drop(&mut self) {
        if let Ok(mut slots) = self.inner.slots.lock() {
            for slot in slots.values() {
                if let Ok(mut slot) = slot.lock() {
                    slot.process = None;
                }
            }
            slots.clear();
        }
    }
}
fn maintenance(weak: Weak<Inner>) {
    loop {
        thread::sleep(Duration::from_secs(1));
        let Some(inner) = weak.upgrade() else {
            return;
        };
        let Ok(slots) = inner
            .slots
            .lock()
            .map(|slots| slots.values().cloned().collect::<Vec<_>>())
        else {
            return;
        };
        for slot in slots {
            if let Ok(mut slot) = slot.try_lock() {
                if slot.retired || slot.last_use.elapsed() >= inner.idle_limit {
                    slot.process = None;
                } else if slot.process.as_ref().is_some_and(|p| !p.alive()) {
                    slot.process = None;
                    // Restart only at the next admitted call: a background
                    // respawn must not bypass current executor authority.
                }
            }
        }
    }
}
fn error_code(message: &str) -> &str {
    match message.split(':').next().unwrap_or("").trim() {
        code @ ("invalid" | "forbidden" | "not_found" | "conflict" | "revoked" | "expired"
        | "quota" | "unavailable") => code,
        _ => "unavailable",
    }
}

fn callback(
    host: &dyn RuntimeHost,
    bundle: &AppBundle,
    actor: &Value,
    method: &str,
    args: &Value,
    migration: bool,
    binding: Option<Value>,
) -> Result<Value> {
    let args = args.as_array().context("invalid: callback arguments")?;
    ensure!(args.len() <= 3, "invalid: callback arguments");
    if migration {
        ensure!(
            matches!(method, "read" | "asset" | "log"),
            "forbidden: migration has no side effects"
        );
    }
    match method {
        "asset" => {
            ensure!(
                args.len() == 1 && args[0].as_str().is_some_and(safe_path),
                "invalid: app-relative asset path"
            );
            ensure!(
                binding.is_some(),
                "unavailable: Core-bound context required"
            );
        }
        "read" | "write" => {
            let name = args
                .first()
                .and_then(Value::as_str)
                .context("invalid: collection")?;
            ensure!(
                method != "read" || name != "business_commands",
                "forbidden: read an app projection instead of command documents"
            );
            if bundle.manifest["collections"].get(name).is_none() {
                let rule = &bundle.manifest["foreign_collections"][name];
                let owner = string(rule, "owner_app")?;
                ensure!(
                    rule["ops"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op == method)),
                    "forbidden: undeclared foreign operation"
                );
                ensure!(
                    host.exported(owner, name, method)?,
                    "forbidden: collection owner withdrew export"
                );
            }
        }
        "cursor" | "transaction.begin" | "transaction.commit" | "transaction.abort" => {
            ensure!(
                binding.is_some(),
                "unavailable: Core-bound context required"
            );
        }
        "command" => {
            let command = args
                .first()
                .and_then(Value::as_str)
                .context("invalid: command")?;
            let options = args.get(2).cloned().unwrap_or(json!({}));
            let target = crate::ipc::target(&bundle.manifest, command, &options)?;
            ensure!(
                target == bundle.app_id() || binding.is_some(),
                "forbidden: foreign command requires Core binding"
            );
        }
        "capability" => {
            let name = args
                .first()
                .and_then(Value::as_str)
                .context("invalid: capability")?;
            ensure!(valid_capability(name), "invalid: capability name");
            ensure!(
                declared(bundle.capabilities(), name)
                    && declared(bundle.grants.iter().map(String::as_str), name),
                "forbidden: capability not declared and approved"
            );
            ensure!(
                host.capability_available(name),
                "unavailable: native capability"
            );
        }
        "log" => {
            ensure!(
                args.len() == 3
                    && matches!(args[0].as_str(), Some("debug" | "info" | "warn" | "error"))
                    && args[1].is_string(),
                "invalid: log entry"
            );
        }
        _ => bail!("forbidden: unknown context operation"),
    }
    let operation = json!({"operation":method,"args":args});
    host.authorize(bundle.app_id(), actor, &operation)?;
    match binding {
        Some(binding) => host.invoke_bound(&binding, method, &Value::Array(args.clone())),
        None => host.invoke(bundle.app_id(), actor, method, &Value::Array(args.clone())),
    }
}
struct Deadline {
    exceeded: Arc<std::sync::atomic::AtomicBool>,
    cancel: Arc<(Mutex<bool>, Condvar)>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Deadline {
    fn cpu(
        child: Arc<Mutex<Child>>,
        cpu_limit: Duration,
        interrupt: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self> {
        use std::sync::atomic::Ordering;
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let pid = Pid::from_u32(
            child
                .lock()
                .map_err(|_| anyhow::anyhow!("process lock poisoned"))?
                .id(),
        );
        let mut system = System::new();
        let refresh = move |system: &mut System| {
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing().with_cpu().without_tasks(),
            );
            system.process(pid).map(|p| p.accumulated_cpu_time())
        };
        let baseline = refresh(&mut system).context("unavailable: cannot measure owned app CPU")?;
        let cancel = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = cancel.clone();
        let exceeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let exhausted = exceeded.clone();
        let handle = thread::Builder::new()
            .name("app-cpu-budget".into())
            .spawn(move || {
                let start = Instant::now();
                loop {
                    let (lock, condition) = &*signal;
                    let Ok(cancelled) = lock.lock() else {
                        return;
                    };
                    let Ok((cancelled, _)) =
                        condition.wait_timeout(cancelled, Duration::from_millis(25))
                    else {
                        return;
                    };
                    if *cancelled {
                        return;
                    }
                    drop(cancelled);
                    let cpu = refresh(&mut system);
                    let over_budget = cpu.is_some_and(|value| {
                        value.saturating_sub(baseline) >= cpu_limit.as_millis() as u64
                    }) || start.elapsed() >= Duration::from_secs(600);
                    if interrupt.load(Ordering::SeqCst) || cpu.is_none() || over_budget {
                        if over_budget {
                            exhausted.store(true, Ordering::SeqCst);
                        }
                        if let Ok(mut child) = child.lock() {
                            let _ = child.kill();
                        }
                        return;
                    }
                }
            })?;
        Ok(Self {
            cancel,
            exceeded,
            handle: Some(handle),
        })
    }
    fn start(child: Arc<Mutex<Child>>, timeout: Duration) -> Result<Self> {
        let cancel = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = cancel.clone();
        let handle = thread::Builder::new()
            .name("app-call-deadline".into())
            .spawn(move || {
                let (lock, condition) = &*signal;
                if let Ok(cancelled) = lock.lock() {
                    if let Ok((cancelled, _)) =
                        condition.wait_timeout_while(cancelled, timeout, |value| !*value)
                    {
                        if !*cancelled {
                            if let Ok(mut child) = child.lock() {
                                let _ = child.kill();
                            }
                        }
                    }
                }
            })?;
        Ok(Self {
            cancel,
            exceeded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            handle: Some(handle),
        })
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        let (lock, condition) = &*self.cancel;
        if let Ok(mut cancelled) = lock.lock() {
            *cancelled = true;
            condition.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Host {
        allow: AtomicBool,
        revoke_after_read: AtomicBool,
        read_delay_ms: std::sync::atomic::AtomicU64,
        export: AtomicBool,
        records: Mutex<Vec<Value>>,
    }
    impl RuntimeHost for Host {
        fn authorize(&self, _app: &str, actor: &Value, _operation: &Value) -> Result<()> {
            ensure!(
                actor["id"] == "actor" && self.allow.load(Ordering::SeqCst),
                "forbidden: actor"
            );
            Ok(())
        }
        fn exported(&self, owner: &str, collection: &str, _operation: &str) -> Result<bool> {
            Ok(owner == "owner" && collection == "shared" && self.export.load(Ordering::SeqCst))
        }
        fn capability_available(&self, name: &str) -> bool {
            name == "model"
        }
        fn invoke(&self, _app: &str, _actor: &Value, method: &str, args: &Value) -> Result<Value> {
            match method {
                "read" => {
                    thread::sleep(Duration::from_millis(
                        self.read_delay_ms.load(Ordering::SeqCst),
                    ));
                    let value = json!(*self.records.lock().unwrap());
                    if self.revoke_after_read.load(Ordering::SeqCst) {
                        self.allow.store(false, Ordering::SeqCst);
                    }
                    Ok(value)
                }
                "write" => {
                    let doc = args[1]["doc"].clone();
                    ensure!(doc.is_object(), "invalid: document");
                    self.records.lock().unwrap().push(doc);
                    Ok(json!({"status":"accepted"}))
                }
                "log" => Ok(Value::Null),
                _ => bail!("unavailable: test capability"),
            }
        }
    }
    struct Harness {
        pool: RuntimePool,
        host: Arc<Host>,
        root: PathBuf,
    }
    impl Harness {
        fn new(call_limit: Duration, idle: Duration) -> Self {
            let node = PathBuf::from(
                std::env::var("CTOX_APP_RUNTIME_TEST_NODE")
                    .expect("set CTOX_APP_RUNTIME_TEST_NODE to a Node >=25 executable"),
            );
            let root = PathBuf::from(
                std::env::var("CTOX_APP_RUNTIME_TEST_ROOT")
                    .expect("set CTOX_APP_RUNTIME_TEST_ROOT to lane-owned scratch storage"),
            )
            .join(format!("t-{}", &Uuid::new_v4().simple().to_string()[..8]));
            let host = Arc::new(Host {
                allow: AtomicBool::new(true),
                revoke_after_read: AtomicBool::new(false),
                read_delay_ms: std::sync::atomic::AtomicU64::new(0),
                export: AtomicBool::new(true),
                records: Mutex::new(vec![]),
            });
            let pool = RuntimePool::with_limits(node, root.clone(), host.clone(), call_limit, idle)
                .unwrap();
            Self { pool, host, root }
        }
        fn register(&self, source: &str) {
            self.pool.register(bundle(source, json!({}))).unwrap();
        }
        fn invoke(&self, input: Value) -> Result<Value> {
            self.pool
                .invoke("test-app", json!({"id":"actor"}), "test.run", input)
        }
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            self.pool.unregister("test-app").unwrap();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn bundle(source: &str, additions: Value) -> AppBundle {
        let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
        let mut manifest = json!({
            "app_id":"test-app", "collections":{"records":{}},
            "handlers":{"test.run":"handler.mjs"},
            "files":{"handler.mjs":hash}, "capabilities_required":[]
        });
        for (key, value) in additions.as_object().unwrap() {
            manifest[key] = value.clone();
        }
        AppBundle::new(
            manifest,
            BTreeMap::from([("handler.mjs".into(), source.into())]),
            BTreeSet::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_changed_modules_and_paths_before_spawn() {
        let source = "export default () => null";
        let valid = bundle(source, json!({}));
        let mut changed = valid.sources.clone();
        changed.insert("handler.mjs".into(), "export default () => 42".into());
        assert!(AppBundle::new(valid.manifest.clone(), changed, BTreeSet::new()).is_err());
        assert!(!safe_path("../outside.mjs"));
        assert!(!safe_path("/outside.mjs"));
        assert!(!safe_path("node:fs"));
    }
    #[test]
    fn real_handler_reads_writes_and_reuses_one_private_process() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default async (input, ctx) => { await ctx.write('records', {type:'put',doc:input}); return await ctx.read('records', {selector:{}}); }");
        assert!(h.pool.process_id("test-app").is_none());
        assert_eq!(
            h.invoke(json!({"id":"one"})).unwrap(),
            json!([{"id":"one"}])
        );
        let pid = h.pool.process_id("test-app").unwrap();
        assert_eq!(
            h.invoke(json!({"id":"two"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(h.pool.process_id("test-app"), Some(pid));
        assert_eq!(h.host.records.lock().unwrap().len(), 2);
    }
    #[test]
    fn cpu_loop_is_killed_and_next_call_starts_a_replacement_without_replay() {
        let h = Harness::new(Duration::from_millis(150), IDLE_LIMIT);
        h.register("export default input => { if(input.loop) { while(true) {} } return input; }");
        h.invoke(json!({"loop":false})).unwrap();
        let pid = h.pool.process_id("test-app").unwrap();
        let start = Instant::now();
        assert!(h.invoke(json!({"loop":true})).is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(h.pool.process_id("test-app").is_none());
        assert_eq!(h.invoke(json!({"value":7})).unwrap(), json!({"value":7}));
        assert_ne!(h.pool.process_id("test-app"), Some(pid));
    }
    #[test]
    fn crashed_owned_process_is_reaped_and_restarted() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default input => input;");
        h.invoke(json!(1)).unwrap();
        let pid = h.pool.process_id("test-app").unwrap();
        {
            let slots = h.pool.inner.slots.lock().unwrap();
            let slot = slots["test-app"].lock().unwrap();
            let mut child = slot.process.as_ref().unwrap().child.lock().unwrap();
            child.kill().unwrap();
            child.wait().unwrap();
        }
        thread::sleep(Duration::from_millis(1200));
        assert!(h.pool.process_id("test-app").is_none());
        assert_eq!(h.invoke(json!(2)).unwrap(), json!(2));
        assert_ne!(h.pool.process_id("test-app"), Some(pid));
    }
    #[test]
    fn actor_policy_withdrawal_and_undeclared_capabilities_fail_closed() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default async (_input,ctx) => await ctx.read('records');");
        h.invoke(Value::Null).unwrap();
        h.host.allow.store(false, Ordering::SeqCst);
        assert!(h.invoke(Value::Null).is_err());
        h.host.allow.store(true, Ordering::SeqCst);
        h.register("export default async (_input,ctx) => await ctx.http({url:'https://unavailable.invalid'});");
        assert!(h.invoke(Value::Null).is_err());
    }
    #[test]
    fn foreign_exports_are_rechecked_for_every_callback() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        let app = bundle(
            "export default async (_input,ctx) => await ctx.read('shared');",
            json!({"foreign_collections":{"shared":{"owner_app":"owner","ops":["read"]}}}),
        );
        h.pool.register(app).unwrap();
        assert_eq!(h.invoke(Value::Null).unwrap(), json!([]));
        h.host.export.store(false, Ordering::SeqCst);
        assert!(h.invoke(Value::Null).is_err());
    }
    #[test]
    fn missing_required_capability_never_starts_app_code() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.pool
            .register(bundle(
                "throw new Error('must not run')",
                json!({"capabilities_required":["model"]}),
            ))
            .unwrap();
        assert!(h.invoke(Value::Null).is_err());
        assert!(h.pool.process_id("test-app").is_none());
    }
    #[test]
    fn migration_proposes_original_revision_and_cannot_write() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.pool
            .register(bundle(
                "export default doc => ({...doc,title:'new'});",
                json!({"migrations":{"records":{"migrate_fn":"handler.mjs"}}}),
            ))
            .unwrap();
        let original = json!({"id":"one","rev":"source","title":"old"});
        let proposals = h
            .pool
            .migrate(
                "test-app",
                json!({"id":"actor"}),
                "records",
                vec![original.clone()],
            )
            .unwrap();
        assert_eq!(proposals[0]["base_rev"], "source");
        assert_eq!(proposals[0]["doc"]["title"], "new");
        assert_eq!(original["title"], "old");
        h.pool.register(bundle("export default async (doc,ctx) => { await ctx.write('records',{type:'put',doc}); return doc; }",
            json!({"migrations":{"records":{"migrate_fn":"handler.mjs"}}}))).unwrap();
        assert!(h
            .pool
            .migrate("test-app", json!({"id":"actor"}), "records", vec![original])
            .is_err());
        assert!(h.host.records.lock().unwrap().is_empty());
    }
    #[test]
    fn idle_process_is_stopped_by_the_supervisor() {
        let h = Harness::new(Duration::from_secs(5), Duration::from_millis(50));
        h.register("export default () => 1;");
        h.invoke(Value::Null).unwrap();
        thread::sleep(Duration::from_millis(1200));
        assert!(h.pool.process_id("test-app").is_none());
    }
    #[test]
    fn imports_cannot_reach_host_files_or_node_builtins() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("import fs from 'node:fs'; export default () => fs.readFileSync('/etc/hostname','utf8');");
        assert!(h.invoke(Value::Null).is_err());
        h.register("export default async () => (await import('node:net')).createConnection(1,'127.0.0.1');");
        assert!(h.invoke(Value::Null).is_err());
    }

    #[test]
    fn rights_revoked_between_callbacks_prevent_the_write() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default async (_input,ctx) => { await ctx.read('records'); await ctx.write('records',{type:'put',doc:{id:'never'}}); return true; }");
        h.host.revoke_after_read.store(true, Ordering::SeqCst);
        assert!(h.invoke(Value::Null).is_err());
        assert!(h.host.records.lock().unwrap().is_empty());
    }
    #[test]
    fn hot_release_replaces_the_process_and_unregister_stops_admission() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default () => 'old';");
        assert_eq!(h.invoke(Value::Null).unwrap(), json!("old"));
        let old_pid = h.pool.process_id("test-app").unwrap();
        h.register("export default () => 'new';");
        assert!(h.pool.process_id("test-app").is_none());
        assert_eq!(h.invoke(Value::Null).unwrap(), json!("new"));
        assert_ne!(h.pool.process_id("test-app"), Some(old_pid));
        h.pool.unregister("test-app").unwrap();
        assert!(h.invoke(Value::Null).is_err());
    }
    #[test]
    fn oversized_result_is_rejected_and_not_replayed() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default () => 'x'.repeat(1024 * 1024 + 1);");
        assert!(h.invoke(Value::Null).is_err());
        assert!(h.pool.process_id("test-app").is_none());
    }

    #[test]
    fn core_io_wait_does_not_spend_the_app_cpu_budget() {
        let h = Harness::new(Duration::from_millis(150), IDLE_LIMIT);
        h.register("export default async (_input,ctx) => await ctx.read('records');");
        h.host.read_delay_ms.store(400, Ordering::SeqCst);
        let start = Instant::now();
        assert_eq!(h.invoke(Value::Null).unwrap(), json!([]));
        assert!(start.elapsed() >= Duration::from_millis(400));
    }
    #[test]
    fn pooled_calls_share_one_vm_context_but_new_releases_reset_it() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        let source = "let count=0; export default () => ++count;";
        h.register(source);
        assert_eq!(h.invoke(Value::Null).unwrap(), json!(1));
        assert_eq!(h.invoke(Value::Null).unwrap(), json!(2));
        h.register(source);
        assert_eq!(h.invoke(Value::Null).unwrap(), json!(1));
    }
    #[test]
    fn cpu_budget_is_reset_for_every_call_of_the_same_process() {
        let h = Harness::new(Duration::from_millis(150), IDLE_LIMIT);
        h.register("export default () => { const end=Date.now()+70; while(Date.now()<end){} return true; };");
        for _ in 0..4 {
            assert_eq!(h.invoke(Value::Null).unwrap(), json!(true));
        }
    }
    #[test]
    fn unregister_interrupts_a_running_handler_without_waiting_for_its_cpu_limit() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default input => { if(input.loop) { while(true){} } return true; };");
        h.invoke(json!({"loop":false})).unwrap();
        thread::scope(|scope| {
            let running = scope.spawn(|| h.invoke(json!({"loop":true})));
            thread::sleep(Duration::from_millis(100));
            let start = Instant::now();
            h.pool.unregister("test-app").unwrap();
            assert!(running.join().unwrap().is_err());
            assert!(start.elapsed() < Duration::from_secs(3));
        });
    }

    #[test]
    fn a_crash_during_a_call_is_unavailable_instead_of_cpu_exhaustion() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        h.register("export default input => { if(input.loop) { while(true){} } return true; };");
        h.invoke(json!({"loop":false})).unwrap();
        let owned_child = {
            let slots = h.pool.inner.slots.lock().unwrap();
            let slot = slots["test-app"].lock().unwrap();
            slot.process.as_ref().unwrap().child.clone()
        };
        thread::scope(|scope| {
            let running = scope.spawn(|| h.invoke(json!({"loop":true})));
            thread::sleep(Duration::from_millis(100));
            {
                let mut child = owned_child.lock().unwrap();
                child.kill().unwrap();
                child.wait().unwrap();
            }
            let error = running.join().unwrap().unwrap_err();
            assert_eq!(error_code(&error.to_string()), "unavailable");
        });
        assert_eq!(h.invoke(json!({"loop":false})).unwrap(), json!(true));
    }

    #[cfg(unix)]
    #[test]
    fn unrelated_inheritable_file_handles_are_closed_before_node_exec() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        let path = h.root.join("owned-fd-sentinel");
        std::fs::write(&path, "owned test data").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        // Deliberately create a high, inheritable descriptor, owned by this test.
        let raw = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 200) };
        assert!(raw >= 200);
        let owned = unsafe { OwnedFd::from_raw_fd(raw) };
        let script = format!("import fs from 'node:fs'; try {{ fs.readFileSync({raw}); process.exit(1); }} catch(e) {{ if(e.code !== 'EBADF') process.exit(2); }}");
        let mut command = Command::new(&h.pool.inner.node);
        command
            .args(["--permission", "--input-type=module", "--eval", &script])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        restrict_inherited_handles(&mut command);
        assert!(command.status().unwrap().success());
        let parent_flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
        assert!(parent_flags >= 0);
        assert_eq!(parent_flags & libc::FD_CLOEXEC, 0);
        drop(owned);
        // Child-only descriptor flags must not alter or close the parent's file.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned test data");
    }
    #[test]
    fn failed_node_spawn_does_not_leave_socket_files() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        let before = std::fs::read_dir(&h.root).unwrap().count();
        assert!(Process::start(&h.root.join("missing-node"), &h.root).is_err());
        assert_eq!(std::fs::read_dir(&h.root).unwrap().count(), before);
    }

    #[test]
    fn restricted_node_denies_direct_filesystem_and_network_access() {
        let h = Harness::new(Duration::from_secs(5), IDLE_LIMIT);
        let sentinel = h.root.join("sentinel.txt");
        std::fs::write(&sentinel, "owned test data").unwrap();
        let script = format!(
            r#"
            import fs from 'node:fs';
            import net from 'node:net';
            let denied = 0;
            try {{ fs.readFileSync({}); }} catch(e) {{ if(e.code === 'ERR_ACCESS_DENIED') denied++; }}
            try {{
                await new Promise((resolve,reject) => {{
                    const socket = net.createConnection(1,'127.0.0.1');
                    socket.once('error',reject);
                    socket.once('connect',() => {{ socket.destroy(); resolve(); }});
                }});
            }} catch(e) {{ if(e.code === 'ERR_ACCESS_DENIED') denied++; }}
            if(denied !== 2) {{ console.error('expected two denied resources, got',denied); process.exit(1); }}
        "#,
            serde_json::to_string(&sentinel).unwrap()
        );
        let status = Command::new(&h.pool.inner.node)
            .args(["--permission", "--input-type=module", "--eval", &script])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }
}
