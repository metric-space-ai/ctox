// Origin: CTOX
// License: Apache-2.0
//! Native execution identity, allocated before execution, separate from finalization.
use super::*;

pub struct WorkerRunInput<'a> {
    pub attempt_id: &'a str,
    pub work_key: &'a str,
    pub conversation_id: i64,
    pub source_label: &'a str,
    pub task_ids: &'a [String],
}

impl LcmEngine {
    pub(super) fn ensure_worker_run_identity_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS worker_run_identities (
                attempt_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL UNIQUE,
                work_key TEXT NOT NULL,
                conversation_id INTEGER NOT NULL,
                source_label TEXT NOT NULL,
                task_ids_json TEXT NOT NULL CHECK(json_valid(task_ids_json)),
                created_at TEXT NOT NULL
             );",
        )?;
        Ok(())
    }

    /// Called only on the admitted execution branch, never finalization recovery.
    /// Replaying the same exact native binding returns its existing random ID.
    pub fn register_worker_run(&self, input: WorkerRunInput<'_>) -> Result<String> {
        for value in [input.attempt_id, input.work_key, input.source_label] {
            anyhow::ensure!(!value.trim().is_empty() && value.len() <= 4096,
                "invalid native worker run binding");
        }
        anyhow::ensure!(input.task_ids.len() <= 1024, "native worker run task window too large");
        let mut task_ids = input.task_ids.to_vec();
        task_ids.sort();
        task_ids.dedup();
        anyhow::ensure!(task_ids.iter().all(|id| !id.trim().is_empty() && id.len() <= 4096),
            "invalid native worker run task binding");
        let task_ids_json = serde_json::to_string(&task_ids)?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let existing: Option<(String, String, i64, String, String)> = tx.query_row(
            "SELECT run_id,work_key,conversation_id,source_label,task_ids_json
             FROM worker_run_identities WHERE attempt_id=?1",
            [input.attempt_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ).optional()?;
        if let Some((run_id, work, conversation, source, tasks)) = existing {
            anyhow::ensure!(work == input.work_key && conversation == input.conversation_id
                && source == input.source_label && tasks == task_ids_json,
                "native worker run identity binding conflict");
            tx.commit()?;
            return Ok(run_id);
        }
        // A legacy finalization already has its own canonical ctox_runs key.
        // Recovery must not mint a replacement identity or invoke a model again.
        let finalized: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM worker_attempt_finalizations WHERE attempt_id=?1)",
            [input.attempt_id], |row| row.get(0),
        )?;
        anyhow::ensure!(!finalized, "cannot allocate a run after native finalization began");
        let run_id = format!("worker-run:{}", uuid::Uuid::new_v4());
        tx.execute(
            "INSERT INTO worker_run_identities
                (attempt_id,run_id,work_key,conversation_id,source_label,task_ids_json,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![input.attempt_id,run_id,input.work_key,input.conversation_id,
                input.source_label,task_ids_json,chrono::Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
        Ok(run_id)
    }
}

pub fn run_register_worker_run(db_path: &Path, input: WorkerRunInput<'_>) -> Result<String> {
    LcmEngine::open(db_path, LcmConfig::default())?.register_worker_run(input)
}

/// Projection compatibility: old finalization keys remain unchanged. New runs
/// retain the identity issued at execution admission through terminal projection.
pub(crate) fn projected_worker_run_id(conn: &Connection, attempt_id: &str) -> Result<String> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='worker_run_identities')",
        [], |row| row.get(0),
    )?;
    if !exists { return Ok(attempt_id.to_owned()); }
    Ok(conn.query_row("SELECT run_id FROM worker_run_identities WHERE attempt_id=?1",
        [attempt_id], |row| row.get(0)).optional()?.unwrap_or_else(|| attempt_id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input<'a>(attempt: &'a str, tasks: &'a [String]) -> WorkerRunInput<'a> {
        WorkerRunInput { attempt_id:attempt, work_key:"work", conversation_id:42,
            source_label:"queue", task_ids:tasks }
    }
    #[test]
    fn active_run_identity_survives_reopen_and_is_independent_of_attempt_and_task() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db = temp.path().join("core.sqlite3");
        let tasks = vec!["task-b".into(), "task-a".into()];
        let run = run_register_worker_run(&db, input("attempt", &tasks))?;
        assert!(uuid::Uuid::parse_str(run.strip_prefix("worker-run:").unwrap()).is_ok());
        assert_ne!(run, "attempt");
        assert!(!tasks.contains(&run));
        let reversed = vec!["task-a".into(), "task-b".into()];
        assert_eq!(run, run_register_worker_run(&db, input("attempt", &reversed))?);
        assert_ne!(run, run_register_worker_run(&db, input("attempt-2", &tasks))?);
        let conn = Connection::open(&db)?;
        assert_eq!(projected_worker_run_id(&conn, "attempt")?, run);
        let count: i64 = conn.query_row("SELECT count(*) FROM worker_attempt_finalizations",[],|r|r.get(0))?;
        assert_eq!(count, 0, "identity admission must not fabricate finalization");
        Ok(())
    }
    #[test]
    fn run_replay_rejects_changed_native_task_work_conversation_or_source_binding() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db = temp.path().join("core.sqlite3");
        let tasks = vec!["task".into()];
        run_register_worker_run(&db, input("attempt", &tasks))?;
        for field in 0..4 {
            let foreign = vec!["foreign-task".into()];
            let mut changed = input("attempt", &tasks);
            match field {
                0 => changed.task_ids = &foreign,
                1 => changed.work_key = "other-work",
                2 => changed.conversation_id = 99,
                _ => changed.source_label = "other-source",
            }
            assert!(run_register_worker_run(&db, changed).is_err());
        }
        Ok(())
    }
    #[test]
    fn legacy_finalization_cannot_allocate_a_new_execution_run() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db = temp.path().join("core.sqlite3");
        let engine = LcmEngine::open(&db, LcmConfig::default())?;
        engine.begin_worker_attempt_finalization(WorkerAttemptFinalizationInput {
            attempt_id:"legacy",work_key:"work",conversation_id:42,source_label:"queue",
            agent_outcome:AgentOutcome::Success,reply_text:"saved",error_text:None,
        })?;
        assert!(engine.register_worker_run(input("legacy", &[])).is_err());
        assert_eq!(projected_worker_run_id(&engine.conn, "legacy")?, "legacy");
        Ok(())
    }
    #[test]
    fn run_identity_remains_stable_when_finalization_is_persisted() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db = temp.path().join("core.sqlite3");
        let engine = LcmEngine::open(&db, LcmConfig::default())?;
        let tasks = vec!["task".into()];
        let run = engine.register_worker_run(input("attempt", &tasks))?;
        engine.begin_worker_attempt_finalization(WorkerAttemptFinalizationInput {
            attempt_id:"attempt",work_key:"work",conversation_id:42,source_label:"queue",
            agent_outcome:AgentOutcome::Success,reply_text:"saved",error_text:None,
        })?;
        assert_eq!(engine.register_worker_run(input("attempt", &tasks))?, run);
        assert_eq!(projected_worker_run_id(&engine.conn, "attempt")?, run);
        Ok(())
    }
}
