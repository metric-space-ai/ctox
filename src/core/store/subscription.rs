//! Commit-driven subscription cursor; initial and live both use the existing snapshot reader.
use crate::change_log::{self, atomic, identifier, integer, invalid, ChangeDocument, Operation};
use crate::{
    read_pool::ReadPool,
    writer::{CommitReceiver, WriterHandle},
    StoreError,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;
use std::sync::Arc;
#[path = "../rxdb/src/plugins/replication_webrtc/v4_stream.rs"]
pub mod protocol;
use protocol::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriptionKey {
    pub participant: String,
    pub collection: String,
    pub scope_hash: String,
    pub gen: u64,
}
/// Native policy must validate current rights and exact resolved scope inside every snapshot.
/// None means unproject, including a document leaving the authorized row filter.
pub trait ReadAuthority: Send + Sync {
    /// Verified app+user binding for E51; absent authority fails closed.
    fn preference_context(&self, _participant: &str) -> Option<crate::preferences::Context> { None }
    fn authorize(&self, tx: &Transaction<'_>, key: &SubscriptionKey) -> rusqlite::Result<()>;
    fn project(
        &self,
        tx: &Transaction<'_>,
        key: &SubscriptionKey,
        doc: &ChangeDocument,
    ) -> rusqlite::Result<Option<Value>>;
}
/// A generation switch is a control event, never a page bearing another generation's rows.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionEvent {
    Page(StreamPacket),
    RequiresPage(RequiresPage),
}
pub struct Subscription {
    key: SubscriptionKey,
    cursor: u64,
    epoch: Option<String>,
    instance_id: Option<String>,
    needs_read: bool,
    initial: bool,
    reader: ReadPool,
    commits: CommitReceiver,
    writer: WriterHandle,
}
impl Subscription {
    pub fn new(
        reader: ReadPool,
        writer: &WriterHandle,
        key: SubscriptionKey,
        checkpoint: Option<Checkpoint>,
    ) -> Result<Self, StoreError> {
        if key.participant.is_empty()
            || key.collection.is_empty()
            || key.scope_hash.is_empty()
            || key.gen == 0
            || reader.generation() != writer.generation().0
        {
            return Err(StoreError::Invalid("invalid subscription binding".into()));
        }
        if checkpoint.as_ref().is_some_and(|c| {
            c.participant != key.participant
                || c.collection != key.collection
                || c.scope_hash != key.scope_hash
                || c.gen != key.gen
                || c.instance_id.is_empty()
                || c.epoch.is_empty()
        }) {
            return Err(StoreError::Invalid(
                "checkpoint scope changed: resnapshot required".into(),
            ));
        }
        // Register BEFORE the first snapshot. Coalesced commit heads are sufficient: cursor reads drain the log.
        let commits = writer.subscribe_commits();
        Ok(Self {
            key,
            cursor: checkpoint.as_ref().map_or(0, |c| c.seq.0),
            instance_id: checkpoint.as_ref().map(|c| c.instance_id.clone()),
            epoch: checkpoint.map(|c| c.epoch),
            needs_read: true,
            initial: true,
            reader,
            commits,
            writer: writer.clone(),
        })
    }
    pub fn key(&self) -> &SubscriptionKey {
        &self.key
    }
    /// Sender read position only, never proof of recipient persistence.
    /// Reconnect MUST use the recipient's durable checkpoint, not this position.
    pub fn checkpoint(&self) -> Option<Checkpoint> {
        self.epoch.as_ref().map(|epoch| Checkpoint {
            instance_id: self
                .instance_id
                .clone()
                .expect("a signed checkpoint has an instance"),
            participant: self.key.participant.clone(),
            collection: self.key.collection.clone(),
            scope_hash: self.key.scope_hash.clone(),
            gen: self.key.gen,
            epoch: epoch.clone(),
            seq: Seq(self.cursor),
        })
    }
    /// No timer or polling: after catch-up, wait for the writer's retained notification.
    /// A read/signing failure retains needs_read and the old cursor, permitting a safe retry.
    /// Compatibility entry point. New protocol consumers use next_event for the typed resnapshot.
    pub async fn next_page(
        &mut self,
        limit: usize,
        authority: Arc<dyn ReadAuthority>,
        signer: &dyn InstanceSigner,
        core_sent_at_ms: impl FnOnce() -> u64 + Send,
    ) -> Result<StreamPacket, StoreError> {
        match self
            .next_event(limit, authority, signer, core_sent_at_ms)
            .await?
        {
            SubscriptionEvent::Page(page) => Ok(page),
            SubscriptionEvent::RequiresPage(value) => Err(StoreError::Invalid(format!(
                "requires_page: collection={} old_gen={} gen={:?}; checkpoint=0",
                value.collection, value.previous_gen, value.gen
            ))),
        }
    }
    /// Initial, live and generation control use the same commit-driven snapshot.
    pub async fn next_event(
        &mut self,
        limit: usize,
        authority: Arc<dyn ReadAuthority>,
        signer: &dyn InstanceSigner,
        core_sent_at_ms: impl FnOnce() -> u64 + Send,
    ) -> Result<SubscriptionEvent, StoreError> {
        if self
            .instance_id
            .as_ref()
            .is_some_and(|id| id != signer.instance_id())
        {
            return Err(StoreError::Invalid("checkpoint instance changed".into()));
        }
        if self.reader.generation() != self.writer.generation().0 {
            return Err(StoreError::Invalid("reader owner changed".into()));
        }
        if !self.needs_read {
            let notification = if self.commits.has_changed()? {
                self.commits.latest().ok_or(StoreError::Closed)?
            } else {
                self.commits.changed().await?
            };
            if notification.db_id != *self.writer.db_id()
                || notification.generation != self.writer.generation()
            {
                return Err(StoreError::Invalid("commit owner changed".into()));
            }
            self.needs_read = true;
        }
        let key = self.key.clone();
        let cursor = self.cursor;
        let epoch = self.epoch.clone();
        let expected_database = self.writer.db_id().path().to_path_buf();
        let snapshot = self
            .reader
            .read(move |tx| {
                let file: String = tx.query_row(
                    "SELECT file FROM pragma_database_list WHERE name='main'",
                    [],
                    |r| r.get(0),
                )?;
                let actual_database = std::fs::canonicalize(&file)
                    .map_err(|_| invalid("reader database identity unavailable"))?;
                if actual_database != expected_database {
                    return Err(invalid("reader database differs from writer owner"));
                }
                authority.authorize(tx, &key)?;
                let prefs = if key.collection == crate::preferences::COLLECTION {
                    let context = authority.preference_context(&key.participant)
                        .ok_or_else(|| invalid("forbidden"))?;
                    if key.scope_hash != context.scope_hash() { return Err(invalid("forbidden")); }
                    Some(context)
                } else { None };
                let state: Option<String> = tx
                    .query_row(
                        "SELECT state FROM _collections WHERE name=?1 AND gen=?2",
                        params![key.collection, integer(key.gen)?],
                        |r| r.get(0),
                    )
                    .optional()?;
                if !matches!(state.as_deref(), Some("active" | "preparing")) {
                    let active = crate::schema::active_collection(tx, &key.collection)?;
                    return Ok(Err(RequiresPage {
                        collection: key.collection.clone(),
                        scope_hash: key.scope_hash.clone(),
                        previous_gen: key.gen,
                        gen: active.as_ref().map(|c| c.gen),
                        checkpoint_seq: Seq(0),
                        migrated_at_seq: active.and_then(|c| c.migrated_at_seq).map(Seq),
                    }));
                }
                let page = change_log::read_page(tx, &key.collection, key.gen, cursor, limit)?;
                if epoch.as_ref().is_some_and(|e| e != &page.epoch) {
                    return Err(invalid("epoch changed: resnapshot required"));
                }
                let docs = page
                    .docs
                    .iter()
                    .filter(|doc| prefs.as_ref().is_none_or(|ctx| ctx.owns_id(&doc.id)))
                    .map(|doc| {
                        let body = if doc.op == Operation::Delete {
                            None
                        } else {
                            if prefs.as_ref().is_some_and(|ctx| !doc.body.as_ref().is_some_and(|body| ctx.owns(body))) {
                                None
                            } else { authority.project(tx, &key, doc)? }
                        };
                        Ok(Document {
                            id: doc.id.clone(),
                            seq: Seq(doc.seq),
                            op: if doc.op == Operation::Delete {
                                DocumentOp::Delete
                            } else if body.is_some() {
                                DocumentOp::Upsert
                            } else {
                                DocumentOp::Unproject
                            },
                            rev: doc.rev.clone(),
                            schema_ver: doc.schema_ver,
                            app_ver: doc.app_ver.clone(),
                            body,
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(Ok((page, docs)))
            })
            .await
            .map_err(StoreError::from)?;
        let (page, docs) = match snapshot {
            Ok(page) => page,
            Err(requirement) => return Ok(SubscriptionEvent::RequiresPage(requirement)),
        };
        let mut packet = StreamPacket {
            kind: if self.initial {
                StreamKind::Page
            } else {
                StreamKind::Live
            },
            instance_id: String::new(),
            key_id: String::new(),
            participant: self.key.participant.clone(),
            collection: self.key.collection.clone(),
            gen: self.key.gen,
            package_id: uuid::Uuid::new_v4().to_string(),
            seq_from: Seq(cursor),
            seq_to: Seq(page.cursor_to),
            head_seq: Seq(page.head_seq),
            scope_hash: self.key.scope_hash.clone(),
            epoch: page.epoch.clone(),
            core_sent_at_ms: core_sent_at_ms(),
            backlog: page.backlog,
            complete: page.cursor_to == page.head_seq,
            docs,
            sig: Vec::new(),
        };
        packet.sign(signer).map_err(StoreError::Invalid)?;
        self.instance_id = Some(packet.instance_id.clone());
        self.cursor = page.cursor_to;
        self.epoch = Some(page.epoch);
        self.needs_read = self.cursor < page.head_seq;
        if !self.needs_read {
            self.initial = false;
        }
        Ok(SubscriptionEvent::Page(packet))
    }
}

#[path = "write_contract.rs"]
pub mod write_contract;

/// Native authority is mandatory, including replay and removes. It may enforce
/// write_guard/schema validation and resolve old-schema input through the owned migration runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritePermission {
    Permit,
    Reject(String),
}
pub trait WriteAuthority: Send + Sync {
    /// Resolved from the participant's authenticated launch/session, never its payload.
    fn preference_context(&self, _participant: &str) -> Option<crate::preferences::Context> { None }
    /// Use the command document's app_id as sender, never a payload claim (§16.40).
    /// Enforce active commands_out/in and the authenticated actor's target right.
    fn authorize_command_intent(
        &self,
        _tx: &Transaction<'_>,
        _participant: &str,
        _command: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Reject("forbidden".into()))
    }
    /// Native capability/route gate; never inferred from a document claim.
    fn authorize_command_executor(
        &self,
        _tx: &Transaction<'_>,
        _participant: &str,
        _command: &Value,
    ) -> rusqlite::Result<WritePermission> {
        Ok(WritePermission::Reject("forbidden".into()))
    }
    fn authorize(
        &self,
        tx: &Transaction<'_>,
        participant: &str,
        request: &Write,
    ) -> rusqlite::Result<WritePermission>;
    /// Validate the final current-schema document and write guards. Reject reasons are public machine codes.
    fn validate_document(
        &self,
        tx: &Transaction<'_>,
        participant: &str,
        collection: &crate::schema::Collection,
        document: &Value,
    ) -> rusqlite::Result<WritePermission>;
    /// Prepared mirror for a pending generation; no JS/I/O is allowed in this callback.
    /// The admission implementation binds output to target definition, op_id and final
    /// post-merge document. None leaves the whole write uncommitted for a safe retry.
    fn generation_output(
        &self,
        _tx: &Transaction<'_>,
        _participant: &str,
        _request: &Write,
        _target: &crate::schema::Collection,
        _document: Option<&Value>,
    ) -> rusqlite::Result<Option<crate::schema::MigrationOutput>> {
        Ok(None)
    }
    /// Trusted semantic migration seam. None is a terminal migration_required rejection.
    /// Use prepared, revision-bound output; no I/O or JS in the writer. A transient unavailable runner returns Err.
    fn migrate(
        &self,
        _tx: &Transaction<'_>,
        _participant: &str,
        _request: &Write,
        _resolved_base_rev: Option<&str>,
        _current: &crate::schema::Collection,
    ) -> rusqlite::Result<Option<Migration>> {
        Ok(None)
    }
}
#[derive(Debug, Clone)]
pub struct Migration {
    pub body: Option<Value>,
    pub base_rev: Option<String>,
    pub archived_ref: Option<String>,
}
pub const OP_HORIZON_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub fn initialize_operations(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _ops(
        op_id TEXT PRIMARY KEY NOT NULL,participant TEXT NOT NULL,collection TEXT NOT NULL,id TEXT NOT NULL,
        gen INTEGER NOT NULL,request_json TEXT NOT NULL,result_json TEXT NOT NULL,result_rev TEXT,
        created_at_ms INTEGER NOT NULL,expires_at_ms INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS _ops_expiry ON _ops(expires_at_ms);")
}
/// UUIDv7 embeds the original operation time, so pruning _ops cannot turn an expired replay into a fresh write.
fn operation_time(id: &str) -> rusqlite::Result<u64> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| invalid("op_id must be UUIDv7"))?;
    if uuid.to_string() != id {
        return Err(invalid("op_id must be canonical UUIDv7"));
    }
    let bytes = uuid.as_bytes();
    if bytes[6] >> 4 != 7 || bytes[8] >> 6 != 2 {
        return Err(invalid("op_id must be UUIDv7"));
    }
    Ok(bytes[..6].iter().fold(0, |v, b| (v << 8) | u64::from(*b)))
}
fn rejected(request: &Write, reason: &str) -> WriteResult {
    WriteResult {
        op_id: request.op_id.clone(),
        status: WriteStatus::Rejected,
        seq: None,
        rev: None,
        gen: request.gen,
        reason: Some(reason.into()),
        code: Some(write_contract::rejection_code(reason).into()),
        archived_ref: None,
    }
}
fn request_identity(request: &Write) -> rusqlite::Result<String> {
    // op_id is first-write-wins. Never retain business payload in the dedup log.
    let mut identity = request.clone();
    identity.body = None;
    serde_json::to_string(&identity).map_err(|e| invalid(e.to_string()))
}
fn remember(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    result: &WriteResult,
    now: u64,
) -> rusqlite::Result<()> {
    tx.execute("INSERT INTO _ops(op_id,participant,collection,id,gen,request_json,result_json,result_rev,created_at_ms,expires_at_ms)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![request.op_id,participant,request.collection,request.id,integer(request.gen)?,
        request_identity(request)?,
        serde_json::to_string(result).map_err(|e|invalid(e.to_string()))?,result.rev,integer(now)?,integer(now.checked_add(OP_HORIZON_MS).ok_or_else(||invalid("operation expiry overflow"))?)?])?;
    Ok(())
}
/// Call inside the ONE writer transaction; document, revision history and dedup result commit together.
/// Missing predecessors return a retryable error without storing a terminal rejection.
pub(crate) fn apply_write(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    now_ms: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<WriteResult> {
    atomic(tx, || {
        initialize_operations(tx)?;
        if participant.is_empty() || request.id.is_empty() || request.gen == 0 {
            return Err(invalid("invalid write binding"));
        }
        let mut permission = authority.authorize(tx, participant, request)?;
        let prefs = if request.collection == crate::preferences::COLLECTION {
            let context = authority.preference_context(participant);
            if !context.as_ref().is_some_and(|ctx| ctx.owns_id(&request.id)) {
                permission = WritePermission::Reject("forbidden".into());
            }
            context
        } else { None };
        let issued = operation_time(&request.op_id)?;
        if issued > now_ms.saturating_add(300_000) {
            return Ok(rejected(request, "invalid_time"));
        }
        if now_ms.saturating_sub(issued) > OP_HORIZON_MS {
            return Ok(rejected(request, "expired"));
        }
        let replay:Option<(String,String,String,i64)>=tx.query_row(
            "SELECT participant,request_json,result_json,expires_at_ms FROM _ops WHERE op_id=?1",[&request.op_id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((owner, json, result, expires)) = replay {
            if owner != participant || json != request_identity(request)? {
                return Ok(rejected(request, "op_id_conflict"));
            }
            if integer(now_ms)? > expires {
                return Ok(rejected(request, "expired"));
            }
            return serde_json::from_str(&result).map_err(|e| invalid(e.to_string()));
        }
        if let WritePermission::Reject(reason) = permission {
            let result = rejected(request, &reason);
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        let mut base = request.base_rev.clone();
        if let Some(previous) = &request.prev_op_id {
            if previous == &request.op_id || base.as_deref() != Some(&format!("op:{previous}")) {
                let result = rejected(request, "invalid_predecessor");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            let predecessor_time = operation_time(previous)?;
            if now_ms.saturating_sub(predecessor_time) > OP_HORIZON_MS {
                let result = rejected(request, "expired");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            if predecessor_time > now_ms.saturating_add(300_000) {
                let result = rejected(request, "invalid_predecessor");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            let previous_row: Option<(String, String, String, u64, String)> = tx
                .query_row(
                    "SELECT participant,collection,id,gen,result_json FROM _ops WHERE op_id=?1",
                    [previous],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let Some((owner, collection, id, gen, json)) = previous_row else {
                return Err(invalid("pending_predecessor"));
            };
            let previous_result: WriteResult =
                serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?;
            if owner != participant
                || collection != request.collection
                || id != request.id
                || gen != request.gen
                || previous_result.status == WriteStatus::Rejected
            {
                let result = rejected(request, "invalid_predecessor");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            base = previous_result.rev;
        } else if base.as_ref().is_some_and(|s| s.starts_with("op:")) {
            let result = rejected(request, "invalid_predecessor");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        let Some(current) = crate::schema::active_collection(tx, &request.collection)? else {
            let result = rejected(request, "collection_unavailable");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        };
        let stale = request.gen != current.gen
            || request.schema_ver != current.definition.schema_ver
            || request.app_ver != current.definition.app_ver;
        let migration = if stale && request.op == WriteOp::Delete && request.gen == current.gen {
            Some(Migration {
                body: None,
                base_rev: base.clone(),
                archived_ref: None,
            })
        } else if stale {
            authority.migrate(tx, participant, request, base.as_deref(), &current)?
        } else {
            None
        };
        if stale && migration.is_none() {
            let result = rejected(request, "migration_required");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        if let Some(m) = &migration {
            base = m.base_rev.clone();
        }
        let table = identifier(&current.table)?;

        let row: Option<(String, Option<String>, bool)> = tx
            .query_row(
                &format!("SELECT rev,data,deleted FROM {table} WHERE id=?1"),
                [&request.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        // E35: CAS is distinct from base_rev field merge. Check the live row here,
        // including buffered writes, under the same writer transaction as the effect
        // and durable rejection receipt. Never merge around an explicit expectation.
        if request.op == WriteOp::Patch
            && request.expected_revision.as_ref().is_some_and(|expected| {
                !row.as_ref()
                    .is_some_and(|(rev, _, deleted)| !deleted && rev == expected)
            })
        {
            let result = rejected(request, "revision_conflict");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        if request.collection == "business_commands"
            && request.op == WriteOp::Put
            && base.is_none()
            && row.is_some()
            && write_contract::duplicate_command(
                tx,
                participant,
                request,
                row.as_ref().and_then(|r| r.1.as_deref()),
            )?
        {
            let (rev, _, _) = row.as_ref().ok_or_else(|| invalid("command missing"))?;
            let seq = tx.query_row(
                "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
                params![request.collection, integer(current.gen)?, request.id],
                |r| r.get(0),
            )?;
            let result = WriteResult {
                op_id: request.op_id.clone(),
                status: WriteStatus::Merged,
                seq: Some(Seq(seq)),
                rev: Some(rev.clone()),
                gen: current.gen,
                reason: None,
                code: None,
                archived_ref: None,
            };
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        if request.collection == "business_commands"
            && request.op == WriteOp::Delete
            && !write_contract::command_can_remove(row.as_ref().and_then(|r| r.1.as_deref()))?
        {
            let result = rejected(request, "command_delete_forbidden");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        // A removed revision cannot resurrect content; an explicit create (no base) may start a new identity revision.
        let conflict = match &row {
            Some((rev, _, deleted)) => {
                if *deleted {
                    request.op != WriteOp::Put || base.is_some()
                } else {
                    base.as_ref() != Some(rev) && (request.op == WriteOp::Delete || base.is_none())
                }
            }
            None => base.is_some() || request.op != WriteOp::Put,
        };
        if conflict {
            let result = rejected(request, "conflict");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        if request.collection == "business_commands" {
            if let (Some((_, Some(json), false)), Some(_)) = (&row, &request.body) {
                let previous: Value =
                    serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
                if let Some(reason) = write_contract::check_command_input(
                    tx,
                    participant,
                    request,
                    &previous,
                    now_ms,
                    authority,
                )? {
                    let result = rejected(request, &reason);
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
            }
        }
        let merging = row
            .as_ref()
            .is_some_and(|(rev, _, deleted)| !deleted && base.as_ref() != Some(rev));
        let merge_base: Option<Value> = if merging {
            let history: Option<Option<String>> = tx.query_row(
                "SELECT body FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 AND rev=?4 AND created_at>=?5",
                params![request.collection, integer(current.gen)?, request.id, base, integer(now_ms.saturating_sub(OP_HORIZON_MS)/1000)?],
                |r| r.get(0)).optional()?;
            let Some(Some(json)) = history else {
                let result = rejected(request, "conflict");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            };
            Some(serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?)
        } else {
            None
        };
        if let Some(seq) = request.base_seq {
            if request.prev_op_id.is_none() && !stale && !merging {
                let actual: Option<u64> = tx
                    .query_row(
                        "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
                        params![request.collection, integer(current.gen)?, request.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                // The Kernel represents a never-seen document with base_seq=0.
                // Only an explicit create of an absent row has that sentinel;
                // a missing change-log row for an existing document is not 0.
                let actual = if row.is_none() && request.op == WriteOp::Put && base.is_none() {
                    Some(0)
                } else {
                    actual
                };
                if actual != Some(seq.0) {
                    let result = rejected(request, "conflict");
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
            }
        }
        // Preserve a pre-existing revision too (e.g. imported/backfilled rows).
        if let Some((prior_rev, Some(prior_body), false)) = &row {
            tx.execute("INSERT OR IGNORE INTO _revs(collection,id,rev,seq,gen,body,created_at)
                SELECT ?1,?2,?3,seq,?4,?5,?6 FROM _changes WHERE collection=?1 AND gen=?4 AND id=?2",
                params![request.collection,request.id,prior_rev,integer(current.gen)?,prior_body,integer(now_ms/1000)?])?;
        }
        let rev = format!("v4:{}", request.op_id);
        let body = if let Some(m) = &migration {
            m.body.clone()
        } else {
            request.body.clone()
        };
        let pending = crate::schema::preparing_target(tx, &request.collection, current.gen)?;
        let mut prepared_mirror = None;
        let mut final_document = None;
        if request.op == WriteOp::Delete {
            if body.is_some() {
                let result = rejected(request, "delete_payload");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            crate::schema::remove_document(
                tx,
                &request.collection,
                current.gen,
                &request.id,
                base.as_deref()
                    .ok_or_else(|| invalid("delete needs base_rev"))?,
                &rev,
            )?;
        } else {
            let Some(input) = body else {
                let result = rejected(request, "missing_body");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            };
            let mut document = if request.op == WriteOp::Patch && !stale {
                let mut doc: Value = if let Some(base) = &merge_base {
                    base.clone()
                } else {
                    let prior = row
                        .as_ref()
                        .and_then(|r| r.1.as_ref())
                        .ok_or_else(|| invalid("patch base unavailable"))?;
                    serde_json::from_str(prior).map_err(|e| invalid(e.to_string()))?
                };
                if request.collection == "business_commands" {
                    write_contract::apply_command_patch(&mut doc, &input);
                } else {
                    merge_patch(&mut doc, &input);
                }
                doc
            } else {
                input
            };
            if let Some(base) = &merge_base {
                let server: Value = serde_json::from_str(
                    row.as_ref()
                        .and_then(|r| r.1.as_deref())
                        .ok_or_else(|| invalid("merge current unavailable"))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                document = write_contract::merge_fields(base, &server, &document);
            }
            if !document.is_object() {
                let result = rejected(request, "object_required");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            if let Some(ctx) = &prefs {
                if let Some(reason) = ctx.validate(tx, &current, &request.id, &document)? {
                    let result = rejected(request, reason);
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
            }
            if let WritePermission::Reject(reason) =
                authority.validate_document(tx, participant, &current, &document)?
            {
                let result = rejected(request, &reason);
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            if request.collection == "business_commands" {
                let prior: Option<Value> = row
                    .as_ref()
                    .and_then(|r| r.1.as_deref())
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|e| invalid(e.to_string()))?;
                if let Some(reason) = write_contract::check_command(
                    tx,
                    participant,
                    request,
                    prior.as_ref(),
                    &document,
                    now_ms,
                    authority,
                )? {
                    let result = rejected(request, &reason);
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
                if prior.as_ref() == Some(&document) {
                    let (prior_rev, _, _) =
                        row.as_ref().ok_or_else(|| invalid("command missing"))?;
                    let seq = tx.query_row(
                        "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
                        params![request.collection, integer(current.gen)?, request.id],
                        |r| r.get(0),
                    )?;
                    let result = WriteResult {
                        op_id: request.op_id.clone(),
                        status: WriteStatus::Merged,
                        seq: Some(Seq(seq)),
                        rev: Some(prior_rev.clone()),
                        gen: current.gen,
                        reason: None,
                        code: None,
                        archived_ref: None,
                    };
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
            }
            if let Some((_, target)) = &pending {
                prepared_mirror = Some(
                    authority
                        .generation_output(tx, participant, request, target, Some(&document))?
                        .ok_or_else(|| invalid("migration_preparation_required"))?,
                );
                if let Some(crate::schema::MigrationOutput::Upsert { body, .. }) = &prepared_mirror
                {
                    if let WritePermission::Reject(reason) =
                        authority.validate_document(tx, participant, target, body)?
                    {
                        let result = rejected(request, &reason);
                        remember(tx, participant, request, &result, now_ms)?;
                        return Ok(result);
                    }
                }
            }
            final_document = Some(document.clone());
            // Schemas operate on business JSON; id/rev/deleted are authoritative table columns.
            let json = serde_json::to_string(&mut document).map_err(|e| invalid(e.to_string()))?;
            tx.execute(&format!("INSERT INTO {table}(id,rev,schema_ver,data,deleted) VALUES(?1,?2,?3,?4,0)
                ON CONFLICT(id) DO UPDATE SET rev=excluded.rev,schema_ver=excluded.schema_ver,data=excluded.data,deleted=0"),
                params![request.id,rev,current.definition.schema_ver,json])?;
        }
        let seq: u64 = tx.query_row(
            "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
            params![request.collection, integer(current.gen)?, request.id],
            |r| r.get(0),
        )?;
        if request.collection == "business_commands" {
            if request.op == WriteOp::Delete {
                write_contract::retire_command(tx, &request.id, current.gen, &rev, seq)?;
            } else {
                write_contract::project_command(
                    tx,
                    &request.id,
                    current.gen,
                    &rev,
                    seq,
                    final_document
                        .as_ref()
                        .ok_or_else(|| invalid("command body missing"))?,
                )?;
            }
        }
        if let Some((lock, _)) = pending {
            let output = if request.op == WriteOp::Delete {
                crate::schema::MigrationOutput::Delete
            } else {
                prepared_mirror.ok_or_else(|| invalid("migration_preparation_required"))?
            };
            let source = ChangeDocument {
                id: request.id.clone(),
                seq,
                op: if request.op == WriteOp::Delete {
                    Operation::Delete
                } else {
                    Operation::Upsert
                },
                rev: rev.clone(),
                gen: current.gen,
                schema_ver: current.definition.schema_ver,
                app_ver: current.definition.app_ver.clone(),
                body: final_document,
            };
            crate::schema::mirror_generation_write(
                tx,
                &lock,
                &request.collection,
                &source,
                &output,
                integer(now_ms / 1000)?,
            )?;
        }
        let result = WriteResult {
            op_id: request.op_id.clone(),
            status: if stale {
                WriteStatus::Migrated
            } else if merging {
                WriteStatus::Merged
            } else {
                WriteStatus::Accepted
            },
            seq: Some(Seq(seq)),
            rev: Some(rev.clone()),
            gen: current.gen,
            reason: None,
            code: None,
            archived_ref: migration.and_then(|m| m.archived_ref),
        };
        if request.op != WriteOp::Delete {
            let body: Option<String> = tx.query_row(
                &format!("SELECT data FROM {table} WHERE id=?1"),
                [&request.id],
                |r| r.get(0),
            )?;
            tx.execute("INSERT INTO _revs(collection,id,rev,seq,gen,body,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![request.collection,request.id,rev,integer(seq)?,integer(current.gen)?,body,integer(now_ms/1000)?])?;
            tx.execute("DELETE FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 AND
                (created_at<?4 OR rev NOT IN(SELECT rev FROM _revs WHERE collection=?1 AND gen=?2 AND id=?3 ORDER BY seq DESC LIMIT 20))",
                params![request.collection,integer(current.gen)?,request.id,integer(now_ms.saturating_sub(OP_HORIZON_MS)/1000)?])?;
        }
        remember(tx, participant, request, &result, now_ms)?;
        Ok(result)
    })
}
fn merge_patch(target: &mut Value, patch: &Value) {
    if let Value::Object(fields) = patch {
        if !target.is_object() {
            *target = Value::Object(Default::default());
        }
        let object = target.as_object_mut().expect("object initialized");
        for (key, value) in fields {
            if value.is_null() {
                object.remove(key);
            } else {
                merge_patch(object.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    } else {
        *target = patch.clone();
    }
}
/// Bounded dedup pruning; operation UUID age still rejects replays after the row is gone.
pub fn purge_operations(
    tx: &Transaction<'_>,
    now_ms: u64,
    limit: usize,
) -> rusqlite::Result<usize> {
    if limit == 0 || limit > 500 {
        return Err(invalid("invalid operation purge batch"));
    }
    tx.execute("DELETE FROM _ops WHERE op_id IN(SELECT op_id FROM _ops WHERE expires_at_ms<?1 ORDER BY expires_at_ms LIMIT ?2)",
        params![integer(now_ms)?,limit as i64])
}
#[cfg(test)]
#[path = "subscription_tests.rs"]
mod tests;
