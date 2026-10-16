//! Real foreground Store owners and the persisted CUTOVER signer, no fixture authority.
use super::*;
use crate::service::store_startup::{isolate_test_roots, Foreground};
use ctox_store::{
    schema::{self, CollectionDefinition},
    WriteJob,
};
use serde_json::json;

fn hello(c: &V4LaunchContext) -> &Hello {
    match &c.v4.hello.offer {
        HelloFrame::Hello(h) => h,
        _ => panic!("not a hello offer"),
    }
}
fn put(tx: &Transaction<'_>, collection: &str, id: &str, doc: &Value) -> rusqlite::Result<()> {
    let table: String = tx.query_row(
        "SELECT table_name FROM _collections WHERE name=?1 AND state='active'",
        [collection],
        |r| r.get(0),
    )?;
    tx.execute(&format!("INSERT INTO {} (id,rev,schema_ver,data,deleted) VALUES(?1,'1',1,?2,0) ON CONFLICT(id) DO UPDATE SET data=excluded.data",quote(&table)),rusqlite::params![id,doc.to_string()])?;
    Ok(())
}
pub(crate) async fn fixture() -> Result<(tempfile::TempDir, Foreground, BusinessOsUser)> {
    let root = tempfile::Builder::new().prefix("e41-").tempdir()?;
    let fg = Foreground::start(root.path().into(), tokio::runtime::Handle::current())
        .await
        .context("start isolated foreground")?;
    let signer = fg.inputs.instance_signer()?;
    fg.inputs.sync.writer.submit(WriteJob::new(Default::default(),move |tx| {
        for (name,owner) in [("business_users","ctox-platform"),("business_apps","ctox-platform"),("business_app_versions","ctox-platform"),("business_commands","ctox-platform"),("notes","notes-app"),("foreign_rows","foreign-app")] {
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM _collections WHERE name=?1)",[name],|r|r.get(0))?;
            if !exists {schema::create_collection(tx,&CollectionDefinition{name:name.into(),app_id:owner.into(),schema_ver:1,schema_json:json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"1".into()})?;}
        }
        put(tx,"business_users","alice",&json!({"id":"alice","active":true,"role":"founder","display_name":"Alice"}))?;
        put(tx,"business_apps","draft-app",&json!({"id":"draft-app","state":"active","assigned_user_ids":["alice"]}))?;
        for app in ["shell","notes-app","foreign-app"] {
            let caps=if app=="shell" {vec!["agent.coding.turn","agent.coding.cancel"]}else if app=="foreign-app" {vec!["agent.task.cancel"]}else{vec!["agent.task"]};
            let manifest=json!({"app_id":app,"app_ver":"1","capabilities_required":caps,"collections":{}});
            let bytes=serde_json::to_vec(&manifest).unwrap();
            let sig=signer.sign(&bytes).unwrap().iter().map(|v|format!("{v:02x}")).collect::<String>();
            put(tx,"business_apps",app,&json!({"id":app,"state":"active","active_ver":"1","assigned_user_ids":if app=="foreign-app"{vec!["bob"]}else{vec!["alice"]},"capabilities_approved":caps,"provenance":{"owner_id":if app=="notes-app"{"alice"}else{"platform"}}}))?;
            let id=format!("{app}@1");
            put(tx,"business_app_versions",&id,&json!({"id":id,"state":"published","manifest":manifest,"manifest_sha256":format!("{:x}",Sha256::digest(bytes)),"signature":sig}))?;
        }
        Ok(())
    }))?.await.context("seed native fixture metadata")?;
    let authority = sync_rights::StoreRightsAuthority::attach(
        fg.inputs.sync.writer.clone(),
        fg.inputs.instance_signer()?.instance_id().into(),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let collections = [
        "business_users",
        "business_apps",
        "business_app_versions",
        "business_commands",
        "notes",
        "foreign_rows",
    ]
    .map(str::to_owned);
    authority
        .replace(policy::base_release_rights(&collections), vec![])
        .await
        .map_err(anyhow::Error::msg)?;
    let config_root = root.path().to_owned();
    // Existing official Runtime owner; bounded synthetic TURN settings, no network.
    crate::persistence::run_core_sync(move || crate::inference::runtime_env::set_runtime_env_value(&config_root,"CTOX_BUSINESS_OS_ICE_SERVERS",r#"[{"urls":"turn:fixture.invalid:3478","username":"synthetic","credential":"synthetic"}]"#)).await.context("configure isolated TURN fixture")?;
    let user = BusinessOsUser {
        id: "alice".into(),
        display_name: "untrusted display".into(),
        role: "admin".into(),
        active: true,
        profile: json!({}),
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    super::super::browser_bootstrap::settle(&fg.inputs).await?;
    Ok((root, fg, user))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn launch_context_v4_real_owners_shape_signature_freshness_and_rights() -> Result<()> {
    let _scope = isolate_test_roots().await;
    ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "fixture requires an isolated explicit root"
    );
    let (_root, mut fg, user) = fixture().await?;
    // A plain OS callback has no admitted Core TLS. Prepared bootstrap reads
    // still succeed: moving computation back here trips the real Core guard.
    let prepared_root = fg.inputs.root.clone();
    let prepared = std::thread::spawn(move ||
        super::super::browser_bootstrap::prepared(&prepared_root, "memory-only-contract")
    ).join().expect("bootstrap callback panicked")?;
    assert!(prepared.config.signaling_room_password.is_empty());
    assert!(prepared.catalog["business_app_versions"].as_array().is_some_and(|v| !v.is_empty()));
    let first = build(&fg.inputs, &user)
        .await
        .context("first launch build")?;
    let second = build(&fg.inputs, &user)
        .await
        .context("second launch build")?;
    let h = hello(&first);
    let key = URL_SAFE_NO_PAD.decode(&first.v4.package_keys[0].public_key)?;
    let key: [u8; 32] = key.try_into().unwrap();
    assert_eq!(
        verify_offer(h, &first.v4.signaling.peer_id, &key)?,
        first.v4.actor
    );
    assert!(verify_offer(h, "other-peer", &key).is_err());
    let mut tampered = h.clone();
    tampered.scopes_wanted.push("foreign_rows".into());
    assert!(verify_offer(&tampered, &first.v4.signaling.peer_id, &key).is_err());
    let mut tampered = h.clone();
    tampered.capabilities.clear();
    assert!(verify_offer(&tampered, &first.v4.signaling.peer_id, &key).is_err());
    assert_ne!(
        h.device_proof.nonce,
        hello(&second).device_proof.nonce,
        "fresh launch nonce per request"
    );
    assert_ne!(
        h.device_proof.device_id,
        hello(&second).device_proof.device_id
    );
    assert_eq!(
        first.v4.actor,
        Actor {
            id: "alice".into(),
            role: "founder".into()
        },
        "canonical role, never caller role"
    );
    assert_eq!(
        h.scopes_wanted,
        vec![
            "business_app_versions",
            "business_apps",
            "business_commands",
            "notes"
        ],
        "foreign app never inherits broad collection grants"
    );
    assert_eq!(
        h.capabilities,
        vec!["agent.coding.cancel", "agent.coding.turn", "agent.task"],
        "all approved required capabilities incl. E43.2 coding"
    );
    assert_eq!(h.transport.codecs, vec![Codec::Gzip]);
    let bytes = first
        .v4
        .hello
        .offer
        .encode()
        .map_err(|e| anyhow!("{e:?}"))?;
    HelloFrame::decode(&bytes).map_err(|e| anyhow!("{e:?}"))?;
    let mut golden = serde_json::to_value(&first)?;
    // Golden includes every property name, nesting level and optional ICE field;
    // only random/key/config values are normalized, not structure or privileges.
    golden["v4"]["instanceId"] = json!("INSTANCE");
    for (field, value) in [
        ("urls", json!(["wss://fixture.invalid"])),
        ("room", json!("ROOM")),
        ("peerId", json!("PEER")),
        ("nativePeerId", json!("NATIVE")),
        ("token", json!("TOKEN")),
    ] {
        golden["v4"]["signaling"][field] = value;
    }
    golden["v4"]["hello"]["offer"]["instance_id"] = json!("INSTANCE");
    for (field, value) in [
        ("device_id", "DEVICE"),
        ("public_key", "KEY"),
        ("nonce", "NONCE"),
        ("signature", "SIG"),
    ] {
        golden["v4"]["hello"]["offer"]["device_proof"][field] = json!(value);
    }
    golden["v4"]["packageKeys"][0]["keyId"] = json!("ID");
    golden["v4"]["packageKeys"][0]["publicKey"] = json!("KEY");
    assert_eq!(
        golden,
        json!({"v4":{"instanceId":"INSTANCE","userId":"alice","actor":{"id":"alice","role":"founder"},"signaling":{"urls":["wss://fixture.invalid"],"room":"ROOM","peerId":"PEER","nativePeerId":"NATIVE","token":"TOKEN","iceServers":[{"urls":"turn:fixture.invalid:3478","username":"synthetic","credential":"synthetic"}]},"hello":{"offer":{"type":"hello","protocol_ver":4,"bootstrap_ver":1,"app_api_ver":{"min":"4.0","max":"4.0"},"instance_id":"INSTANCE","capabilities":["agent.coding.cancel","agent.coding.turn","agent.task"],"device_proof":{"device_id":"DEVICE","public_key":"KEY","nonce":"NONCE","signature":"SIG"},"scopes_wanted":["business_app_versions","business_apps","business_commands","notes"],"transport":{"codecs":["gzip"],"max_frame_bytes":16384,"fragmentation":true,"reassembly":true}}},"packageKeys":[{"keyId":"ID","alg":"Ed25519","publicKey":"KEY"}]}})
    );
    // Test-only export of a synthetic bootstrap for the actual bootV4 consumer.
    if let Some(path) = std::env::var_os("CTOX_LAUNCH_CONTEXT_PROOF") {
        std::fs::write(&path, serde_json::to_vec(&first)?)?;
        export_browser_fixture(&fg.inputs, &first, std::path::Path::new(&path)).await?;
    }
    // A user assigned to an app may use it; only its source owner may code.
    fg.inputs
        .sync
        .writer
        .submit(WriteJob::new(Default::default(), |tx| {
            let mut record = document(tx, "business_apps", "notes-app")
                .map_err(|e| ctox_store::StoreError::Invalid(e.to_string()))?;
            record["provenance"]["owner_id"] = json!("other-owner");
            put(tx, "business_apps", "notes-app", &record)?;
            Ok(())
        }))?
        .await
        .context("change target source owner")?;
    assert_eq!(
        hello(
            &build(&fg.inputs, &user)
                .await
                .context("assigned non-owner launch")?
        )
        .capabilities,
        vec!["agent.task"],
        "assignment is not target source-edit authority"
    );
    fg.inputs
        .sync
        .writer
        .submit(WriteJob::new(Default::default(), |tx| {
            let mut record = document(tx, "business_apps", "notes-app")
                .map_err(|e| ctox_store::StoreError::Invalid(e.to_string()))?;
            record["provenance"]["owner_id"] = json!("alice");
            put(tx, "business_apps", "notes-app", &record)?;
            Ok(())
        }))?
        .await
        .context("restore target source owner")?;
    // Revoke coding permission through the same writer: no cached role/grant.
    let authority = sync_rights::StoreRightsAuthority::attach(
        fg.inputs.sync.writer.clone(),
        fg.inputs.instance_signer()?.instance_id().into(),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let (_, mut rights) = authority
        .hello_snapshot()
        .await
        .map_err(anyhow::Error::msg)?;
    rights
        .rules
        .retain(|r| !(r.role == "founder" && r.collection == "policy:apps.source.edit"));
    authority
        .replace(rights.rules, vec![])
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        hello(&build(&fg.inputs, &user).await?).capabilities,
        vec!["agent.task"]
    );
    fg.inputs
        .sync
        .writer
        .submit(WriteJob::new(Default::default(), |tx| {
            put(
                tx,
                "business_users",
                "alice",
                &json!({"id":"alice","role":"founder","active":false}),
            )?;
            Ok(())
        }))?
        .await?;
    assert!(
        build(&fg.inputs, &user).await.is_err(),
        "revoked authenticated principal cannot bootstrap"
    );
    fg.shutdown().await?;
    assert!(
        build(&fg.inputs, &user).await.is_err(),
        "retired owners cannot bootstrap"
    );
    Ok(())
}


/// Real retained foreground owners, production HTTP workers and packaged V4 assets.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prepared_v4_http_cold_latency_and_next_request_rights() -> Result<()> {
    let _scope = isolate_test_roots().await;
    let (root, mut fg, _user) = fixture().await?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(std::env::current_dir()?.join("src/apps/business-os"), root.path().join("business-os"))?;
    let hash=URL_SAFE_NO_PAD.encode(Sha256::digest(b"synthetic-bootstrap-cookie"));
    let bos=fg.inputs.databases.get("bos").unwrap();
    bos.writer.submit(WriteJob::new(Default::default(),move|tx| {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS business_users(user_id TEXT PRIMARY KEY,display_name TEXT NOT NULL,role TEXT NOT NULL,active INTEGER NOT NULL,capability_epoch INTEGER NOT NULL,created_at_ms INTEGER NOT NULL,updated_at_ms INTEGER NOT NULL,profile_json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS business_sessions(token_hash TEXT PRIMARY KEY,user_id TEXT NOT NULL,role TEXT NOT NULL,display_name TEXT NOT NULL,created_at_ms INTEGER NOT NULL,expires_at_ms INTEGER NOT NULL,revoked INTEGER NOT NULL,last_seen_ms INTEGER NOT NULL);")?;
        tx.execute("INSERT OR REPLACE INTO business_users VALUES('alice','Alice','founder',1,0,0,0,'{}')",[])?;
        tx.execute("INSERT OR REPLACE INTO business_sessions VALUES(?1,'alice','founder','Alice',0,9223372036854775807,0,0)",[hash])?;
        Ok(())
    }))?.await?;
    async fn get(addr:std::net::SocketAddr,path:&str)->Result<(u16,usize,Option<Value>,f64)> {
        let url=format!("http://{addr}{path}"); let json=path.ends_with("launch-context");
        let started=std::time::Instant::now();
        let (status,bytes,body)=tokio::task::spawn_blocking(move||->Result<_>{
            let response=ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(5)).build()
                .get(&url).set("Cookie","ctox_business_os_auth=synthetic-bootstrap-cookie").call()?;
            let status=response.status(); let mut body=Vec::new();
            std::io::Read::read_to_end(&mut response.into_reader(),&mut body)?;
            let value=if json {serde_json::from_slice::<Value>(&body).ok()} else {None};
            Ok((status,body.len(),value))
        }).await??;
        Ok((status,bytes,body,started.elapsed().as_secs_f64()*1000.0))
    }
    async fn listen(root:&std::path::Path)->Result<std::net::SocketAddr> {
        let handle=ctox_store::ipc::request(crate::service::queue_store::endpoint(root),ctox_store::ipc::Request {
            request_id:91,kind:ctox_store::ipc::JobKind::Read,method:"http.listen".into(),
            payload:serde_json::to_vec(&json!({"addr":"127.0.0.1:0"}))?,
        },Default::default())?;
        let response:Value=serde_json::from_slice(&handle.await?)?;
        Ok(response["addr"].as_str().context("missing bound address")?.parse()?)
    }
    let mut rows=Vec::new();
    for cycle in 0..2 {
        let addr=listen(root.path()).await?;
        for route in ["/","/sw.js","/kernel/boot.mjs","/api/business-os/launch-context"] {
            for request in 0..20 {
                let (status,bytes,body,ms)=get(addr,route).await?;
                rows.push(json!({"cycle":cycle,"request":request,"route":route,"status":status,"bytes":bytes,"milliseconds":ms}));
                if let Some(path)=std::env::var_os("CTOX_BOOTSTRAP_PERF_PROOF") {
                    std::fs::write(path,serde_json::to_vec_pretty(&json!({"scope":"real foreground HTTP component; partial until terminal proof","cold_first_included":true,"requests":rows}))?)?;
                }
                assert_eq!(status,200,"V4 route failed");
                if let Some(body)=body {assert_eq!(body["v4"]["userId"],"alice");}
            }
            let max=rows.iter().filter(|r|r["route"]==route).map(|r|r["milliseconds"].as_f64().unwrap()).fold(0.0,f64::max);
            assert!(max<50.0,"V4 cold-including p99 >= 50 ms for {route}: {max}");
        }
        if cycle==0 {
            fg.inputs.sync.writer.submit(WriteJob::new(Default::default(),|tx| {
                let rights=sync_rights::read(tx)?;
                let rules=rights.rules.into_iter().filter(|r|!(r.role=="founder" && r.collection=="policy:apps.source.edit")).collect::<Vec<_>>();
                sync_rights::replace(tx,rights.rights_ver,&rules)?;Ok(())
            }))?.await?;
            // Deliberately no cache-settle call after revocation.
            let (_,_,body,_)=get(addr,"/api/business-os/launch-context").await?;
            let body=body.context("missing launch")?;
            let caps=body["v4"]["hello"]["offer"]["capabilities"].as_array().context("missing caps")?;
            assert!(!caps.iter().any(|v|v.as_str().is_some_and(|v|v.starts_with("agent.coding."))),"next HTTP request retained revoked rights");
            fg.shutdown().await?;
            fg=Foreground::start(root.path().into(),tokio::runtime::Handle::current()).await?;
        }
    }
    fg.shutdown().await?;
    let mut maxima=serde_json::Map::new();
    for route in ["/","/sw.js","/kernel/boot.mjs","/api/business-os/launch-context"] {
        let max=rows.iter().filter(|r|r["route"]==route).map(|r|r["milliseconds"].as_f64().unwrap()).fold(0.0,f64::max);
        maxima.insert(route.into(),json!(max));
    }
    if let Some(path)=std::env::var_os("CTOX_BOOTSTRAP_PERF_PROOF") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"scope":"real foreground/HTTP component with isolated synthetic rights; release profile","cold_first_included":true,"two_owner_starts":true,"rights_next_http_request":true,"requests":rows,"p99_ms":maxima}))?)?;
    }
    assert!(maxima.values().all(|v|v.as_f64().unwrap()<50.0),"V4 cold-including p99 >= 50 ms: {maxima}");
    Ok(())
}

