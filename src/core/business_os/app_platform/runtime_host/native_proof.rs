//! Isolated native foreground, real encrypted signer and quorum; only relay/data are test inputs.
use super::*;
use crate::service::store_startup::Foreground;
use ctox_store::{
    schema::{self, CollectionDefinition},
    WriteJob,
};
use ctox_sync::{authority::Peer, host_config::HostMember};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
// A production peer has its own runtime and policy context. Keep test peers
// equally isolated; P6 deliberately rejects multiple owners in one context.
struct PeerRuntime(Option<tokio::runtime::Runtime>);
impl Drop for PeerRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            runtime.shutdown_background();
        }
    }
}
struct Signal(tokio::task::JoinHandle<()>, String);
impl Drop for Signal {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl Signal {
    async fn start() -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("ws://{}?role=ctox_instance", listener.local_addr()?);
        let task = tokio::spawn(async move {
            let members: Arc<Mutex<BTreeMap<String, mpsc::Sender<Message>>>> = Arc::default();
            let mut children = tokio::task::JoinSet::new();
            let mut next = 0;
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                next += 1;
                let id = format!("native{next:06}");
                let members = members.clone();
                children.spawn(async move {
                    let (mut writer, mut reader) = tokio_tungstenite::accept_async(stream).await.unwrap().split();
                    let (tx, mut rx) = mpsc::channel(128);
                    tx.try_send(Message::text(serde_json::json!({"type":"init","yourPeerId":id}).to_string())).unwrap();
                    loop {
                        tokio::select! {
                            next = rx.recv() => {
                                let Some(next) = next else { break; };
                                if writer.send(next).await.is_err() { break; }
                            }
                            next = reader.next() => {
                                let Some(Ok(Message::Text(text))) = next else { break; };
                                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                                match value["type"].as_str() {
                                    Some("join") => {
                                        let mut all = members.lock().unwrap();
                                        all.insert(id.clone(), tx.clone());
                                        let ids: Vec<_> = all.keys().cloned().collect();
                                        let peers: Vec<_> = ids.iter().map(|id|serde_json::json!({"peerId":id,"role":"ctox_instance"})).collect();
                                        let packet = Message::text(serde_json::json!({"type":"joined","otherPeerIds":ids,"peers":peers}).to_string());
                                        for target in all.values() { target.try_send(packet.clone()).unwrap(); }
                                    }
                                    Some("signal") => {
                                        assert_eq!(value["senderPeerId"], id);
                                        let target = members.lock().unwrap().get(value["receiverPeerId"].as_str().unwrap()).cloned();
                                        if let Some(target) = target { target.send(Message::text(value.to_string())).await.unwrap(); }
                                    }
                                    Some("ping") => {}
                                    other => panic!("unexpected native signaling {other:?}"),
                                }
                            }
                        }
                    }
                    members.lock().unwrap().remove(&id);
                });
            }
        });
        Ok(Self(task, url))
    }
}

