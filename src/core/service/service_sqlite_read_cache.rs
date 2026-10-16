// Compatibility name retained for status callers. The process-local connection
// cache is gone: every read borrows the injected owner's bounded WAL pool.
fn with_cached_service_sqlite_read_only<T: Send + 'static>(
    path: &Path,
    purpose: &str,
    f: impl FnOnce(Option<&rusqlite::Connection>) -> Result<T> + Send + 'static,
) -> Result<T> {
    crate::persistence::require_sync_owner_context()?;
    if !path.exists() {
        return f(None);
    }
    crate::lcm::with_lcm_read(path, move |conn| f(Some(conn)))
        .with_context(|| format!("read injected SQLite snapshot for {purpose}"))
}

#[cfg(test)]
mod service_sqlite_read_cache_tests {
    use super::*;

    #[tokio::test]
    async fn status_reads_use_query_only_owner_snapshots_and_observe_commits() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("status.sqlite3");
        let (writer, readers) =
            ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
                .await?
                .into_parts();
        let adapter =
            crate::lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone())
                .await?;
        writer
            .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
                tx.execute_batch(
                    "CREATE TABLE probe(value INTEGER NOT NULL);INSERT INTO probe VALUES(1);",
                )?;
                Ok(())
            }))?
            .await?;
        let read_path = path.clone();
        let first = crate::persistence::run_core_sync(move || {
            with_cached_service_sqlite_read_only(&read_path, "status invariant", |conn| {
                let conn = conn.expect("owned file exists");
                assert_eq!(
                    conn.query_row("PRAGMA query_only", [], |r| r.get::<_, i64>(0))?,
                    1
                );
                assert!(
                    !conn.is_autocommit(),
                    "status must use the owner's read transaction"
                );
                Ok(conn.query_row("SELECT value FROM probe", [], |r| r.get::<_, i64>(0))?)
            })
        })
        .await?;
        assert_eq!(first, 1);
        writer
            .submit(ctox_store::WriteJob::new(Default::default(), |tx| {
                tx.execute("UPDATE probe SET value=11", [])?;
                Ok(())
            }))?
            .await?;
        let read_path = path.clone();
        let second = crate::persistence::run_core_sync(move || {
            with_cached_service_sqlite_read_only(&read_path, "status invariant", |conn| {
                Ok(conn
                    .unwrap()
                    .query_row("SELECT value FROM probe", [], |r| r.get::<_, i64>(0))?)
            })
        })
        .await?;
        assert_eq!(second, 11);
        drop(adapter);
        drop(readers);
        writer.shutdown().await?;
        Ok(())
    }
}
