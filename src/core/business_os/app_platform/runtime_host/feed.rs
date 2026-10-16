// Native platform actions remain outside held Store transactions and reuse Platform.
//! Commit-driven delivery. One cursor over the canonical _changes, no table poll.
use super::*;
use ctox_app_runtime::{
    core_host::StoreCodec, registration::AppRuntimeIngress, service::ChangeEvent,
};
use ctox_store::{change_log::subscription::write_contract, WriteJob};
use tokio::{sync::watch, task::JoinHandle};

/// Read pressure is observable even while the committed feed remains attached.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum FeedHealth {
    Healthy,
    Unavailable {
        code: &'static str,
    },
    Degraded {
        code: &'static str,
        stage: &'static str,
        attempts: u32,
        callback_cpu_us: Option<u64>,
        page_limit: u32,
    },
}

// Retry only pure snapshots, before any handler/effect or cursor acknowledgement.
// The store still owns its admission/deadline/generation guards. Wall-clock
// preemption must not be interpreted as a terminal failure of the subscription.
async fn resilient_read<T: Send + 'static>(
    pool: ctox_store::read_pool::ReadPool,
    stage: &'static str,
    health: &watch::Sender<FeedHealth>,
    page_limit: &mut u32,
    mut job: impl FnMut(u32) -> ReadJob<T>,
) -> Result<T, StoreError> {
    let mut attempts = 0u32;
    let mut limit = *page_limit;
    loop {
        let (result, cpu) = measured_read(pool.clone(), stage, job(limit)).await;
        // Spend bounded CPU per page, rather than charging scheduler delays.
        if cpu.is_some_and(|us| us > 20_000) {
            limit = (limit / 2).max(1);
        }
        match result {
            Ok(value) => {
                if *health.borrow() != FeedHealth::Healthy {
                    health.send_replace(FeedHealth::Healthy);
                    eprintln!(
                        "native app feed health={{\"state\":\"healthy\",\"stage\":\"{stage}\"}}"
                    );
                }
                *page_limit = limit;
                return Ok(value);
            }
            Err(StoreError::DeadlineExceeded | StoreError::Busy) => {
                attempts = attempts.saturating_add(1);
                if attempts >= 3 {
                    let status = FeedHealth::Degraded {
                        code: "read_overload",
                        stage,
                        attempts,
                        callback_cpu_us: cpu,
                        page_limit: limit,
                    };
                    health.send_replace(status.clone());
                    if attempts == 3 || attempts % 32 == 0 {
                        eprintln!(
                            "native app feed health={}",
                            serde_json::to_string(&status).expect("feed health")
                        );
                    }
                }
                tokio::time::sleep(Duration::from_millis((10u64 << attempts.min(5)).min(500)))
                    .await;
            }
            Err(error) => return Err(error),
        }
    }
}

// A scoped test-only source delay exercises the actual native feed and owners.
#[cfg(test)]
pub(super) mod load_source {
    use super::*;
    use std::{
        path::{Path, PathBuf},
        sync::{Mutex, OnceLock},
    };
    static DELAYS: OnceLock<Mutex<BTreeMap<PathBuf, u32>>> = OnceLock::new();
    pub(crate) struct Delay(PathBuf);
    pub(crate) fn delay_pages(path: &Path, pages: u32) -> Delay {
        let path = path.to_owned();
        assert!(DELAYS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(path.clone(), pages)
            .is_none());
        Delay(path)
    }
    impl Delay {
        pub(crate) fn exhausted(&self) -> bool {
            DELAYS.get().unwrap().lock().unwrap().get(&self.0) == Some(&0)
        }
    }
    impl Drop for Delay {
        fn drop(&mut self) {
            DELAYS.get().unwrap().lock().unwrap().remove(&self.0);
        }
    }
    pub(super) fn before_page(db: &Connection) {
        let delayed = {
            let Some(delays) = DELAYS.get() else {
                return;
            };
            let mut delays = delays.lock().unwrap();
            let Some(left) = db.path().and_then(|p| delays.get_mut(Path::new(p))) else {
                return;
            };
            let delayed = *left > 0;
            *left = left.saturating_sub(1);
            delayed
        };
        if delayed {
            eprintln!("APP_PLATFORM_SLOW_CHANGE_SOURCE delay_ms=80");
            std::thread::sleep(Duration::from_millis(80));
        }
    }
}