pub(super) async fn terminal(database: &Database, name: &str, id: &str) -> Result<Value> {
    let mut commits = database.writer.subscribe_commits();
    let name = name.to_owned();
    let id = id.to_owned();
    let started = Instant::now();
    let mut observed = json!({"present":false});
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let n = name.clone();
            let i = id.clone();
            let doc = observed_read(database.readers.clone(), "proof_terminal", ReadJob::new(Default::default(), move |db| {
                document(db, &n, &i)
            })).await.with_context(|| format!("native terminal read {name}/{id}"))?;
            if let Some(doc) = doc {
                observed = json!({"present":true,"status":doc["status"],"error_code":doc["error"]["code"]});
                if name != "business_commands"
                    || matches!(
                        doc["status"].as_str(),
                        Some("done" | "failed" | "cancelled")
                    )
                {
                    return Ok::<_, anyhow::Error>(doc);
                }
            }
            commits.changed().await?;
        }
    })
    .await
    .with_context(|| format!("native daemon did not deliver {name}/{id}; elapsed_ms={} last={observed}", started.elapsed().as_millis()))?
}
pub(super) async fn submit(
    database: &Database,
    id: &str,
    app: &str,
    kind: &str,
    payload: Value,
) -> Result<Value> {
    let doc = json!({"id":id,"app_id":app,"type":kind,"actor_id":"owner","created_at":chrono::Utc::now().to_rfc3339(),"payload":payload,"status":"queued"});
    database
        .writer
        .submit(WriteJob::new(Default::default(), move |tx| {
            store_command(tx, &doc)
        }))?
        .await?;
    terminal(database, "business_commands", id).await
}
fn seed(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    let names = [
        "business_apps",
        "business_app_versions",
        "business_users",
        "business_commands",
        "app_drafts",
        "app_catalog",
        "app_activations",
        "files",
        "ctox_runtime_settings",
    ];
    for name in names {
        schema::create_collection(
            tx,
            &CollectionDefinition {
                name: name.into(),
                app_id: "ctox-platform".into(),
                schema_ver: 1,
                schema_json: json!({"version":1,"primaryKey":"id","type":"object","properties":{"id":{"type":"string","maxLength":128}},"required":["id"]}),
                class: "full".into(),
                client_limit: 0,
                byte_budget: 1048576,
                rights_ver: 1,
                app_ver: "platform-1".into(),
            },
        )?;
    }
    sync_rights::initialize(tx)?;
    sync_rights::replace(
        tx,
        1,
        &policy::base_release_rights(&names.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
    )?;
    super::super::commands::put(
        tx,
        "business_users",
        "owner",
        &json!({"id":"owner","user_id":"owner","role":"admin","active":true}),
    )?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_daemon_hotplug_roundtrip_and_change() -> Result<()> {
    run_native_proof(Proof::All).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_daemon_e42_app_handlers() -> Result<()> {
    run_native_proof(Proof::E42).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_daemon_e39_routing_contracts() -> Result<()> {
    run_native_proof(Proof::E39).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_daemon_platform_route_stays_in_core() -> Result<()> {
    run_native_proof(Proof::Platform).await
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Proof {
    All,
    E39,
    Platform,
    E42,
}
async fn run_native_proof(proof: Proof) -> Result<()> {
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "refuse installed state root"
    );
    let relay = Signal::start().await?;
    let mut roots = Vec::new();
    let mut bootstrap = Vec::new();
    let mut peers = BTreeMap::new();
    for node in 1..=3 {
        let root = tempfile::Builder::new()
            .prefix("ap4b-")
            .tempdir_in("/tmp")?;
        let owner = Foreground::start(root.path().to_owned(), Handle::current()).await?;
        owner
            .inputs
            .sync
            .writer
            .submit(WriteJob::new(Default::default(), seed))?
            .await?;
        peers.insert(
            node,
            Peer {
                identity: owner.inputs.instance_signer()?.key_id().into(),
                executor: true,
                data_replica: true,
            },
        );
        roots.push(root);
        bootstrap.push(owner);
    }
    for (index, owner) in bootstrap.iter_mut().enumerate() {
        let config = ctox_sync::host_config::HostConfiguration {
            version: 1,
            scope_id: "app-runtime-native-proof".into(),
            local: HostMember::Voter {
                node_id: index as u64 + 1,
            },
            voters: peers.clone(),
            timing: Default::default(),
        };
        config.validate()?;
        let encoded = serde_json::to_string(&config)?;
        owner.inputs.databases["runtime"].writer.submit(WriteJob::new(Default::default(),move |tx|{
            tx.execute_batch("CREATE TABLE ctox_sync_host(singleton INTEGER PRIMARY KEY CHECK(singleton=1),configuration TEXT NOT NULL)")?;
            tx.execute("INSERT INTO ctox_sync_host VALUES(1,?1)",[encoded])?;Ok(())
        }))?.await?;
        let root = owner.inputs.root.clone();
        let transport = json!({"signalingUrls":[relay.1],"iceServers":[]}).to_string();
        crate::persistence::run_core_sync(move || {
            crate::secrets::write_secret_record(
                &root,
                crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0,
                "transport:app-runtime-native-proof",
                &transport,
                None,
                json!({"source":"isolated-native-proof"}),
            )
        })
        .await?;
        owner.shutdown().await?;
    }
    let mut peer_runtimes = Vec::new();
    let mut owners = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        if index == 0 {
            owners.push(Foreground::start(root.path().to_owned(), Handle::current()).await?);
        } else {
            let peer = PeerRuntime(Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()?,
            ));
            let runtime = peer.0.as_ref().unwrap().handle().clone();
            let owner_runtime = runtime.clone();
            let path = root.path().to_owned();
            owners.push(
                runtime
                    .spawn(async move { Foreground::start(path, owner_runtime).await })
                    .await??,
            );
            peer_runtimes.push(peer);
        }
    }
    let proof:Result<Value>=async {
        for owner in &owners {
            tokio::time::timeout(Duration::from_secs(15),owner.inputs.native_session()?.execution_authority.worker_membership(9001)).await??;
        }
        let database=&owners[0].inputs.sync;
        // Validation rollback publishes no rights. It must produce a typed
        // terminal receipt and leave the shared feed available for installation.
        let invalid=submit(database,"hot-invalid-publication","ctox-platform","ctox.app.publish",json!({"app_id":"hot-app"})).await?;
        anyhow::ensure!(invalid["status"]=="failed"&&invalid["error"]["code"]=="invalid","invalid publication must not revoke the feed: {invalid}");
        let delayed_pages = feed::load_source::delay_pages(database.writer.db_id().path(), 3);
        let source=b"export async function handle(command,ctx){if(command.payload.write)await ctx.put('hot_rows',{id:command.payload.id,n:command.payload.n});return {n:command.payload.n,sender:command.app_id,target:'hot-app'};}";
        let platform_trap=b"export async function handle(){throw new Error('E39 platform reached app process');}";
        let forward=b"export async function handle(command,ctx){return await ctx.command('hot.cross',command.payload,{id:'e39-child',on_complete:'hot.completed'});}";
        let callback=b"export async function handle(command,ctx){await ctx.put('hot_rows',{id:'e39-callback',n:command.payload.result.n});return true;}";
        let trigger=b"export default async(command,ctx)=>{await ctx.put('hot_events',{id:'change',n:command.payload.seq});return true;}";
        let cas=Cas::open(crate::paths::runtime_dir(roots[0].path()).join("transfers"))?;
        let mut files=serde_json::Map::new();
        for (path,bytes) in [("handlers/run.js",source.as_slice()),("handlers/change.js",trigger.as_slice()),("handlers/forward.js",forward.as_slice()),("handlers/callback.js",callback.as_slice()),("handlers/platform-trap.js",platform_trap.as_slice())] {
            let hash=digest(bytes);let mut receiver=cas.receive(&digest(path.as_bytes()),&hash,bytes.len() as u64)?;
            receiver.append(0,bytes)?;receiver.finish()?;
            let size=bytes.len();let hash_for_row=hash.clone();let file_path=path.to_owned();
            database.writer.submit(WriteJob::new(Default::default(),move |tx|super::super::commands::put(tx,"files",&hash_for_row,&json!({"id":hash_for_row,"ref":hash_for_row,"size":size,"name":file_path,"owner_app":"hot-app","mime":"text/javascript"}))))?.await?;
            files.insert(path.into(),json!(hash));
        }
        let definitions=json!({"hot_rows":{"schema":{"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"n":{"type":"integer"}},"required":["id","n"]},"class":"full","client_limit":0,"byte_budget":1048576},"hot_events":{"schema":{"version":1,"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"n":{"type":"integer"}},"required":["id","n"]},"class":"full","client_limit":0,"byte_budget":1048576}});
        let draft=json!({"id":"hot-draft","app_id":"hot-app","author":"owner","files":files,"collections_schema":definitions,"manifest_fields":{"title":"Native hot-plug proof","handlers":{"hot.echo":"handlers/run.js","hot.core":"handlers/run.js","hot.cross":"handlers/run.js","hot.own":"handlers/run.js","hot.forward":"handlers/forward.js","hot.completed":"handlers/callback.js","ctox.workjet.project.list":"handlers/platform-trap.js"},"commands_out":[{"app":"news","type":"news.issue.generate"},{"app":"matching","type":"matching.match"},{"app":"receiver","type":"hot.cross"},{"app":"receiver","type":"hot.denied"},{"app":"ghost","type":"hot.unknown"},{"app":"receiver","type":"hot.ambiguous"},{"app":"ghost","type":"hot.ambiguous"},{"app":"hot-app","type":"hot.own"},{"app":"receiver","type":"hot.own"},{"app":"receiver","type":"hot.wildcard"},{"app":"receiver","type":"hot.nohandler"},{"app":"receiver","type":"hot.restricted"},{"app":"receiver","type":"hot.pre.run"},{"app":"receiver","type":"hot.allow.run"},{"app":"receiver","type":"hot.fallback"}],"command_rights":{"hot.core":{"roles":[],"assigned_users":false},"ctox.workjet.project.list":{"roles":["admin"]}},"triggers":[{"handler":"handlers/change.js","on":{"collection":"hot_rows","op":"insert"}}]}});
        database.writer.submit(WriteJob::new(Default::default(),move |tx|super::super::commands::put(tx,"app_drafts","hot-draft",&draft)))?.await?;
        let started=Instant::now();
        let published=submit(database,"hot-publish","ctox-platform","ctox.app.publish",json!({"app_id":"hot-app","base_app_ver":null,"draft_id":"hot-draft"})).await?;
        anyhow::ensure!(published["status"]=="done","native publish failed: {published}");
        let publish_us=started.elapsed().as_micros();
        eprintln!("APP_FEED_EVENT ts={} event=install_complete app=hot-app", now());
        anyhow::ensure!(delayed_pages.exhausted(), "native change source load was not exercised");
        let result=submit(database,"hot-write","hot-app","hot.echo",json!({"id":"one","n":7,"write":true})).await?;
        anyhow::ensure!(result["status"]=="done"&&result["result"]["n"]==7,"native handler roundtrip failed: {result}");
        if proof == Proof::Platform {
            let result=submit(database,"e39-platform-only","hot-app","ctox.workjet.project.list",json!({})).await?;
            anyhow::ensure!(feed::core_route_probe::observed("e39-platform-only"),"E39 platform route must invoke Core, never an app process: {result}");
            anyhow::ensure!(result["app_id"]=="hot-app","E39 platform result sender changed: {result}");
            return Ok(json!({"core_route_observed":true,"app_process_route":false,"core_result_status":result["status"]}));
        }
        if proof == Proof::E42 {
            e42_proof::prove(database, &cas).await?;
            return Ok(json!({"e42_only":true}));
        }
        if proof == Proof::All {
            e25_binding::assert_native_workjet_route(database).await?;
        }
        routing_proof::prove(database, &cas).await?;
        if proof == Proof::All {
            e42_proof::prove(database, &cas).await?;
        }
        let changed=terminal(database,"hot_events","change").await?;
        anyhow::ensure!(changed["n"].as_u64().is_some_and(|n|n>0),"committed change did not run real trigger");
        // E23.1: forged client fields confer no native provenance, including admin.
        let forged=json!({"id":"hot-client-core","app_id":"hot-app","type":"hot.core","actor_id":"owner","payload":{"n":99},"status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"origin":"core","runtime_event":true,"client_context":{"origin":"scheduler","role":"admin"}});
        database.writer.submit(WriteJob::new(Default::default(),move|tx|store_command(tx,&forged)))?.await?;
        let denied=terminal(database,"business_commands","hot-client-core").await?;
        anyhow::ensure!(denied["status"]=="failed"&&denied["error"]["code"]=="forbidden","E23 client core-only command admitted: {denied}");
        let audited=database.readers.read_job(ReadJob::new(Default::default(),|db|Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM _app_runtime_command_audit WHERE id='hot-client-core' AND code='forbidden')",[],|r|r.get::<_,bool>(0))?))).await?;
        anyhow::ensure!(audited,"E23 core-only denial was not audited");
        database.writer.submit(WriteJob::new(Default::default(),|tx|enqueue_core_command(tx,CoreCommandSource::Scheduler,"hot-native-core","hot-app","hot.core","owner",json!({"id":"core-row","n":23,"write":true}))))?.await?;
        let core=terminal(database,"business_commands","hot-native-core").await?;
        anyhow::ensure!(core["status"]=="done"&&core["result"]["n"]==23,"E23 native core command denied: {core}");
        let row=terminal(database,"hot_rows","core-row").await?;
        anyhow::ensure!(row["n"]==23,"E23 core handler write missing");
        // A client rewrite cannot reuse the private native admission receipt.
        let mut rewritten=core.clone();rewritten["status"]=json!("queued");rewritten["payload"]["n"]=json!(24);
        database.writer.submit(WriteJob::new(Default::default(),move|tx|super::super::commands::put(tx,"business_commands","hot-native-core",&rewritten)))?.await?;
        let denied=terminal(database,"business_commands","hot-native-core").await?;
        anyhow::ensure!(denied["status"]=="failed"&&denied["error"]["code"]=="forbidden","E23 rewritten core command admitted: {denied}");
        let audited=database.readers.read_job(ReadJob::new(Default::default(),|db|Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM _app_runtime_command_audit WHERE id='hot-native-core' AND code='forbidden')",[],|r|r.get::<_,bool>(0))?))).await?;
        anyhow::ensure!(audited,"E23 rewritten core command denial was not audited");
        let mut samples=Vec::new();
        for i in 0..100 {
            let started=Instant::now();let result=submit(database,&format!("hot-echo-{i}"),"hot-app","hot.echo",json!({"n":i})).await?;
            anyhow::ensure!(result["status"]=="done"&&result["result"]["n"]==i,"native echo failed: {result}");samples.push(started.elapsed().as_micros());
        }
        samples.sort_unstable();
        let active=database.readers.read_job(ReadJob::new(Default::default(),|db|document(db,"business_apps","hot-app"))).await?.context("active app missing")?;
        let removed=submit(database,"hot-remove","ctox-platform","ctox.app.remove",json!({"app_id":"hot-app","base_app_ver":active["active_ver"]})).await?;
        anyhow::ensure!(removed["status"]=="done","native remove failed: {removed}");
        // E27 now rejects an unknown post-removal type at the writer, before
        // a queued document or process exists; do not expect delivery failure.
        let denied=database.writer.submit(WriteJob::new(Default::default(),|tx|store_command(tx,&json!({"id":"after-remove","app_id":"hot-app","type":"hot.echo","actor_id":"owner","created_at":chrono::Utc::now().to_rfc3339(),"payload":{"n":-1},"status":"queued"}))))?.await;
        anyhow::ensure!(matches!(&denied,Err(StoreError::Invalid(reason)) if error_code(reason)=="unknown_command"),"removed app must reject unknown command at write: {denied:?}");
        let absent=database.readers.read_job(ReadJob::new(Default::default(),|db|Ok(document(db,"business_commands","after-remove")?.is_none()))).await?;
        anyhow::ensure!(absent,"removed app persisted a rejected command");
        Ok(json!({"publish_us":publish_us,"command_roundtrip_p99_us":samples[98],"samples":samples.len(),"native_quorum":true,"published":true,"change":true,"removed":true,"post_remove_denied":true,"e23_core_delivered":true,"e23_client_forbidden_audited":true,"slow_source_recovered":true}))
    }.await;
    if let Err(error) = &proof {
        eprintln!("native hot-plug operation failed: {error:#}");
    }
    let mut shutdown_errors = Vec::new();
    for owner in &mut owners {
        if let Err(error) = owner.shutdown().await {
            shutdown_errors.push(error.to_string());
        }
    }
    assert!(
        proof.is_ok() && shutdown_errors.is_empty(),
        "productive native hot-plug failed: {proof:?}; shutdown: {shutdown_errors:?}"
    );
    println!("APP_PLATFORM_NATIVE_PROOF {}", proof?);
    Ok(())
}

mod routing_proof {
    use super::*;
    include!("routing_proof.rs");
}

mod e25_binding {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/sync_v4_e25/native_binding.rs"
    ));
}
