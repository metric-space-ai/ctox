// Retained native execution is an effect guard, not a permission inferred from JSON.
use super::*;
use ctox_app_runtime::cap_contract::{Actor, CapCallContext};
use ctox_app_runtime::core_host::{CoreAuthority, StoreCodec};
use ctox_store::{
    change_log::subscription::{
        protocol::{Seq, Write},
        write_contract, WriteAuthority, WritePermission,
    },
    WriteJob,
};
use ctox_sync::{
    authority::{Command, Receipt, Request},
    contracts::ExecutionSpec,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{RwLock, Weak},
};

struct Held {
    command: Value,
    fence: Value,
    ownership: Ownership,
    effect: String,
    callback: Option<Value>,
}
pub(crate) struct RuntimeAuthority {
    pub(super) database: Database,
    pub(super) runtime: Handle,
    execution: Arc<dyn ExecutionAuthority>,
    pub(super) codec: Arc<RuntimeCodec>,
    held: RwLock<HashMap<String, Held>>,
    myself: Weak<RuntimeAuthority>,
}
impl RuntimeAuthority {
    pub(super) fn new(
        database: Database,
        runtime: Handle,
        execution: Arc<dyn ExecutionAuthority>,
        codec: Arc<RuntimeCodec>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|myself| Self {
            database,
            runtime,
            execution,
            codec,
            held: Default::default(),
            myself: myself.clone(),
        })
    }
    fn query<T: Send + 'static>(
        &self,
        stage: &'static str,
        read: impl FnOnce(&Connection) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T> {
        let pool = self.database.readers.clone();
        wait(&self.runtime, async move {
            observed_read(pool, stage, ReadJob::new(JobOptions::default(), read)).await
        })
    }
    pub(super) fn binding(
        &self,
        db: &Connection,
        doc: &Value,
        lease: &write_contract::CommandLease,
    ) -> Result<Value, StoreError> {
        let a = actor(db, text(doc, "actor_id").map_err(sql_error)?)?;
        let manifest = self.codec.route(db, doc)?;
        let mut command = doc.clone();
        command["actor"] = actor_json(&a);
        command["app_ver"] = manifest["app_ver"].clone();
        if manifest["app_id"] != command["app_id"] { command["target_app"] = manifest["app_id"].clone(); }
        Ok(
            json!({"command":command,"manifest":manifest,"fence":{"run_id":format!("app-runtime:{}",lease.command_id),"participant":lease.participant,"fence":lease.fence.0,"expires_at_core_ms":lease.expires_at_core,"rights_ver":rights(db)?.rights_ver}}),
        )
    }
    /// Only the command/feed owner enters here, after the canonical command committed.
    /// P9's row lease and the actual Raft job protect different effect boundaries.
    pub(super) async fn retain(self: &Arc<Self>, doc: Value) -> Result<Value> {
        let id = text(&doc, "id")?.to_owned();
        let participant = self.execution.node_id().to_string();
        let gate = ExecutorGate {
            codec: self.codec.clone(),
            participant: participant.clone(),
        };
        let command_id = id.clone();
        let owner = self.clone();
        let binding = self
            .database
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                let mut saved = document(tx, "business_commands", &command_id)?
                    .ok_or_else(|| invalid("not_found: command"))?;
                if saved != doc {
                    return Err(invalid("conflict: committed command changed"));
                }
                let lease = write_contract::claim_command(
                    tx,
                    &participant,
                    &command_id,
                    None,
                    now(),
                    &gate,
                )?
                .map_err(|e| invalid(format!("{}: {}", e.code, e.reason)))?;
                saved["status"] = json!("running");
                store_command(tx, &saved)?;
                owner.binding(tx, &saved, &lease)
            }))?
            .await?;
        let run = text(&binding["fence"], "run_id")?.to_owned();
        let spec = ExecutionSpec {
            job_id: run.clone(),
            session_id: id.clone(),
            scope_id: self.execution.scope_id().into(),
            harness: "ctox-app-runtime".into(),
            harness_version: env!("CARGO_PKG_VERSION").into(),
            model_route_id: "native-app-handler".into(),
            gateway_account_id: "native-app-handler".into(),
            model_id: "native-app-handler".into(),
            required_capabilities: BTreeSet::new(),
        };
        let created = self
            .execution
            .submit(Request {
                request_id: format!("create:{run}"),
                actor: self.execution.node_id(),
                command: Command::Create {
                    spec: spec.clone(),
                    owner: self.execution.node_id(),
                },
            })
            .await?;
        let job = match created {
            Receipt::Applied(job) => job,
            _ => {
                return Err(anyhow!(
                    "unavailable: execution admission requires reconciliation"
                ))
            }
        };
        anyhow::ensure!(
            job.spec == spec && !job.stopped && job.ownership.node_id == self.execution.node_id(),
            "revoked: execution admission differs"
        );
        let effect = format!("handler:{id}");
        match self
            .execution
            .submit(Request {
                request_id: format!("begin:{run}"),
                actor: self.execution.node_id(),
                command: Command::BeginEffect {
                    job_id: run.clone(),
                    ownership: job.ownership.clone(),
                    effect_id: effect.clone(),
                },
            })
            .await?
        {
            Receipt::Applied(j)
                if j.ownership == job.ownership
                    && !j.stopped
                    && j.pending_effects.contains(&effect) =>
            {
                ()
            }
            _ => {
                return Err(anyhow!(
                    "unavailable: execution effect requires reconciliation"
                ))
            }
        }
        self.execution
            .validate_ownership(&run, &job.ownership)
            .await?;
        let mut held = self
            .held
            .write()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?;
        anyhow::ensure!(!held.contains_key(&id), "conflict: invocation already held");
        held.insert(
            id,
            Held {
                command: binding["command"].clone(),
                fence: binding["fence"].clone(),
                ownership: job.ownership,
                effect,
                callback: None,
            },
        );
        Ok(binding)
    }
    pub(super) async fn release(&self, binding: &Value) -> Result<()> {
        let id = text(&binding["command"], "id")?;
        let held = self
            .held
            .write()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?
            .remove(id)
            .context("revoked: invocation no longer held")?;
        // The retained guard is withdrawn before reporting native effect completion.
        let run = text(&held.fence, "run_id")?;
        match self
            .execution
            .submit(Request {
                request_id: format!("complete:{run}"),
                actor: self.execution.node_id(),
                command: Command::CompleteEffect {
                    job_id: run.into(),
                    ownership: held.ownership.clone(),
                    effect_id: held.effect,
                },
            })
            .await?
        {
            Receipt::Applied(_) => match self
                .execution
                .submit(Request {
                    request_id: format!("stop:{run}"),
                    actor: self.execution.node_id(),
                    command: Command::Stop {
                        job_id: run.into(),
                        ownership: held.ownership,
                    },
                })
                .await?
            {
                Receipt::Applied(job) if job.stopped => Ok(()),
                _ => Err(anyhow!("unavailable: stop requires reconciliation")),
            },
            _ => Err(anyhow!("unavailable: completion requires reconciliation")),
        }
    }
    pub(super) async fn renew(self: &Arc<Self>, binding: &Value) -> Result<()> {
        let id = text(&binding["command"], "id")?.to_owned();
        let (ownership, run) = {
            let h = self
                .held
                .read()
                .map_err(|_| anyhow!("unavailable: invocation registry"))?;
            let h = h.get(&id).context("revoked: invocation")?;
            (h.ownership.clone(), text(&h.fence, "run_id")?.to_owned())
        };
        self.execution.validate_ownership(&run, &ownership).await?;
        let participant = self.execution.node_id().to_string();
        let expected = binding["fence"]["fence"]
            .as_u64()
            .context("invalid: fence")?;
        let gate = ExecutorGate {
            codec: self.codec.clone(),
            participant: participant.clone(),
        };
        let command_id = id.clone();
        let lease = self
            .database
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                Ok(write_contract::claim_command(
                    tx,
                    &participant,
                    &command_id,
                    Some(Seq(expected)),
                    now(),
                    &gate,
                )?
                .map_err(|e| invalid(format!("{}: {}", e.code, e.reason)))?)
            }))?
            .await?;
        if let Some(held) = self
            .held
            .write()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?
            .get_mut(&id)
        {
            held.fence["expires_at_core_ms"] = json!(lease.expires_at_core);
        }
        Ok(())
    }
    fn live<'a>(
        &'a self,
        binding: &Value,
    ) -> Result<std::sync::RwLockReadGuard<'a, HashMap<String, Held>>> {
        let h = self
            .held
            .read()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?;
        let parent = text(&binding["fence"], "run_id")?
            .strip_prefix("app-runtime:")
            .context("invalid: native run")?;
        let retained = h
            .get(parent)
            .context("revoked: native invocation is not retained")?;
        let matches = retained.command == binding["command"]
            || retained.callback.as_ref() == Some(&binding["command"]);
        anyhow::ensure!(
            matches
                && retained.fence["run_id"] == binding["fence"]["run_id"]
                && retained.fence["fence"] == binding["fence"]["fence"]
                && retained.fence["expires_at_core_ms"].as_u64().unwrap_or(0) > now(),
            "revoked: native invocation differs or expired"
        );
        Ok(h)
    }
    pub(super) fn check_context(
        &self,
        ctx: &crate::app_capabilities::CapCallContext,
    ) -> Result<()> {
        let h = self
            .held
            .read()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?;
        let parent = ctx
            .fence
            .run_id
            .strip_prefix("app-runtime:")
            .context("invalid: invocation run")?;
        let h = h.get(parent).context("revoked: invocation")?;
        let scoped = h.callback.as_ref().filter(|c| c["id"] == ctx.command_id).unwrap_or(&h.command);
        anyhow::ensure!(
            h.command["id"] == ctx.command_id
                || h.callback
                    .as_ref()
                    .is_some_and(|c| c["id"] == ctx.command_id),
            "revoked: callback identity differs"
        );
        anyhow::ensure!(
            ctox_app_runtime::execution_app(scoped)? == ctx.app_id
                && h.command["app_ver"] == ctx.app_ver
                && h.command["actor"]["id"] == ctx.actor.id
                && h.fence["fence"] == ctx.fence.fence
                && h.fence["run_id"] == ctx.fence.run_id
                && h.fence["expires_at_core_ms"].as_u64().unwrap_or(0) > now(),
            "revoked: scoped file context differs"
        );
        Ok(())
    }
}
impl CoreAuthority for RuntimeAuthority {
    fn platform_command(&self, kind: &str) -> bool { routing::core_route(kind) }
    fn admit(&self, manifest: &Value, binding: &Value) -> Result<()> {
        let _held = self.live(binding)?;
        let codec = self.codec.clone();
        let b = binding.clone();
        let expected = manifest.clone();
        self.query("authority.admit", move |db| {
            let (a, checked_manifest, snapshot) = codec.checked_snapshot(db, &b)?;
            // Preserve native-platform admission semantics; ordinary app releases
            // were already verified by checked_snapshot in this exact WAL view.
            let native =
                super::super::commands::handles(text(&b["command"], "type").map_err(sql_error)?);
            let current = if !native {
                checked_manifest
            } else if b["command"]["runtime_completion"] == true {
                codec.version(
                    db,
                    text(&b["command"], "app_id").map_err(sql_error)?,
                    text(&b["command"], "app_ver").map_err(sql_error)?,
                )?["manifest"]
                    .clone()
            } else {
                codec.manifest(db, text(&b["command"], "app_id").map_err(sql_error)?)?
            };
            if current != expected {
                return Err(invalid("revoked: manifest changed"));
            }
            let _ = (a, snapshot);
            Ok(())
        })
    }
    fn authorize(&self, app: &str, claimed: &Value, operation: &Value) -> Result<()> {
        let app = app.to_owned();
        let claimed = claimed.clone();
        let op = operation.clone();
        let codec = self.codec.clone();
        self.query("authority.authorize", move |db| {
            let a = actor(db, text(&claimed, "id").map_err(sql_error)?)?;
            if actor_json(&a) != claimed {
                return Err(invalid("revoked: actor changed"));
            }
            let m = codec.manifest(db, &app)?;
            let kind = op["type"]
                .as_str()
                .or_else(|| op["command"]["type"].as_str());
            if let Some(kind) = kind {
                permit(db, &app, &a, kind)?;
            } else {
                let r = rights(db)?;
                let scope = policy::BusinessOsScope::module(
                    &app,
                    assigned(db, &app, a.id.as_deref().unwrap())?,
                );
                if !policy::evaluate_with_rights(
                    &r,
                    &a,
                    policy::BusinessOsPermission::AppsView,
                    &scope,
                )
                .allowed
                {
                    return Err(invalid("forbidden: app access"));
                }
            }
            let _ = m;
            Ok(())
        })
    }
    fn exported(&self, owner: &str, name: &str, operation: &str) -> Result<bool> {
        let owner = owner.to_owned();
        let name = name.to_owned();
        let operation = operation.to_owned();
        let codec = self.codec.clone();
        self.query("authority.exported", move |db| {
            let c = collection(db, &name)?;
            if c.owner != owner {
                return Ok(false);
            }
            let m = codec.manifest(db, &owner)?;
            Ok(m["exports"][&name]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op == &operation)))
        })
    }
    fn capability_available(&self, name: &str) -> bool {
        crate::app_capabilities::cap2::ROUTES
            .iter()
            .chain(crate::app_capabilities::cap2::mail::ROUTES)
            .any(|r| {
                crate::app_capabilities::cap2::call_path(r) == name
                    || format!("{}.{}", r.capability, r.function) == name
            })
    }
    fn with_fence(
        &self,
        binding: &Value,
        effect: &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value> {
        // The driver holds the actual Raft BeginEffect for the whole invocation.
        // The read guard excludes driver teardown while this effect executes.
        // StoreCodec additionally checks the P9 row + current policy INSIDE the writer.
        let _held = self.live(binding)?;
        effect()
    }
    fn capability_context(&self, binding: &Value, call_id: &str) -> Result<CapCallContext> {
        let h = self.live(binding)?;
        let parent = text(&binding["fence"], "run_id")?
            .strip_prefix("app-runtime:")
            .context("invalid: native run")?;
        let held = h.get(parent).unwrap();
        let fence = serde_json::from_value(
            json!({"run_id":held.fence["run_id"],"participant":held.fence["participant"],"fence":held.fence["fence"],"expires_at_core_ms":held.fence["expires_at_core_ms"]}),
        )?;
        Ok(CapCallContext {
            app_id: ctox_app_runtime::execution_app(&binding["command"])?.into(),
            app_ver: text(&held.command, "app_ver")?.into(),
            actor: Actor {
                id: text(&held.command["actor"], "id")?.into(),
                role: text(&held.command["actor"], "role")?.into(),
            },
            command_id: text(&binding["command"], "id")?.into(),
            call_id: call_id.into(),
            fence,
        })
    }
    fn grants(&self, app: &str, version: &str) -> Result<BTreeSet<String>> {
        let app = app.to_owned();
        let version = version.to_owned();
        let codec = self.codec.clone();
        self.query("authority.grants", move |db| {
            let m = codec.version(db, &app, &version)?["manifest"].clone();
            let record = document(db, "business_apps", &app)?
                .ok_or_else(|| invalid("revoked: app removed"))?;
            let requested = m["capabilities_required"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let approved = record["capabilities_approved"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            Ok(requested
                .iter()
                .filter(|v| approved.contains(v))
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect())
        })
    }
    fn event_binding(&self, app: &str, version: &str, event: &Value) -> Result<Value> {
        self.native_event(app, version, event, None)
    }
    fn now_ms(&self) -> u64 {
        now()
    }
    fn log(&self, binding: &Value, args: &Value) -> Result<Value> {
        eprintln!(
            "app runtime log app={} level={}",
            binding["command"]["app_id"].as_str().unwrap_or(""),
            args[0].as_str().unwrap_or("info")
        );
        Ok(Value::Null)
    }
}
/// Current command permission is required before P9 can grant a local lease.
pub(super) struct ExecutorGate {
    pub codec: Arc<RuntimeCodec>,
    pub participant: String,
}
impl WriteAuthority for ExecutorGate {
    fn authorize_command_executor(
        &self,
        tx: &rusqlite::Transaction<'_>,
        participant: &str,
        command: &Value,
    ) -> rusqlite::Result<WritePermission> {
        let result = (|| -> Result<()> {
            anyhow::ensure!(
                participant == self.participant,
                "forbidden: executor differs"
            );
            self.codec.route(tx, command)?;
            Ok(())
        })();
        Ok(match result {
            Ok(()) => WritePermission::Permit,
            Err(_) => WritePermission::Reject("forbidden".into()),
        })
    }
    fn authorize(
        &self,
        _: &rusqlite::Transaction<'_>,
        _: &str,
        _: &Write,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Reject("forbidden".into()))
    }
    fn validate_document(
        &self,
        _: &rusqlite::Transaction<'_>,
        _: &str,
        _: &ctox_store::schema::Collection,
        _: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Reject("forbidden".into()))
    }
}