// Producer/consumer proof: real owner policy + signer generate the native half;
// the unmodified #900 boot fixture negotiates and verifies these scopes/pages.
async fn export_browser_fixture(
    owners: &Arc<StoreInputs>,
    context: &V4LaunchContext,
    path: &std::path::Path,
) -> Result<()> {
    let actor = context.v4.actor.clone();
    let wanted = hello(context).scopes_wanted.clone();
    let (version, scopes) = owners
        .activation_snapshot(move |tx| {
            let rights = sync_rights::read(tx)?;
            let scopes = wanted
                .iter()
                .map(|c| {
                    rights
                        .resolve_collection(&actor.role, &actor.id, c)
                        .map_err(rusqlite::Error::InvalidParameterName)
                })
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((rights.rights_ver, scopes))
        })
        .await?
        .value;
    let signer = owners.instance_signer()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut tokens = Vec::new();
    let mut hashes = Vec::new();
    for scope in scopes {
        let hash = scope.hash().map_err(anyhow::Error::msg)?;
        let claims = sync_rights::scope::ScopeClaims {
            instance_id: context.v4.instance_id.clone(),
            device_id: hello(context).device_proof.device_id.clone(),
            rights_ver: version,
            scope_hash: hash.clone(),
            exp: now + 300,
        };
        let mut bytes = b"ctox:v4:scope\0".to_vec();
        bytes.extend(
            sync_rights::scope::canonical_json(&serde_json::to_value(&claims)?)
                .map_err(anyhow::Error::msg)?,
        );
        let token = sync_rights::scope::SignedScope {
            claims,
            signature: URL_SAFE_NO_PAD.encode(signer.sign(&bytes).map_err(anyhow::Error::msg)?),
        };
        tokens.push(token.to_wire().map_err(anyhow::Error::msg)?);
        hashes.push(hash);
    }
    std::fs::write(
        path.with_extension("session.json"),
        serde_json::to_vec(
            &json!({"scope_tokens":tokens,"scope_hashes":hashes,"rights_ver":version.to_string(),"core_time_ms":now*1000}),
        )?,
    )?;
    Ok(())
}
