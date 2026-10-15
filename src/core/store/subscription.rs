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
    fn authorize(&self, tx: &Transaction<'_>, key: &SubscriptionKey) -> rusqlite::Result<()>;
    fn project(
        &self,
        tx: &Transaction<'_>,
        key: &SubscriptionKey,
        doc: &ChangeDocument,
    ) -> rusqlite::Result<Option<Value>>;
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
    pub async fn next_page(
        &mut self,
        limit: usize,
        authority: Arc<dyn ReadAuthority>,
        signer: &dyn InstanceSigner,
        core_sent_at_ms: impl FnOnce() -> u64 + Send,
    ) -> Result<StreamPacket, StoreError> {
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
        let (page, docs) = self
            .reader
            .read(move |tx| {
                authority.authorize(tx, &key)?;
                let page = change_log::read_page(tx, &key.collection, key.gen, cursor, limit)?;
                if epoch.as_ref().is_some_and(|e| e != &page.epoch) {
                    return Err(invalid("epoch changed: resnapshot required"));
                }
                let docs = page
                    .docs
                    .iter()
                    .map(|doc| {
                        let body = if doc.op == Operation::Delete {
                            None
                        } else {
                            authority.project(tx, &key, doc)?
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
                Ok((page, docs))
            })
            .await
            .map_err(StoreError::from)?;
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
        Ok(packet)
    }
}

/// Native authority is mandatory, including replay and removes. It may enforce
/// write_guard/schema validation and resolve old-schema input through the owned migration runner.
pub trait WriteAuthority: Send + Sync {
    fn authorize(
        &self,
        tx: &Transaction<'_>,
        participant: &str,
        request: &Write,
    ) -> rusqlite::Result<()>;
    /// Trusted semantic migration seam. None rejects stale schema/generation input.
    /// Implementations must use already prepared, revision-bound output; no I/O or JS in the writer.
    fn validate_document(
        &self,
        tx: &Transaction<'_>,
        participant: &str,
        collection: &crate::schema::Collection,
        document: &Value,
    ) -> rusqlite::Result<()>;
    fn migrate(
        &self,
        _tx: &Transaction<'_>,
        _participant: &str,
        _request: &Write,
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
pub fn apply_write(
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
        authority.authorize(tx, participant, request)?;
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
        let mut base = request.base_rev.clone();
        if let Some(previous) = &request.prev_op_id {
            if previous == &request.op_id || base.as_deref() != Some(&format!("op:{previous}")) {
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
                if now_ms.saturating_sub(operation_time(previous)?) > OP_HORIZON_MS {
                    return Ok(rejected(request, "expired"));
                }
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
            authority.migrate(tx, participant, request, &current)?
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
        // A removed revision cannot resurrect content; an explicit create (no base) may start a new identity revision.
        let conflict = match &row {
            Some((rev, _, deleted)) => {
                if *deleted {
                    request.op != WriteOp::Put || base.is_some()
                } else {
                    base.as_ref() != Some(rev)
                }
            }
            None => base.is_some() || request.op != WriteOp::Put,
        };
        if conflict {
            let result = rejected(request, "conflict");
            remember(tx, participant, request, &result, now_ms)?;
            return Ok(result);
        }
        if let Some(seq) = request.base_seq {
            if request.prev_op_id.is_none() && !stale {
                let actual: Option<u64> = tx
                    .query_row(
                        "SELECT seq FROM _changes WHERE collection=?1 AND gen=?2 AND id=?3",
                        params![request.collection, integer(current.gen)?, request.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if actual != Some(seq.0) {
                    let result = rejected(request, "conflict");
                    remember(tx, participant, request, &result, now_ms)?;
                    return Ok(result);
                }
            }
        }
        let rev = format!("v4:{}", request.op_id);
        let body = if let Some(m) = &migration {
            m.body.clone()
        } else {
            request.body.clone()
        };
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
                let prior = row
                    .as_ref()
                    .and_then(|r| r.1.as_ref())
                    .ok_or_else(|| invalid("patch base unavailable"))?;
                let mut doc: Value =
                    serde_json::from_str(prior).map_err(|e| invalid(e.to_string()))?;
                merge_patch(&mut doc, &input);
                doc
            } else {
                input
            };
            if !document.is_object() {
                let result = rejected(request, "object_required");
                remember(tx, participant, request, &result, now_ms)?;
                return Ok(result);
            }
            authority.validate_document(tx, participant, &current, &document)?;
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
        let result = WriteResult {
            op_id: request.op_id.clone(),
            status: if stale {
                WriteStatus::Migrated
            } else {
                WriteStatus::Accepted
            },
            seq: Some(Seq(seq)),
            rev: Some(rev.clone()),
            gen: current.gen,
            reason: None,
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
