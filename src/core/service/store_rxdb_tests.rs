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
            Arc::new(|_| Ok(())),
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
    let rejected = jobs.write(Box::new(|tx| {
        tx.execute("UPDATE owner_fixture__notes__v0 SET data=json_set(data,'$.title','must-rollback') WHERE id='one'", []).map_err(|_| new_rx_error("FIXTURE_SQL",None))?;
        Err(new_rx_error("FIXTURE_REJECTED", Some(json!({"status":422,"field":"evidence"}))))
    })).unwrap().await.unwrap();
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
