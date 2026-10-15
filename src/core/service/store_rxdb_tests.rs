#[tokio::test]
async fn completed_sqlite_jobs_cannot_create_unbounded_reply_futures() {
    let dir = tempfile::tempdir().unwrap();
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let database = stores
        .register(
            "sync",
            DbId::new(dir.path().join("sync.sqlite3")).unwrap(),
            true,
            Arc::new(|_| Ok(())),
        )
        .await
        .unwrap();
    let jobs = RxdbJobs::attach(
        database,
        Handle::current(),
        Arc::new(|_, _, _, _| {
            Err(StoreError::Invalid(
                "no application schema in this fixture".into(),
            ))
        }),
    )
    .await
    .unwrap();
    let (finished, wait) = std::sync::mpsc::channel();
    let mut replies = Vec::new();
    for _ in 0..1023 {
        replies.push(
            jobs.write(Box::new(|_| Ok(Box::new(()) as SqliteJobValue)))
                .unwrap(),
        );
    }
    replies.push(
        jobs.write(Box::new(move |_| {
            let _ = finished.send(());
            Ok(Box::new(()) as SqliteJobValue)
        }))
        .unwrap(),
    );
    // Deliberately do not yield this current-thread test runtime. The actual
    // SQLite OS thread drains its queue, while reply tasks cannot run yet.
    // The boundary must remain bounded even after the writer queue is empty.
    wait.recv_timeout(Duration::from_secs(2)).unwrap();
    let overflow = jobs.write(Box::new(|_| Ok(Box::new(()) as SqliteJobValue)));
    assert_eq!(
        overflow.err().expect("unbounded reply admission").code(),
        "SQLITE_OWNER_BUSY"
    );
    for reply in replies {
        assert!(reply.await.unwrap().is_ok());
    }
    drop(jobs);
    stores.shutdown().await.unwrap();
}

