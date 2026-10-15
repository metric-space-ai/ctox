use super::*;

async fn fixture(root:&Path)->Result<std::sync::Arc<CorePersistence>> {
    let path=sqlite_path(root);
    anyhow::ensure!(path.starts_with(root),"isolated state root required");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let writer=ctox_store::registry::Registry.open(ctox_store::DbId::new(path)?,Default::default())?;
    writer.ready().await?;
    let readers=ctox_store::read_pool::ReadPool::for_database(writer.db_id(),writer.generation(),Default::default(),Default::default()).await?;
    install_core_owner(root,writer,readers).await
}

#[tokio::test]
async fn text_reader_reuses_connection_but_observes_committed_values()->Result<()> {
    let root=tempfile::tempdir()?;let core=fixture(root.path()).await?;
    // The pool owns its connections; each facade read gets a fresh snapshot.
    for expected in [Some("before"),Some("after"),None,Some("returned")] {
        store_text_value_owned(root.path(),"probe",expected).await?;
        let path=root.path().to_path_buf();
        let actual=run_core_sync(move||load_text_value(&path,"probe")).await?;
        assert_eq!(actual.as_deref(),expected);
    }
    assert!(load_text_value(root.path(),"probe").is_err(),"a sync facade must never block an async worker");
    assert!(matches!(ctox_store::registry::Registry.open(ctox_store::DbId::new(sqlite_path(root.path()))?,Default::default()),Err(ctox_store::StoreError::AlreadyOwned)));
    core.shutdown().await?;Ok(())
}

#[tokio::test]
async fn text_reader_does_not_reuse_another_root_or_retargeted_path()->Result<()> {
    let first=tempfile::tempdir()?;let second=tempfile::tempdir()?;let aliases=tempfile::tempdir()?;
    let first_owner=fixture(first.path()).await?;let second_owner=fixture(second.path()).await?;
    store_text_value_owned(first.path(),"probe",Some("first")).await?;
    store_text_value_owned(second.path(),"probe",Some("second")).await?;
    let alias=aliases.path().join("active");std::os::unix::fs::symlink(first.path(),&alias)?;
    let path=alias.clone();assert_eq!(run_core_sync(move||load_text_value(&path,"probe")).await?.as_deref(),Some("first"));
    std::fs::remove_file(&alias)?;std::os::unix::fs::symlink(second.path(),&alias)?;
    assert_eq!(run_core_sync(move||load_text_value(&alias,"probe")).await?.as_deref(),Some("second"));
    first_owner.shutdown().await?;second_owner.shutdown().await?;Ok(())
}

#[tokio::test]
async fn text_reader_rejects_corrupt_replacement_instead_of_serving_cached_value()->Result<()> {
    let first=tempfile::tempdir()?;let replacement=tempfile::tempdir()?;let aliases=tempfile::tempdir()?;
    let owner=fixture(first.path()).await?;
    store_text_value_owned(first.path(),"probe",Some("first")).await?;
    let replacement_db=sqlite_path(replacement.path());
    anyhow::ensure!(replacement_db.starts_with(replacement.path()),"isolated state root required");
    std::fs::create_dir_all(replacement_db.parent().unwrap())?;std::fs::write(&replacement_db,b"not a SQLite database")?;
    let alias=aliases.path().join("active");std::os::unix::fs::symlink(first.path(),&alias)?;
    let path=alias.clone();assert_eq!(run_core_sync(move||load_text_value(&path,"probe")).await?.as_deref(),Some("first"));
    std::fs::remove_file(&alias)?;std::os::unix::fs::symlink(replacement.path(),&alias)?;
    assert!(run_core_sync(move||load_text_value(&alias,"probe")).await.is_err());
    assert_eq!(std::fs::read(replacement_db)?,b"not a SQLite database");
    owner.shutdown().await?;Ok(())
}
