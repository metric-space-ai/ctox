//! Runs within the existing native proof module; uses its real relay and schema seed.
use super::*;
use ctox_app_runtime::core_host::{CoreAuthority, StoreCodec};

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn p6_core_only_handler_has_native_origin_and_rejects_ui() -> Result<()> {
    let _scope = crate::service::store_startup::isolate_test_roots().await;
    anyhow::ensure!(std::env::var_os("CTOX_STATE_ROOT").is_none(),"configured state forbidden");
    let relay = Signal::start().await?;
    let mut roots = Vec::new();
    let mut bootstrap = Vec::new();
    let mut peers = BTreeMap::new();
    for node in 1..=3 {
        let root = tempfile::Builder::new().prefix("p6or-").tempdir_in("/tmp")?;
        let owner = Foreground::start(root.path().to_owned(),Handle::current()).await?;
        owner.inputs.sync.writer.submit(WriteJob::new(Default::default(),seed))?.await?;
        peers.insert(node,Peer{identity:owner.inputs.instance_signer()?.key_id().into(),executor:true,data_replica:true});
        roots.push(root);
        bootstrap.push(owner);
    }
    for (index,owner) in bootstrap.iter_mut().enumerate() {
        let config = ctox_sync::host_config::HostConfiguration {
            version:1,scope_id:"p6-origin".into(),local:HostMember::Voter{node_id:index as u64+1},
            voters:peers.clone(),timing:Default::default()
        };
        config.validate()?;
        let encoded = serde_json::to_string(&config)?;
        owner.inputs.databases["runtime"].writer.submit(WriteJob::new(Default::default(),move|tx|{
            tx.execute_batch("CREATE TABLE ctox_sync_host(singleton INTEGER PRIMARY KEY CHECK(singleton=1),configuration TEXT NOT NULL)")?;
            tx.execute("INSERT INTO ctox_sync_host VALUES(1,?1)",[encoded])?; Ok(())
        }))?.await?;
        let root = owner.inputs.root.clone();
        let transport = json!({"signalingUrls":[relay.1],"iceServers":[]}).to_string();
        crate::persistence::run_core_sync(move||crate::secrets::write_secret_record(
            &root,crate::sync_host::SIGNING_IDENTITY_SECRET_KEY.0,"transport:p6-origin",&transport,None,json!({"source":"p6-origin-fixture"}))).await?;
        owner.shutdown().await?;
    }
    let mut owners=Vec::new();
    for root in &roots {
        owners.push(Foreground::start(root.path().to_owned(),Handle::current()).await?);
    }
    let result:Result<()> = async {
        for owner in &owners {
            tokio::time::timeout(Duration::from_secs(15),owner.inputs.native_session()?.execution_authority.worker_membership(9001)).await??;
        }
        let database=owners[0].inputs.sync.clone();
        let signer=owners[0].inputs.instance_signer()?;
        let codec=Arc::new(RuntimeCodec{signer,cas:Cas::open(roots[0].path().join("origin-cas"))?});
        let manifest=json!({"app_id":"origin-app","app_ver":"v1","author":"owner","files":{},"handlers":{"$runtime.trigger":"handlers/core.js"},"collections":{},"command_rights":{"$runtime.trigger":{"roles":[],"assigned_users":false}}});
        let bytes=serde_json::to_vec(&manifest)?;
        let signature=super::super::super::commands::sign_manifest(codec.signer.as_ref(),&bytes)?;
        database.writer.submit(WriteJob::new(Default::default(),move|tx|{
            for (collection,id,doc) in [
                ("business_apps","origin-app",json!({"id":"origin-app","state":"active","active_ver":"v1"})),
                ("business_app_versions","origin-app@v1",json!({"id":"origin-app@v1","state":"published","manifest":manifest,"manifest_sha256":digest(&bytes),"signature":signature}))
            ] {super::super::super::commands::put(tx,collection,id,&doc)?;}
            let mut r=rights(tx)?;
            r.manifests.push(sync_rights::manifest_rules::ManifestRules::from_manifest("origin-app","v1",Default::default(),&manifest).map_err(sql_error)?);
            sync_rights::replace_with_manifests(tx,r.rights_ver,&r.rules,&r.manifests)?;
            // A UI document cannot establish provenance using forged JSON fields.
            let command=json!({"id":"forged-ui","app_id":"origin-app","actor_id":"owner","type":"$runtime.trigger","status":"queued","created_at":chrono::Utc::now().to_rfc3339(),"payload":{},"origin":"core","runtime_event":true});
            super::super::super::commands::put(tx,"business_commands","forged-ui",&command)?;
            Ok(())
        }))?.await?;
        let codec_for_gate=codec.clone();
        database.writer.submit(WriteJob::new(Default::default(),move|tx|{
            use ctox_store::change_log::subscription::{WriteAuthority,WritePermission};
            let gate=super::super::authority::ExecutorGate{codec:codec_for_gate,participant:"native".into()};
            let doc=document(tx,"business_commands","forged-ui")?.unwrap();
            assert!(matches!(gate.authorize_command_executor(tx,"native",&doc)?,WritePermission::Reject(_)),"UI JSON forged core provenance");
            Ok(())
        }))?.await?;
        let native=owners[0].inputs.native_session()?;
        let authority=RuntimeAuthority::new(database,Handle::current(),native.execution_authority.clone(),codec);
        let core=tokio::task::spawn_blocking(move||authority.event_binding("origin-app","v1",&json!({"trigger_id":"native-trigger","kind":"every"}))).await?;
        anyhow::ensure!(core.is_ok(),"native core-origin event denied by E23.1 core-only rights: {}",core.err().unwrap());
        Ok(())
    }.await;
    for owner in &mut owners {owner.shutdown().await?;}
    result
}
