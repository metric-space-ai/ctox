use super::*;
use ctox_store::{
    ipc,
    rxdb_schema_adapter::{self, CanonicalChangeLog, RxdbOwner, RxdbTable},
    WriteJob,
};

#[tokio::test]
async fn owner_composition_fences_aliases_and_keeps_changes_and_documents_in_one_commit() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = StoreEndpoint::UnixSocket {
        path: dir.path().join("private/store.sock"),
    };
    let mut stores = DaemonStores::start(endpoint.clone(), Handle::current()).unwrap();
    let id = DbId::new(dir.path().join("sync.sqlite3")).unwrap();
    let database = stores.register("sync", id.clone(), true, Arc::new(|tx| {
        tx.execute_batch("CREATE TABLE docs(id TEXT PRIMARY KEY,revision TEXT NOT NULL,data TEXT NOT NULL,deleted INTEGER NOT NULL);
            INSERT INTO docs VALUES('old','1-old',json_object('id','old'),0);
            CREATE TABLE __rxdb_changed_tables(table_name TEXT,changed_at INTEGER);
            INSERT INTO __rxdb_changed_tables VALUES('docs',7);
            CREATE TRIGGER docs__rxdb_changed_insert AFTER INSERT ON docs BEGIN
                UPDATE __rxdb_changed_tables SET changed_at=changed_at+1; END;")?;
        rxdb_schema_adapter::install(tx, &RxdbTable {
            collection:"docs".into(),table:"docs".into(),generation:3,schema_version:0,app_version:"app.1".into(),
        }, &CanonicalChangeLog)?;
        assert_eq!(rxdb_schema_adapter::backfill(tx, "docs", None)?, None);
        Ok(())
    })).await.unwrap();
    // Both aliasing the file and reusing its logical name must be refused.
    assert!(matches!(
        stores
            .register("alias", id.clone(), true, Arc::new(|_| Ok(())))
            .await,
        Err(StoreError::AlreadyOwned)
    ));
    assert!(matches!(
        stores
            .register(
                "sync",
                DbId::new(dir.path().join("other.sqlite3")).unwrap(),
                false,
                Arc::new(|_| Ok(()))
            )
            .await,
        Err(StoreError::AlreadyOwned)
    ));
    assert!(ipc::MaintenanceOwner::acquire(&endpoint, id).is_err());
    let rxdb = RxdbOwner::attach(database.writer.clone(), database.readers.clone())
        .await
        .unwrap();
    let mut commits = rxdb.subscribe_commits();
    let before = rxdb
        .read(JobOptions::default(), |conn| {
            Ok(conn.query_row("SELECT v FROM _seq", [], |row| row.get::<_, u64>(0))?)
        })
        .await
        .unwrap();
    rxdb.write(JobOptions::default(), |tx| {
        tx.execute(
            "INSERT INTO docs VALUES('new','1-new',json_object('id','new'),0)",
            [],
        )?;
        Ok(())
    })
    .unwrap()
    .await
    .unwrap();
    let published = commits.latest().unwrap();
    assert_eq!(published.head_seq, before + 1);
    let page = database
        .readers
        .read(move |tx| change_log::read_page(tx, "docs", 3, before, 10))
        .await
        .unwrap();
    assert_eq!(page.head_seq, published.head_seq);
    assert_eq!(page.docs[0].body.as_ref().unwrap()["id"], "new");
    assert_eq!(page.docs[0].schema_ver, 0);
    assert_eq!(page.docs[0].app_ver, "app.1");
    let head = page.head_seq;
    assert!(rxdb
        .write(JobOptions::default(), |tx| {
            tx.execute(
                "INSERT INTO docs VALUES('rollback','1-r',json_object('id','rollback'),0)",
                [],
            )?;
            Err::<(), _>(StoreError::Invalid("rollback".into()))
        })
        .unwrap()
        .await
        .is_err());
    assert_eq!(
        rxdb.read(JobOptions::default(), |conn| Ok(conn.query_row(
            "SELECT v FROM _seq",
            [],
            |r| r.get::<_, u64>(0)
        )?))
        .await
        .unwrap(),
        head
    );
    let inspect = ipc::request(
        endpoint.clone(),
        Request {
            request_id: 1,
            kind: JobKind::Read,
            method: "store.inspect".into(),
            payload: serde_json::to_vec("sync").unwrap(),
        },
        JobOptions::default(),
    )
    .unwrap()
    .await
    .unwrap();
    let inspect: serde_json::Value = serde_json::from_slice(&inspect).unwrap();
    assert_eq!(inspect["queryOnly"], true);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let readers = database.readers.clone();
    let pending = tokio::spawn(async move {
        readers
            .read_job(ReadJob::new(JobOptions::default(), move |_conn| {
                let _ = started.send(());
                wait.recv_timeout(std::time::Duration::from_secs(2))
                    .map_err(|_| StoreError::DeadlineExceeded)?;
                Ok(())
            }))
            .await
    });
    ready.await.unwrap();
    // A bounded drain timeout retains the daemon lease and the file owner.
    // No second daemon may start while the old snapshot is still executing.
    assert!(stores.shutdown().await.is_err());
    assert!(DaemonStores::start(endpoint, Handle::current()).is_err());
    release.send(()).unwrap();
    let _ = pending.await.unwrap();
    stores.shutdown().await.unwrap();
    assert!(database
        .writer
        .submit(WriteJob::new(JobOptions::default(), |_| Ok(())))
        .is_err());
}

#[tokio::test]
async fn a_matching_generation_never_allows_readers_from_another_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let a = stores
        .register(
            "a",
            DbId::new(dir.path().join("a.sqlite3")).unwrap(),
            false,
            Arc::new(|_| Ok(())),
        )
        .await
        .unwrap();
    let b = stores
        .register(
            "b",
            DbId::new(dir.path().join("b.sqlite3")).unwrap(),
            false,
            Arc::new(|_| Ok(())),
        )
        .await
        .unwrap();
    // Match the numeric generation deliberately: checking only that integer
    // cannot establish that these readers belong to this writer's file.
    let foreign = ReadPool::for_database(
        b.writer.db_id(),
        a.writer.generation(),
        ConnectionConfig::default(),
        AdmissionBudget::default(),
    )
    .await
    .unwrap();
    assert!(matches!(
        RxdbOwner::attach(a.writer, foreign.clone()).await,
        Err(StoreError::Invalid(_))
    ));
    foreign.shutdown().await.unwrap();
    stores.shutdown().await.unwrap();
}
