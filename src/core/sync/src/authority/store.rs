//! Durable Raft log and state machine. Every acknowledged write commits with FULL sync.
#[cfg(test)]
#[path = "store_diagnostics_tests.rs"]
mod diagnostics_tests;
use super::{
    diagnostics::{OperationTiming, Phase, Timings},
    Job, NodeId, Peer, Receipt, State, TypeConfig,
};
use openraft::storage::{LogFlushed, RaftLogStorage, RaftStateMachine};
use openraft::{
    Entry, EntryPayload, LogId, LogState, RaftLogReader, RaftSnapshotBuilder, Snapshot,
    SnapshotMeta, StorageError, StorageIOError, StoredMembership, Vote,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fmt::Debug,
    io::{self, Cursor},
    ops::{Bound, RangeBounds},
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct SqliteStore {
    connection: Arc<Mutex<Connection>>,
    timings: Timings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MachineMeta {
    applied: Option<LogId<NodeId>>,
    membership: StoredMembership<NodeId, Peer>,
}
#[derive(Serialize, Deserialize)]
struct MachineSnapshot {
    meta: MachineMeta,
    state: State,
}
#[derive(Serialize, Deserialize)]
struct SavedSnapshot {
    meta: SnapshotMeta<NodeId, Peer>,
    data: Vec<u8>,
}

fn other(e: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(e)
}
fn storage_error(e: io::Error) -> StorageError<NodeId> {
    StorageIOError::write(openraft::AnyError::new(&e)).into()
}
fn get<T: DeserializeOwned>(conn: &Connection, key: &str) -> io::Result<Option<T>> {
    let bytes: Option<Vec<u8>> = conn
        .query_row("SELECT value FROM sync_meta WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(other)?;
    bytes
        .map(|v| serde_json::from_slice(&v).map_err(other))
        .transpose()
}
fn put<T: Serialize>(conn: &Connection, key: &str, value: &T) -> io::Result<()> {
    conn.execute("INSERT INTO sync_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, serde_json::to_vec(value).map_err(other)?]).map_err(other)?;
    Ok(())
}
fn read_job(conn: &Connection, key: &str) -> io::Result<Option<Job>> {
    let bytes: Option<Vec<u8>> = conn
        .query_row("SELECT value FROM sync_jobs WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()
        .map_err(other)?;
    bytes
        .map(|v| serde_json::from_slice(&v).map_err(other))
        .transpose()
}
fn store_job(conn: &Connection, job: &Job) -> io::Result<()> {
    conn.execute("INSERT INTO sync_jobs(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![job.spec.job_id, serde_json::to_vec(job).map_err(other)?]).map_err(other)?;
    Ok(())
}
fn collect_state(conn: &Connection) -> io::Result<State> {
    let mut state = State {
        workers: get(conn, "workers")?.unwrap_or_default(),
        ..State::default()
    };
    let mut stmt = conn
        .prepare("SELECT key,value FROM sync_jobs ORDER BY key")
        .map_err(other)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(other)?;
    for row in rows {
        let (id, bytes) = row.map_err(other)?;
        state
            .jobs
            .insert(id, serde_json::from_slice(&bytes).map_err(other)?);
    }
    let mut stmt = conn
        .prepare("SELECT key,fingerprint,value FROM sync_receipts ORDER BY key")
        .map_err(other)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(other)?;
    for row in rows {
        let (id, fingerprint, bytes) = row.map_err(other)?;
        state.receipts.insert(
            id,
            (fingerprint, serde_json::from_slice(&bytes).map_err(other)?),
        );
    }
    Ok(state)
}

impl SqliteStore {
    pub async fn bind_identity(
        &self,
        node_id: NodeId,
        scope_id: String,
        peers: std::collections::BTreeMap<NodeId, Peer>,
    ) -> Result<(), StorageError<NodeId>> {
        self.run("bind_identity", move |conn| {
            let expected = serde_json::to_value((node_id, scope_id, peers)).map_err(other)?;
            let tx = conn.transaction().map_err(other)?;
            if let Some(existing) = get::<serde_json::Value>(&tx, "group_identity")? {
                if existing != expected { return Err(io::Error::other("authority store belongs to a different node, scope or membership configuration")); }
            } else {
                put(&tx, "group_identity", &expected)?;
            }
            tx.commit().map_err(other)
        }).await
    }
    pub fn open(path: &Path) -> io::Result<Self> {
        let connection = Connection::open(path).map_err(other)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(other)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS sync_meta(key TEXT PRIMARY KEY,value BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_logs(idx TEXT PRIMARY KEY,value BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_jobs(key TEXT PRIMARY KEY,value BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_receipts(key TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,value BLOB NOT NULL);").map_err(other)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            timings: Timings::default(),
        })
    }
    async fn run<T: Send + 'static>(
        &self,
        operation: &'static str,
        f: impl FnOnce(&mut Connection) -> io::Result<T> + Send + 'static,
    ) -> Result<T, StorageError<NodeId>> {
        let connection = self.connection.clone();
        // The guard belongs to the blocking work, even if its async caller leaves.
        let mut observation = self.timings.start(operation, Phase::Queued);
        tokio::task::spawn_blocking(move || {
            observation.enter(Phase::Waiting);
            let result = (|| {
                let mut conn = connection
                    .lock()
                    .map_err(|_| io::Error::other("sync SQLite mutex poisoned"))?;
                observation.enter(Phase::Running);
                f(&mut conn)
            })();
            observation.finish(result.is_ok());
            result
        })
        .await
        .map_err(|e| storage_error(other(e)))?
        .map_err(storage_error)
    }
    pub fn diagnostics(&self) -> std::collections::BTreeMap<&'static str, OperationTiming> {
        self.timings.snapshot()
    }
    /// Local projection only; establish a linearizable read before authorizing effects.
    pub async fn worker(
        &self,
        id: NodeId,
    ) -> Result<Option<super::WorkerMembership>, StorageError<NodeId>> {
        self.run("worker", move |conn| {
            let workers: std::collections::BTreeMap<NodeId, super::WorkerMembership> =
                get(conn, "workers")?.unwrap_or_default();
            Ok(workers.get(&id).cloned())
        })
        .await
    }
    /// Local projection only; callers must establish a linearizable read before authorizing effects.
    pub async fn job(&self, id: &str) -> Result<Option<Job>, StorageError<NodeId>> {
        let id = id.to_owned();
        self.run("job", move |conn| read_job(conn, &id)).await
    }
}

impl RaftLogReader<TypeConfig> for SqliteStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + Send>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<NodeId>> {
        let start = match range.start_bound() {
            Bound::Included(v) => *v,
            Bound::Excluded(v) => match v.checked_add(1) {
                Some(v) => v,
                None => return Ok(vec![]),
            },
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(v) => *v,
            Bound::Excluded(v) => match v.checked_sub(1) {
                Some(v) => v,
                None => return Ok(vec![]),
            },
            Bound::Unbounded => u64::MAX,
        };
        self.run("read_logs", move |conn| {
            let mut stmt = conn
                .prepare("SELECT value FROM sync_logs WHERE idx>=?1 AND idx<=?2 ORDER BY idx")
                .map_err(other)?;
            let rows = stmt
                .query_map(params![format!("{start:020}"), format!("{end:020}")], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .map_err(other)?;
            rows.map(|r| serde_json::from_slice(&r.map_err(other)?).map_err(other))
                .collect()
        })
        .await
    }
}
impl RaftLogStorage<TypeConfig> for SqliteStore {
    type LogReader = Self;
    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<NodeId>> {
        self.run("log_state", |conn| {
            let purged: Option<LogId<NodeId>> = get(conn, "purged")?;
            let bytes: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT value FROM sync_logs ORDER BY idx DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(other)?;
            let entry: Option<Entry<TypeConfig>> = bytes
                .map(|v| serde_json::from_slice(&v).map_err(other))
                .transpose()?;
            Ok(LogState {
                last_purged_log_id: purged,
                last_log_id: entry.map(|e| e.log_id).or(purged),
            })
        })
        .await
    }
    async fn get_log_reader(&mut self) -> Self {
        self.clone()
    }
    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        let vote = *vote;
        self.run("save_vote", move |c| put(c, "vote", &vote)).await
    }
    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        self.run("read_vote", |c| get(c, "vote")).await
    }
    async fn save_committed(
        &mut self,
        committed: Option<LogId<NodeId>>,
    ) -> Result<(), StorageError<NodeId>> {
        self.run("save_committed", move |c| put(c, "committed", &committed))
            .await
    }
    async fn read_committed(&mut self) -> Result<Option<LogId<NodeId>>, StorageError<NodeId>> {
        self.run("read_committed", |c| Ok(get(c, "committed")?.flatten()))
            .await
    }
    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<TypeConfig>,
    ) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        let result = self
            .run("append", move |conn| {
                let tx = conn.transaction().map_err(other)?;
                for entry in entries {
                    tx.execute(
                        "INSERT OR REPLACE INTO sync_logs(idx,value) VALUES(?1,?2)",
                        params![
                            format!("{:020}", entry.log_id.index),
                            serde_json::to_vec(&entry).map_err(other)?
                        ],
                    )
                    .map_err(other)?;
                }
                tx.commit().map_err(other)
            })
            .await;
        callback.log_io_completed(
            result
                .as_ref()
                .map(|_| ())
                .map_err(|e| io::Error::other(e.to_string())),
        );
        result
    }
    async fn truncate(&mut self, id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        self.run("truncate", move |c| {
            c.execute(
                "DELETE FROM sync_logs WHERE idx>=?1",
                [format!("{:020}", id.index)],
            )
            .map_err(other)?;
            Ok(())
        })
        .await
    }
    async fn purge(&mut self, id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        self.run("purge", move |c| {
            let tx = c.transaction().map_err(other)?;
            put(&tx, "purged", &id)?;
            tx.execute(
                "DELETE FROM sync_logs WHERE idx<=?1",
                [format!("{:020}", id.index)],
            )
            .map_err(other)?;
            tx.commit().map_err(other)
        })
        .await
    }
}
impl RaftStateMachine<TypeConfig> for SqliteStore {
    type SnapshotBuilder = Self;
    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<NodeId>>, StoredMembership<NodeId, Peer>), StorageError<NodeId>> {
        self.run("applied_state", |c| {
            let meta: MachineMeta = get(c, "machine")?.unwrap_or_default();
            Ok((meta.applied, meta.membership))
        })
        .await
    }
    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Receipt>, StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let entries: Vec<_> = entries.into_iter().collect();
        self.run("apply", move|conn|{
            let tx=conn.transaction().map_err(other)?;
            let mut meta:MachineMeta=get(&tx,"machine")?.unwrap_or_default();
            let mut replies=Vec::new();
            for entry in entries {
                if meta.applied.is_some_and(|id|entry.log_id.index<=id.index) {return Err(io::Error::other("Raft applied an entry twice"));}
                meta.applied=Some(entry.log_id);
                let response=match entry.payload {
                    EntryPayload::Blank=>Receipt::Rejected(super::Rejection::InvalidRequest),
                    EntryPayload::Membership(m)=>{meta.membership=StoredMembership::new(Some(entry.log_id),m);Receipt::Rejected(super::Rejection::InvalidRequest)},
                    EntryPayload::Normal(request)=>{
                        let id=match &request.command {
                            super::Command::AdmitWorker{..}|super::Command::RevokeWorker{..}=>None,
                            super::Command::Create{spec,..}=>Some(&spec.job_id),
                            super::Command::ProtectCheckpoint{job_id,..}|super::Command::CommitEffectCheckpoint{job_id,..}|super::Command::TakeOver{job_id,..}|super::Command::BeginEffect{job_id,..}|super::Command::CompleteEffect{job_id,..}|super::Command::Stop{job_id,..}=>Some(job_id),
                        };
                        let mut state=State {workers:get(&tx,"workers")?.unwrap_or_default(),..State::default()};
                        if let Some(id)=id {if let Some(job)=read_job(&tx,id)? {state.jobs.insert(id.clone(),job);}}
                        let previous:Option<(String,Vec<u8>)>=tx.query_row("SELECT fingerprint,value FROM sync_receipts WHERE key=?1",[&request.request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(other)?;
                        if let Some((hash,bytes))=previous {state.receipts.insert(request.request_id.clone(),(hash,serde_json::from_slice(&bytes).map_err(other)?));}
                        let peers=meta.membership.nodes().map(|(id,peer)|(*id,peer.clone())).collect();
                        let response=state.apply(&request,&peers);
                        if let Some(id)=id {if let Some(job)=state.jobs.get(id) {store_job(&tx,job)?;}}
                        if matches!(response,Receipt::WorkerApplied(_)) {put(&tx,"workers",&state.workers)?;}
                        if let Some((hash,receipt))=state.receipts.get(&request.request_id) {
                            tx.execute("INSERT OR IGNORE INTO sync_receipts(key,fingerprint,value) VALUES(?1,?2,?3)",params![request.request_id,hash,serde_json::to_vec(receipt).map_err(other)?]).map_err(other)?;
                        }
                        response
                    }
                };
                replies.push(response);
            }
            put(&tx,"machine",&meta)?;tx.commit().map_err(other)?;Ok(replies)
        }).await
    }
    async fn get_snapshot_builder(&mut self) -> Self {
        self.clone()
    }
    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, Peer>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        let meta = meta.clone();
        let data = snapshot.into_inner();
        self.run("install_snapshot", move |conn| {
            let snapshot: MachineSnapshot = serde_json::from_slice(&data).map_err(other)?;
            if snapshot.meta.applied != meta.last_log_id
                || snapshot.meta.membership != meta.last_membership
            {
                return Err(io::Error::other("snapshot metadata mismatch"));
            }
            let tx = conn.transaction().map_err(other)?;
            tx.execute("DELETE FROM sync_jobs", []).map_err(other)?;
            tx.execute("DELETE FROM sync_receipts", []).map_err(other)?;
            for job in snapshot.state.jobs.values() {
                store_job(&tx, job)?;
            }
            for (id, (hash, receipt)) in &snapshot.state.receipts {
                tx.execute(
                    "INSERT INTO sync_receipts(key,fingerprint,value) VALUES(?1,?2,?3)",
                    params![id, hash, serde_json::to_vec(receipt).map_err(other)?],
                )
                .map_err(other)?;
            }
            put(&tx, "workers", &snapshot.state.workers)?;
            put(&tx, "machine", &snapshot.meta)?;
            put(&tx, "snapshot", &SavedSnapshot { meta, data })?;
            tx.commit().map_err(other)
        })
        .await
    }
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        self.run("current_snapshot", |c| {
            let saved: Option<SavedSnapshot> = get(c, "snapshot")?;
            Ok(saved.map(|s| Snapshot {
                meta: s.meta,
                snapshot: Box::new(Cursor::new(s.data)),
            }))
        })
        .await
    }
}
impl RaftSnapshotBuilder<TypeConfig> for SqliteStore {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        self.run("build_snapshot", |conn| {
            let tx = conn.transaction().map_err(other)?;
            let machine: MachineMeta = get(&tx, "machine")?.unwrap_or_default();
            let meta = SnapshotMeta {
                last_log_id: machine.applied,
                last_membership: machine.membership.clone(),
                snapshot_id: format!("ctox-sync-{:?}", machine.applied),
            };
            let data = serde_json::to_vec(&MachineSnapshot {
                meta: machine,
                state: collect_state(&tx)?,
            })
            .map_err(other)?;
            put(
                &tx,
                "snapshot",
                &SavedSnapshot {
                    meta: meta.clone(),
                    data: data.clone(),
                },
            )?;
            tx.commit().map_err(other)?;
            Ok(Snapshot {
                meta,
                snapshot: Box::new(Cursor::new(data)),
            })
        })
        .await
    }
}