#[cfg(test)]
mod load_tests {

    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_change_source_recovers_without_closing_feed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("feed.sqlite3");
        let writer = Connection::open(&path).unwrap();
        ctox_store::config::ConnectionConfig::default()
            .apply_writer(&writer)
            .unwrap();
        writer
            .execute_batch("CREATE TABLE head(seq INTEGER); INSERT INTO head VALUES(42)")
            .unwrap();
        let pool =
            ctox_store::read_pool::ReadPool::open(path, 1, Default::default(), Default::default())
                .await
                .unwrap();
        let (send, mut receive) = watch::channel(FeedHealth::Healthy);
        let attempts = Arc::new(AtomicUsize::new(0));
        let source = attempts.clone();
        let recovered = Arc::new(AtomicBool::new(false));
        let source_recovered = recovered.clone();
        let task_pool = pool.clone();
        let task = tokio::spawn(async move {
            resilient_read(task_pool, "change_page", &send, &mut 256, |_| {
                let source = source.clone();
                let source_recovered = source_recovered.clone();
                ReadJob::new(Default::default(), move |db| {
                    // Genuine owner-enforced read deadlines, not fabricated errors.
                    if source.fetch_add(1, Ordering::SeqCst) < 3
                        || !source_recovered.load(Ordering::SeqCst)
                    {
                        std::thread::sleep(Duration::from_millis(80));
                    }
                    db.query_row("SELECT seq FROM head", [], |r| r.get::<_, u64>(0))
                        .map_err(Into::into)
                })
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                receive
                    .changed()
                    .await
                    .expect("feed closed instead of degrading");
                let state = receive.borrow_and_update().clone();
                if let FeedHealth::Degraded {
                    code,
                    stage,
                    callback_cpu_us,
                    ..
                } = state
                {
                    assert_eq!(code, "read_overload");
                    assert_eq!(stage, "change_page");
                    #[cfg(target_os = "linux")]
                    assert!(
                        callback_cpu_us.unwrap() < 20_000,
                        "sleep is not callback CPU"
                    );
                    break;
                }
            }
        })
        .await
        .expect("sustained overload must be visible");
        recovered.store(true, Ordering::SeqCst);
        assert_eq!(
            task.await.unwrap().expect("slow page must not stop feed"),
            42
        );
        assert_eq!(*receive.borrow(), FeedHealth::Healthy);
        assert!(attempts.load(Ordering::SeqCst) >= 4);
        #[cfg(target_os = "linux")]
        {
            let (health, _) = watch::channel(FeedHealth::Healthy);
            let mut page_limit = 8;
            let head = tokio::time::timeout(
                Duration::from_secs(10),
                resilient_read(
                    pool.clone(),
                    "change_page",
                    &health,
                    &mut page_limit,
                    |limit| {
                        ReadJob::new(Default::default(), move |db| {
                            if limit > 1 {
                                let start = thread_cpu_us().unwrap();
                                while thread_cpu_us().unwrap().saturating_sub(start) < 25_000 {
                                    std::hint::black_box(());
                                }
                            }
                            db.query_row("SELECT seq FROM head", [], |r| r.get::<_, u64>(0))
                                .map_err(Into::into)
                        })
                    },
                ),
            )
            .await
            .expect("CPU-heavy pages must shrink and make progress")
            .unwrap();
            assert_eq!(head, 42);
            assert_eq!(page_limit, 1, "CPU budget must survive the successful page");
        }
        pool.close();
        let (health, _) = watch::channel(FeedHealth::Healthy);
        let closed = resilient_read(pool.clone(), "change_page", &health, &mut 256, |_| {
            ReadJob::new(Default::default(), |_| Ok(()))
        })
        .await;
        assert!(
            matches!(closed, Err(StoreError::Closed)),
            "owner shutdown is not overload"
        );
        pool.shutdown().await.unwrap();
    }
}

