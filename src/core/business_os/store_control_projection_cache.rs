// Origin: CTOX
// License: AGPL-3.0-only

use super::*;

// One command-collection writer per executing thread, never a global pool.
// No transaction, authorization result or business record is retained here.
#[cfg(unix)]
thread_local! {
    static CONTROL_WRITER: RefCell<Option<CachedControlWriter>> = RefCell::new(None);
    static CONTROL_WRITER_IN_USE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(unix)]
struct CachedControlWriter {
    key: BusinessOsStoreDbKey,
    opened_at: std::time::Instant,
    remaining_uses: u8,
    writers: RxdbProjectionWriterCache,
}

#[cfg(unix)]
struct ControlWriterScope;

#[cfg(unix)]
impl Drop for ControlWriterScope {
    fn drop(&mut self) {
        CONTROL_WRITER_IN_USE.with(|active| active.set(false));
    }
}

#[cfg(unix)]
pub(in crate::business_os) fn with_control_projection_writers<T>(
    root: &Path,
    apply: impl FnOnce(&mut RxdbProjectionWriterCache) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    // Nested control execution gets an independent writer. Do not borrow a
    // RefCell across application code or overwrite the outer command's slot.
    if CONTROL_WRITER_IN_USE.with(|active| active.replace(true)) {
        return apply(&mut RxdbProjectionWriterCache::new(root));
    }
    let _scope = ControlWriterScope;
    let path = rxdb_store_path(root);
    let key = business_os_store_db_key(&path);
    let previous = CONTROL_WRITER.with(|slot| slot.borrow_mut().take());
    let mut entry = previous
        .filter(|entry| {
            entry.key == key
                && entry.writers.root.as_path() == root
                && entry.remaining_uses > 0
                && entry.opened_at.elapsed() < Duration::from_secs(30)
        })
        .unwrap_or_else(|| CachedControlWriter {
            key,
            opened_at: std::time::Instant::now(),
            remaining_uses: 64,
            writers: RxdbProjectionWriterCache::new(root),
        });
    let result = apply(&mut entry.writers);
    // Errors, absent collections and a database replacement discard the whole
    // entry. A later command retries normal discovery; absence is never cached.
    let retain = result.is_ok()
        && entry.key == business_os_store_db_key(&path)
        && entry.writers.writers.len() == 1
        && entry
            .writers
            .writers
            .get("business_commands")
            .and_then(Option::as_ref)
            .is_some_and(|writer| writer.conn.is_autocommit());
    if retain {
        entry.remaining_uses -= 1;
        CONTROL_WRITER.with(|slot| *slot.borrow_mut() = Some(entry));
    }
    result
}

// The existing portable DB key has no file identity on these platforms. Keep
// the original per-command lifetime until an equivalent identity is available.
#[cfg(not(unix))]
pub(in crate::business_os) fn with_control_projection_writers<T>(
    root: &Path,
    apply: impl FnOnce(&mut RxdbProjectionWriterCache) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    apply(&mut RxdbProjectionWriterCache::new(root))
}

impl RxdbProjectionWriterCache {
    pub(in crate::business_os) fn upsert_control(
        &mut self,
        collection: &str,
        record_id: &str,
        updated_at_ms: i64,
        payload: Value,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            collection == "business_commands",
            "control writer collection mismatch"
        );
        for _ in 0..2 {
            if !matches!(self.writers.get(collection), Some(Some(_))) {
                self.writers.insert(
                    collection.to_owned(),
                    RxdbCollectionWriter::open(&self.root, collection)?,
                );
            }
            let Some(writer) = self.writers.get_mut(collection).and_then(Option::as_mut) else {
                return Ok(());
            };
            match writer.upsert_control(record_id, updated_at_ms, payload.clone()) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    self.writers.remove(collection);
                }
                Err(error) => {
                    self.writers.remove(collection);
                    return Err(error);
                }
            }
        }
        anyhow::bail!("RxDB control projection generation changed during publication")
    }
}

