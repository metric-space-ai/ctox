// Origin: CTOX
// License: AGPL-3.0-only
//! Native-only durable build envelopes. Callers supply authenticated owner identity.
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LEASE_MS: i64 = 300_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildJob {
    pub owner: String,
    pub job_id: String,
    pub admission: Value,
    pub progress: Value,
    pub state: String,
    pub revision: i64,
    pub lease_generation: i64,
    pub claimant: Option<String>,
    pub lease_until_ms: Option<i64>,
}

/// A capability for exactly one claim generation. Keep private to the launcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJobLease {
    pub owner: String,
    pub job_id: String,
    pub generation: i64,
    pub claimant: String,
}

pub struct BuildJobStore {
    conn: Connection,
}

impl BuildJobStore {
    pub fn open(root: &Path) -> Result<Self> {
        Self::from_connection(super::store::open_store(root)?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS native_build_jobs (
                owner TEXT NOT NULL, job_id TEXT NOT NULL,
                admission_json TEXT NOT NULL, progress_json TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('admitted','running','succeeded','failed','cancelled')),
                revision INTEGER NOT NULL CHECK(revision >= 0),
                lease_generation INTEGER NOT NULL CHECK(lease_generation >= 0),
                claimant TEXT, lease_until_ms INTEGER,
                PRIMARY KEY(owner, job_id),
                CHECK((state = 'running' AND claimant IS NOT NULL AND lease_until_ms IS NOT NULL)
                   OR (state <> 'running' AND claimant IS NULL AND lease_until_ms IS NULL))
             );
             CREATE TRIGGER IF NOT EXISTS native_build_jobs_admission_immutable
             BEFORE UPDATE OF owner, job_id, admission_json ON native_build_jobs
             BEGIN SELECT RAISE(ABORT, 'immutable build admission'); END;",
        )?;
        Ok(Self { conn })
    }

    /// Duplicate identity is rejected, even if the admission payload matches.
    pub fn create(&self, owner: &str, job_id: &str, admission: &Value) -> Result<BuildJob> {
        identity(owner, job_id)?;
        self.conn.execute(
            "INSERT INTO native_build_jobs
             (owner,job_id,admission_json,progress_json,state,revision,lease_generation)
             VALUES (?1,?2,?3,'{}','admitted',0,0)",
            params![owner, job_id, serde_json::to_string(admission)?],
        )?;
        self.read(owner, job_id)?
            .ok_or_else(|| anyhow::anyhow!("build job missing"))
    }

    pub fn read(&self, owner: &str, job_id: &str) -> Result<Option<BuildJob>> {
        identity(owner, job_id)?;
        read(&self.conn, owner, job_id)
    }

    /// Bounded deterministic pagination. Never lists another owner's records.
    pub fn list(&self, owner: &str, after_job_id: &str, limit: u32) -> Result<Vec<BuildJob>> {
        ensure!(!owner.trim().is_empty(), "owner required");
        ensure!((1..=1000).contains(&limit), "list limit must be 1..=1000");
        let mut stmt = self.conn.prepare(
            "SELECT owner,job_id,admission_json,progress_json,state,revision,
                    lease_generation,claimant,lease_until_ms FROM native_build_jobs
             WHERE owner=?1 AND job_id>?2 ORDER BY job_id LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![owner, after_job_id, limit], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn claim(
        &mut self,
        owner: &str,
        job_id: &str,
        revision: i64,
        lease_ms: i64,
    ) -> Result<(BuildJob, BuildJobLease)> {
        self.claim_at(owner, job_id, revision, lease_ms, now_ms()?)
    }

    fn claim_at(
        &mut self,
        owner: &str,
        job_id: &str,
        revision: i64,
        lease_ms: i64,
        now: i64,
    ) -> Result<(BuildJob, BuildJobLease)> {
        identity(owner, job_id)?;
        let until = deadline(now, lease_ms)?;
        let claimant = uuid::Uuid::new_v4().to_string();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE native_build_jobs SET state='running',revision=revision+1,
             lease_generation=lease_generation+1,claimant=?4,lease_until_ms=?5
             WHERE owner=?1 AND job_id=?2 AND revision=?3
             AND (state='admitted' OR (state='running' AND lease_until_ms<=?6))",
            params![owner, job_id, revision, claimant, until, now],
        )?;
        ensure!(changed == 1, "build claim unavailable or stale revision");
        let job = read(&tx, owner, job_id)?.ok_or_else(|| anyhow::anyhow!("build job missing"))?;
        tx.commit()?;
        let lease = BuildJobLease {
            owner: owner.into(),
            job_id: job_id.into(),
            generation: job.lease_generation,
            claimant,
        };
        Ok((job, lease))
    }

    /// Atomic checkpoint and lease renewal. Expected revision is the last returned revision.
    pub fn checkpoint(
        &mut self,
        lease: &BuildJobLease,
        revision: i64,
        progress: &Value,
        lease_ms: i64,
    ) -> Result<BuildJob> {
        self.write_at(lease, revision, progress, None, lease_ms, now_ms()?)
    }

    /// Terminal completion releases the lease and permanently excludes future claims.
    pub fn complete(
        &mut self,
        lease: &BuildJobLease,
        revision: i64,
        progress: &Value,
        terminal: &str,
    ) -> Result<BuildJob> {
        ensure!(
            matches!(terminal, "succeeded" | "failed" | "cancelled"),
            "invalid terminal state"
        );
        self.write_at(lease, revision, progress, Some(terminal), 0, now_ms()?)
    }

    fn write_at(
        &mut self,
        lease: &BuildJobLease,
        revision: i64,
        progress: &Value,
        terminal: Option<&str>,
        lease_ms: i64,
        now: i64,
    ) -> Result<BuildJob> {
        identity(&lease.owner, &lease.job_id)?;
        let until = if terminal.is_some() {
            None
        } else {
            Some(deadline(now, lease_ms)?)
        };
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE native_build_jobs SET progress_json=?6,revision=revision+1,
             state=COALESCE(?7,'running'),claimant=CASE WHEN ?7 IS NULL THEN claimant ELSE NULL END,
             lease_until_ms=?8 WHERE owner=?1 AND job_id=?2 AND revision=?3
             AND lease_generation=?4 AND claimant=?5 AND state='running' AND lease_until_ms>?9",
            params![
                lease.owner,
                lease.job_id,
                revision,
                lease.generation,
                lease.claimant,
                serde_json::to_string(progress)?,
                terminal,
                until,
                now
            ],
        )?;
        ensure!(
            changed == 1,
            "build lease expired, fenced, or stale revision"
        );
        let job = read(&tx, &lease.owner, &lease.job_id)?
            .ok_or_else(|| anyhow::anyhow!("build job missing"))?;
        tx.commit()?;
        Ok(job)
    }
}

