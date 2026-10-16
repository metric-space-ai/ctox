// Native platform actions remain outside held Store transactions and reuse Platform.
//! Commit-driven delivery. One cursor over the canonical _changes, no table poll.
use super::*;
use ctox_app_runtime::{
    core_host::StoreCodec, registration::AppRuntimeIngress, service::ChangeEvent,
};
use ctox_store::{change_log::subscription::write_contract, WriteJob};
use tokio::{sync::watch, task::JoinHandle};

pub(crate) struct RuntimeBinding {
    pub ingress: AppRuntimeIngress,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<()>>>,
}
impl RuntimeBinding {
    /// Explicit native inputs; CUTOVER-4's final attach passes these actual owners.
    pub(crate) async fn attach(root: std::path::PathBuf, owners: RuntimeOwners) -> Result<Self> {
        let database = owners.sync.clone();
        let runtime = owners.runtime.clone();
        let platform = super::super::Platform::from_database(&owners.sync, owners.signer.clone())?
            .with_secret_database(&owners.secrets)?;
        let policy_owner = sync_rights::StoreRightsAuthority::attach(
            database.writer.clone(),
            owners.signer.instance_id().into(),
        )
        .await
        .map_err(|e| anyhow!(e))?;
        // Subscribe BEFORE the common WAL activation snapshot.
        let commits = database.writer.subscribe_commits();
        let (host, authority) = tokio::task::spawn_blocking(move || super::host(&owners)).await??;
        let codec = host.codec.clone();
        let (active,initial,epoch)=observed_read(database.readers.clone(), "activation_snapshot", ReadJob::new(JobOptions::default(),move|db|{
            let c=collection(db,"business_apps")?;
            let rows=db.prepare(&format!("SELECT data FROM {} WHERE deleted=0 AND json_extract(data,'$.state')='active' ORDER BY id",quote(&c.table)))?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut active=Vec::new();for raw in rows {let app:Value=serde_json::from_str(&raw).map_err(sql_error)?;let id=text(&app,"id").map_err(sql_error)?;let m=codec.active_manifest(db,id)?;active.push((id.into(),text(&m,"app_ver").map_err(sql_error)?.into()));}
            let head=db.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get::<_,u64>(0))?;
            let epoch:String=db.query_row("SELECT epoch FROM _meta WHERE singleton=1",[],|r|r.get(0))?;
            Ok((active,head,epoch))
        })).await?;
        let cursor=database.writer.submit(WriteJob::new(JobOptions::default(),move|tx|{
            tx.execute_batch("CREATE TABLE IF NOT EXISTS _app_runtime_feed(singleton INTEGER PRIMARY KEY CHECK(singleton=1),epoch TEXT NOT NULL,cursor INTEGER NOT NULL)")?;
            let saved:Option<(String,u64)>=tx.query_row("SELECT epoch,cursor FROM _app_runtime_feed WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            match saved {
                Some((old,cursor)) if old==epoch=>Ok(cursor),
                _=>{tx.execute("INSERT INTO _app_runtime_feed VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET epoch=excluded.epoch,cursor=excluded.cursor",params![epoch,initial])?;Ok(initial)}
            }
        }))?.await?;
        let inputs = AppRuntimeInputs {
            host: host.clone(),
            runtime: runtime.clone(),
            active,
        };
        let native_root = root.clone();
        let registration =
            tokio::task::spawn_blocking(move || AppRuntimeRegistration::start(root, inputs))
                .await??;
        let ingress = registration.ingress.clone();
        let (stop, stopping) = watch::channel(false);
        let feed_ingress = ingress.clone();
        let task = runtime.spawn(async move {
            let result = drive(
                native_root,
                database,
                authority,
                platform,
                policy_owner,
                feed_ingress,
                commits,
                cursor,
                stopping,
            )
            .await;
            if result.is_err() {
                eprintln!("native app runtime feed stopped; ingress is closing: {result:?}");
            }
            let drained = registration.shutdown().await;
            result.and(drained)
        });
        Ok(Self {
            ingress,
            stop,
            task: Some(task),
        })
    }
    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            task.await??;
        }
        Ok(())
    }
}
impl Drop for RuntimeBinding {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

async fn drive(
    native_root: std::path::PathBuf,
    database: Database,
    authority: Arc<RuntimeAuthority>,
    platform: super::super::Platform,
    policy_owner: Arc<sync_rights::StoreRightsAuthority>,
    ingress: AppRuntimeIngress,
    mut commits: ctox_store::writer::CommitReceiver,
    mut cursor: u64,
    mut stop: watch::Receiver<bool>,
) -> Result<()> {
    // Recover pending commands once. Running/uncertain effects are never replayed.
    let startup=observed_read(database.readers.clone(), "startup_commands", ReadJob::new(JobOptions::default(),|db|{
        let c=collection(db,"business_commands")?;
        db.prepare(&format!("SELECT data FROM {} WHERE deleted=0 AND json_extract(data,'$.status') IN ('pending','accepted','queued') ORDER BY id",quote(&c.table)))?.query_map([],|r|r.get::<_,String>(0))?.map(|r|r.map_err(Into::into).and_then(|s|serde_json::from_str::<Value>(&s).map_err(sql_error))).collect::<Result<Vec<_>,StoreError>>()
    })).await?;
    for doc in startup {
        if !is_native_event(&database, &doc).await? {
            dispatch_committed(
                &native_root,
                &database,
                &authority,
                &platform,
                &policy_owner,
                &ingress,
                doc,
            )
            .await?;
        }
    }
    let mut lease_tick = tokio::time::interval(Duration::from_secs(10));
    lease_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    lease_tick.tick().await;
    loop {
        // Drain retained heads before awaiting the next coalesced wakeup.
        loop {
            let after = cursor;
            let page=observed_read(database.readers.clone(), "change_page", ReadJob::new(JobOptions::default(),move|db|{
                let mut statement=db.prepare("SELECT c.collection,c.id,c.seq,c.op,c.gen FROM _changes c JOIN _collections r ON r.name=c.collection AND r.gen=c.gen AND r.state='active' WHERE c.seq>?1 ORDER BY c.seq LIMIT 256")?;
                let rows=statement.query_map([after],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u64>(2)?,r.get::<_,u8>(3)?,r.get::<_,u64>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                rows.into_iter().map(|(collection,id,seq,op,gen)|{
                    let doc=if op==1{None}else{document(db,&collection,&id)?};
                    let prior:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 AND seq<?4)",params![collection,gen,id,seq],|r|r.get(0))?;
                    Ok((collection,id,seq,if op==1{"delete"}else if prior{"update"}else{"insert"}.to_owned(),doc))
                }).collect::<Result<Vec<_>,StoreError>>()
            })).await?;
            if page.is_empty() {
                authority
                    .finish_events(true)
                    .await
                    .context("native idle event finalization")?;
                break;
            }
            for (collection, id, seq, op, doc) in page {
                if collection == "business_apps" {
                    match &doc {
                        Some(app) if app["state"] == "active" => ingress
                            .published(id.clone(), text(app, "active_ver")?.into())
                            .await
                            .with_context(|| format!("native published seq={seq} app={id}"))?,
                        _ => ingress
                            .removed(id.clone())
                            .await
                            .with_context(|| format!("native removed seq={seq} app={id}"))?,
                    }
                } else if collection == "business_commands" {
                    if let Some(doc) = doc {
                        if !is_native_event(&database, &doc).await? {
                            match doc["status"].as_str() {
                                Some("pending" | "accepted" | "queued") => {
                                    dispatch_committed(
                                        &native_root,
                                        &database,
                                        &authority,
                                        &platform,
                                        &policy_owner,
                                        &ingress,
                                        doc,
                                    )
                                    .await?
                                }
                                Some(
                                    "done" | "failed" | "cancelled" | "completed" | "rejected",
                                ) => deliver_completion(&database, &authority, &ingress, doc)
                                    .await
                                    .with_context(|| {
                                        format!("native terminal seq={seq} id={id}")
                                    })?,
                                _ => (),
                            }
                        }
                    }
                }
                let changed = ingress
                    .change(ChangeEvent {
                        seq,
                        collection: collection.clone(),
                        op,
                        record_id: id.clone(),
                    })
                    .await;
                authority
                    .finish_events(changed.is_ok())
                    .await
                    .with_context(|| {
                        format!(
                            "native change finalization seq={seq} collection={collection} id={id}"
                        )
                    })?;
                changed.with_context(|| {
                    format!("native change seq={seq} collection={collection} id={id}")
                })?;
                cursor = seq;
                database
                    .writer
                    .submit(WriteJob::new(JobOptions::default(), move |tx| {
                        tx.execute(
                            "UPDATE _app_runtime_feed SET cursor=MAX(cursor,?1) WHERE singleton=1",
                            [seq],
                        )?;
                        Ok(())
                    }))?
                    .await?;
            }
        }
        tokio::select! {biased;_=stop.changed()=>break,result=commits.changed()=>{result?;},_=lease_tick.tick()=>{authority.renew_events().await?;}}
    }
    Ok(())
}

async fn dispatch_committed(
    native_root: &std::path::Path,
    database: &Database,
    authority: &Arc<RuntimeAuthority>,
    platform: &super::super::Platform,
    policy_owner: &Arc<sync_rights::StoreRightsAuthority>,
    ingress: &AppRuntimeIngress,
    doc: Value,
) -> Result<()> {
    let native = super::super::commands::handles(text(&doc, "type")?);
    let binding = match authority.retain(doc.clone()).await {
        Ok(binding) => binding,
        Err(error) => {
            // These errors arise in the rolled-back admission transaction, before
            // any native effect is begun. A denied command must not stop the feed.
            let reason = match error.downcast_ref::<StoreError>() {
                Some(StoreError::Invalid(reason))
                    if matches!(
                        error_code(reason),
                        "forbidden" | "revoked" | "not_found" | "invalid"
                    ) =>
                {
                    reason.clone()
                }
                _ => {
                    return Err(error).with_context(|| {
                        format!(
                            "native command admission id={} type={}",
                            doc["id"], doc["type"]
                        )
                    })
                }
            };
            database
                .writer
                .submit(WriteJob::new(JobOptions::default(), move |tx| {
                    let id = text(&doc, "id").map_err(sql_error)?;
                    if document(tx, "business_commands", id)?.as_ref() != Some(&doc) {
                        return Err(invalid("conflict: command changed during rejection"));
                    }
                    origin::audit_rejection(tx, &doc, error_code(&reason))?;
                    let mut rejected = doc;
                    rejected["status"] = json!("failed");
                    rejected["error"] =
                        json!({"code":error_code(&reason),"reason":"admission_denied"});
                    store_command(tx, &rejected)
                }))?
                .await?;
            return Ok(());
        }
    };
    if native {
        return dispatch_platform(database, authority, platform, policy_owner, &binding)
            .await
            .with_context(|| {
                format!(
                    "native platform id={} type={}",
                    binding["command"]["id"], binding["command"]["type"]
                )
            });
    }
    let command = binding["command"].clone();
    let fence = binding["fence"].clone();
    let e25_core = write_contract::routes::resolve(text(&command, "type")?)
        .is_ok_and(write_contract::routes::is_command_bus);
    let native_root = native_root.to_path_buf();
    let retained = authority.clone();
    let native_binding = binding.clone();
    let call = async {
        if e25_core {
            tokio::task::spawn_blocking(move || {
                crate::business_os::command_plane::execute_v4_core_registered_command(
                    &native_root,
                    &native_binding,
                    &*retained,
                )
            })
            .await?
        } else {
            ingress.command(command, fence).await
        }
    };
    tokio::pin!(call);
    let mut renew = tokio::time::interval(Duration::from_secs(10));
    renew.tick().await;
    let result = loop {
        tokio::select! {result=&mut call=>break result,_=renew.tick()=>authority.renew(&binding).await?}
    };
    // A failed handler is a typed terminal result; an uncertain effect stays running.
    let outcome = match result {
        Ok(value) => json!({"status":"done","result":value}),
        Err(error) => {
            let uncertain = error
                .downcast_ref::<ctox_app_runtime::HandlerError>()
                .map_or(true, |e| e.uncertain);
            if uncertain {
                return Err(error).with_context(|| {
                    format!(
                        "native handler id={} type={}",
                        binding["command"]["id"], binding["command"]["type"]
                    )
                });
            }
            json!({"status":"failed","error":{"status":"failed","code":error_code(&error.to_string()),"reason":"handler_failed"}})
        }
    };
    let id = text(&binding["command"], "id")?.to_owned();
    let codec = authority.codec.clone();
    let b = binding.clone();
    database.writer.submit(WriteJob::new(JobOptions::default(),move|tx|{
        codec.check_fence(tx,&b)?;
        let mut doc=document(tx,"business_commands",&id)?.ok_or_else(||invalid("not_found: command"))?;
        doc["status"]=outcome["status"].clone();for key in ["result","error"]{if let Some(value)=outcome.get(key){doc[key]=value.clone();}}
        store_command(tx,&doc)?;
        let c=collection(tx,"business_commands")?;
        let (rev,seq):(String,u64)=tx.query_row("SELECT rev,seq FROM _changes WHERE collection='business_commands' AND gen=?1 AND id=?2",params![c.gen,id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        write_contract::project_command(tx,&id,c.gen,&rev,seq,&doc)?;Ok(())
    }))?.await?;
    authority
        .release(&binding)
        .await
        .with_context(|| format!("native release id={}", binding["command"]["id"]))?;
    Ok(())
}

async fn deliver_completion(
    _database: &Database,
    authority: &Arc<RuntimeAuthority>,
    ingress: &AppRuntimeIngress,
    doc: Value,
) -> Result<()> {
    let owner = authority.clone();
    let id = text(&doc, "id")?.to_owned();
    let command_id = id.clone();
    let binding =
        tokio::task::spawn_blocking(move || owner.completion_binding(&command_id)).await??;
    if let Some(binding) = binding {
        let status = match doc["status"].as_str() {
            Some("done" | "completed") => "completed",
            Some("cancelled") => "cancelled",
            _ => "failed",
        };
        let result = ingress
            .command_completed(
                binding.clone(),
                id.clone(),
                json!({"status":status,"result":doc["result"],"error":doc["error"]}),
            )
            .await;
        authority
            .finish_callback(&binding, &id, result.is_ok())
            .await?;
        authority
            .finish_event_record(&binding, if result.is_ok() { "done" } else { "failed" })
            .await?;
        authority
            .release(&binding)
            .await
            .with_context(|| format!("native release id={}", binding["command"]["id"]))?;
        result?;
    }
    Ok(())
}

async fn dispatch_platform(
    database: &Database,
    authority: &Arc<RuntimeAuthority>,
    platform: &super::super::Platform,
    policy_owner: &Arc<sync_rights::StoreRightsAuthority>,
    binding: &Value,
) -> Result<()> {
    let b = binding.clone();
    let (actor, command) = database
        .writer
        .submit(WriteJob::new(JobOptions::default(), move |tx| {
            let id = text(&b["command"], "id").map_err(sql_error)?;
            let participant = text(&b["fence"], "participant").map_err(sql_error)?;
            let fence = b["fence"]["fence"]
                .as_u64()
                .ok_or_else(|| invalid("invalid: native fence"))?;
            write_contract::check_executor_fence(
                tx,
                participant,
                id,
                ctox_store::change_log::subscription::protocol::Seq(fence),
                now(),
            )?
            .map_err(|e| invalid(e.reason))?;
            let mut doc = document(tx, "business_commands", id)?
                .ok_or_else(|| invalid("not_found: platform command"))?;
            let a = actor(tx, text(&doc, "actor_id").map_err(sql_error)?)?;
            doc["module"] = doc["app_id"].clone();
            doc["command_type"] = doc["type"].clone();
            doc["client_context"]["actor"] = json!({"id":a.id});
            store_command(tx, &doc)?;
            let command = super::super::commands::CommandDocument {
                id: id.into(),
                app_id: text(&doc, "app_id").map_err(sql_error)?.into(),
                module: text(&doc, "module").map_err(sql_error)?.into(),
                command_type: text(&doc, "type").map_err(sql_error)?.into(),
                payload: doc["payload"].clone(),
            };
            Ok((a, command))
        }))?
        .await?;
    // Suspend publication before the schema/rights transaction. The existing P6
    // router is the same writer/generation owner used by replication.
    let pending = if command.command_type.starts_with("ctox.app.") {
        Some(
            policy_owner
                .router
                .begin_rights_update()
                .map_err(|e| anyhow!(e))?,
        )
    } else {
        None
    };
    let outcome = platform.dispatch(actor, command, now()).await;
    if let Some(pending) = pending {
        let refreshed = database
            .readers
            .read(|tx| sync_rights::read(tx))
            .await
            .map_err(|e| e.to_string());
        policy_owner
            .router
            .finish_rights_update(pending, refreshed)
            .map_err(|e| anyhow!(e))?;
    }
    // Platform owns its own idempotent terminal/pending receipt; do not overwrite it.
    authority.release(binding).await?;
    if outcome["status"] == "failed" {
        return Err(anyhow!(
            "unavailable: native platform effect failed; inspect typed command result"
        ));
    }
    Ok(())
}

impl crate::service::store_startup::RunningEngine for RuntimeBinding {
    fn shutdown(&mut self) -> crate::service::store_startup::EngineFuture<()> {
        self.stop.send_replace(true);
        let task = self.task.take();
        Box::pin(async move {
            if let Some(task) = task {
                task.await
                    .map_err(|_| StoreError::Panicked)?
                    .map_err(|_| invalid("unavailable: app runtime drain failed"))?;
            }
            Ok(())
        })
    }
}

async fn is_native_event(database: &Database, doc: &Value) -> Result<bool> {
    let doc = doc.clone();
    Ok(observed_read(
        database.readers.clone(),
        "native_event_receipt",
        ReadJob::new(Default::default(), move |db| origin::native_event(db, &doc)),
    )
    .await?)
}