impl RxdbCollectionWriter {
    fn upsert_control(
        &mut self,
        record_id: &str,
        updated_at_ms: i64,
        payload: Value,
    ) -> anyhow::Result<bool> {
        // Schema validation and the read/merge/write share an IMMEDIATE
        // transaction. No schema writer or concurrent record writer can race
        // cached table metadata or revision allocation inside this boundary.
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let schema: i64 = tx.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
        if schema != self.schema_version
            || business_os_store_db_key(&self.database_path) != self.database_key
        {
            tx.rollback()?;
            return Ok(false);
        }
        upsert_rxdb_collection_record_with_writer(
            &tx,
            &self.table,
            &self.columns,
            record_id,
            updated_at_ms,
            updated_at_ms,
            payload,
            self.demand_file_storage,
            false,
            true,
        )?;
        tx.commit()?;
        self.notify_committed_change();
        Ok(true)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_commands(root: &Path) -> anyhow::Result<Connection> {
        let path = rxdb_store_path(root);
        std::fs::create_dir_all(path.parent().expect("database parent"))?;
        let conn = Connection::open(path)?;
        conn.execute_batch(&format!(
            "CREATE TABLE ctox_business_os__business_commands__v{} (
                id TEXT PRIMARY KEY, revision TEXT, deleted INTEGER,
                lastWriteTime REAL, data TEXT NOT NULL
            )",
            rxdb_schema_version("business_commands")
        ))?;
        Ok(conn)
    }

    fn table() -> String {
        format!(
            "ctox_business_os__business_commands__v{}",
            rxdb_schema_version("business_commands")
        )
    }

    fn write(root: &Path, id: &str) -> anyhow::Result<()> {
        with_control_projection_writers(root, |writers| {
            writers.upsert_control(
                "business_commands",
                id,
                1000,
                serde_json::json!({
                    "id": id, "status": "completed"
                }),
            )
        })
    }

    fn rows(conn: &Connection) -> anyhow::Result<i64> {
        Ok(
            conn.query_row(&format!("SELECT COUNT(*) FROM {}", table()), [], |row| {
                row.get(0)
            })?,
        )
    }

    #[test]
    fn control_writer_reuses_one_connection_and_expires_by_age_use_and_root() -> anyhow::Result<()>
    {
        let first = tempdir()?;
        let second = tempdir()?;
        let first_db = create_commands(first.path())?;
        let second_db = create_commands(second.path())?;
        reset_rxdb_collection_writer_open_count(first.path(), "business_commands");
        write(first.path(), "one")?;
        write(first.path(), "two")?;
        assert_eq!(
            rxdb_collection_writer_open_count(first.path(), "business_commands"),
            1
        );
        CONTROL_WRITER.with(|slot| {
            slot.borrow_mut().as_mut().unwrap().opened_at -= Duration::from_secs(31);
        });
        write(first.path(), "age")?;
        assert_eq!(
            rxdb_collection_writer_open_count(first.path(), "business_commands"),
            2
        );
        CONTROL_WRITER.with(|slot| {
            slot.borrow_mut().as_mut().unwrap().remaining_uses = 0;
        });
        write(first.path(), "uses")?;
        assert_eq!(
            rxdb_collection_writer_open_count(first.path(), "business_commands"),
            3
        );
        write(second.path(), "other-root")?;
        assert_eq!(rows(&first_db)?, 4);
        assert_eq!(rows(&second_db)?, 1);
        Ok(())
    }

    #[test]
    fn control_writer_reopens_after_database_replacement() -> anyhow::Result<()> {
        let root = tempdir()?;
        let conn = create_commands(root.path())?;
        write(root.path(), "old")?;
        drop(conn);
        let path = rxdb_store_path(root.path());
        let old_path = path.with_extension("old.sqlite3");
        std::fs::rename(&path, &old_path)?;
        let replacement = create_commands(root.path())?;
        write(root.path(), "new")?;
        assert_eq!(rows(&replacement)?, 1);
        let old = Connection::open(old_path)?;
        assert_eq!(rows(&old)?, 1);
        let id: String =
            replacement.query_row(&format!("SELECT id FROM {}", table()), [], |row| row.get(0))?;
        assert_eq!(id, "new");
        Ok(())
    }

