use super::{queue_turn_store_identity, resolve_db_path, QueueTurnLeaseFence};
use anyhow::{ensure, Context, Result};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Retained only by the actual service worker. Clones cannot outlive revocation
/// as authority: Drop/shutdown waits for an in-flight bounded callback, then
/// revokes all previously issued execution fences under this same lock.
#[derive(Debug)]
pub(crate) struct QueueWorkerLifetime {
    root: PathBuf,
    message_keys: Vec<String>,
    worker_id: Option<String>,
    state: Mutex<WorkerLifetimeState>,
}

#[derive(Debug, Default)]
struct WorkerLifetimeState {
    revoked: bool,
    attempt_id: Option<String>,
}

impl QueueWorkerLifetime {
    pub(crate) fn for_native_worker(
        root: &std::path::Path,
        message_keys: &[String],
        worker_id: Option<&str>,
    ) -> Self {
        Self {
            root: root.to_owned(),
            message_keys: message_keys.to_vec(),
            worker_id: worker_id.map(str::to_owned),
            state: Mutex::new(WorkerLifetimeState::default()),
        }
    }

    pub(crate) fn revoke(&self) {
        // Poison is denial too; do not revive a worker after a callback panic.
        if let Ok(mut state) = self.state.lock() {
            state.revoked = true;
        }
    }
}

/// A native queue execution binding, not a Raft job/session or guest permit.
/// Created after real worker admission, with the service's attempt and lease
/// identity. A guest owner must separately resolve its actual authenticated
/// ExecutionSpec/session to this exact binding; client strings cannot do so.
#[derive(Debug, Clone)]
pub(crate) struct QueueExecutionFence {
    root: PathBuf,
    worker_id: String,
    attempt_id: String,
    rows: Vec<(String, i64)>,
    lifetime: Arc<QueueWorkerLifetime>,
    identity: (u64, u64),
}

