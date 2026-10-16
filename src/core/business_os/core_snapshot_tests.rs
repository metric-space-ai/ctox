use super::*;

#[tokio::test]
async fn core_snapshot_registry_reads_use_readers_and_fail_closed_without_injection(
) -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = crate::paths::core_db(root.path());
    std::fs::create_dir_all(path.parent().unwrap())?;
    let (writer, readers) = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
        .await?
        .into_parts();
    let adapter =
        crate::lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone())
            .await?;
    writer.submit(ctox_store::WriteJob::new(Default::default(), |tx| {
        tx.execute_batch("CREATE TABLE scrape_account_state(target_id TEXT,reason TEXT,causal_run_id TEXT,last_probe_run_id TEXT,failed_probes INTEGER,next_probe_at_ms INTEGER,probe_lease_owner TEXT,probe_lease_until_ms INTEGER,updated_at_ms INTEGER);
            INSERT INTO scrape_account_state VALUES('target','first','run','probe',1,0,NULL,0,1);
            CREATE TEMP TABLE scrape_account_state AS SELECT * FROM main.scrape_account_state WHERE 0;")?;
        Ok(())
    }))?.await?;
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let state = outbound_registry_account_states(&work)?;
        assert_eq!(
            state
                .get("target")
                .and_then(|row| row.get("reason"))
                .and_then(Value::as_str),
            Some("first"),
            "Core read used writer-local state"
        );
        Ok(())
    })
    .await?;
    writer
        .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
            tx.execute("UPDATE main.scrape_account_state SET reason='second'", [])?;
            Ok(())
        }))?
        .await?;
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        assert_eq!(
            outbound_registry_account_states(&work)?["target"]["reason"],
            "second",
            "next snapshot missed the acknowledged write"
        );
        Ok(())
    })
    .await?;
    drop(adapter);
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        assert!(
            outbound_registry_account_states(&work).is_err(),
            "reader opened another live connection instead of requiring the injected owner"
        );
        Ok(())
    })
    .await?;
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}