fn identity(owner: &str, job_id: &str) -> Result<()> {
    ensure!(
        !owner.trim().is_empty() && !job_id.trim().is_empty(),
        "owner and job identity required"
    );
    Ok(())
}
fn deadline(now: i64, lease_ms: i64) -> Result<i64> {
    ensure!(
        (1..=MAX_LEASE_MS).contains(&lease_ms),
        "lease must be 1..=300000 ms"
    );
    now.checked_add(lease_ms)
        .ok_or_else(|| anyhow::anyhow!("lease timestamp overflow"))
}
fn now_ms() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BuildJob> {
    fn json(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Value> {
        let text: String = row.get(index)?;
        serde_json::from_str(&text).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })
    }
    Ok(BuildJob {
        owner: row.get(0)?,
        job_id: row.get(1)?,
        admission: json(row, 2)?,
        progress: json(row, 3)?,
        state: row.get(4)?,
        revision: row.get(5)?,
        lease_generation: row.get(6)?,
        claimant: row.get(7)?,
        lease_until_ms: row.get(8)?,
    })
}
fn read(conn: &Connection, owner: &str, job_id: &str) -> Result<Option<BuildJob>> {
    Ok(conn
        .query_row(
            "SELECT owner,job_id,admission_json,progress_json,state,revision,lease_generation,
         claimant,lease_until_ms FROM native_build_jobs WHERE owner=?1 AND job_id=?2",
            params![owner, job_id],
            row,
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn open(path: &Path) -> BuildJobStore {
        BuildJobStore::from_connection(Connection::open(path).unwrap()).unwrap()
    }
    #[test]
    fn sqlite_reopen_fences_expired_claim_and_terminal_job() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs.sqlite3");
        let mut store = open(&path);
        store
            .create("alice", "job", &json!({"recipe":"immutable"}))
            .unwrap();
        assert!(store.create("alice", "job", &json!({})).is_err());
        let (first, old) = store.claim_at("alice", "job", 0, 10, 100).unwrap();
        drop(store);
        let mut store = open(&path);
        assert_eq!(store.read("alice", "job").unwrap().unwrap(), first);
        assert!(store.read("bob", "job").unwrap().is_none());
        assert!(store.list("bob", "", 100).unwrap().is_empty());
        assert!(store.claim_at("bob", "job", 1, 10, 111).is_err());
        assert!(store.claim_at("alice", "job", 1, 10, 109).is_err());
        assert!(store.write_at(&old, 1, &json!({}), None, 10, 110).is_err());
        let (second, current) = store.claim_at("alice", "job", 1, 10, 110).unwrap();
        assert!(second.lease_generation > old.generation);
        assert!(store.write_at(&old, 2, &json!({}), None, 10, 111).is_err());
        let checkpoint = store
            .write_at(&current, 2, &json!({"offset":42}), None, 20, 111)
            .unwrap();
        assert!(store
            .write_at(&current, 2, &json!({}), None, 20, 112)
            .is_err());
        let done = store
            .write_at(
                &current,
                checkpoint.revision,
                &checkpoint.progress,
                Some("succeeded"),
                0,
                112,
            )
            .unwrap();
        assert!(store
            .claim_at("alice", "job", done.revision, 10, 999)
            .is_err());
        drop(store);
        let store = open(&path);
        assert_eq!(store.read("alice", "job").unwrap().unwrap(), done);
        assert_eq!(store.list("alice", "", 1).unwrap(), vec![done]);
        assert!(store
            .conn
            .execute("UPDATE native_build_jobs SET admission_json='{}'", [])
            .is_err());
    }
    #[test]
    fn independent_sqlite_connections_allow_only_one_concurrent_claim() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs.sqlite3");
        open(&path).create("alice", "job", &json!({})).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut store = open(&path);
                    barrier.wait();
                    store.claim_at("alice", "job", 0, 100, 10).is_ok()
                })
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().expect("claim thread panicked"))
                .filter(|ok| *ok)
                .count(),
            1
        );
        assert_eq!(
            open(&path).read("alice", "job").unwrap().unwrap().revision,
            1
        );
    }
    #[test]
    fn bounded_leases_and_owner_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = open(&dir.path().join("jobs.sqlite3"));
        assert!(store.create("", "job", &json!({})).is_err());
        store.create("alice", "job", &json!({})).unwrap();
        store
            .create("bob", "job", &json!({"different":true}))
            .unwrap();
        assert!(store.claim_at("alice", "job", 0, 0, 10).is_err());
        assert!(store
            .claim_at("alice", "job", 0, MAX_LEASE_MS + 1, 10)
            .is_err());
        let (_, lease) = store.claim_at("alice", "job", 0, 10, 10).unwrap();
        let mut wrong_owner = lease.clone();
        wrong_owner.owner = "bob".into();
        assert!(store
            .write_at(&wrong_owner, 1, &json!({}), None, 10, 11)
            .is_err());
        assert_eq!(store.read("bob", "job").unwrap().unwrap().state, "admitted");
    }
}
