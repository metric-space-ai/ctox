use super::*;
use std::fs;

fn fixture() -> anyhow::Result<(tempfile::TempDir, Connection)> {
    let root = tempfile::tempdir()?;
    let path = super::super::business_os_store_path(root.path());
    fs::create_dir_all(path.parent().unwrap())?;
    let writer = Connection::open(path)?;
    writer.execute_batch("CREATE TABLE business_peer_revocations (peer_id TEXT PRIMARY KEY)")?;
    Ok((root, writer))
}

#[test]
fn missing_policy_store_is_an_error_and_is_not_created() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = super::super::business_os_store_path(root.path());
    assert!(is_business_peer_revoked(root.path(), "peer").is_err());
    assert!(
        !path.exists(),
        "authorization must not initialize an empty authority store"
    );
    Ok(())
}

#[test]
fn reader_observes_external_commits_and_never_caches_allow_or_deny() -> anyhow::Result<()> {
    let (root, writer) = fixture()?;
    for _ in 0..3 {
        assert!(!is_business_peer_revoked(root.path(), "peer")?);
        writer.execute("INSERT INTO business_peer_revocations VALUES ('peer')", [])?;
        assert!(is_business_peer_revoked(root.path(), "peer")?);
        writer.execute("DELETE FROM business_peer_revocations", [])?;
    }
    assert!(!is_business_peer_revoked(root.path(), "peer")?);
    let mut timings = Vec::new();
    for _ in 0..100 {
        let start = std::time::Instant::now();
        assert!(!is_business_peer_revoked(root.path(), "peer")?);
        timings.push(start.elapsed().as_micros());
    }
    timings.sort_unstable();
    eprintln!(
        "peer_revocation_read n=100 p50_us={} p95_us={} scope=isolated_sqlite_reader",
        timings[49], timings[94]
    );
    Ok(())
}

#[test]
fn missing_table_and_corrupt_database_do_not_become_allow_decisions() -> anyhow::Result<()> {
    let (root, writer) = fixture()?;
    assert!(!is_business_peer_revoked(root.path(), "peer")?);
    writer.execute_batch("DROP TABLE business_peer_revocations")?;
    assert!(is_business_peer_revoked(root.path(), "peer").is_err());
    writer.execute_batch(
        "CREATE TABLE business_peer_revocations (peer_id TEXT PRIMARY KEY);
        INSERT INTO business_peer_revocations VALUES ('peer')",
    )?;
    assert!(is_business_peer_revoked(root.path(), "peer")?);
    let corrupt = tempfile::tempdir()?;
    let path = super::super::business_os_store_path(corrupt.path());
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, [b'x'; 4096])?;
    assert!(is_business_peer_revoked(corrupt.path(), "peer").is_err());
    assert_eq!(fs::metadata(&path)?.len(), 4096);
    Ok(())
}

#[test]
fn empty_identity_cannot_be_admitted() -> anyhow::Result<()> {
    let (root, _writer) = fixture()?;
    assert!(is_business_peer_revoked(root.path(), "  ").is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn replacement_removal_and_instance_switch_discard_old_policy_handles() -> anyhow::Result<()> {
    let (root, writer) = fixture()?;
    let (replacement, replacement_writer) = fixture()?;
    writer.execute("INSERT INTO business_peer_revocations VALUES ('peer')", [])?;
    assert!(is_business_peer_revoked(root.path(), "peer")?);
    assert!(!is_business_peer_revoked(replacement.path(), "peer")?);
    assert!(is_business_peer_revoked(root.path(), "peer")?);
    drop((writer, replacement_writer));
    let path = super::super::business_os_store_path(root.path());
    fs::rename(
        super::super::business_os_store_path(replacement.path()),
        &path,
    )?;
    assert!(!is_business_peer_revoked(root.path(), "peer")?);
    fs::remove_file(&path)?;
    assert!(is_business_peer_revoked(root.path(), "peer").is_err());
    assert!(!path.exists());
    Ok(())
}
