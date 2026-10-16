use super::*;
use ctox_store::{
    ipc::{self, JobKind, Request},
    JobOptions, WriteJob,
};

struct FixtureFactory;
struct FixtureEngine(ctox_store::writer::WriterHandle);
impl EngineFactory for FixtureFactory {
    fn name(&self) -> &'static str {
        "cutover-invariant"
    }
    fn start(&self, inputs: Arc<StoreInputs>) -> EngineFuture<Box<dyn RunningEngine>> {
        Box::pin(async move {
            inputs
                .core
                .writer
                .submit(WriteJob::new(Default::default(), |tx| {
                    tx.execute_batch(
                        "CREATE TABLE IF NOT EXISTS shutdown_probe(id INTEGER PRIMARY KEY)",
                    )?;
                    Ok(())
                }))?
                .await?;
            Ok(Box::new(FixtureEngine(inputs.core.writer.clone())) as Box<dyn RunningEngine>)
        })
    }
}
impl RunningEngine for FixtureEngine {
    fn shutdown(&mut self) -> EngineFuture<()> {
        let writer = self.0.clone();
        Box::pin(async move {
            writer
                .submit(WriteJob::new(Default::default(), |tx| {
                    tx.execute("INSERT OR REPLACE INTO shutdown_probe VALUES(1)", [])?;
                    Ok(())
                }))?
                .await
        })
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_owns_real_files_and_drains_engine_before_store() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "fixture refuses configured state root"
    );
    let fixture = tempfile::Builder::new().prefix("c4f-").tempdir_in("/tmp")?;
    let root = fixture.path();
    bind_engine(Arc::new(FixtureFactory))?;
    let mut foreground = Foreground::start(root.to_owned(), Handle::current()).await?;
    // The normal composition root, not the fixture, installs these weak routes.
    // Removing NativeOwners::attach must fail here before any fake executor runs.
    crate::persistence::store_text_value_owned(root, "foreground-owner-probe", Some("retained"))
        .await?;
    assert_eq!(
        crate::persistence::load_text_value_owned(root, "foreground-owner-probe")
            .await?
            .as_deref(),
        Some("retained")
    );
    let native_root = root.to_owned();
    crate::persistence::run_core_sync(move || {
        crate::runtime_env::set_runtime_env_value(&native_root, "CTOX_FIXTURE_THEME", "owned")?;
        assert_eq!(
            crate::runtime_env::get_runtime_env_value(&native_root, "CTOX_FIXTURE_THEME")
                .as_deref(),
            Some("owned")
        );
        let records = crate::secrets::list_secret_records(&native_root, None)?;
        let (scope, name) = crate::sync_host::SIGNING_IDENTITY_SECRET_KEY;
        assert_eq!(records.len(), 1);
        assert_eq!(
            (records[0].scope.as_str(), records[0].secret_name.as_str()),
            (scope, name)
        );
        Ok(())
    })
    .await?;
    for name in ["runtime", "secrets"] {
        let db = foreground
            .inputs
            .databases
            .get(name)
            .expect("native file missing");
        assert_eq!(db.readers.generation(), db.writer.generation().0);
        assert!(matches!(
            ctox_store::registry::Registry.open(db.writer.db_id().clone(), Default::default()),
            Err(StoreError::AlreadyOwned)
        ));
    }
    let core = foreground.inputs.core.clone();
    let sync = foreground.inputs.sync.clone();
    assert!(matches!(
        ctox_store::registry::Registry.open(sync.writer.db_id().clone(), Default::default()),
        Err(StoreError::AlreadyOwned)
    ));
    assert!(ipc::MaintenanceOwner::acquire(
        &queue_store::endpoint(root),
        sync.writer.db_id().clone()
    )
    .is_err());
    let inspect = ipc::request(
        queue_store::endpoint(root),
        Request {
            request_id: 1,
            kind: JobKind::Read,
            method: "store.inspect".into(),
            payload: serde_json::to_vec("sync")?,
        },
        JobOptions::default(),
    )?
    .await?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&inspect)?["generation"],
        sync.writer.generation().0
    );
    let mut commits = sync.writer.subscribe_commits();
    sync.writer.submit(WriteJob::new(Default::default(),|tx|{
        let _collection=ctox_store::schema::create_collection(tx,&ctox_store::schema::CollectionDefinition{
            name:"foreground_probe".into(),app_id:"probe".into(),schema_ver:0,
            schema_json:serde_json::json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),
            class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"v1".into()
        })?;
        tx.execute("INSERT INTO c_foreground_probe(id,rev,schema_ver,data,deleted) VALUES('one','1-one',0,'{\"id\":\"one\"}',0)",[])?;
        Ok(())
    }))?.await?;
    assert!(
        commits.latest().unwrap().head_seq > 0,
        "foreground did not install the live change log"
    );

    // The foreground owner, not a separately assembled fixture, must install
    // the real queue dispatcher and its Core -> Sync publication driver.
    sync.writer.submit(WriteJob::new(Default::default(),|tx| {
        ctox_store::schema::create_collection(tx,&ctox_store::schema::CollectionDefinition{
            name:"ctox_queue_tasks".into(),app_id:"ctox-platform".into(),schema_ver:0,
            schema_json:serde_json::json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"}}}),
            class:"projection".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"v1".into()
        })?;Ok(())
    }))?.await?;
    core.writer.submit(WriteJob::new(Default::default(),|tx| {
        crate::communication_store::upsert_communication_message_tx(tx,crate::communication_store::UpsertMessage {
            message_key:"foreground-ack",channel:"queue",account_key:"synthetic",thread_key:"synthetic",remote_id:"foreground-ack",direction:"inbound",folder_hint:"queue",
            sender_display:"fixture",sender_address:"fixture",recipient_addresses_json:"[]",cc_addresses_json:"[]",bcc_addresses_json:"[]",subject:"synthetic foreground ACK",preview:"fixture",body_text:"synthetic",body_html:"",raw_payload_ref:"",trust_level:"high",status:"received",seen:false,has_attachments:false,
            external_created_at:"2026-10-15T00:00:00Z",observed_at:"2026-10-15T00:00:00Z",metadata_json:r#"{"source":"ctox-queue","priority":"normal"}"#
        }).map_err(|_|StoreError::Invalid("foreground fixture seed".into()))?;
        tx.execute("INSERT INTO communication_routing_state(message_key,route_status,lease_owner,updated_at) VALUES('foreground-ack','leased','fixture','2026-10-15T00:00:00Z')",[])?;
        Ok(())
    }))?.await?;
    let before = commits.latest().unwrap().head_seq;
    let response=ipc::request(queue_store::endpoint(root),Request {
        request_id:2,kind:JobKind::Write,method:"queue.ack".into(),payload:serde_json::to_vec(&serde_json::json!({
            "operation_id":"foreground-ack-1","keys":["foreground-ack"],"status":"failed","failure":"synthetic bounded failure","reason":null,"attempt":null
        }))?
    },Default::default())?.await?;
    assert_eq!(serde_json::from_slice::<usize>(&response)?, 1);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while commits.latest().is_none_or(|head| head.head_seq <= before) {
        tokio::time::timeout_at(deadline, commits.changed()).await??;
    }
    assert!(sync.readers.read(|tx|tx.query_row("SELECT EXISTS(SELECT 1 FROM c_ctox_queue_tasks WHERE id='foreground-ack' AND deleted=0)",[],|r|r.get::<_,bool>(0))).await?);
    let first_signer = foreground.inputs.instance_signer()?;
    let first_public = first_signer.key_id().to_owned();
    assert!(
        foreground.inputs.native_session().is_err(),
        "unenrolled daemon minted authority"
    );
    foreground.shutdown().await?;
    assert!(first_signer.sign(b"after shutdown").is_err());
    assert!(super::super::store_inputs::store_inputs(root).is_err());
    assert!(matches!(
        core.writer
            .submit(WriteJob::new(Default::default(), |_| Ok(()))),
        Err(StoreError::Closed)
    ));
    // Reopening through the real registry after complete drain proves the
    // engine's final effect committed before owner admission was closed.
    let reopened =
        ctox_store::registry::Registry.open(core.writer.db_id().clone(), Default::default())?;
    reopened.ready().await?;
    let count = reopened
        .submit(WriteJob::new(Default::default(), |tx| {
            Ok(
                tx.query_row("SELECT COUNT(*) FROM shutdown_probe", [], |r| {
                    r.get::<_, u64>(0)
                })?,
            )
        }))?
        .await?;
    assert_eq!(count, 1);
    reopened.shutdown().await?;
    // The real foreground loads its same encrypted identity on every start.
    let mut second = Foreground::start(root.to_owned(), Handle::current()).await?;
    let second_signer = second.inputs.instance_signer()?;
    assert_eq!(
        second_signer.key_id(),
        first_public,
        "instance identity changed after restart"
    );
    assert_eq!(second_signer.instance_id(), first_signer.instance_id());
    let message = b"canonical instance restart proof";
    let signature = second_signer.sign(message).map_err(anyhow::Error::msg)?;
    let hex = second_signer.key_id().strip_prefix("ed25519:").unwrap();
    let public: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|part| u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
        .collect();
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(message, &signature)
        .map_err(|_| anyhow::anyhow!("persisted instance signature invalid"))?;
    second.shutdown().await?;
    if let Some(destination) = std::env::var_os("CTOX_CUTOVER_FIXTURE_EXPORT") {
        let root = root.to_owned();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let destination = std::path::PathBuf::from(destination).join("runtime");
            std::fs::create_dir_all(&destination)?;
            for source in [
                crate::paths::core_db(&root),
                crate::paths::sync_db(&root),
                crate::paths::runtime_config_db(&root),
                crate::paths::secrets_db(&root),
            ] {
                let _offline = ipc::MaintenanceOwner::acquire(
                    &queue_store::endpoint(&root),
                    DbId::new(&source)?,
                )?;
                let wal = std::path::PathBuf::from(format!("{}-wal", source.display()));
                anyhow::ensure!(
                    !wal.exists() || wal.metadata()?.len() == 0,
                    "offline fixture retains nonempty WAL"
                );
                std::fs::copy(&source, destination.join(source.file_name().unwrap()))?;
            }
            // Only this isolated synthetic fixture's protected key accompanies
            // its encrypted fixture DBs. Never export a configured real root.
            let key = crate::paths::runtime_dir(&root).join(crate::secrets::SECRET_MASTER_KEY_FILE);
            std::fs::copy(
                key,
                destination.join(crate::secrets::SECRET_MASTER_KEY_FILE),
            )?;
            Ok(())
        })
        .await??;
    }

    // Normal boot must not replay obsolete Core configuration into the canonical
    // Runtime/Secrets owners. Migration is an explicit, separately owned step.
    let legacy_scratch = tempfile::Builder::new().prefix("c4l-").tempdir_in("/tmp")?;
    let injected = std::env::var_os("CTOX_CUTOVER_LEGACY_PROFILE");
    let legacy_profile = if let Some(path) = &injected {
        let path = std::fs::canonicalize(path)?;
        let export = std::env::var_os("CTOX_CUTOVER_FIXTURE_EXPORT")
            .ok_or_else(|| anyhow::anyhow!("injected fixture requires export scope"))?;
        let scope = std::path::PathBuf::from(export)
            .parent()
            .unwrap()
            .canonicalize()?;
        anyhow::ensure!(
            path.starts_with(&scope) && path != scope,
            "injected profile is outside this run"
        );
        path
    } else {
        legacy_scratch.path().to_owned()
    };
    if injected.is_none() {
        std::fs::create_dir_all(crate::paths::runtime_dir(&legacy_profile))?;
        let legacy_writer = ctox_store::registry::Registry.open(
            DbId::new(crate::paths::core_db(&legacy_profile))?,
            Default::default(),
        )?;
        legacy_writer.ready().await?;
        legacy_writer.submit(WriteJob::new(Default::default(),|tx| {
            tx.execute_batch("CREATE TABLE runtime_env_kv(env_key TEXT PRIMARY KEY,env_value TEXT NOT NULL);INSERT INTO runtime_env_kv VALUES('CTOX_FIXTURE_THEME','obsolete-core-value');")?;
            Ok(())
        }))?.await?;
        legacy_writer.shutdown().await?;
        if let Some(export) = std::env::var_os("CTOX_CUTOVER_FIXTURE_EXPORT") {
            let source = legacy_profile.clone();
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let target = std::path::PathBuf::from(export)
                    .parent()
                    .unwrap()
                    .join("legacy-source/runtime");
                std::fs::create_dir_all(&target)?;
                let file = crate::paths::core_db(&source);
                let _offline = ipc::MaintenanceOwner::acquire(
                    &queue_store::endpoint(&source),
                    DbId::new(&file)?,
                )?;
                let wal = std::path::PathBuf::from(format!("{}-wal", file.display()));
                anyhow::ensure!(
                    !wal.exists() || wal.metadata()?.len() == 0,
                    "legacy fixture retains WAL"
                );
                std::fs::copy(file, target.join("ctox.sqlite3"))?;
                Ok(())
            })
            .await??;
        }
    }
    let mut restarted = Foreground::start(legacy_profile.clone(), Handle::current()).await?;
    let read_root = legacy_profile.clone();
    let value = crate::persistence::run_core_sync(move || {
        Ok(crate::runtime_env::get_runtime_env_value(
            &read_root,
            "CTOX_FIXTURE_THEME",
        ))
    })
    .await?;
    assert_eq!(value, None, "normal boot replayed legacy configuration");
    restarted.shutdown().await?;

    Ok(())
}