impl RuntimeAuthority {
    fn native_event(
        &self,
        app: &str,
        version: &str,
        event: &Value,
        callback: Option<Value>,
    ) -> Result<Value> {
        let app = app.to_owned();
        let version = version.to_owned();
        let event = event.clone();
        let id = format!(
            "runtime-event:{}",
            digest(
                serde_json::to_string(&json!([app, version, event]))
                    .unwrap()
                    .as_bytes()
            )
        );
        let command_id = id.clone();
        let codec = self.codec.clone();
        let expected_callback = callback.clone();
        let job=self.database.writer.submit(WriteJob::new(JobOptions::default(),move|tx|{
            let current=codec.manifest(tx,&app)?;
            let m=if expected_callback.is_some(){codec.version(tx,&app,&version)?["manifest"].clone()}else{current};
            if m["app_ver"]!=version{return Err(invalid("revoked: event release changed"));}
            let author=if let Some(cb)=&expected_callback {text(&cb["actor"],"id").map_err(sql_error)?}else{text(&m,"author").map_err(sql_error)?};
            let a=actor(tx,author)?;
            let kind=expected_callback.as_ref().and_then(|c|c["type"].as_str()).unwrap_or("$runtime.trigger");
            // Only this in-process event owner may admit core-only rules. The
            // canonical command gets a private receipt below before delivery;
            // serialized client origin/runtime_event fields confer no authority.
            let snapshot=rights(tx)?;
            if !origin::core_only(&snapshot,&app,kind)? {
                permit_with_rights(tx,&snapshot,&app,&a,kind)?;
            }
            let payload=if event["kind"]=="trigger"{event["event"].clone()}else{event.clone()};
            let doc=json!({"id":command_id,"app_id":app,"type":kind,"actor_id":a.id,"status":"queued","payload":payload,"client_context":{"trigger_id":event["trigger_id"]},"created_at":chrono::Utc::now().to_rfc3339(),"runtime_event":true,"runtime_completion":expected_callback.is_some(),"native_version":version});
            if document(tx,"business_commands",&command_id)?.is_some(){return Err(invalid("conflict: native event already admitted; reconcile"));}
            store_command(tx,&doc)?;origin::stamp(tx,"runtime_event",&doc)?;Ok(doc)
        }))?;
        let doc = wait(&self.runtime, async move { job.await })?;
        let myself = self
            .myself
            .upgrade()
            .context("revoked: native host stopped")?;
        let binding = wait(&self.runtime, async move {
            myself.retain(doc).await.map_err(sql_error)
        })?;
        if let Some(callback) = callback {
            let mut held = self
                .held
                .write()
                .map_err(|_| anyhow!("unavailable: invocation registry"))?;
            held.get_mut(text(&binding["command"], "id")?)
                .unwrap()
                .callback = Some(callback);
        }
        Ok(binding)
    }
    pub(super) async fn finish_events(&self, succeeded: bool) -> Result<()> {
        let bindings = {
            let h = self
                .held
                .read()
                .map_err(|_| anyhow!("unavailable: invocation registry"))?;
            h.values()
                .filter(|v| v.command["runtime_event"] == true && v.callback.is_none())
                .map(|v| json!({"command":v.command,"fence":v.fence}))
                .collect::<Vec<_>>()
        };
        // An unclassified/uncertain handler failure never confirms the native
        // effect. Keep its durable BeginEffect for reconciliation.
        if !succeeded {
            return Ok(());
        }
        for binding in bindings {
            let b = binding.clone();
            let settled = self.database.readers.read_job(ReadJob::new(Default::default(), move |db| {
                let app=text(&b["command"],"app_id").map_err(sql_error)?;
                let trigger=text(&b["command"]["client_context"],"trigger_id").map_err(sql_error)?;
                let row: Option<(bool,String,u64,Option<String>)> = db.query_row(
                    "SELECT running,run_id,fence,outcome FROM _app_runtime_triggers WHERE app_id=?1 AND trigger_id=?2",
                    params![app,trigger],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
                Ok(row.is_some_and(|(running,run,fence,outcome)| !running && b["fence"]["run_id"]==run && b["fence"]["fence"]==fence && outcome.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok()).is_some_and(|v|v["status"]=="completed" && v["uncertain"]!=true)))
            })).await?;
            if settled {
                self.finish_event_record(&binding, "done").await?;
                self.release(&binding).await?;
            }
        }
        Ok(())
    }
    pub(super) async fn renew_events(self: &Arc<Self>) -> Result<()> {
        let bindings = {
            let held = self
                .held
                .read()
                .map_err(|_| anyhow!("unavailable: invocation registry"))?;
            held.values()
                .filter(|v| v.command["runtime_event"] == true && v.callback.is_none())
                .map(|v| json!({"command":v.command,"fence":v.fence}))
                .collect::<Vec<_>>()
        };
        self.finish_events(true).await?;
        for binding in bindings {
            let id = text(&binding["command"], "id")?;
            let still_held = self
                .held
                .read()
                .map_err(|_| anyhow!("unavailable: invocation registry"))?
                .contains_key(id);
            if still_held {
                self.renew(&binding).await?;
            }
        }
        Ok(())
    }
    pub(super) async fn finish_event_record(&self, binding: &Value, status: &str) -> Result<()> {
        let id = text(&binding["command"], "id")?.to_owned();
        let status = status.to_owned();
        let codec = self.codec.clone();
        let b = binding.clone();
        self.database
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                codec.check_fence(tx, &b)?;
                let mut doc = document(tx, "business_commands", &id)?
                    .ok_or_else(|| invalid("not_found: event"))?;
                doc["status"] = json!(status);
                store_command(tx, &doc)?;
                Ok(())
            }))?
            .await?;
        Ok(())
    }
    pub(super) fn completion_binding(&self, id: &str) -> Result<Option<Value>> {
        let id = id.to_owned();
        let task = format!("command:{id}");
        let codec = self.codec.clone();
        let saved=self.query("authority.completion_binding", move |db| {
            let row:Option<(String,String,String,Option<String>,Option<String>)>=db.query_row("SELECT app_id,app_ver,actor_id,on_complete,on_failed FROM _app_runtime_completions WHERE task_id=?1 AND state='pending'",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let Some((app,ver,actor_id,complete,failed))=row else{return Ok(None)};
            let target=document(db,"business_commands",&id)?.ok_or_else(||invalid("not_found: completed command"))?;
            let status=match target["status"].as_str(){Some("done"|"completed")=>"completed",Some("cancelled")=>"cancelled",Some("failed"|"rejected")=>"failed",_=>return Err(invalid("conflict: command is not terminal"))};
            let Some(kind)=(if status=="completed"{complete}else{failed})else{return Ok(None)};
            let a=actor(db,&actor_id)?;permit(db,&app,&a,&kind)?;
            let _=codec.version(db,&app,&ver)?;
            let payload=json!({"command_id":id,"status":status,"result":target["result"],"error":target["error"]});
            let callback=json!({"id":format!("command-callback:{id}"),"app_id":app,"app_ver":ver,"type":kind,"actor":actor_json(&a),"actor_id":actor_id,"status":"queued","payload":payload});
            Ok(Some((app,ver,callback)))
        })?;
        let Some((app, ver, callback)) = saved else {
            return Ok(None);
        };
        let binding = self.native_event(
            &app,
            &ver,
            &json!({"kind":"completion","target":callback["payload"]["command_id"]}),
            Some(callback.clone()),
        )?;
        let mut expected = binding["command"].clone();
        for key in ["id", "type", "payload"] {
            expected[key] = callback[key].clone();
        }
        let mut wire = expected.clone();
        wire["status"] = json!("running");
        let callback_id = text(&wire, "id")?.to_owned();
        let job = self
            .database
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                if document(tx, "business_commands", &callback_id)?.is_some() {
                    return Err(invalid(
                        "conflict: callback already materialized; reconcile",
                    ));
                }
                super::super::commands::put(tx, "business_commands", &callback_id, &wire)?;
                Ok(())
            }))?;
        wait(&self.runtime, async move { job.await })?;
        self.held
            .write()
            .map_err(|_| anyhow!("unavailable: invocation registry"))?
            .get_mut(text(&binding["command"], "id")?)
            .unwrap()
            .callback = Some(expected);
        Ok(Some(binding))
    }
    pub(super) async fn finish_callback(
        &self,
        binding: &Value,
        target: &str,
        succeeded: bool,
    ) -> Result<()> {
        let b = binding.clone();
        let codec = self.codec.clone();
        let id = format!("command-callback:{target}");
        self.database
            .writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                codec.check_fence(tx, &b)?;
                let mut doc = document(tx, "business_commands", &id)?
                    .ok_or_else(|| invalid("not_found: callback record"))?;
                doc["status"] = json!(if succeeded { "done" } else { "failed" });
                store_command(tx, &doc)?;
                Ok(())
            }))?
            .await?;
        Ok(())
    }
}
