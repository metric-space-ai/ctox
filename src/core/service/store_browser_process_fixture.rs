use super::*;
/// Produces synthetic state for a separate normal foreground process. This
/// helper is not a browser-acceptance assertion and exports no credentials.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn export_installed_browser_fixture() -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let _scope = isolate_test_roots().await;
    anyhow::ensure!(std::env::var_os("CTOX_STATE_ROOT").is_none());
    let output = std::path::PathBuf::from(
        std::env::var_os("CTOX_C10_BROWSER_OUTPUT")
            .ok_or_else(|| anyhow::anyhow!("isolated browser output required"))?,
    );
    let signal = std::env::var("CTOX_C10_BROWSER_SIGNAL")?;
    anyhow::ensure!(signal.starts_with("ws://127.0.0.1:"));
    let (root, mut owner, _, _) = write_fixture().await?;
    let signer = owner.inputs.instance_signer()?;
    owner.inputs.sync.writer.submit(ctox_store::WriteJob::new(Default::default(),move |tx| {
        let manifest = serde_json::json!({"app_id":"notes-app","app_ver":"1","collections":{"notes":{"type":"object","primaryKey":"id","properties":{"id":{"type":"string"},"title":{"type":"string"},"owner":{"type":"string"}},"required":["id"]}},"generations":{"notes":1},"files":[],"capabilities_required":[]});
        let bytes=serde_json::to_vec(&manifest).unwrap();
        let signature=signer.sign(&bytes).unwrap().iter().map(|v|format!("{v:02x}")).collect::<String>();
        let version=serde_json::json!({"id":"notes-app@1","app_id":"notes-app","app_ver":"1","state":"published","manifest":manifest,"manifest_sha256":format!("{:x}",Sha256::digest(bytes)),"signature":signature});
        tx.execute("UPDATE c_business_app_versions SET data=?1 WHERE id='notes-app@1'",[version.to_string()])?;
        tx.execute("UPDATE c_business_apps SET data=json_set(data,'$.state','archived') WHERE id IN ('shell','foreign-app','draft-app')",[])?;
        tx.execute("INSERT INTO c_notes(id,rev,schema_ver,data,deleted) VALUES('native-seed','1',1,?1,0)",[serde_json::json!({"id":"native-seed","owner":"local-dev","title":"native persisted"}).to_string()])?;
        Ok(())
    }))?.await?;
    let config_root = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_ICE_SERVERS",
            r#"[{"urls":"stun:127.0.0.1:9"}]"#,
        )?;
        crate::inference::runtime_env::set_runtime_env_value(
            &config_root,
            "CTOX_BUSINESS_OS_WEB_AUTOSTART",
            "0",
        )
    })
    .await?;
    owner.shutdown().await?;
    let path = root.keep();
    // Explicit offline ownership before transferring this isolated profile.
    let _offline = ctox_store::ipc::MaintenanceOwner::acquire(
        &crate::service::queue_store::endpoint(&path),
        ctox_store::DbId::new(crate::paths::sync_db(&path))?,
    )?;
    std::fs::write(
        path.join("runtime/business-os-signaling-urls.json"),
        serde_json::to_vec(&vec![signal])?,
    )?;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apps/business-os");
    std::os::unix::fs::symlink(source, path.join("business-os"))?;
    std::fs::create_dir_all(path.join("src/core"))?;
    std::fs::write(path.join("src/core/main.rs"), "// isolated M3 fixture")?;
    std::fs::write(
        path.join("Cargo.toml"),
        "[package]\nname='ctox'\nversion='0.0.0'\nedition='2021'\n",
    )?;
    std::fs::create_dir_all(path.join("contracts/history"))?;
    std::fs::write(
        path.join("contracts/history/creation-ledger.md"),
        "synthetic",
    )?;
    std::fs::write(
        output,
        serde_json::to_vec(
            &serde_json::json!({"root":path,"synthetic":true,"daemon_started":false}),
        )?,
    )?;
    Ok(())
}
