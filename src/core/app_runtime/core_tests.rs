#[cfg(feature = "bundled-node")]
#[test]
fn production_uses_embedded_node_and_rejects_changed_delivered_bytes() {
    let f = Fixture::new();
    f.publish("v1","export default()=>({url:new URL('/x','https://example.invalid').href,uuid:crypto.randomUUID()});",json!({}));
    let root = f.root.path().to_path_buf();
    let service = AppRuntimeService::production(root.clone(), f.host.clone()).unwrap();
    service.published("app", "v1").unwrap();
    let value = service
        .command(command("app", "v1", "cmd", Value::Null), fence())
        .unwrap();
    assert_eq!(value["url"], "https://example.invalid/x");
    assert!(Uuid::parse_str(value["uuid"].as_str().unwrap()).is_ok());
    drop(service);
    let node = crate::delivery::bundled_node(&root).unwrap();
    assert!(node.is_absolute());
    std::fs::write(&node, b"tampered executable").unwrap();
    assert!(crate::delivery::bundled_node(&root)
        .unwrap_err()
        .to_string()
        .starts_with("conflict:"));
}
#[test]
fn client_context_cannot_grant_trigger_cursor_authority() {
    let f = Fixture::new();
    f.publish(
        "v1",
        "export default async(_command,ctx)=>await ctx.cursor.get();",
        json!({}),
    );
    let service = f.service();
    service.published("app", "v1").unwrap();
    let mut cmd = command("app", "v1", "cmd", Value::Null);
    cmd["client_context"] = json!({"trigger_id":"foreign"});
    assert!(service
        .command(cmd, fence())
        .unwrap_err()
        .to_string()
        .starts_with("forbidden:"));
    let mut binding = f.binding();
    binding["trigger_id"] = json!("foreign");
    f.host
        .platform(
            &binding,
            "trigger.claim",
            &json!({"trigger_id":"foreign","event":{"kind":"change","seq":1}}),
        )
        .unwrap();
    let error = f
        .host
        .platform(&binding, "trigger.cursor", &json!(["get", "default", null]))
        .unwrap_err();
    assert!(error.to_string().starts_with("forbidden:"));
}
// Isolated SQLite/Node/registry fixtures. No tenant/profile or model calls.
use super::*;
use crate::core_host::*;
use crate::service::{AppRuntimeService, ChangeEvent};
use ctox_app_capabilities_contract::{
    caps::NativeCall, registry::AuditEntry, Actor, CapCallContext, CapabilityBackend,
    CapabilityGrant, CapabilityHost, CapabilityRegistry, CtoxError, ErrorCode,
};
use ctox_store::{
    read_pool::ReadPool, registry::Registry, writer::WriterOptions, DbId, JobOptions, ReadJob,
    StoreError, WriteJob,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
struct Authority {
    allowed: AtomicBool,
    watch: AtomicBool,
    now: AtomicU64,
    audit: Mutex<Vec<Value>>,
    effects: Mutex<Vec<Value>>,
    started: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}
impl Authority {
    fn check(&self, binding: &Value) -> Result<()> {
        ensure!(self.allowed.load(Ordering::SeqCst), "revoked: actor");
        ensure!(
            binding["command"]["actor"]["id"] == "owner"
                && binding["command"]["actor"]["role"] == "writer",
            "forbidden: authenticated actor differs"
        );
        ensure!(binding["fence"]["fence"] == 7, "revoked: stale fence");
        ensure!(
            binding["fence"]["expires_at_core_ms"].as_u64().unwrap_or(0)
                > self.now.load(Ordering::SeqCst),
            "expired: lease"
        );
        Ok(())
    }
}
impl CoreAuthority for Authority {
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()> {
        self.check(binding)?;
        // Fixture's Core policy uses actual role, never the is_admin claim.
        if manifest["command_rights"]["admin.run"]["roles"].is_array()
            && binding["command"]["type"] == "admin.run"
        {
            anyhow::bail!("forbidden: Core command_rights");
        }
        Ok(())
    }
    fn authorize(&self, _app: &str, actor: &Value, _operation: &Value) -> Result<()> {
        ensure!(
            self.allowed.load(Ordering::SeqCst) && actor["id"] == "owner",
            "revoked: actor"
        );
        Ok(())
    }
    fn exported(&self, _owner: &str, _collection: &str, _op: &str) -> Result<bool> {
        Ok(self.watch.load(Ordering::SeqCst))
    }
    fn capability_available(&self, name: &str) -> bool {
        name == "agent.queue"
            || name.starts_with("agent.queue.")
            || name == "agent.task"
            || name.starts_with("agent.task.")
    }
    fn with_fence(
        &self,
        binding: &Value,
        effect: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value> {
        self.check(binding)?;
        effect()
    }
    fn capability_context(&self, binding: &Value, call_id: &str) -> Result<CapCallContext> {
        self.check(binding)?;
        Ok(CapCallContext {
            app_id: binding["command"]["app_id"].as_str().unwrap().into(),
            app_ver: binding["command"]["app_ver"].as_str().unwrap().into(),
            actor: Actor {
                id: "owner".into(),
                role: "writer".into(),
            },
            command_id: binding["command"]["id"].as_str().unwrap().into(),
            call_id: call_id.into(),
            fence: serde_json::from_value(binding["fence"].clone())?,
        })
    }
    fn grants(&self, _app: &str, _version: &str) -> Result<BTreeSet<String>> {
        Ok(BTreeSet::from(["agent.queue".into(), "agent.task".into()]))
    }
    fn event_binding(&self, app: &str, version: &str, event: &Value) -> Result<Value> {
        Ok(
            json!({"command":command(app,version,&format!("event-{}",Uuid::new_v4()),event.clone()),"fence":fence()}),
        )
    }
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn log(&self, _binding: &Value, args: &Value) -> Result<Value> {
        if args[1] == "started" {
            if let Some(tx) = self.started.lock().unwrap().take() {
                let _ = tx.send(());
            }
        }
        Ok(Value::Null)
    }
}
struct CapHost(Arc<Authority>);
impl CapabilityHost for CapHost {
    fn authorize(
        &self,
        ctx: &CapCallContext,
        name: &str,
        _function: &str,
        _args: &Value,
    ) -> std::result::Result<CapabilityGrant, CtoxError> {
        if !self.0.allowed.load(Ordering::SeqCst)
            || ctx.actor.id != "owner"
            || ctx.actor.role != "writer"
        {
            return Err(CtoxError::new(ErrorCode::Revoked, "actor revoked"));
        }
        Ok(CapabilityGrant {
            app_id: ctx.app_id.clone(),
            app_ver: ctx.app_ver.clone(),
            actor: ctx.actor.clone(),
            capabilities_required: BTreeSet::from([name.into()]),
            allowed_roles: BTreeSet::from(["writer".into()]),
            command_id: ctx.command_id.clone(),
        })
    }
    fn now_ms(&self) -> u64 {
        self.0.now.load(Ordering::SeqCst)
    }
    fn with_fence(
        &self,
        ctx: &CapCallContext,
        _name: &str,
        _function: &str,
        _args: &Value,
        effect: &mut dyn FnMut() -> std::result::Result<Value, CtoxError>,
    ) -> std::result::Result<Value, CtoxError> {
        if ctx.fence.fence != 7 || ctx.fence.expires_at_core_ms <= self.now_ms() {
            return Err(CtoxError::new(ErrorCode::Expired, "lease"));
        }
        effect()
    }
    fn audit(&self, entry: &AuditEntry) -> std::result::Result<(), CtoxError> {
        self.0
            .audit
            .lock()
            .unwrap()
            .push(serde_json::to_value(entry).unwrap());
        Ok(())
    }
}
struct Backend(Arc<Authority>, ctox_store::writer::WriterHandle);
impl CapabilityBackend for Backend {
    fn execute(
        &self,
        ctx: &CapCallContext,
        call: &NativeCall,
    ) -> std::result::Result<Value, CtoxError> {
        self.0.effects.lock().unwrap().push(json!({"type":call.route().command_type(),"payload":call.payload(),"record_id":call.record_id()}));
        if call.route().capability == "agent.task" && call.route().function == "request" {
            let task = Uuid::new_v4().to_string();
            let id = task.clone();
            let payload = call.payload().clone();
            let fence = ctx.fence.fence;
            self.1.submit(WriteJob::new(JobOptions::default(),move|tx| {
                if meta(tx,"fence")?!=json!(fence){return Err(StoreError::Invalid("revoked: fence".into()));}
                tx.execute("INSERT INTO docs(collection,id,rev,deleted,data) VALUES('ctox_queue_tasks',?,1,0,?)",params![id,json!({"id":id,"state":"queued","payload":payload}).to_string()])?;
                Ok(())
            })).map_err(|_|CtoxError::new(ErrorCode::Unavailable,"writer"))?.blocking_wait().map_err(|_|CtoxError::new(ErrorCode::Unavailable,"writer"))?;
            return Ok(json!({"task_id":task,"status":"queued"}));
        }
        Ok(json!({"capacity":2}))
    }
}
struct Codec;
fn meta(db: &Connection, key: &str) -> std::result::Result<Value, StoreError> {
    let value: Option<String> = db
        .query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0))
        .optional()?;
    Ok(value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(Value::Null))
}
fn set_meta(db: &Connection, key: &str, value: &Value) -> std::result::Result<(), StoreError> {
    db.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()])?;
    Ok(())
}
fn check_fence(db: &Connection, binding: &Value) -> std::result::Result<(), StoreError> {
    if meta(db, "fence")? != binding["fence"]["fence"] {
        return Err(StoreError::Invalid("revoked: fence changed".into()));
    }
    Ok(())
}
impl StoreCodec for Codec {
    fn check_fence(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
    ) -> std::result::Result<(), StoreError> {
        check_fence(tx, binding)
    }
    fn read(
        &self,
        db: &Connection,
        _binding: &Value,
        collection: &str,
    ) -> std::result::Result<Vec<Value>, StoreError> {
        let mut stmt = db
            .prepare("SELECT id,rev,data FROM docs WHERE collection=? AND deleted=0 ORDER BY id")?;
        let rows = stmt.query_map([collection], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, rev, data) = row?;
            let mut doc: Value = serde_json::from_str(&data).unwrap();
            doc["id"] = json!(id);
            doc["rev"] = json!(format!("r{rev}"));
            doc.as_object_mut().unwrap().remove("private");
            result.push(doc);
        }
        Ok(result)
    }
    fn snapshot(
        &self,
        db: &Connection,
        binding: &Value,
        collections: &[String],
    ) -> std::result::Result<StoreSnapshot, StoreError> {
        Ok(StoreSnapshot {
            stamp: json!({"seq":meta(db,"seq")?,"fence":meta(db,"fence")?}),
            collections: collections
                .iter()
                .map(|c| Ok((c.clone(), self.read(db, binding, c)?)))
                .collect::<std::result::Result<_, StoreError>>()?,
        })
    }
    fn write(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        stamp: Option<&Value>,
        operations: &[StoreWrite],
    ) -> std::result::Result<Vec<Value>, StoreError> {
        check_fence(tx, binding)?;
        if stamp.is_some_and(|s| {
            s["seq"] != meta(tx, "seq").unwrap() || s["fence"] != binding["fence"]["fence"]
        }) {
            return Err(StoreError::Invalid("conflict: snapshot changed".into()));
        }
        let mut results = Vec::new();
        for write in operations {
            let op = &write.op;
            let id = op["id"]
                .as_str()
                .or_else(|| op["doc"]["id"].as_str())
                .ok_or_else(|| StoreError::Invalid("invalid: id".into()))?;
            let current: Option<(u64, bool, Option<String>)> = tx
                .query_row(
                    "SELECT rev,deleted,data FROM docs WHERE collection=? AND id=?",
                    params![write.collection, id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let previous = current
                .as_ref()
                .map(|r| json!(format!("r{}", r.0)))
                .unwrap_or(Value::Null);
            if current.as_ref().is_some_and(|r| r.1)
                || op.get("base_rev").is_some_and(|v| *v != previous)
            {
                return Err(StoreError::Invalid("conflict: revision".into()));
            }
            let revision = current.as_ref().map_or(1, |r| r.0 + 1);
            let old = current
                .as_ref()
                .and_then(|r| r.2.as_ref())
                .and_then(|v| serde_json::from_str::<Value>(v).ok());
            let removed = op["type"] == "remove";
            let data = if removed {
                None
            } else {
                let mut next = if op["type"] == "patch" {
                    let mut doc = old
                        .clone()
                        .ok_or_else(|| StoreError::Invalid("not_found: patch".into()))?;
                    doc.as_object_mut()
                        .unwrap()
                        .extend(op["patch"].as_object().unwrap().clone());
                    doc
                } else {
                    op["doc"].clone()
                };
                if next.get("forbidden").is_some() {
                    return Err(StoreError::Invalid("forbidden: field".into()));
                }
                if let Some(private) = old.as_ref().and_then(|v| v.get("private")) {
                    next["private"] = private.clone();
                }
                next.as_object_mut().unwrap().remove("rev");
                Some(next.to_string())
            };
            tx.execute("INSERT INTO docs(collection,id,rev,deleted,data) VALUES(?,?,?,?,?) ON CONFLICT(collection,id) DO UPDATE SET rev=excluded.rev,deleted=excluded.deleted,data=excluded.data",params![write.collection,id,revision,removed,data])?;
            if removed {
                tx.execute(
                    "DELETE FROM history WHERE collection=? AND id=?",
                    params![write.collection, id],
                )?;
                tx.execute(
                    "DELETE FROM archive WHERE collection=? AND id=?",
                    params![write.collection, id],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO history(collection,id,body) VALUES(?,?,?)",
                    params![write.collection, id, data],
                )?;
            }
            set_meta(tx, "seq", &json!(meta(tx, "seq")?.as_u64().unwrap() + 1))?;
            results.push(json!({"status":"accepted","collection":write.collection,"id":id,"rev":format!("r{revision}")}));
        }
        Ok(results)
    }
    fn platform(
        &self,
        tx: &Transaction<'_>,
        binding: &Value,
        method: &str,
        args: &Value,
    ) -> std::result::Result<Value, StoreError> {
        check_fence(tx, binding)?;
        if method != "command.accept" {
            return Err(StoreError::Invalid(
                "invalid: only command acceptance belongs to Store codec".into(),
            ));
        }
        let id = args["options"]["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let task_id = Uuid::new_v4().to_string();
        let mut cmd = args.clone();
        cmd["id"] = json!(id);
        cmd["actor"] = binding["command"]["actor"].clone();
        cmd["record_id"] = args["options"]["record_id"].clone();
        cmd["module"] = args["options"]["module"].clone();
        cmd["client_context"] = args["options"]["client_context"].clone();
        self.write(
            tx,
            binding,
            None,
            &[
                StoreWrite {
                    collection: "business_commands".into(),
                    op: json!({"type":"put","doc":cmd}),
                },
                StoreWrite {
                    collection: "ctox_queue_tasks".into(),
                    op: json!({"type":"put","doc":{"id":task_id,"command_id":id,"state":"queued"}}),
                },
            ],
        )?;
        Ok(json!({"id":id,"status":"accepted","task_id":task_id}))
    }
    fn version(
        &self,
        db: &Connection,
        app: &str,
        version: &str,
    ) -> std::result::Result<Value, StoreError> {
        let source: String = db.query_row(
            "SELECT manifest FROM versions WHERE app=? AND version=?",
            params![app, version],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&source).unwrap())
    }
    fn artifact(&self, db: &Connection, hash: &str) -> std::result::Result<Vec<u8>, StoreError> {
        Ok(
            db.query_row("SELECT bytes FROM artifacts WHERE hash=?", [hash], |r| {
                r.get(0)
            })?,
        )
    }
}
fn fence() -> Value {
    json!({"run_id":"run","participant":"core","fence":7,"expires_at_core_ms":100000000})
}
fn command(app: &str, version: &str, id: &str, payload: Value) -> Value {
    json!({"id":id,"type":"run","payload":payload,"app_id":app,"app_ver":version,"actor":{"id":"owner","role":"writer","display_name":"Owner","is_admin":false}})
}
struct Fixture {
    runtime: tokio::runtime::Runtime,
    host: Arc<CoreRuntimeHost>,
    authority: Arc<Authority>,
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        std::fs::create_dir_all(std::env::var("CTOX_APP_RUNTIME_TEST_ROOT").unwrap()).unwrap();
        let root = tempfile::Builder::new()
            .prefix("app-core-")
            .tempdir_in(std::env::var("CTOX_APP_RUNTIME_TEST_ROOT").unwrap())
            .unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let writer = Registry
            .open(
                DbId::new(root.path().join("store.sqlite")).unwrap(),
                WriterOptions {
                    configure: Some(Arc::new(|db| {
                        ctox_store::config::ConnectionConfig::default()
                            .apply_writer(db)
                            .map_err(Into::into)
                    })),
                    ..Default::default()
                },
            )
            .unwrap();
        runtime.block_on(writer.ready()).unwrap();
        let job=writer.submit(WriteJob::new(JobOptions::default(),|tx|{
            tx.execute_batch("CREATE TABLE docs(collection TEXT,id TEXT,rev INTEGER,deleted INTEGER,data TEXT,PRIMARY KEY(collection,id));CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT);INSERT INTO meta VALUES('seq','0'),('fence','7');CREATE TABLE history(collection TEXT,id TEXT,body TEXT);CREATE TABLE archive(collection TEXT,id TEXT,body TEXT);CREATE TABLE versions(app TEXT,version TEXT,manifest TEXT,PRIMARY KEY(app,version));CREATE TABLE artifacts(hash TEXT PRIMARY KEY,bytes BLOB);")?;Ok(())
        })).unwrap();
        job.blocking_wait().unwrap();
        let readers = runtime
            .block_on(ReadPool::for_database(
                writer.db_id(),
                writer.generation(),
                Default::default(),
                Default::default(),
            ))
            .unwrap();
        let authority = Arc::new(Authority {
            allowed: AtomicBool::new(true),
            watch: AtomicBool::new(true),
            now: AtomicU64::new(1000),
            audit: Mutex::new(Vec::new()),
            effects: Mutex::new(Vec::new()),
            started: Mutex::new(None),
        });
        let router = Arc::new(RegistryRouter {
            registry: CapabilityRegistry::cap1(),
            host: CapHost(authority.clone()),
            backend: Backend(authority.clone(), writer.clone()),
        });
        let host = Arc::new(
            CoreRuntimeHost::new(
                writer,
                readers,
                Arc::new(Codec),
                authority.clone(),
                router,
                runtime.handle().clone(),
            )
            .unwrap(),
        );
        Self {
            runtime,
            host,
            authority,
            root,
        }
    }
    fn publish(&self, version: &str, source: &str, extra: Value) {
        let mut manifest = json!({"app_id":"app","app_ver":version,"collections":{"rows":{}},"handlers":{"run":"handlers/run.js"},"capabilities_required":[]});
        let mut sources = BTreeMap::from([("handlers/run.js".to_owned(), source.to_owned())]);
        if let Some(extra_sources) = extra["sources"].as_object() {
            for (path, source) in extra_sources {
                sources.insert(path.clone(), source.as_str().unwrap().into());
            }
        }
        for (key, value) in extra.as_object().unwrap() {
            if key != "sources" {
                manifest[key] = value.clone();
            }
        }
        let files: BTreeMap<_, _> = sources
            .iter()
            .map(|(path, source)| {
                (
                    path.clone(),
                    format!("{:x}", Sha256::digest(source.as_bytes())),
                )
            })
            .collect();
        manifest["files"] = json!(files);
        let version = version.to_owned();
        self.host
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                tx.execute(
                    "INSERT INTO versions VALUES(?,?,?)",
                    params!["app", version.to_owned(), manifest.to_string()],
                )?;
                for (path, source) in sources {
                    let hash = manifest["files"][&path].as_str().unwrap();
                    tx.execute(
                        "INSERT OR IGNORE INTO artifacts VALUES(?,?)",
                        params![hash, source.as_bytes()],
                    )?;
                }
                Ok(())
            }))
            .unwrap()
            .blocking_wait()
            .unwrap();
    }
    fn service(&self) -> AppRuntimeService {
        let node = PathBuf::from(std::env::var("CTOX_APP_RUNTIME_TEST_NODE").unwrap());
        let pool = RuntimePool::new(node, self.root.path().join("ipc"), self.host.clone()).unwrap();
        AppRuntimeService::with_pool(pool, self.host.clone())
    }
    fn binding(&self) -> Value {
        json!({"command":command("app","v1","cmd",Value::Null),"fence":fence(),"invocation_id":"fixture","manifest":{"app_id":"app","app_ver":"v1","collections":{"rows":{}}}})
    }
    fn raw(&self, collection: &str) -> Vec<Value> {
        let collection = collection.to_owned();
        self.runtime
            .block_on(
                self.host
                    .readers
                    .read_job(ReadJob::new(JobOptions::default(), move |db| {
                        Codec.read(db, &Value::Null, &collection)
                    })),
            )
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.runtime.block_on(self.host.readers.shutdown()).unwrap();
        self.runtime.block_on(self.host.writer.shutdown()).unwrap();
    }
}