    #[test]
    fn control_writer_recovers_missing_collection_and_wal_schema_change() -> anyhow::Result<()> {
        let root = tempdir()?;
        let path = rxdb_store_path(root.path());
        std::fs::create_dir_all(path.parent().expect("database parent"))?;
        let empty = Connection::open(path)?;
        empty.execute_batch("PRAGMA journal_mode=WAL")?;
        write(root.path(), "missing")?;
        assert!(CONTROL_WRITER.with(|slot| slot.borrow().is_none()));
        let conn = create_commands(root.path())?;
        write(root.path(), "before-schema")?;
        conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN generation_probe TEXT",
            table()
        ))?;
        write(root.path(), "after-schema")?;
        assert_eq!(rows(&conn)?, 2);
        CONTROL_WRITER.with(|slot| {
            let slot = slot.borrow();
            let writer = slot.as_ref().unwrap().writers.writers["business_commands"]
                .as_ref()
                .unwrap();
            assert!(writer.columns.contains("generation_probe"));
            assert!(writer.conn.is_autocommit());
        });
        Ok(())
    }

    #[test]
    fn control_writer_discards_errors_and_reads_external_record_revision() -> anyhow::Result<()> {
        let root = tempdir()?;
        let conn = create_commands(root.path())?;
        write(root.path(), "one")?;
        conn.execute_batch(&format!(
            "CREATE TRIGGER deny_projection BEFORE INSERT ON {} BEGIN
                SELECT RAISE(ABORT, 'injected projection failure'); END;",
            table()
        ))?;
        assert!(write(root.path(), "denied").is_err());
        assert_eq!(rows(&conn)?, 1);
        assert!(CONTROL_WRITER.with(|slot| slot.borrow().is_none()));
        conn.execute_batch("DROP TRIGGER deny_projection")?;
        // Use a different connection while the cached writer is idle.
        write(root.path(), "one")?;
        conn.execute(
            &format!(
                "UPDATE {} SET data = ?1, revision = '7-external' WHERE id = 'one'",
                table()
            ),
            [
                serde_json::json!({"id": "one", "_rev": "7-external", "external": "preserved"})
                    .to_string(),
            ],
        )?;
        write(root.path(), "one")?;
        let raw: String = conn.query_row(
            &format!("SELECT data FROM {} WHERE id = 'one'", table()),
            [],
            |row| row.get(0),
        )?;
        let record: Value = serde_json::from_str(&raw)?;
        assert_eq!(record["external"], "preserved");
        assert!(record["_rev"].as_str().unwrap().starts_with("8-"));
        Ok(())
    }

    #[test]
    fn control_writer_nested_roots_and_panic_do_not_poison_thread_slot() -> anyhow::Result<()> {
        let root = tempdir()?;
        let other = tempdir()?;
        let conn = create_commands(root.path())?;
        let other_conn = create_commands(other.path())?;
        with_control_projection_writers(root.path(), |writers| {
            writers.upsert_control("business_commands", "outer", 1000, serde_json::json!({}))?;
            write(other.path(), "nested")?;
            writers.upsert_control(
                "business_commands",
                "outer-two",
                1001,
                serde_json::json!({}),
            )
        })?;
        assert_eq!(rows(&conn)?, 2);
        assert_eq!(rows(&other_conn)?, 1);
        let panic = std::panic::catch_unwind(|| {
            let _ = with_control_projection_writers(root.path(), |_| -> anyhow::Result<()> {
                panic!("injected control scope panic");
            });
        });
        assert!(panic.is_err());
        assert!(!CONTROL_WRITER_IN_USE.with(|active| active.get()));
        assert!(CONTROL_WRITER.with(|slot| slot.borrow().is_none()));
        write(root.path(), "after-panic")?;
        assert_eq!(rows(&conn)?, 3);
        Ok(())
    }
}
