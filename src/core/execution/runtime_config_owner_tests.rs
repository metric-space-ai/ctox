use super::*;

#[tokio::test]
async fn runtime_owner_proxy_cas_and_signature_cache_share_one_runtime_writer() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let path = crate::inference::runtime_env::runtime_config_path(root.path());
    std::fs::create_dir_all(path.parent().unwrap())?;
    let (writer, readers) = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
        .await?
        .into_parts();
    let adapter =
        crate::lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone())
            .await?;
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        assert_eq!(save_instance_proxy_config_row(&work, 0, "fixture".into(), "{}".into())?, 1);
        let stale = save_instance_proxy_config_row(&work, 0, "stale".into(), "{}".into()).unwrap_err();
        assert!(stale.downcast_ref::<ProxyConfigRevisionConflict>().is_some(), "proxy CAS lost its typed conflict");
        let state = with_instance_proxy_read(&work, |conn| {
            assert!(!conn.is_autocommit(), "runtime read escaped its snapshot");
            assert_eq!(conn.query_row("PRAGMA query_only", [], |r| r.get::<_,i64>(0))?, 1);
            Ok(conn.query_row(&format!("SELECT revision,default_provider FROM {INSTANCE_PROXY_CONFIG_TABLE}"), [], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,String>(1)?)))?)
        })?.unwrap();
        assert_eq!(state, (1, "fixture".into()), "stale proxy topology overwrote the acknowledged value");
        with_instance_proxy_write(&work, |conn| {conn.execute_batch("CREATE TEMP TABLE runtime_config_audit(n INTEGER); CREATE TEMP TRIGGER audit_proxy_revision AFTER UPDATE ON main.cliproxyapi_runtime_config BEGIN INSERT INTO runtime_config_audit VALUES(1); END;")?; Ok(())})?;
        assert_eq!(save_instance_proxy_config_row(&work, 1, "next".into(), "{}".into())?, 2);
        let writes = with_instance_proxy_write(&work, |conn| Ok(conn.query_row("SELECT COUNT(*) FROM temp.runtime_config_audit", [], |r| r.get::<_,i64>(0))?))?;
        assert_eq!(writes, 1, "proxy revision opened a second writer");
        let cache = CtoxSignatureKvStore::new(work.clone());
        assert!(cache.set("fixture-key", b"opaque-test-value", Duration::from_secs(60))?);
        assert_eq!(cache.get("fixture-key")?, Some(b"opaque-test-value".to_vec()));
        assert!(cache.expire("fixture-key", Duration::from_secs(120))?);
        assert!(cache.delete("fixture-key")?);
        assert_eq!(cache.get("fixture-key")?, None);
        assert!(!crate::paths::core_db(&work).exists(), "runtime callers created a Core file");
        Ok(())
    }).await?;
    drop(adapter);
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let cache = CtoxSignatureKvStore::new(work);
        assert_eq!(
            cache.get("fixture-key"),
            Err(SignatureCacheStoreError::Unavailable),
            "missing Runtime injection used a native fallback"
        );
        assert_eq!(
            cache.set("fixture-key", b"opaque-test-value", Duration::from_secs(60)),
            Err(SignatureCacheStoreError::Unavailable)
        );
        assert_eq!(
            cache.delete("fixture-key"),
            Err(SignatureCacheStoreError::Unavailable)
        );
        assert_eq!(
            cache.expire("fixture-key", Duration::from_secs(60)),
            Err(SignatureCacheStoreError::Unavailable)
        );
        Ok(())
    })
    .await?;
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}