#[test]
fn core_dispatch_preserves_envelope_and_writes_via_the_single_writer() {
    let f = Fixture::new();
    f.publish("v1","export default async (command,ctx)=>{await ctx.put('rows',{id:'x',n:command.payload.n});const accepted=await ctx.command('run',{next:true},{id:'follow',record_id:command.record_id,module:command.module,client_context:command.client_context});return {command,accepted,row:await ctx.get('rows','x'),now:ctx.now(),uuid:ctx.uuid()};}",json!({}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    let mut cmd = command("app", "v1", "cmd", json!({"n":3}));
    cmd["record_id"] = json!("record");
    cmd["module"] = json!("mod");
    cmd["client_context"] = json!({"view":"v"});
    let result = service.command(cmd, fence()).unwrap();
    assert_eq!(result["row"]["n"], 3);
    assert_eq!(result["now"], 1000);
    assert!(Uuid::parse_str(result["uuid"].as_str().unwrap()).is_ok());
    assert_eq!(result["command"]["actor"]["display_name"], "Owner");
    assert_eq!(result["command"]["actor"]["is_admin"], false);
    assert_eq!(result["accepted"]["id"], "follow");
    assert!(result["accepted"]["task_id"].is_string());
    let stored = f.raw("business_commands");
    assert_eq!(stored[0]["record_id"], "record");
    assert_eq!(stored[0]["module"], "mod");
    assert_eq!(stored[0]["client_context"]["view"], "v");
    assert_eq!(
        f.raw("ctox_queue_tasks")[0]["id"],
        result["accepted"]["task_id"]
    );
}
#[test]
fn transaction_is_one_atomic_store_job_and_failed_second_write_rolls_back_first() {
    let f = Fixture::new();
    f.publish("v1","export default async (command,ctx)=>ctx.transaction(async tx=>{await tx.put('rows',{id:'a',n:1});await tx.put('rows',{id:'b',...(command.payload.deny?{forbidden:1}:{n:2})});return (await tx.read('rows')).length;});",json!({}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    assert!(service
        .command(command("app", "v1", "cmd", json!({"deny":true})), fence())
        .unwrap_err()
        .to_string()
        .starts_with("forbidden:"));
    assert!(f.raw("rows").is_empty());
    assert_eq!(
        service
            .command(command("app", "v1", "cmd2", json!({"deny":false})), fence())
            .unwrap(),
        json!(2)
    );
    assert_eq!(f.raw("rows").len(), 2);
}
#[test]
fn transaction_snapshot_rejects_concurrent_commit_and_tokens_cannot_cross_calls() {
    let f = Fixture::new();
    let binding = f.binding();
    let snapshot = f
        .host
        .invoke_bound(&binding, "transaction.begin", &json!([]))
        .unwrap();
    f.host
        .invoke_bound(
            &binding,
            "write",
            &json!(["rows",{"type":"put","doc":{"id":"outside","n":9}}]),
        )
        .unwrap();
    let mut other = binding.clone();
    other["invocation_id"] = json!("other");
    assert!(f
        .host
        .invoke_bound(
            &other,
            "transaction.commit",
            &json!([snapshot["token"], []])
        )
        .unwrap_err()
        .to_string()
        .starts_with("forbidden:"));
    let failed=f.host.invoke_bound(&binding,"transaction.commit",&json!([snapshot["token"],[{"collection":"rows","op":{"type":"put","doc":{"id":"inside"},"base_rev":null}}]])).unwrap_err();
    assert!(failed.to_string().starts_with("conflict:"));
    assert_eq!(f.raw("rows").len(), 1);
    assert_eq!(f.raw("rows")[0]["id"], "outside");
}
#[test]
fn native_registry_routes_cap_invoke_with_policy_fence_and_audit() {
    let f = Fixture::new();
    f.publish("v1","export default async (_command,ctx)=>await ctx.cap.agent.queue.capacity({n:2,record_id:'r'});",json!({"capabilities_required":["agent.queue"]}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    assert_eq!(
        service
            .command(command("app", "v1", "cmd", Value::Null), fence())
            .unwrap()["capacity"],
        2
    );
    let effects = f.authority.effects.lock().unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0]["payload"]["n"], 2);
    assert_eq!(effects[0]["record_id"], "r");
    drop(effects);
    let audit = f.authority.audit.lock().unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0]["stage"], "requested");
    assert_eq!(audit[1]["stage"], "completed");
    drop(audit);
    let mut expired = fence();
    expired["expires_at_core_ms"] = json!(1);
    assert!(service
        .command(command("app", "v1", "cmd2", Value::Null), expired)
        .is_err());
    assert_eq!(f.authority.effects.lock().unwrap().len(), 1);
}
#[test]
fn unsupported_native_enqueue_is_explicit_without_effect() {
    let f = Fixture::new();
    f.publish(
        "v1",
        "export default async (_command,ctx)=>await ctx.cap.agent.task.not_installed({task:'x'});",
        json!({"capabilities_required":["agent.task"]}),
    );
    let service = f.service();
    service.published("app", "v1").unwrap();
    let error = service
        .command(command("app", "v1", "cmd", Value::Null), fence())
        .unwrap_err();
    assert!(error.to_string().starts_with("unavailable:"), "{error:#}");
    assert!(f.authority.effects.lock().unwrap().is_empty());
}
#[test]
fn command_rights_rejects_before_node_start_even_with_forged_admin_flag() {
    let f = Fixture::new();
    f.publish("v1","throw new Error('must not load');",json!({"handlers":{"admin.run":"handlers/run.js"},"command_rights":{"admin.run":{"roles":["admin"],"assigned_users":false}}}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    let mut cmd = command("app", "v1", "cmd", Value::Null);
    cmd["type"] = json!("admin.run");
    cmd["actor"]["is_admin"] = json!(true);
    assert!(service
        .command(cmd, fence())
        .unwrap_err()
        .to_string()
        .starts_with("forbidden:"));
    assert!(service.process_id("app").is_none());
}
#[test]
fn published_cas_version_restarts_after_running_call_finishes_and_refreshes_helpers() {
    let f = Fixture::new();
    f.publish("v1","import {value} from './_lib.js';export default async (_command,ctx)=>{ctx.log('info','started');const until=Date.now()+150;while(Date.now()<until){};return value;}",json!({"sources":{"handlers/_lib.js":"export const value='old';"}}));
    f.publish(
        "v2",
        "import {value} from './_lib.js';export default()=>value;",
        json!({"sources":{"handlers/_lib.js":"export const value='new';"}}),
    );
    let node = PathBuf::from(std::env::var("CTOX_APP_RUNTIME_TEST_NODE").unwrap());
    let pool = Arc::new(RuntimePool::new(node, f.root.path().join("p"), f.host.clone()).unwrap());
    let load = |version| {
        crate::delivery::load_release(f.host.as_ref(), "app", version, BTreeSet::new()).unwrap()
    };
    pool.register(load("v1")).unwrap();
    assert_eq!(
        pool.dispatch(command("app", "v1", "prewarm", Value::Null), fence())
            .unwrap(),
        "old"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    *f.authority.started.lock().unwrap() = Some(tx);
    let runner = pool.clone();
    let call = std::thread::spawn(move || {
        runner.dispatch(command("app", "v1", "cmd", Value::Null), fence())
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    pool.register(load("v2")).unwrap();
    assert_eq!(call.join().unwrap().unwrap(), "old");
    assert_eq!(
        pool.dispatch(command("app", "v2", "cmd2", Value::Null), fence())
            .unwrap(),
        "new"
    );
}
#[test]
fn change_stream_triggers_watch_exports_and_durable_cursor_and_skip_replay() {
    let f = Fixture::new();
    f.publish("v1","export default async(command,ctx)=>{const before=await ctx.cursor.get();await ctx.put('rows',{id:'trigger-result',seq:command.payload.seq});await ctx.cursor.set(command.payload.seq);return{before,after:await ctx.cursor.get()};}",json!({"triggers":[{"handler":"handlers/run.js","on":{"collection":"shared","op":"update"}}],"foreign_collections":{"shared":{"owner_app":"owner-app","ops":["read","watch"]}}}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    let event = ChangeEvent {
        seq: 11,
        collection: "shared".into(),
        op: "update".into(),
        record_id: "row".into(),
    };
    let results = service.change(&event).unwrap();
    assert_eq!(results[0]["after"], 11);
    assert_eq!(results[0]["before"], Value::Null);
    assert_eq!(service.change(&event).unwrap()[0]["status"], "skipped");
    let next = ChangeEvent {
        seq: 12,
        ..event.clone()
    };
    assert_eq!(service.change(&next).unwrap()[0]["before"], 11);
    f.authority.watch.store(false, Ordering::SeqCst);
    assert!(service
        .change(&ChangeEvent { seq: 13, ..event })
        .unwrap_err()
        .to_string()
        .starts_with("forbidden:"));
    assert_eq!(f.raw("rows")[0]["seq"], 12);
}
#[test]
fn timer_trigger_minimum_is_one_minute_and_due_time_survives_service_restart() {
    let f = Fixture::new();
    f.publish("v1","export default async(_command,ctx)=>{const n=(await ctx.get('rows','timer'))?.n??0;await ctx.put('rows',{id:'timer',n:n+1});return n+1;}",json!({"triggers":[{"handler":"handlers/run.js","every":"PT1M"}]}));
    {
        let service = f.service();
        service.published("app", "v1").unwrap();
        assert_eq!(service.tick().unwrap()[0], 1);
        assert_eq!(service.tick().unwrap()[0]["status"], "skipped");
    }
    let service = f.service();
    service.published("app", "v1").unwrap();
    assert_eq!(service.tick().unwrap()[0]["status"], "skipped");
    f.authority.now.store(61000, Ordering::SeqCst);
    assert_eq!(service.tick().unwrap()[0], 2);
    assert!(crate::service::interval_ms("PT59S").is_err());
}
#[test]
fn completion_uses_original_cas_export_and_is_at_most_once_across_versions() {
    let f = Fixture::new();
    f.publish("v1","export default async(_command,ctx)=>ctx.cap.agent.task.enqueue({action:'request',on_complete:'handlers/run.js#onReply',on_failed:'handlers/run.js#onFailed'});export async function onReply(result,ctx){await ctx.put('rows',{id:'reply',answer:result.answer});return 'old:'+result.answer;}export async function onFailed(error,ctx){await ctx.put('rows',{id:'failure',message:error.message});return 'failed';}",json!({"capabilities_required":["agent.task"]}));
    f.publish("v2", "export default()=>2;", json!({}));
    let service = f.service();
    service.published("app", "v1").unwrap();
    let receipt = service
        .command(command("app", "v1", "cmd", Value::Null), fence())
        .unwrap();
    let task = receipt["task_id"].as_str().unwrap();
    assert_eq!(f.raw("ctox_queue_tasks")[0]["id"], task);
    service.published("app", "v2").unwrap();
    let binding = f.binding();
    assert_eq!(
        service
            .completed(binding.clone(), task, true, json!({"answer":"ok"}))
            .unwrap(),
        "old:ok"
    );
    assert_eq!(
        service
            .completed(binding.clone(), task, true, json!({"answer":"again"}))
            .unwrap()["status"],
        "skipped"
    );
    assert_eq!(f.raw("rows")[0]["answer"], "ok");
    let failed = service
        .command(command("app", "v1", "cmd-failed", Value::Null), fence())
        .unwrap();
    assert_eq!(
        service
            .completed(
                binding,
                failed["task_id"].as_str().unwrap(),
                false,
                json!({"message":"expected failure"})
            )
            .unwrap(),
        "failed"
    );
    assert_eq!(
        service
            .command(command("app", "v2", "cmd-new", Value::Null), fence())
            .unwrap(),
        2
    );
}