use super::*;
use crate::service::store_runtime::DaemonStores;
use ctox_store::{change_log, ipc::StoreEndpoint, DbId};
use rxdb::{
    storage::sqlite::index_mod::create_storage_instance,
    types::{BulkWriteRow, RxJsonSchema, RxStorageInstance, RxStorageInstanceCreationParams},
};
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn real_rxdb_factory_uses_owner_and_rejected_write_rolls_back_without_losing_original_error()
{
    let dir = tempfile::tempdir().unwrap();
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let database = stores
        .register(
            "sync",
            DbId::new(dir.path().join("sync.sqlite3")).unwrap(),
            true,
            Arc::new(|tx| {
                ctox_store::schema::create_collection(tx, &ctox_store::schema::CollectionDefinition {
                    name:"notes".into(), app_id:"fixture.notes".into(),schema_ver:0,
                    schema_json:json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"}}}),
                    class:"replicated".into(),client_limit:100,byte_budget:1048576,rights_ver:1,app_ver:"notes.1".into()
                })?;
                Ok(())
            }),
        )
        .await
        .unwrap();
    // This fixture explicitly declares its one application's binding. Production
    // supplies the committed schema resolver, with no permissive fallback.
    let jobs = RxdbJobs::attach(
        database.clone(),
        Handle::current(),
        Arc::new(|_tx, table, collection, version| {
            if collection != "notes" || version != 0 {
                return Err(StoreError::Invalid("unknown fixture schema".into()));
            }
            Ok(RxdbTable {
                table: table.into(),
                collection: collection.into(),
                generation: 1,
                schema_version: 0,
                app_version: "notes.1".into(),
            })
        }),
    )
    .await
    .unwrap();
    let storage = jobs.storage();
    assert!(storage.connection().is_err());
    let schema: RxJsonSchema = serde_json::from_value(json!({
        "version":0,"type":"object","primaryKey":"id",
        "properties":{"id":{"type":"string","maxLength":120},"title":{"type":"string"}},
        "required":["id","title"]
    }))
    .unwrap();
    let instance = create_storage_instance(
        &storage,
        RxStorageInstanceCreationParams {
            database_instance_token: "owner-proof".into(),
            database_name: "owner_fixture".into(),
            collection_name: "notes".into(),
            schema,
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        },
    )
    .await
    .unwrap();
    let doc = json!({"id":"one","title":"owned","_rev":"1-a","_deleted":false,"_meta":{"lwt":1.0},"_attachments":{}});
    let response = instance
        .bulk_write(
            vec![BulkWriteRow {
                previous: None,
                document: doc,
            }],
            "owner-proof",
        )
        .await
        .unwrap();
    assert!(response.error.is_empty());
    let documents = instance
        .find_documents_by_id(&["one".into()], false)
        .await
        .unwrap();
    assert_eq!(documents[0]["title"], "owned");
    let page = database
        .readers
        .read(|tx| change_log::read_page(tx, "notes", 1, 0, 10))
        .await
        .unwrap();
    assert_eq!(page.docs[0].body.as_ref().unwrap()["title"], "owned");
    let before = page.head_seq;
    let rejected = jobs
        .write(Box::new(|tx| {
            tx.execute(
                "UPDATE c_notes SET data=json_set(data,'$.title','must-rollback') WHERE id='one'",
                [],
            )
            .map_err(|_| new_rx_error("FIXTURE_SQL", None))?;
            Err(new_rx_error(
                "FIXTURE_REJECTED",
                Some(json!({"status":422,"field":"evidence"})),
            ))
        }))
        .unwrap()
        .await
        .unwrap();
    assert_eq!(rejected.err().unwrap().code(), "FIXTURE_REJECTED");
    let after = database
        .readers
        .read(|tx| change_log::read_page(tx, "notes", 1, 0, 10))
        .await
        .unwrap();
    assert_eq!(after.head_seq, before);
    assert_eq!(after.docs[0].body.as_ref().unwrap()["title"], "owned");
    let mut wake = jobs.subscribe_commits();
    tokio::time::timeout(Duration::from_secs(1), async {
        while *wake.borrow_and_update() < before {
            wake.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    // Epoch/horizon changes can commit without increasing seq. They must
    // still wake the consumer so it can re-read the authoritative metadata.
    wake.borrow_and_update();
    database
        .writer
        .submit(ctox_store::WriteJob::new(options(), |tx| {
            change_log::rotate_epoch(tx)?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), wake.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*wake.borrow(), before);
    instance.close().await.unwrap();
    drop(instance);
    drop(storage);
    drop(jobs);
    stores.shutdown().await.unwrap();
}

#[tokio::test]
async fn canonical_cutover_counts_restartable_import_and_refuses_double_activation() {
    let dir = tempfile::tempdir().unwrap();
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let database=stores.register("sync",DbId::new(dir.path().join("sync.sqlite3")).unwrap(),true,Arc::new(|tx| {
        ctox_store::schema::create_collection(tx,&ctox_store::schema::CollectionDefinition {
            name:"notes".into(),app_id:"notes".into(),schema_ver:0,
            schema_json:json!({"primaryKey":"id","type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"}}}),
            class:"replicated".into(),client_limit:1000,byte_budget:1048576,rights_ver:1,app_ver:"1".into()
        })?;
        tx.execute_batch("CREATE TABLE ctox_business_os__notes__v0(id TEXT PRIMARY KEY,revision TEXT,deleted INTEGER,lastWriteTime REAL,data TEXT)")?;
        for n in 0..503 {
            let id=format!("legacy-{n:04}");
            tx.execute("INSERT INTO ctox_business_os__notes__v0 VALUES(?1,'1-import',?2,999999999,?3)",rusqlite::params![id,i64::from(n==502),json!({"id":id,"title":"preserved","_rev":"stale-body","_meta":{"lwt":1}}).to_string()])?;
        }
        Ok(())
    })).await.unwrap();
    // Commit exactly the first bounded page, retire the real file owner, and
    // open it again: resume must use the durable cursor, not an in-memory one.
    assert!(!database
        .writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            rxdb::storage::sqlite::sql::migrate_legacy_page(
                tx,
                "ctox_business_os__notes__v0",
                "c_notes",
                0,
            )
            .map_err(|_| StoreError::Invalid("fixture import failed".into()))
        }))
        .unwrap()
        .await
        .unwrap());
    stores.shutdown().await.unwrap();
    drop(database);
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let database = stores
        .register(
            "sync",
            DbId::new(dir.path().join("sync.sqlite3")).unwrap(),
            true,
            Arc::new(|_| Ok(())),
        )
        .await
        .unwrap();
    let jobs = RxdbJobs::attach_canonical(database.clone(), Handle::current())
        .await
        .unwrap();
    let storage = jobs.storage();
    assert!(storage.connection().is_err(), "raw-opener bypass");
    let parameters = || {
        RxStorageInstanceCreationParams {
        database_instance_token:"cutover".into(),database_name:"ctox-business-os".into(),collection_name:"notes".into(),
        schema:serde_json::from_value(json!({"version":0,"type":"object","primaryKey":"id","properties":{"id":{"type":"string","maxLength":120},"title":{"type":"string"}},"required":["id","title"]})).unwrap(),
        options:HashMap::new(),multi_instance:false,dev_mode:false,password:None
    }
    };
    let first = create_storage_instance(&storage, parameters())
        .await
        .unwrap();
    let docs = first
        .find_documents_by_id(&["legacy-0000".into(), "legacy-0502".into()], true)
        .await
        .unwrap();
    assert_eq!(docs[0]["title"], "preserved");
    assert_eq!(docs[0]["_rev"], "1-import");
    assert_eq!(docs[1]["_deleted"], true);
    database.readers.read(|tx| {
        let receipt:(i64,i64,String)=tx.query_row("SELECT expected,copied,state FROM _rxdb_imports",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!(receipt,(503,503,"complete".into()));
        assert_eq!(tx.query_row("SELECT COUNT(*) FROM c_notes",[],|r|r.get::<_,i64>(0))?,503);
        assert!(tx.query_row("SELECT data IS NULL FROM c_notes WHERE id='legacy-0502'",[],|r|r.get::<_,bool>(0))?);
        assert!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='ctox_business_os__notes__v0')",[],|r|r.get::<_,bool>(0))?);
        Ok(())
    }).await.unwrap();
    first.close().await.unwrap();
    let second = create_storage_instance(&storage, parameters())
        .await
        .unwrap();
    second.close().await.unwrap();
    // Recreating a source after successful cutover must not silently activate
    // another current table or overwrite already accepted canonical rows.
    database
        .writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute_batch("CREATE TABLE ctox_business_os__notes__v0(id TEXT)")?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    assert!(
        create_storage_instance(&storage, parameters())
            .await
            .is_err(),
        "double activation accepted"
    );
    drop(storage);
    drop(jobs);
    stores.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_factory_requires_current_daemon_registration_without_raw_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.sqlite3");
    assert!(native_storage(path.clone()).await.is_err());
    assert!(!path.exists(), "unregistered native factory opened SQLite");
    let mut stores = DaemonStores::start(
        StoreEndpoint::UnixSocket {
            path: dir.path().join("ipc/store.sock"),
        },
        Handle::current(),
    )
    .unwrap();
    let database = stores
        .register(
            "sync",
            DbId::new(&path).unwrap(),
            true,
            Arc::new(|_| Ok(())),
        )
        .await
        .unwrap();
    stores.install_native_rxdb(database).await.unwrap();
    let storage = native_storage(path.clone()).await.unwrap();
    assert!(storage.connection().is_err(), "raw-opener bypass");
    let instance = create_storage_instance(
        &storage,
        RxStorageInstanceCreationParams {
            database_instance_token: "native-proof".into(),
            database_name: "ctox-business-os".into(),
            collection_name: rxdb::rx_storage_helper::INTERNAL_STORAGE_NAME.into(),
            schema: rxdb::rx_database_internal_store::build_internal_store_schema(),
            options: HashMap::new(),
            multi_instance: false,
            dev_mode: false,
            password: None,
        },
    )
    .await
    .unwrap();
    instance.close().await.unwrap();
    stores.shutdown().await.unwrap();
    assert!(
        native_storage(path).await.is_err(),
        "retired generation still installed"
    );
}