pub(crate) struct RuntimeBinding {
    pub ingress: AppRuntimeIngress,
    pub health: watch::Receiver<FeedHealth>,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<()>>>,
}
impl RuntimeBinding {
    /// Explicit native inputs; CUTOVER-4's final attach passes these actual owners.
    pub(crate) async fn attach(root: std::path::PathBuf, owners: RuntimeOwners) -> Result<Self> {
        let database = owners.sync.clone();
        let (health_send, health) = watch::channel(FeedHealth::Healthy);
        let runtime = owners.runtime.clone();
        let platform = super::super::Platform::from_database(&owners.sync, owners.signer.clone())?
            .with_secret_database(&owners.secrets)?
            .with_native_model_checks(owners.root.clone());
        let policy_owner = sync_rights::StoreRightsAuthority::attach(
            database.writer.clone(),
            owners.signer.instance_id().into(),
        )
        .await
        .map_err(|e| anyhow!(e))?;
        // Subscribe BEFORE the common WAL activation snapshot.
        let commits = database.writer.subscribe_commits();
        let (host, authority) =
            crate::persistence::run_core_sync(move || super::host(&owners)).await?;
        let codec = host.codec.clone();
        let (active,initial,epoch)=resilient_read(database.readers.clone(), "activation_snapshot", &health_send, &mut 256, |_| { let codec = codec.clone(); ReadJob::new(JobOptions::default(),move|db|{
            let c=collection(db,"business_apps")?;
            let rows=db.prepare(&format!("SELECT data FROM {} WHERE deleted=0 AND json_extract(data,'$.state')='active' ORDER BY id",quote(&c.table)))?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut active=Vec::new();for raw in rows {let app:Value=serde_json::from_str(&raw).map_err(sql_error)?;let id=text(&app,"id").map_err(sql_error)?;let m=codec.active_manifest(db,id)?;active.push((id.into(),text(&m,"app_ver").map_err(sql_error)?.into()));}
            let head=db.query_row("SELECT v FROM _seq WHERE singleton=1",[],|r|r.get::<_,u64>(0))?;
            let epoch:String=db.query_row("SELECT epoch FROM _meta WHERE singleton=1",[],|r|r.get(0))?;
            Ok((active,head,epoch))
        })}).await?;
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
        let registration = crate::persistence::run_core_sync(move || {
            AppRuntimeRegistration::start_with_executor(
                root,
                inputs,
                Some(Arc::new(crate::persistence::CoreCompatibilityExecutor)),
            )
        })
        .await?;
        let ingress = registration.ingress.clone();
        let (stop, stopping) = watch::channel(false);
        let feed_ingress = ingress.clone();
        let task = runtime.spawn(async move {
            let feed_health = health_send.clone();
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
                health_send,
            )
            .await;
            if result.is_err() {
                feed_health.send_replace(FeedHealth::Unavailable {
                    code: "feed_stopped",
                });
                eprintln!("native app runtime feed stopped; ingress is closing: {result:?}");
            }
            let drained = registration.shutdown().await;
            result.and(drained)
        });
        Ok(Self {
            ingress,
            health,
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
    health: watch::Sender<FeedHealth>,
) -> Result<()> {
    // Recover pending commands once. Running/uncertain effects are never replayed.
    let mut startup_limit = 256;
    let startup_read = resilient_read(
        database.readers.clone(),
        "startup_commands",
        &health,
        &mut startup_limit,
        |_| {
            ReadJob::new(JobOptions::default(), |db| {
                let c = collection(db, "business_commands")?;
                db.prepare(&format!("SELECT data FROM {} WHERE deleted=0 AND json_extract(data,'$.status') IN ('pending','accepted','queued') ORDER BY id",quote(&c.table)))?.query_map([],|r|r.get::<_,String>(0))?.map(|r|r.map_err(Into::into).and_then(|s|serde_json::from_str::<Value>(&s).map_err(sql_error))).collect::<Result<Vec<_>,StoreError>>()
            })
        },
    );
    tokio::pin!(startup_read);
    let startup = tokio::select! { biased;
        _ = stop.changed() => return Ok(()),
        result = &mut startup_read => result?,
    };
    for doc in startup {
        if !tokio::select! { biased;
            _ = stop.changed() => return Ok(()),
            result = is_native_event(&database, &doc, &health) => result?,
        } {
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
    let mut page_limit = 256;
    loop {
        // Drain retained heads before awaiting the next coalesced wakeup.
        loop {
            let after = cursor;
            if *stop.borrow() {
                return Ok(());
            }
            let read = resilient_read(
                database.readers.clone(),
                "change_page",
                &health,
                &mut page_limit,
                |limit| {
                    ReadJob::new(JobOptions::default(), move |db| {
                        #[cfg(test)]
                        load_source::before_page(db);
                        let mut statement=db.prepare("SELECT c.collection,c.id,c.seq,c.op,c.gen FROM _changes c JOIN _collections r ON r.name=c.collection AND r.gen=c.gen AND r.state='active' WHERE c.seq>?1 ORDER BY c.seq LIMIT ?2")?;
                        let rows = statement
                            .query_map(params![after, limit], |r| {
                                Ok((
                                    r.get::<_, String>(0)?,
                                    r.get::<_, String>(1)?,
                                    r.get::<_, u64>(2)?,
                                    r.get::<_, u8>(3)?,
                                    r.get::<_, u64>(4)?,
                                ))
                            })?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                        rows.into_iter().map(|(collection,id,seq,op,gen)|{
                    let doc=if op==1{None}else{document(db,&collection,&id)?};
                    let prior:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 AND seq<?4)",params![collection,gen,id,seq],|r|r.get(0))?;
                    Ok((collection,id,seq,if op==1{"delete"}else if prior{"update"}else{"insert"}.to_owned(),doc))
                }).collect::<Result<Vec<_>,StoreError>>()
                    })
                },
            );
            tokio::pin!(read);
            let page = loop {
                tokio::select! { biased;
                    _ = stop.changed() => return Ok(()),
                    page = &mut read => break page?,
                    _ = lease_tick.tick() => authority.renew_events().await?,
                }
            };
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
                        if !tokio::select! { biased;
                            _ = stop.changed() => return Ok(()),
                            result = is_native_event(&database, &doc, &health) => result?,
                        } {
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
        crate::persistence::run_core_sync(move || owner.completion_binding(&command_id)).await?;
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

/// ReadPool already pins a transaction before calling us. This is the same
/// native fence check as commit, borrowing that snapshot without nesting BEGIN.
pub(super) async fn check_model_fence(
    pool: ctox_store::read_pool::ReadPool,
    check: Arc<dyn Fn(&rusqlite::Transaction<'_>) -> Result<(), StoreError> + Send + Sync>,
) -> Result<(), StoreError> {
    pool.read(move |tx| check(tx).map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string())))
        .await.map_err(Into::into)
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
    let model_fence = if command.command_type == "instance.providers.models.check" {
        let native = authority.clone();
        let invocation = binding.clone();
        let pool = database.readers.clone();
        let runtime = authority.runtime.clone();
        let codec = authority.codec.clone();
        let effect: super::super::ModelEffectFence = Arc::new(move |effect| {
            ctox_app_runtime::core_host::CoreAuthority::with_fence(
                native.as_ref(),
                &invocation,
                &mut || {
                    let pool = pool.clone();
                    let codec = codec.clone();
                    let invocation = invocation.clone();
                    // Current P9 row, actor, app/version and rights, using the
                    // existing reader owner, immediately before the model effect.
                    wait(&runtime, async move {
                        check_model_fence(pool, Arc::new(move |tx| codec.check_fence(tx, &invocation))).await
                    })?;
                    effect()
                },
            )
        });
        let invocation = binding.clone();
        let codec = authority.codec.clone();
        Some(super::super::ModelCheckFence {
            effect,
            commit: Arc::new(move |tx| codec.check_fence(tx, &invocation)),
        })
    } else {
        None
    };
    let outcome = platform
        .dispatch_fenced(actor, command, now(), model_fence)
        .await;
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

async fn is_native_event(
    database: &Database,
    doc: &Value,
    health: &watch::Sender<FeedHealth>,
) -> Result<bool> {
    let doc = doc.clone();
    Ok(resilient_read(
        database.readers.clone(),
        "native_event_receipt",
        health,
        &mut 256,
        |_| {
            let doc = doc.clone();
            ReadJob::new(Default::default(), move |db| origin::native_event(db, &doc))
        },
    )
    .await?)
}
