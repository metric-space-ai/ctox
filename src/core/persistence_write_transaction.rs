//! Separate SQLite writer acquisition from lock ownership in diagnostics.
//! Payloads and SQL are never logged. A log sink runs only after release.
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::ops::Deref;
use std::time::{Duration, Instant};

pub(crate) struct SqliteWriteTransaction<'conn> {
    transaction: Option<Transaction<'conn>>,
    operation: &'static str,
    database: String,
    wait: Duration,
    acquired: Instant,
    outcome: &'static str,
}

impl<'conn> SqliteWriteTransaction<'conn> {
    pub(crate) fn begin(
        conn: &'conn Connection,
        operation: &'static str,
    ) -> rusqlite::Result<Self> {
        let started = Instant::now();
        let transaction = Transaction::new_unchecked(conn, TransactionBehavior::Immediate);
        let wait = started.elapsed();
        match transaction {
            Ok(transaction) => Ok(Self {
                transaction: Some(transaction),
                operation,
                database: conn.path().unwrap_or(":memory:").to_string(),
                wait,
                acquired: Instant::now(),
                outcome: "rollback",
            }),
            Err(error) => {
                if wait >= Duration::from_millis(50) {
                    eprintln!("[ctox sqlite writer] operation={operation} database={} wait_us={} hold_us=0 outcome=acquire_failed code={:?}",
                        conn.path().unwrap_or(":memory:"), wait.as_micros(), error.sqlite_error_code());
                }
                Err(error)
            }
        }
    }

    pub(crate) fn commit(mut self) -> rusqlite::Result<()> {
        let result = self
            .transaction
            .take()
            .expect("active transaction")
            .commit();
        self.outcome = if result.is_ok() {
            "commit"
        } else {
            "commit_failed"
        };
        result
    }
}

impl<'conn> Deref for SqliteWriteTransaction<'conn> {
    type Target = Transaction<'conn>;
    fn deref(&self) -> &Self::Target {
        self.transaction.as_ref().expect("active transaction")
    }
}

impl Drop for SqliteWriteTransaction<'_> {
    fn drop(&mut self) {
        // Roll back first on every early-return/unwind path. A blocked log sink
        // must never extend SQLite's lock window.
        drop(self.transaction.take());
        let hold = self.acquired.elapsed();
        if self.wait >= Duration::from_millis(50) || hold >= Duration::from_millis(50) {
            eprintln!(
                "[ctox sqlite writer] operation={} database={} wait_us={} hold_us={} outcome={}",
                self.operation,
                self.database,
                self.wait.as_micros(),
                hold.as_micros(),
                self.outcome
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn early_return_releases_writer_and_rolls_back() -> rusqlite::Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("writer.sqlite3");
        let first = Connection::open(&path)?;
        first.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE evidence(id INTEGER)")?;
        let second = Connection::open(&path)?;
        second.busy_timeout(Duration::ZERO)?;
        {
            let tx = SqliteWriteTransaction::begin(&first, "test.rollback")?;
            tx.execute("INSERT INTO evidence VALUES(1)", [])?;
            assert!(SqliteWriteTransaction::begin(&second, "test.competing").is_err());
        }
        let tx = SqliteWriteTransaction::begin(&second, "test.after_release")?;
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM evidence", [], |r| r.get::<_, i64>(0))?,
            0
        );
        tx.execute("INSERT INTO evidence VALUES(2)", [])?;
        tx.commit()?;
        assert_eq!(
            first.query_row("SELECT id FROM evidence", [], |r| r.get::<_, i64>(0))?,
            2
        );
        Ok(())
    }
}
