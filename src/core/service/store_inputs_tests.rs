use super::*;
use crate::service::{queue_store, store_runtime::DaemonStores};
use ctox_store::{DbId, WriteJob};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installed_inputs_keep_owner_generation_and_capture_commits_during_activation_read(
) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "fixture refuses configured state root"
    );
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("runtime"))?;
    assert!(matches!(store_inputs(root), Err(StoreError::Closed)));
    let mut owner = DaemonStores::start(queue_store::endpoint(root), Handle::current())?;
    let core = owner
        .register(
            "core",
            DbId::new(crate::paths::core_db(root))?,
            false,
            Arc::new(|_| Ok(())),
        )
        .await?;
    let sync = owner
        .register_snapshot_fixture(
            "sync",
            DbId::new(crate::paths::sync_db(root))?,
            true,
            Arc::new(|tx| {
                tx.execute("INSERT INTO _schema_apps VALUES('example','v1')", [])?;
                Ok(())
            }),
        )
        .await?;
    owner.publish_inputs(root).await?;
    assert!(matches!(
        owner.publish_inputs(root).await,
        Err(StoreError::AlreadyOwned)
    ));
    let inputs = store_inputs(root)?;
    assert_eq!(inputs.core.writer.generation(), core.writer.generation());
    assert_eq!(inputs.sync.writer.generation(), sync.writer.generation());
    assert!(
        owned_read(root, false, |_| Ok(())).is_err(),
        "runtime worker entered synchronous store bridge"
    );
    // Tokio blocking workers retain a runtime Handle. Only the existing private,
    // bounded Core compatibility admission may use the synchronous facade there.
    let bridge_root = root.to_owned();
    let value = crate::persistence::run_core_sync(move || {
        owned_read(&bridge_root, false, |db| {
            Ok(db.query_row("SELECT 42", [], |r| r.get::<_, i64>(0))?)
        })
    })
    .await?;
    assert_eq!(
        value, 42,
        "admitted Core worker could not use its real reader pool"
    );
    let callback_root = root.to_owned();
    core.writer
        .submit(WriteJob::new(Default::default(), move |_| {
            assert!(
                owned_read(&callback_root, false, |_| Ok(())).is_err(),
                "writer callback waited on another queue"
            );
            Ok(())
        }))?
        .await?;
    // The real WAL snapshot is already established when the concurrent commit
    // happens. Hold its decoder until that commit, without sleeping or polling.
    let (entered, reading) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let cloned = inputs.clone();
    let capture = tokio::spawn(async move {
        cloned
            .activation_snapshot(move |tx| {
                let version: String = tx.query_row(
                    "SELECT app_ver FROM _schema_apps WHERE app_id='example'",
                    [],
                    |r| r.get(0),
                )?;
                let _ = entered.send(());
                wait.recv_timeout(std::time::Duration::from_secs(2))
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(version)
            })
            .await
    });
    reading.await?;
    sync.writer
        .submit(WriteJob::new(Default::default(), |tx| {
            tx.execute(
                "UPDATE _schema_apps SET app_ver='v2' WHERE app_id='example'",
                [],
            )?;
            tx.execute("UPDATE _seq SET v=v+1 WHERE singleton=1", [])?;
            Ok(())
        }))?
        .await?;
    release.send(())?;
    let mut snapshot = capture.await??;
    assert_eq!(
        snapshot.value, "v1",
        "activation decode left its WAL snapshot"
    );
    assert_eq!(snapshot.head_seq, 0);
    assert!(
        snapshot.commits.has_changed()?,
        "commit made during activation read was not retained by the pre-snapshot feed"
    );
    assert_eq!(snapshot.commits.latest().unwrap().head_seq, 1);
    owner.shutdown().await?;
    assert!(matches!(store_inputs(root), Err(StoreError::Closed)));
    assert!(matches!(
        inputs.activation_snapshot(|_| Ok(())).await,
        Err(StoreError::Closed)
    ));
    Ok(())
}
