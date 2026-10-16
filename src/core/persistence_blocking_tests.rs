use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_followup_blocking_entry_preserves_owner_and_context_boundaries() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = sqlite_path(root.path());
    anyhow::ensure!(
        path.starts_with(root.path()),
        "isolated Core fixture required"
    );
    std::fs::create_dir_all(path.parent().unwrap())?;
    let (writer, readers) = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path)?)
        .await?
        .into_parts();
    let core = install_core_owner(root.path(), writer.clone(), readers).await?;
    let runtime = tokio::runtime::Handle::current();
    let work = root.path().to_owned();
    let handle = runtime.clone();
    std::thread::spawn(move || -> Result<()> {
        assert_eq!(
            require_sync_owner_context()
                .unwrap_err()
                .downcast_ref::<CoreSyncError>(),
            Some(&CoreSyncError::Required)
        );
        let work2 = work.clone();
        run_core_blocking(&handle, move || {
            store_text_value(&work2, "ordinary-thread", Some("acknowledged"))?;
            assert_eq!(
                load_text_value(&work2, "ordinary-thread")?.as_deref(),
                Some("acknowledged")
            );
            Ok(())
        })?;
        let callback_root = work.clone();
        crate::business_os::store::run_subscription_persistence(&work, move || {
            store_text_value(&callback_root, "provider-callback", Some("same-owner"))?;
            assert_eq!(
                load_text_value(&callback_root, "provider-callback")?.as_deref(),
                Some("same-owner")
            );
            Ok(())
        })?;
        assert!(
            require_sync_owner_context().is_err(),
            "admission leaked to the caller"
        );
        let error =
            run_core_blocking::<()>(&handle, || panic!("isolated worker panic")).unwrap_err();
        assert_eq!(
            error.downcast_ref::<CoreSyncError>(),
            Some(&CoreSyncError::WorkerClosed)
        );
        run_core_blocking(&handle, || require_sync_owner_context())?;
        Ok(())
    })
    .join()
    .expect("ordinary-thread caller joins")?;
    let work = root.path().to_owned();
    ctox_app_runtime::daemon::BlockingExecutor::run(
        &CoreCompatibilityExecutor,
        Box::new(move || {
            require_sync_owner_context()?;
            store_text_value(&work, "app-runtime-callback", Some("same-owner"))
        }),
    )
    .await?;
    assert_eq!(
        load_text_value_owned(root.path(), "app-runtime-callback")
            .await?
            .as_deref(),
        Some("same-owner")
    );
    let error = run_core_blocking::<()>(&runtime, || {
        panic!("async worker must not execute blocking body")
    })
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<CoreSyncError>(),
        Some(&CoreSyncError::AsyncContext)
    );
    let handle = runtime.clone();
    writer
        .submit(ctox_store::WriteJob::new(Default::default(), move |_| {
            let error =
                run_core_blocking::<()>(&handle, || panic!("owner must not wait on itself"))
                    .unwrap_err();
            assert_eq!(
                error.downcast_ref::<CoreSyncError>(),
                Some(&CoreSyncError::OwnerCallback)
            );
            Ok(())
        }))?
        .await?;
    assert_eq!(
        load_text_value_owned(root.path(), "ordinary-thread")
            .await?
            .as_deref(),
        Some("acknowledged")
    );
    core.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_followup_origin_on_plain_thread_distinguishes_absence_from_store_error() -> Result<()>
{
    use crate::app_capabilities::{cap3::config, ErrorCode};
    let root = tempfile::tempdir()?;
    let fixture = crate::inference::runtime_env::RuntimeOwnerFixture::open(root.path()).await?;
    let work = root.path().to_owned();
    std::thread::spawn(move || -> Result<()> {
        assert_eq!(config::public_origin(&work)?, None);
        let seed = work.clone();
        run_core_blocking_at(&work, move || {
            crate::inference::runtime_env::set_runtime_env_value(
                &seed,
                config::PUBLIC_ORIGIN_KEY,
                "https://origin.example.test",
            )
        })?;
        assert_eq!(
            config::public_origin(&work)?.as_deref(),
            Some("https://origin.example.test")
        );
        Ok(())
    })
    .join()
    .expect("plain HTTP-style caller joins")?;
    let path = crate::inference::runtime_env::runtime_config_path(root.path());
    let owner = crate::lcm::owner_adapter::OwnerAdapter::for_path(&path)?;
    owner
        .write(Default::default(), |tx| {
            tx.execute_batch("ALTER TABLE runtime_env_kv RENAME COLUMN env_key TO broken_key;")?;
            Ok(())
        })
        .await?;
    let work = root.path().to_owned();
    std::thread::spawn(move || {
        let error = config::public_origin(&work)
            .expect_err("SQL failure must never become unconfigured origin");
        assert_eq!(error.code, ErrorCode::Unavailable);
    })
    .join()
    .expect("typed SQL failure returned");
    fixture.close().await?;
    Ok(())
}
