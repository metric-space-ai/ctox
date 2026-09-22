use super::*;
use std::time::Instant;

#[test]
fn text_reader_reuses_connection_but_observes_committed_values() -> Result<()> {
    let root = tempfile::tempdir()?;
    store_text_value(root.path(), "probe", Some("before"))?;
    let conn = open_sqlite(root.path())?;
    // Parsing unrelated tables/triggers must not recur on every tiny KV read.
    // These are isolated fixture tables, not generated Business OS schemas.
    for index in 0..256 {
        conn.execute_batch(&format!(
            "CREATE TABLE text_probe_{index} (id INTEGER PRIMARY KEY, value TEXT);
             CREATE TRIGGER text_probe_trigger_{index} AFTER INSERT ON text_probe_{index}
             BEGIN UPDATE text_probe_{index} SET value=NEW.value WHERE id=NEW.id; END;"
        ))?;
    }
    drop(conn);
    TEXT_READER.with(|slot| *slot.borrow_mut() = None);
    TEXT_READER_OPENS.with(|count| count.set(0));
    let mut cached_micros = Vec::new();
    let mut reopened_micros = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        assert_eq!(
            load_text_value(root.path(), "probe")?.as_deref(),
            Some("before")
        );
        cached_micros.push(started.elapsed().as_micros());
        let started = Instant::now();
        assert_eq!(
            read_text_value(&open_sqlite(root.path())?, "probe")?.as_deref(),
            Some("before")
        );
        reopened_micros.push(started.elapsed().as_micros());
    }
    assert_eq!(TEXT_READER_OPENS.with(|count| count.get()), 1);
    store_text_value(root.path(), "probe", Some("after"))?;
    assert_eq!(
        load_text_value(root.path(), "probe")?.as_deref(),
        Some("after")
    );
    store_text_value(root.path(), "probe", None)?;
    assert_eq!(load_text_value(root.path(), "probe")?, None);
    store_text_value(root.path(), "probe", Some("returned"))?;
    assert_eq!(
        load_text_value(root.path(), "probe")?.as_deref(),
        Some("returned")
    );
    assert_eq!(TEXT_READER_OPENS.with(|count| count.get()), 1);
    cached_micros.sort_unstable();
    reopened_micros.sort_unstable();
    // Explicit order-statistic summaries; these are not Browser/CTOX budgets.
    println!(
        "text_reader_fixture samples=30 tables=256 cached_opens=1 reopened_opens=30 cached_p50_nearest_rank_us={} cached_p95_nearest_rank_us={} reopened_p50_nearest_rank_us={} reopened_p95_nearest_rank_us={}",
        cached_micros[14], cached_micros[28], reopened_micros[14], reopened_micros[28]
    );
    TEXT_READER.with(|slot| *slot.borrow_mut() = None);
    Ok(())
}

#[test]
fn text_reader_does_not_reuse_another_root_or_retargeted_path() -> Result<()> {
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    let aliases = tempfile::tempdir()?;
    store_text_value(first.path(), "probe", Some("first"))?;
    store_text_value(second.path(), "probe", Some("second"))?;
    assert_eq!(
        load_text_value(first.path(), "probe")?.as_deref(),
        Some("first")
    );
    assert_eq!(
        load_text_value(second.path(), "probe")?.as_deref(),
        Some("second")
    );
    let alias = aliases.path().join("active");
    std::os::unix::fs::symlink(first.path(), &alias)?;
    assert_eq!(load_text_value(&alias, "probe")?.as_deref(), Some("first"));
    std::fs::remove_file(&alias)?;
    std::os::unix::fs::symlink(second.path(), &alias)?;
    assert_eq!(load_text_value(&alias, "probe")?.as_deref(), Some("second"));
    TEXT_READER.with(|slot| *slot.borrow_mut() = None);
    Ok(())
}

#[test]
fn text_reader_rejects_corrupt_replacement_instead_of_serving_cached_value() -> Result<()> {
    let first = tempfile::tempdir()?;
    let replacement = tempfile::tempdir()?;
    let aliases = tempfile::tempdir()?;
    store_text_value(first.path(), "probe", Some("first"))?;
    std::fs::create_dir_all(replacement.path().join("runtime"))?;
    std::fs::write(sqlite_path(replacement.path()), b"not a SQLite database")?;
    let alias = aliases.path().join("active");
    std::os::unix::fs::symlink(first.path(), &alias)?;
    assert_eq!(load_text_value(&alias, "probe")?.as_deref(), Some("first"));
    std::fs::remove_file(&alias)?;
    std::os::unix::fs::symlink(replacement.path(), &alias)?;
    assert!(load_text_value(&alias, "probe").is_err());
    assert!(TEXT_READER.with(|slot| slot.borrow().is_none()));
    Ok(())
}
