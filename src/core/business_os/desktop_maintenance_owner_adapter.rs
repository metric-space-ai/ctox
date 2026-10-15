//! Desktop maintenance uses the shared Sync owner and its authoritative log.
use super::owner_adapter::{install_legacy, CatalogOwner};
use ctox_store::{JobHandle, StoreError};
use rusqlite::Transaction;
use std::path::Path;

pub fn install(tx: &Transaction<'_>, app_version: &str) -> Result<(), StoreError> {
    for (collection, table) in [
        ("desktop_files", "ctox_business_os__desktop_files__v0"),
        (
            "desktop_file_chunks",
            "ctox_business_os__desktop_file_chunks__v0",
        ),
    ] {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |r| r.get(0),
        )?;
        if exists {
            install_legacy(tx, collection, table, app_version)?;
        }
    }
    Ok(())
}

pub fn submit<T: Send + 'static>(
    path: &Path,
    write: impl FnOnce(&Transaction<'_>) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<JobHandle<T>> {
    let owner = CatalogOwner::for_path(path)?;
    let app_version = owner.base_app_version().to_owned();
    Ok(owner.submit(move |tx| {
        install(tx, &app_version)?;
        write(tx).map_err(|error| {
            error
                .downcast_ref::<StoreError>()
                .cloned()
                .unwrap_or_else(|| StoreError::Invalid(error.to_string()))
        })
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_store::change_log;
    use ctox_store::{registry::Registry, writer::WriterOptions, DbId, JobOptions, WriteJob};
    #[tokio::test]
    async fn physical_file_delete_is_a_client_delete_with_the_global_sequence() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sync.sqlite3");
        let writer = Registry.open(
            DbId::new(&path)?,
            WriterOptions {
                head_seq: Some(std::sync::Arc::new(|conn| {
                    Ok(conn.query_row("SELECT v FROM _seq WHERE singleton=1", [], |r| r.get(0))?)
                })),
                ..WriterOptions::default()
            },
        )?;
        writer.ready().await?;
        let _owner = CatalogOwner::register(writer.clone(), "fixture-base-release".into())?;
        let before = writer.submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute_batch("CREATE TABLE ctox_business_os__desktop_files__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,data TEXT);")?;
            install(tx, "fixture-base-release")?;
            tx.execute("INSERT INTO ctox_business_os__desktop_files__v0 VALUES('file','1-r',0,'{\"id\":\"file\"}')", [])?;
            Ok(change_log::head(tx)?.head_seq)
        }))?.await?;
        let readers = ctox_store::read_pool::ReadPool::for_database(
            writer.db_id(),
            writer.generation(),
            ctox_store::config::ConnectionConfig::default(),
            ctox_store::config::AdmissionBudget::default(),
        )
        .await?;
        let mut notifications = writer.subscribe_commits();
        notifications.latest();
        submit(&path, |tx| {
            tx.execute(
                "DELETE FROM ctox_business_os__desktop_files__v0 WHERE id='file'",
                [],
            )?;
            Ok(())
        })?
        .await?;
        let notification =
            tokio::time::timeout(std::time::Duration::from_secs(2), notifications.changed())
                .await??;
        let page = readers
            .read(move |tx| change_log::read_page(tx, "desktop_files", 1, before, 100))
            .await?;
        assert_eq!(page.docs.len(), 1);
        assert_eq!(page.docs[0].id, "file");
        assert_eq!(page.docs[0].op, change_log::Operation::Delete);
        assert!(page.docs[0].seq > before);
        assert!(page.docs[0].body.is_none());
        // Encode the actual page used by the client, not an invented event.
        let wire = serde_json::to_value(&page)?;
        assert_eq!(wire["docs"][0]["op"], "delete");
        assert!(wire["docs"][0]["seq"].is_string());
        assert_eq!(notification.head_seq, page.head_seq);
        readers.shutdown().await?;
        drop(_owner);
        writer.shutdown().await?;
        Ok(())
    }
}
