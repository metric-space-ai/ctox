use super::*;
use serde_json::json;

fn database(path: &std::path::Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS domain_counter (id INTEGER PRIMARY KEY, value INTEGER NOT NULL);
         INSERT OR IGNORE INTO domain_counter VALUES (1, 0);",
    )?;
    Ok(conn)
}

fn apply_once(conn: &mut Connection) -> anyhow::Result<AppliedDomainEffect> {
    DomainEffectAdmission::newly_claimed("command-1", "canonical-hash", "owner-1")?.apply(
        conn,
        |tx| {
            tx.execute("UPDATE domain_counter SET value = value + 1", [])?;
            Ok(AppliedDomainEffect {
                result: json!({"original": "created"}),
                projections: vec![DomainRecordRef {
                    collection: "workjet_projects".into(),
                    id: "project-1".into(),
                }],
            })
        },
    )
}

#[test]
fn receipt_failure_rolls_back_domain_mutation() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let mut conn = database(&root.path().join("domain.sqlite3"))?;
    conn.execute_batch(
        "CREATE TRIGGER refuse_receipt BEFORE INSERT ON business_command_domain_effects
         BEGIN SELECT RAISE(ABORT, 'receipt unavailable'); END;",
    )?;
    assert!(apply_once(&mut conn).is_err());
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    assert!(!contains(&conn, "command-1")?);
    conn.execute_batch("DROP TRIGGER refuse_receipt")?;
    apply_once(&mut conn)?;
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn mutation_failure_creates_neither_effect_nor_receipt() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let mut conn = database(&root.path().join("domain.sqlite3"))?;
    let admission = DomainEffectAdmission::newly_claimed("command-1", "canonical-hash", "owner-1")?;
    let result = admission.apply(&mut conn, |tx| {
        tx.execute("UPDATE domain_counter SET value = 99", [])?;
        anyhow::bail!("domain validation failed after first write")
    });
    assert!(result.is_err());
    assert!(!contains(&conn, "command-1")?);
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn receipt_replay_is_immutable_and_binds_actor_and_intent() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let mut conn = database(&root.path().join("domain.sqlite3"))?;
    let original = apply_once(&mut conn)?;
    conn.execute("UPDATE domain_counter SET value = 7", [])?;
    let replay = apply_once(&mut conn)?;
    assert_eq!(original, replay);
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        7
    );
    assert!(load(&conn, "command-1", "different-hash", "owner-1").is_err());
    assert!(load(&conn, "command-1", "canonical-hash", "owner-2").is_err());
    assert!(load(&conn, "missing-command", "canonical-hash", "owner-1")?.is_none());
    let changed = DomainEffectAdmission::newly_claimed("command-1", "different-hash", "owner-1")?;
    assert!(changed
        .apply(&mut conn, |_| panic!("mismatched intent reran mutation"))
        .is_err());
    Ok(())
}

#[test]
fn domain_effect_crash_child() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("CTOX_TEST_DOMAIN_RECEIPT_CRASH_PATH") else {
        return Ok(());
    };
    let mut conn = database(std::path::Path::new(&path))?;
    apply_once(&mut conn)?;
    // Exit without Rust destructors immediately after the committed effect;
    // no result publication and no orderly connection shutdown.
    std::process::exit(73);
}

#[test]
fn process_exit_after_commit_retains_one_effect_and_receipt() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("domain.sqlite3");
    let child_name = format!("{}::domain_effect_crash_child", module_path!());
    // Test binaries omit the crate prefix in test names.
    let child_name = child_name.split_once("::").context("test module path")?.1;
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", child_name, "--nocapture"])
        .env("CTOX_TEST_DOMAIN_RECEIPT_CRASH_PATH", &path)
        .status()?;
    assert_eq!(
        status.code(),
        Some(73),
        "child did not execute the crash scenario"
    );
    let mut conn = database(&path)?;
    assert!(contains(&conn, "command-1")?);
    apply_once(&mut conn)?;
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn concurrent_receipt_replay_applies_only_once() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("domain.sqlite3");
    database(&path)?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let mut conn = Connection::open(path)?;
                conn.busy_timeout(std::time::Duration::from_secs(5))?;
                barrier.wait();
                apply_once(&mut conn)?;
                Ok(())
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("receipt worker panicked")?;
    }
    let conn = Connection::open(path)?;
    assert_eq!(
        conn.query_row("SELECT value FROM domain_counter", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    Ok(())
}