fn current_attempt(conn: &Connection, key: &str, worker: &str) -> Result<i64> {
    let (attempt, expiry): (i64, String) = conn
        .query_row(
            "SELECT attempt, lease_expires_at FROM communication_routing_state
             WHERE message_key=?1 AND route_status='leased'
             AND lease_owner='ctox-service' AND lease_worker_id=?2",
            rusqlite::params![key, worker],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("native execution has no exact live worker lease")?;
    ensure!(
        attempt > 0 && chrono::DateTime::parse_from_rfc3339(&expiry)? > chrono::Utc::now(),
        "native execution lease is expired or has no admitted attempt"
    );
    Ok(attempt)
}

impl QueueExecutionFence {
    pub(crate) fn capture(
        fence: &QueueTurnLeaseFence,
        attempt_id: &str,
        lifetime: Arc<QueueWorkerLifetime>,
    ) -> Result<Self> {
        ensure!(
            !attempt_id.trim().is_empty()
                && !fence.worker_id.trim().is_empty()
                && !fence.message_keys.is_empty(),
            "native execution is missing its admitted attempt or worker"
        );
        ensure!(
            lifetime.root == fence.root
                && lifetime.message_keys == fence.message_keys
                && lifetime.worker_id.as_deref() == Some(fence.worker_id.as_str()),
            "native execution does not match its service-owned worker scope"
        );
        let mut state = lifetime
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native execution lifetime is poisoned"))?;
        ensure!(!state.revoked, "native execution lifetime is revoked");
        ensure!(
            state
                .attempt_id
                .as_deref()
                .is_none_or(|id| id == attempt_id),
            "native worker cannot adopt a different execution attempt"
        );
        let path = resolve_db_path(&fence.root, None);
        let identity = queue_turn_store_identity(&path)?;
        let mut conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(std::time::Duration::from_millis(100))?;
        let tx = conn.transaction()?;
        let rows = fence
            .message_keys
            .iter()
            .map(|key| {
                current_attempt(&tx, key, &fence.worker_id).map(|attempt| (key.clone(), attempt))
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit()?;
        ensure!(
            queue_turn_store_identity(&path)? == identity,
            "native execution store changed during admission"
        );
        state.attempt_id = Some(attempt_id.to_owned());
        drop(state);
        Ok(Self {
            root: fence.root.clone(),
            worker_id: fence.worker_id.clone(),
            attempt_id: attempt_id.to_owned(),
            rows,
            lifetime,
            identity,
        })
    }

    pub(crate) fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    pub(crate) fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub(crate) fn is_live(&self) -> bool {
        self.lifetime.state.lock().is_ok_and(|state| {
            !state.revoked && state.attempt_id.as_deref() == Some(&self.attempt_id)
        })
    }

    pub(crate) fn matches_current_rows(&self, conn: &Connection) -> bool {
        self.is_live()
            && self.rows.iter().all(|(key, attempt)| {
                current_attempt(conn, key, &self.worker_id).is_ok_and(|current| current == *attempt)
            })
    }

    /// The native execution guard is outermost; the callback may acquire the
    /// retained guest controller guard, never the reverse. SQLite's IMMEDIATE
    /// transaction serializes supported lease cancellation/reclaim/updates,
    /// and the lifetime lock serializes actual worker teardown. Check expiry
    /// at the operation's linearization point, not as a reusable future permit.
    /// The callback must be bounded and synchronous, and must not re-enter this
    /// channel database. A vanished/replaced store is never recreated.
    pub(crate) fn with_current_execution<T>(
        &self,
        publish: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.with_current_transaction(|_| publish())
    }

    pub(super) fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub(super) fn routing_attempts(&self) -> &[(String, i64)] {
        &self.rows
    }

    pub(super) fn with_current_transaction<T>(
        &self,
        publish: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let state = self
            .lifetime
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native execution lifetime is poisoned"))?;
        ensure!(
            !state.revoked && state.attempt_id.as_deref() == Some(&self.attempt_id),
            "native execution lifetime is revoked or replaced"
        );
        let path = resolve_db_path(&self.root, None);
        ensure!(
            queue_turn_store_identity(&path)? == self.identity,
            "native execution store was replaced"
        );
        let mut conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(std::time::Duration::from_millis(100))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            queue_turn_store_identity(&path)? == self.identity,
            "native execution store changed before publication"
        );
        for (key, expected_attempt) in &self.rows {
            ensure!(
                current_attempt(&tx, key, &self.worker_id)? == *expected_attempt,
                "native execution routing attempt was replaced"
            );
        }
        let result = publish(&tx)?;
        // Unexpected out-of-band filesystem replacement is an uncertain effect,
        // not a successful publication receipt. The caller must reconcile it.
        ensure!(
            queue_turn_store_identity(&path)? == self.identity,
            "native execution store changed during publication; reconcile effect"
        );
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    // Direct SQL below changes only isolated native lease fixtures.
    // ctox-allow-direct-state-write: test fixture module
    use super::*;
    use crate::channels::{
        create_queue_task, lease_queue_task, record_queue_lease_worker, update_queue_task,
        QueueTaskCreateRequest, QueueTaskUpdateRequest,
    };

    fn admitted() -> Result<(
        tempfile::TempDir,
        QueueTurnLeaseFence,
        Arc<QueueWorkerLifetime>,
    )> {
        let root = tempfile::tempdir()?;
        let task = create_queue_task(
            root.path(),
            QueueTaskCreateRequest {
                title: "retained native execution".into(),
                prompt: "retained native execution".into(),
                thread_key: "queue/execution-fence".into(),
                workspace_root: None,
                priority: "normal".into(),
                suggested_skill: None,
                parent_message_key: None,
                extra_metadata: None,
            },
        )?;
        lease_queue_task(root.path(), &task.message_key, "ctox-service")?;
        record_queue_lease_worker(
            root.path(),
            &[task.message_key.clone()],
            "ctox-service",
            "native-worker",
        )?;
        let fence = QueueTurnLeaseFence {
            root: root.path().to_owned(),
            message_keys: vec![task.message_key],
            worker_id: "native-worker".into(),
            execution: None,
        };
        let lifetime = Arc::new(QueueWorkerLifetime::for_native_worker(
            root.path(),
            &fence.message_keys,
            Some(&fence.worker_id),
        ));
        Ok((root, fence, lifetime))
    }

    #[test]
    fn queue_execution_guard_rejects_replaced_cancelled_and_expired_leases() -> Result<()> {
        for mutation in [
            "route_status='cancelled'",
            "route_status='pending'",
            "lease_owner='foreign-worker'",
            "lease_worker_id='replacement-worker'",
            "attempt=attempt+1",
            "lease_expires_at='2000-01-01T00:00:00Z'",
            "lease_expires_at='not-a-date'",
        ] {
            let (_root, fence, lifetime) = admitted()?;
            let guard = QueueExecutionFence::capture(&fence, "native-attempt", lifetime)?;
            assert_eq!(guard.attempt_id(), "native-attempt");
            assert_eq!(guard.worker_id(), "native-worker");
            assert_eq!(guard.with_current_execution(|| Ok(17))?, 17);
            let conn = Connection::open(resolve_db_path(&fence.root, None))?;
            conn.execute(
                &format!("UPDATE communication_routing_state SET {mutation} WHERE message_key=?1"),
                [&fence.message_keys[0]],
            )?;
            let mut invoked = false;
            assert!(
                guard
                    .with_current_execution(|| {
                        invoked = true;
                        Ok(())
                    })
                    .is_err(),
                "{mutation}"
            );
            assert!(!invoked, "{mutation}");
            assert!(!guard.matches_current_rows(&conn), "{mutation}");
        }
        Ok(())
    }

    #[test]
    fn queue_execution_guard_rejects_released_and_released_again_by_native_api() -> Result<()> {
        let (root, fence, lifetime) = admitted()?;
        let old = QueueExecutionFence::capture(&fence, "native-attempt", lifetime)?;
        let first_attempt = old.routing_attempts()[0].1;
        update_queue_task(
            root.path(),
            QueueTaskUpdateRequest {
                message_key: fence.message_keys[0].clone(),
                route_status: Some("pending".into()),
                ..Default::default()
            },
        )?;
        lease_queue_task(root.path(), &fence.message_keys[0], "ctox-service")?;
        // Reuse even the worker ID: the persisted lease epoch must still deny
        // the retained old consumer without relying on worker teardown.
        record_queue_lease_worker(
            root.path(),
            &fence.message_keys,
            "ctox-service",
            &fence.worker_id,
        )?;
        let next_lifetime = Arc::new(QueueWorkerLifetime::for_native_worker(
            root.path(),
            &fence.message_keys,
            Some(&fence.worker_id),
        ));
        let next = QueueExecutionFence::capture(&fence, "next-native-attempt", next_lifetime)?;
        assert_eq!(next.routing_attempts()[0].1, first_attempt + 1);
        let mut published = false;
        assert!(old
            .with_current_execution(|| {
                published = true;
                Ok(())
            })
            .is_err());
        assert!(!published);
        assert_eq!(next.with_current_execution(|| Ok(17))?, 17);
        Ok(())
    }

    #[test]
    fn queue_execution_guard_serializes_cancellation_and_worker_teardown() -> Result<()> {
        let (_root, fence, lifetime) = admitted()?;
        let guard = QueueExecutionFence::capture(&fence, "native-attempt", Arc::clone(&lifetime))?;
        let path = resolve_db_path(&fence.root, None);
        guard.with_current_execution(|| {
            // A real competing native store writer cannot cancel/reclaim under
            // the retained callback. Its transition is serialized after this.
            let conn = Connection::open(&path)?;
            conn.busy_timeout(std::time::Duration::ZERO)?;
            let error = conn
                .execute(
                    "UPDATE communication_routing_state SET route_status='cancelled'
                 WHERE message_key=?1",
                    [&fence.message_keys[0]],
                )
                .unwrap_err();
            assert_eq!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy)
            );
            assert!(lifetime.state.try_lock().is_err());
            Ok(())
        })?;
        update_queue_task(
            &fence.root,
            QueueTaskUpdateRequest {
                message_key: fence.message_keys[0].clone(),
                route_status: Some("cancelled".into()),
                ..Default::default()
            },
        )?;
        assert!(guard.with_current_execution(|| Ok(())).is_err());

        let (_root, fence, lifetime) = admitted()?;
        let guard = QueueExecutionFence::capture(&fence, "native-attempt", Arc::clone(&lifetime))?;
        // Supported teardown uses the same lock. A retained clone is revoked.
        lifetime.revoke();
        assert!(!guard.is_live());
        assert!(guard
            .with_current_execution::<()>(|| panic!("revoked callback"))
            .is_err());
        assert!(QueueExecutionFence::capture(&fence, "native-attempt", lifetime).is_err());
        Ok(())
    }

    #[test]
    fn queue_execution_guard_rejects_store_replacement_and_missing_admission() -> Result<()> {
        let (_root, fence, lifetime) = admitted()?;
        let guard = QueueExecutionFence::capture(&fence, "native-attempt", Arc::clone(&lifetime))?;
        assert!(QueueExecutionFence::capture(&fence, "different-attempt", lifetime).is_err());
        let path = resolve_db_path(&fence.root, None);
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let retained = path.with_extension("retained-native-store");
        std::fs::rename(&path, &retained)?;
        std::fs::copy(&retained, &path)?;
        assert!(guard
            .with_current_execution::<()>(|| panic!("replaced store callback"))
            .is_err());
        drop(conn);

        let missing = tempfile::tempdir()?;
        let fence = QueueTurnLeaseFence {
            root: missing.path().to_owned(),
            message_keys: vec!["queue:system::missing".into()],
            worker_id: "native-worker".into(),
            execution: None,
        };
        let path = resolve_db_path(&fence.root, None);
        assert!(QueueExecutionFence::capture(
            &fence,
            "native-attempt",
            Arc::new(QueueWorkerLifetime::for_native_worker(
                &fence.root,
                &fence.message_keys,
                Some(&fence.worker_id)
            ))
        )
        .is_err());
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn queue_execution_guard_cannot_adopt_another_native_worker_scope() -> Result<()> {
        let (_root, fence, lifetime) = admitted()?;
        let guard = QueueExecutionFence::capture(&fence, "native-attempt", Arc::clone(&lifetime))?;
        let (_other_root, other_fence, _) = admitted()?;
        // Both roots have a genuinely live lease and deliberately equal worker
        // and attempt strings. Their native admissions still cannot alias.
        assert!(QueueExecutionFence::capture(
            &other_fence,
            "native-attempt",
            Arc::clone(&lifetime),
        )
        .is_err());
        assert_eq!(guard.with_current_execution(|| Ok(31))?, 31);
        Ok(())
    }

    #[test]
    fn queue_execution_guard_callback_error_does_not_revoke_the_worker() -> Result<()> {
        let (_root, mut fence, lifetime) = admitted()?;
        let guard = QueueExecutionFence::capture(&fence, "native-attempt", Arc::clone(&lifetime))?;
        assert!(guard
            .with_current_execution::<()>(|| anyhow::bail!("controller denied"))
            .is_err());
        assert_eq!(guard.with_current_execution(|| Ok(29))?, 29);
        let reader = fence.open_reader()?;
        fence.execution = Some(guard);
        assert!(fence.still_owned(&reader)?);
        lifetime.revoke();
        assert!(!fence.still_owned(&reader)?);
        Ok(())
    }
}
