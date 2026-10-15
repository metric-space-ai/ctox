//! Core bindings for the single Store writer and the CAP-1 registry.
//! App-data schemas stay in Core; scheduling metadata shares its Store writer.
use crate::{HandlerError, RuntimeHost};
use anyhow::{bail, ensure, Context, Result};
use ctox_app_capabilities_contract::{
    policy::CapabilityHost,
    protocol::{CapMessage, CapOutcome},
    CapCallContext, CapabilityBackend, CapabilityRegistry,
};
use ctox_store::{
    read_pool::ReadPool, writer::WriterHandle, JobOptions, ReadJob, StoreError, WriteJob,
};
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::runtime::{Handle, RuntimeFlavor};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreWrite {
    pub collection: String,
    pub op: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreSnapshot {
    /// Core's revision/generation/change-head witness, not interpreted by JS.
    pub stamp: Value,
    pub collections: BTreeMap<String, Vec<Value>>,
}
/// Implemented by the authoritative Store adapter as its table API lands.
/// Every method applies current row/field/schema policy to binding, including
/// base_rev/generation checks and payload-free delete + history erasure (§16.23).
/// write/platform MUST check the actual fence row inside this same transaction;
/// revoke/claim uses this same writer, so it cannot interleave before commit.
/// A batch is applied wholly or returns Err; the writer rolls the whole job back.
pub trait StoreCodec: Send + Sync + 'static {
    fn check_fence(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
    ) -> std::result::Result<(), StoreError>;
    fn read(
        &self,
        db: &Connection,
        binding: &Value,
        collection: &str,
    ) -> std::result::Result<Vec<Value>, StoreError>;
    fn snapshot(
        &self,
        db: &Connection,
        binding: &Value,
        collections: &[String],
    ) -> std::result::Result<StoreSnapshot, StoreError>;
    fn write(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        stamp: Option<&Value>,
        operations: &[StoreWrite],
    ) -> std::result::Result<Vec<Value>, StoreError>;
    /// Core command acceptance; runtime cursor/trigger/completion metadata is
    /// implemented by state.rs in the same writer transaction.
    fn platform(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        method: &str,
        args: &Value,
    ) -> std::result::Result<Value, StoreError>;
    /// Immutable admitted business_app_versions manifest and Transfer-CAS bytes.
    fn version(
        &self,
        db: &Connection,
        app: &str,
        version: &str,
    ) -> std::result::Result<Value, StoreError>;
    fn artifact(&self, db: &Connection, hash: &str) -> std::result::Result<Vec<u8>, StoreError>;
}
/// Authenticated Core state, never constructed from handler arguments.
/// No permissive defaults: command_rights/app-write rights before handler,
//  active grants, actual executor lease and retained authority at effect point.
pub trait CoreAuthority: Send + Sync + 'static {
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()>;
    fn authorize(&self, app: &str, actor: &Value, operation: &Value) -> Result<()>;
    fn exported(&self, owner: &str, collection: &str, operation: &str) -> Result<bool>;
    fn capability_available(&self, name: &str) -> bool;
    fn with_fence(
        &self,
        binding: &Value,
        effect: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value>;
    fn capability_context(&self, binding: &Value, call_id: &str) -> Result<CapCallContext>;
    fn grants(&self, app: &str, version: &str) -> Result<std::collections::BTreeSet<String>>;
    fn event_binding(&self, app: &str, version: &str, event: &Value) -> Result<Value>;
    fn now_ms(&self) -> u64;
    fn log(&self, binding: &Value, args: &Value) -> Result<Value>;
}
/// Bytes use the exact owned CAP-1 cap.invoke/cap.result contract. The daemon
/// may implement this with its native registry; this adapter uses that same
/// production registry source, including policy/fence/audit, via its contract crate.
pub trait CapabilityRouter: Send + Sync + 'static {
    fn prepare(&self, function: &str, request: &Value) -> Result<(String, String, Value)>;
    fn exchange(&self, message: &[u8]) -> Result<Vec<u8>>;
}
pub struct RegistryRouter<H, B> {
    pub registry: CapabilityRegistry,
    pub host: H,
    pub backend: B,
}
impl<H: CapabilityHost + Send + Sync + 'static, B: CapabilityBackend + Send + Sync + 'static>
    CapabilityRouter for RegistryRouter<H, B>
{
    fn prepare(&self, full_name: &str, request: &Value) -> Result<(String, String, Value)> {
        let descriptors = self.registry.descriptors();
        let matches: Vec<_> = descriptors
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                format!(
                    "{}.{}",
                    d["name"].as_str().unwrap(),
                    d["fn"].as_str().unwrap()
                ) == full_name
                    || d["name"] == full_name && d["fn"] == "invoke"
            })
            .collect();
        ensure!(
            matches.len() == 1,
            "unavailable: native capability function requires update"
        );
        let descriptor = matches[0];
        let properties = &descriptor["input"]["properties"];
        let input = request
            .as_object()
            .context("invalid: capability request object")?;
        // Preserve the closed CAP-1 wire envelope when explicitly supplied.
        let args = if input.get("payload").is_some_and(Value::is_object)
            && input.keys().all(|k| properties.get(k).is_some())
        {
            request.clone()
        } else {
            let mut payload = input.clone();
            let mut args = json!({});
            if properties.get("action").is_some() {
                if let Some(action) = payload.remove("action") {
                    args["action"] = action;
                }
            }
            if let Some(id) = input.get("record_id") {
                args["record_id"] = id.clone();
            }
            args["payload"] = Value::Object(payload);
            args
        };
        Ok((
            descriptor["name"].as_str().unwrap().into(),
            descriptor["fn"].as_str().unwrap().into(),
            args,
        ))
    }
    fn exchange(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        Ok(self
            .registry
            .handle(&self.host, &self.backend, CapMessage::decode(bytes)?)?
            .encode()?)
    }
}
#[derive(Clone)]
struct PendingTransaction {
    invocation: String,
    snapshot: StoreSnapshot,
}
pub struct CoreRuntimeHost {
    pub writer: WriterHandle,
    pub readers: ReadPool,
    pub codec: Arc<dyn StoreCodec>,
    pub authority: Arc<dyn CoreAuthority>,
    pub capabilities: Arc<dyn CapabilityRouter>,
    handle: Handle,
    transactions: Mutex<BTreeMap<String, PendingTransaction>>,
}
fn failure(code: &str, message: &str, uncertain: bool) -> anyhow::Error {
    HandlerError {
        code: code.into(),
        message: message.into(),
        path: None,
        uncertain,
    }
    .into()
}
fn store_failure(error: StoreError) -> anyhow::Error {
    let code = match &error {
        StoreError::Busy => "quota",
        StoreError::Cancelled | StoreError::DeadlineExceeded => "expired",
        StoreError::Invalid(value) => crate::error_code(value),
        _ => "unavailable",
    };
    // SQL/error parameters can contain unmasked records.
    failure(code, "Core Store rejected operation", false)
}
fn denied(error: anyhow::Error) -> StoreError {
    StoreError::Invalid(format!(
        "{}: runtime authority denied",
        crate::error_code(&error.to_string())
    ))
}
impl CoreRuntimeHost {
    pub fn new(
        writer: WriterHandle,
        readers: ReadPool,
        codec: Arc<dyn StoreCodec>,
        authority: Arc<dyn CoreAuthority>,
        capabilities: Arc<dyn CapabilityRouter>,
        handle: Handle,
    ) -> Result<Self> {
        ensure!(
            matches!(handle.runtime_flavor(), RuntimeFlavor::MultiThread),
            "invalid: multi-thread Core runtime required"
        );
        let host = Self {
            writer,
            readers,
            codec,
            authority,
            capabilities,
            handle,
            transactions: Mutex::new(BTreeMap::new()),
        };
        let init = host
            .writer
            .submit(WriteJob::new(Self::options(), |tx| {
                crate::state::initialize(tx)
            }))
            .map_err(store_failure)?;
        host.wait(init)?;
        Ok(host)
    }
    fn wait<T>(
        &self,
        future: impl Future<Output = std::result::Result<T, StoreError>>,
    ) -> Result<T> {
        let run = || self.handle.block_on(future).map_err(store_failure);
        match Handle::try_current() {
            Ok(current) => {
                ensure!(
                    matches!(current.runtime_flavor(), RuntimeFlavor::MultiThread),
                    "unavailable: cannot block single-thread executor"
                );
                tokio::task::block_in_place(run)
            }
            Err(_) => run(),
        }
    }
    fn options() -> JobOptions {
        JobOptions {
            deadline: Some(Instant::now() + Duration::from_secs(10)),
            ..Default::default()
        }
    }
    pub fn release(&self, app: &str, version: &str) -> Result<Value> {
        let codec = self.codec.clone();
        let app = app.to_owned();
        let version = version.to_owned();
        self.wait(
            self.readers
                .read_job(ReadJob::new(Self::options(), move |db| {
                    codec.version(db, &app, &version)
                })),
        )
    }
    pub fn artifact(&self, hash: &str) -> Result<Vec<u8>> {
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid: CAS hash"
        );
        let codec = self.codec.clone();
        let hash = hash.to_owned();
        self.wait(
            self.readers
                .read_job(ReadJob::new(Self::options(), move |db| {
                    codec.artifact(db, &hash)
                })),
        )
    }
    fn read(&self, binding: &Value, collection: &str) -> Result<Value> {
        let codec = self.codec.clone();
        let binding = binding.clone();
        let collection = collection.to_owned();
        let rows = self.wait(
            self.readers
                .read_job(ReadJob::new(Self::options(), move |db| {
                    codec.read(db, &binding, &collection)
                })),
        )?;
        ensure!(
            serde_json::to_vec(&rows)?.len() <= 6 * 1024 * 1024,
            "quota: authorized snapshot exceeds IPC budget"
        );
        Ok(json!({"kind":"ctox.authorized_snapshot.v1","documents":rows}))
    }
    pub fn platform(&self, binding: &Value, method: &str, args: &Value) -> Result<Value> {
        let codec = self.codec.clone();
        let authority = self.authority.clone();
        let binding = binding.clone();
        let method = method.to_owned();
        let args = args.clone();
        let job = self
            .writer
            .submit(WriteJob::new(Self::options(), move |tx| {
                authority
                    .with_fence(&binding, &mut || {
                        codec.check_fence(tx, &binding).map_err(store_failure)?;
                        if method.starts_with("trigger.") || method.starts_with("completion.") {
                            crate::state::apply(tx, &binding, &method, &args).map_err(store_failure)
                        } else {
                            codec
                                .platform(tx, &binding, &method, &args)
                                .map_err(store_failure)
                        }
                    })
                    .map_err(denied)
            }))
            .map_err(store_failure)?;
        self.wait(job)
    }
    fn write_batch(
        &self,
        binding: &Value,
        stamp: Option<Value>,
        operations: Vec<StoreWrite>,
    ) -> Result<Value> {
        ensure!(operations.len() <= 4096, "quota: transaction write count");
        let codec = self.codec.clone();
        let authority = self.authority.clone();
        let binding = binding.clone();
        let job = self
            .writer
            .submit(WriteJob::new(Self::options(), move |tx| {
                authority
                    .with_fence(&binding, &mut || {
                        codec.check_fence(tx, &binding).map_err(store_failure)?;
                        let result = codec
                            .write(tx, &binding, stamp.as_ref(), &operations)
                            .map_err(store_failure)?;
                        Ok(serde_json::to_value(result)?)
                    })
                    .map_err(denied)
            }))
            .map_err(store_failure)?;
        self.wait(job)
    }
    fn begin(&self, binding: &Value) -> Result<Value> {
        let mut names: Vec<String> = binding["manifest"]["collections"]
            .as_object()
            .context("invalid: manifest collections")?
            .keys()
            .cloned()
            .collect();
        for (name, rule) in binding["manifest"]["foreign_collections"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.iter())
        {
            if rule["ops"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == "read"))
            {
                ensure!(
                    self.authority
                        .exported(crate::string(rule, "owner_app")?, name, "read")?,
                    "forbidden: foreign export withdrawn"
                );
                names.push(name.clone());
            }
        }
        // A declared write-only foreign collection must not leak a read snapshot.
        let mut write_only = Vec::new();
        for (name, rule) in binding["manifest"]["foreign_collections"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.iter())
        {
            if !names.contains(name)
                && rule["ops"]
                    .as_array()
                    .is_some_and(|ops| ops.iter().any(|op| op == "write"))
            {
                ensure!(
                    self.authority
                        .exported(crate::string(rule, "owner_app")?, name, "write")?,
                    "forbidden: foreign write export withdrawn"
                );
                write_only.push(name.clone());
            }
        }
        let codec = self.codec.clone();
        let context = binding.clone();
        let snapshot = self.wait(
            self.readers
                .read_job(ReadJob::new(Self::options(), move |db| {
                    codec.snapshot(db, &context, &names)
                })),
        )?;
        let invocation = crate::string(binding, "invocation_id")?.to_owned();
        let mut pending = self
            .transactions
            .lock()
            .map_err(|_| failure("unavailable", "Transaction lock poisoned", false))?;
        ensure!(
            !pending.values().any(|v| v.invocation == invocation),
            "conflict: transaction already active"
        );
        ensure!(
            serde_json::to_vec(&snapshot.collections)?.len() <= 6 * 1024 * 1024,
            "quota: transaction snapshot exceeds IPC budget"
        );
        let token = Uuid::new_v4().to_string();
        let collections = serde_json::to_value(&snapshot.collections)?;
        pending.insert(
            token.clone(),
            PendingTransaction {
                invocation,
                snapshot,
            },
        );
        Ok(json!({"token":token,"collections":collections,"write_only":write_only}))
    }
    fn capability(&self, binding: &Value, name: &str, args: &Value) -> Result<Value> {
        let args = args.as_array().context("invalid: capability args")?;
        ensure!(
            args.len() == 1 && args[0].is_object(),
            "invalid: native capability takes one request object"
        );
        let options = args[0]
            .get("payload")
            .filter(|v| v.is_object())
            .unwrap_or(&args[0]);
        let callbacks = if name == "agent.task.enqueue" {
            let mut refs = json!({});
            for key in ["on_complete", "on_failed"] {
                if let Some(reference) = options.get(key) {
                    let file = crate::reference_file(
                        reference
                            .as_str()
                            .context("invalid: completion reference")?,
                    )?;
                    ensure!(
                        file.starts_with("handlers/")
                            && binding["manifest"]["files"].get(file).is_some(),
                        "invalid: completion is not in the admitted handler bundle"
                    );
                    refs[key] = reference.clone();
                }
            }
            refs
        } else {
            json!({})
        };
        let (namespace, function, native_args) = self.capabilities.prepare(name, &args[0])?;
        let id = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}:{}:{}",
                    binding["command"]["id"], binding["invocation_id"], binding["callback_id"]
                )
                .as_bytes()
            )
        );
        let ctx = self.authority.capability_context(binding, &id)?;
        let request = CapMessage::Invoke {
            request_id: id.clone(),
            ctx,
            name: namespace.into(),
            function: function.into(),
            args: native_args,
        };
        let reply = CapMessage::decode(&self.capabilities.exchange(&request.encode()?)?)?;
        match reply {
            CapMessage::Result {
                request_id,
                outcome,
            } if request_id == id => match outcome {
                CapOutcome::Ok { value } => {
                    if callbacks.as_object().is_some_and(|m| !m.is_empty()) {
                        let task_id = value["task_id"].as_str().ok_or_else(|| {
                            failure(
                                "unavailable",
                                "Task effect has no completion-binding receipt; reconcile",
                                true,
                            )
                        })?;
                        self.platform(
                            binding,
                            "completion.register",
                            &json!({"task_id":task_id,"callbacks":callbacks}),
                        )
                        .map_err(|_| {
                            failure(
                                "unavailable",
                                "Task accepted but completion binding requires reconciliation",
                                true,
                            )
                        })?;
                    }
                    Ok(value)
                }
                CapOutcome::Error { error } => Err(HandlerError {
                    code: serde_json::to_value(error.code)?.as_str().unwrap().into(),
                    message: error.message,
                    path: error.path,
                    uncertain: error.uncertain,
                }
                .into()),
            },
            _ => bail!("invalid: CAP-1 response correlation"),
        }
    }
}
impl RuntimeHost for CoreRuntimeHost {
    fn authorize(&self, app: &str, actor: &Value, operation: &Value) -> Result<()> {
        self.authority.authorize(app, actor, operation)
    }
    fn exported(&self, owner: &str, collection: &str, operation: &str) -> Result<bool> {
        self.authority.exported(owner, collection, operation)
    }
    fn capability_available(&self, name: &str) -> bool {
        self.authority.capability_available(name)
    }
    fn invoke(&self, _app: &str, _actor: &Value, _method: &str, _args: &Value) -> Result<Value> {
        bail!("forbidden: Core command/fence binding required")
    }
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()> {
        self.authority.admit(manifest, binding)
    }
    fn now_ms(&self) -> u64 {
        self.authority.now_ms()
    }
    fn start_bound(&self, binding: &Value, effect: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.authority.with_fence(binding, &mut || {
            effect()?;
            Ok(Value::Null)
        })?;
        Ok(())
    }
    fn finish(&self, binding: &Value) {
        if let Ok(mut pending) = self.transactions.lock() {
            pending.retain(|_, v| Some(v.invocation.as_str()) != binding["invocation_id"].as_str());
        }
    }
    fn invoke_bound(&self, binding: &Value, method: &str, args: &Value) -> Result<Value> {
        let args = args.as_array().context("invalid: context args")?;
        self.authority.admit(&binding["manifest"], binding)?;
        match method {
            "read" => self.read(binding, args[0].as_str().context("invalid: collection")?),
            "write" => {
                let result = self.write_batch(
                    binding,
                    None,
                    vec![StoreWrite {
                        collection: args[0].as_str().context("invalid: collection")?.into(),
                        op: args[1].clone(),
                    }],
                )?;
                result
                    .as_array()
                    .and_then(|r| r.first())
                    .cloned()
                    .context("unavailable: missing write result")
            }
            "transaction.begin" => self.begin(binding),
            "transaction.abort" | "transaction.commit" => {
                let token = args[0].as_str().context("invalid: transaction token")?;
                let mut pending = self
                    .transactions
                    .lock()
                    .map_err(|_| failure("unavailable", "Transaction lock poisoned", false))?;
                let snapshot = pending
                    .get(token)
                    .context("conflict: transaction unavailable")?;
                ensure!(
                    Some(snapshot.invocation.as_str()) == binding["invocation_id"].as_str(),
                    "forbidden: foreign transaction"
                );
                let snapshot = pending.remove(token).unwrap();
                drop(pending);
                if method == "transaction.abort" {
                    return Ok(Value::Null);
                }
                let operations: Vec<StoreWrite> = serde_json::from_value(args[1].clone())?;
                // Repeat manifest and live foreign-export checks for every staged
                // write. JS's snapshot and transaction plan confer no authority.
                for op in &operations {
                    if binding["manifest"]["collections"]
                        .get(&op.collection)
                        .is_none()
                    {
                        let rule = &binding["manifest"]["foreign_collections"][&op.collection];
                        ensure!(
                            rule["ops"]
                                .as_array()
                                .is_some_and(|ops| ops.iter().any(|v| v == "write"))
                                && self.authority.exported(
                                    crate::string(rule, "owner_app")?,
                                    &op.collection,
                                    "write"
                                )?,
                            "forbidden: transaction foreign write"
                        );
                    }
                    self.authority.authorize(
                        crate::string(&binding["command"], "app_id")?,
                        &binding["command"]["actor"],
                        &json!({"operation":"write","args":[op.collection,op.op]}),
                    )?;
                }
                self.write_batch(binding, Some(snapshot.snapshot.stamp), operations)
            }
            "command" => {
                let value=self.platform(binding,"command.accept",&json!({"type":args[0],"payload":args[1],"options":args.get(2).cloned().unwrap_or(json!({}))}))?;
                ensure!(
                    value["id"].is_string()
                        && matches!(value["status"].as_str(), Some("accepted" | "rejected")),
                    "invalid: command acceptance result"
                );
                Ok(value)
            }
            "cursor" => {
                ensure!(
                    binding["kind"] == "trigger" && binding["trigger_id"].is_string(),
                    "forbidden: cursor requires a Core trigger binding"
                );
                self.platform(binding, "trigger.cursor", &json!(args))
            }
            "capability" => self.capability(
                binding,
                args[0].as_str().context("invalid: capability")?,
                &args[1],
            ),
            "log" => self.authority.log(binding, &json!(args)),
            _ => bail!("forbidden: unknown Core operation"),
        }
    }
}
