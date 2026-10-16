//! Provision an actual checked-in app through the native publisher and CAS.
//! This is fixture setup, never a substitute for browser lifecycle acceptance.
use super::*;

pub(super) async fn publish(
    inputs:Arc<StoreInputs>, source:std::path::PathBuf, app:String, actor:BusinessOsActor,
) -> Result<Value> {
    anyhow::ensure!(!app.is_empty() && app.len()<96 && app.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'),"source app identifier");
    let app_for_io=app.clone();let root=inputs.root.clone();
    let (manifest,files)=tokio::task::spawn_blocking(move||->Result<_>{
        let app_root=std::fs::canonicalize(source.join("src/apps/business-os"))?;
        let manifest:Value=serde_json::from_slice(&std::fs::read(app_root.join("v4/manifests").join(format!("{app_for_io}.manifest.json")))?)?;
        anyhow::ensure!(manifest["app_id"]==app_for_io,"source app identity mismatch");
        let source_dir=std::fs::canonicalize(source.join(text(&manifest,"_source_dir")?))?;
        anyhow::ensure!(source_dir.starts_with(&app_root),"source app outside checked-in Business OS");
        let cas=Cas::open(crate::paths::runtime_dir(&root).join("transfers"))?;
        let mut files=Vec::new();let mut total=0;
        for (path,expected) in manifest["files"].as_object().context("source file manifest")? {
            anyhow::ensure!(files.len()<2000,"source file count budget");
            let full=std::fs::canonicalize(source_dir.join(path))?;
            anyhow::ensure!(full.starts_with(&source_dir),"source file outside app");
            let size=std::fs::metadata(&full)?.len();total+=size;
            anyhow::ensure!(size<=16*1024*1024 && total<=64*1024*1024,"source artifact byte budget");
            let bytes=std::fs::read(full)?;let hash=digest(&bytes);
            anyhow::ensure!(expected.as_str()==Some(&hash),"not_run(source-artifact-hash): {app_for_io}/{path}");
            let mut receiver=cas.receive(&digest(format!("{app_for_io}:{path}").as_bytes()),&hash,size)?;
            for (index,chunk) in bytes.chunks(ctox_store::cas::CHUNK_BYTES).enumerate(){
                receiver.append((index*ctox_store::cas::CHUNK_BYTES) as u64,chunk)?;
            }
            receiver.finish()?;
            files.push(json!({"id":hash,"ref":hash,"size":size,"name":path,"owner_app":app_for_io}));
        }
        let mut manifest=manifest;
        let schema_path=app_root.join("v4/schemas").join(format!("{app_for_io}.collections.schema.json"));
        let schemas:Value=if schema_path.is_file(){serde_json::from_slice(&std::fs::read(schema_path)?)?}else{json!({"collections":{}})};
        for (name,metadata) in manifest["collections"].as_object_mut().context("source collections")? {
            if !metadata["schema"].is_object() {
                let entry=&schemas["collections"][name];
                let schema=entry.get("schema").unwrap_or(entry);
                anyhow::ensure!(schema.is_object(),"source collection schema absent: {name}");
                metadata["schema"]=schema.clone();
            }
            // Explicit isolated-fixture budgets, not invented production grants.
            if metadata["byte_budget"].is_null(){metadata["byte_budget"]=json!(32*1024*1024);}
            if metadata["client_limit"].is_null(){metadata["client_limit"]=json!(100000);}
        }
        Ok((manifest,files))
    }).await??;
    let id=app.clone();
    let active=inputs.sync.readers.read_job(ReadJob::new(Default::default(),move|db|document(db,"business_apps",&id))).await?;
    anyhow::ensure!(active.is_none(),"source release only installs a fresh fixture app");
    for batch in files.chunks(100) {
        let batch=batch.to_vec();
        inputs.sync.writer.submit(WriteJob::new(Default::default(),move|tx|{
            for file in batch {crate::business_os::app_platform::commands::put(tx,"files",text(&file,"id").map_err(sql_error)?,&file)?;}Ok(())
        }))?.await?;
    }
    let draft_id=format!("p6-source-{app}");let draft_key=draft_id.clone();let app_id=app.clone();
    let fields=manifest.clone();let author=actor.id.clone();
    inputs.sync.writer.submit(WriteJob::new(Default::default(),move|tx|{
        let draft=json!({"id":draft_key,"app_id":app_id,"author":author,"base_app_ver":null,
            "files":fields["files"],"collections_schema":fields["collections"],"manifest_fields":fields});
        crate::business_os::app_platform::commands::put(tx,"app_drafts",&draft_key,&draft)
    }))?.await?;
    let platform=crate::business_os::app_platform::Platform::from_database(&inputs.sync,inputs.instance_signer()?)?;
    let command=crate::business_os::app_platform::CommandDocument {
        id:format!("p6-publish-{app}"),app_id:"ctox-platform".into(),module:"ctox-platform".into(),
        command_type:"ctox.app.publish".into(),payload:json!({"app_id":app,"base_app_ver":null,"draft_id":draft_id}),
    };
    let receipt=platform.dispatch(actor,command,now()).await;
    anyhow::ensure!(matches!(receipt["status"].as_str(),Some("accepted"|"migrated")),"not_run(native-source-publication): native publisher did not accept the checked-in app");
    Ok(json!({"appId":app,"source":"native-publisher","receipt":receipt,"files":files.len(),"manifest":manifest}))
}
