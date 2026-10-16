use super::*;
use ctox_store::{testkit::StoreFixture, DbId, WriteJob};

pub(super) async fn fixture(
    root: &std::path::Path,
) -> Result<(
    ctox_store::writer::WriterHandle,
    ctox_store::read_pool::ReadPool,
    crate::lcm::owner_adapter::OwnerAdapter,
)> {
    let path = crate::paths::core_db(root);
    std::fs::create_dir_all(path.parent().unwrap())?;
    drop(crate::lcm::LcmEngine::open(
        &path,
        crate::lcm::LcmConfig::default(),
    )?);
    let (writer, readers) = StoreFixture::open(DbId::new(&path)?).await?.into_parts();
    let adapter =
        crate::lcm::owner_adapter::OwnerAdapter::attach(&path, writer.clone(), readers.clone())
            .await?;
    let work = path.clone();
    crate::persistence::run_core_sync(move || crate::lcm::with_lcm_write(&work, |conn| {
        conn.execute_batch("CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT,lease_owner TEXT,updated_at TEXT);")?;
        ensure_schema(conn)?;
        crate::service::harness_flow::ensure_event_schema(conn)?;
        Ok(())
    })).await?;
    Ok((writer, readers, adapter))
}

#[tokio::test]
async fn crew_owner_routing_releases_snapshot_and_revalidates_the_held_lease() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (writer, readers, adapter) = fixture(root.path()).await?;
    writer.submit(WriteJob::new(Default::default(), |tx| {
        tx.execute_batch("INSERT INTO communication_routing_state(message_key,route_status,lease_owner) VALUES('task','leased','worker');")?;
        Ok(())
    }))?.await?;
    struct ChangeLease {
        writer: ctox_store::writer::WriterHandle,
        runtime: tokio::runtime::Handle,
        completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }
    impl RouterJudge for ChangeLease {
        fn judge(&self, _: &str) -> Result<String> {
            let job = self.writer.submit(WriteJob::new(Default::default(), |tx| {
                tx.execute("UPDATE communication_routing_state SET lease_owner='new-worker' WHERE message_key='task'", [])?;
                Ok(())
            }))?;
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            self.runtime.spawn(async move {
                let _ = send
                    .send(tokio::time::timeout(std::time::Duration::from_millis(500), job).await);
            });
            receive.recv_timeout(std::time::Duration::from_secs(2))???;
            self.completed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(r#"{"member_id":"crew-milo","reason":"Erfahrung passt zur Aufgabe"}"#.into())
        }
    }
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = ChangeLease {
        writer: writer.clone(),
        runtime: tokio::runtime::Handle::current(),
        completed: completed.clone(),
    };
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let error = prepare_attempt(
            &work,
            &["task".into()],
            "worker",
            "attempt",
            None,
            &serde_json::json!({}),
            None,
            "Prüfe den Code",
            Some(&probe),
        )
        .unwrap_err();
        assert!(error.to_string().contains("lost its lease"), "{error:#}");
        Ok(())
    })
    .await?;
    assert!(
        completed.load(std::sync::atomic::Ordering::SeqCst),
        "router retained the owner executor"
    );
    let count = readers
        .read_job(ctox_store::ReadJob::new(Default::default(), |conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM crew_attempts", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        }))
        .await?;
    assert_eq!(count, 0, "a stale lease attached a Crew attempt");
    drop(adapter);
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn crew_owner_learning_document_and_marker_roll_back_together() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (writer, readers, adapter) = fixture(root.path()).await?;
    writer.submit(WriteJob::new(Default::default(), |tx| {
        tx.execute_batch("INSERT INTO crew_member_learnings(id,member_id,text,normalized_text,kind,scope_json,evidence_run_id,created_at) VALUES('learning','crew-milo','Schema prüfen','schema prüfen','pitfall','{}','run','2026');
          CREATE TRIGGER fail_import_marker BEFORE UPDATE OF migrated_to_lcm ON crew_member_learnings BEGIN SELECT RAISE(ABORT,'marker rejected'); END;")?;
        Ok(())
    }))?.await?;
    let work = root.path().to_owned();
    crate::persistence::run_core_sync(move || {
        let error = migrate_learnings_into_memory(&work, "crew-milo").unwrap_err();
        assert!(error.to_string().contains("marker rejected"), "{error:#}");
        let conversation = member_conversation_id("crew-milo");
        let documents = crate::lcm::with_lcm_read(&crate::paths::core_db(&work), move |conn| Ok(conn.query_row("SELECT COUNT(*) FROM continuity_documents WHERE conversation_id=?1", [conversation], |r| r.get::<_,i64>(0))?))?;
        assert_eq!(documents, 0, "a failed marker left an imported document committed");
        crate::lcm::with_lcm_write(&crate::paths::core_db(&work), |conn| { conn.execute_batch("DROP TRIGGER fail_import_marker; CREATE TEMP TABLE import_audit(n INTEGER); CREATE TEMP TRIGGER audit_import AFTER UPDATE OF migrated_to_lcm ON main.crew_member_learnings BEGIN INSERT INTO import_audit VALUES(1); END;")?; Ok(()) })?;
        assert_eq!(migrate_learnings_into_memory(&work, "crew-milo")?, 1);
        assert_eq!(migrate_learnings_into_memory(&work, "crew-milo")?, 0);
        let writes = crate::lcm::with_lcm_write(&crate::paths::core_db(&work), |conn| Ok(conn.query_row("SELECT COUNT(*) FROM temp.import_audit", [], |r| r.get::<_,i64>(0))?))?;
        assert_eq!(writes, 1, "learning marker escaped the owner or replayed");
        Ok(())
    }).await?;
    drop(adapter);
    readers.shutdown().await?;
    writer.shutdown().await?;
    Ok(())
}
