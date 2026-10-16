//! Native ingress for §16.31. All state below belongs to the ONE Store writer.
//! Transport supplies the authenticated participant/session identity; no caller-selected
//! starting cursor is accepted. A reconnect reuses that identity; a new session uses a new one.
use super::{
    apply_write, initialize_operations, integer, invalid, operation_time, protocol::*, rejected,
    remember, request_identity, WriteAuthority, OP_HORIZON_MS,
};
use crate::{change_log::atomic, writer::WriterHandle, JobOptions, StoreError, WriteJob};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;
use std::sync::Arc;

#[path = "command_contract.rs"]
mod command;
#[path = "command_routes.rs"]
pub mod routes;
pub(super) use command::check_command_input;
pub use command::{
    activate_command_projections, backfill_command_projections, command_document_schema,
    project_command, read_command_projection, validate_migrated_command, CommandProjection,
};
pub(super) use command::{check_command, duplicate_command};
pub use command::{
    check_executor_fence, claim_command, command_lease, rejection_code, with_command_fence,
    CommandError, CommandLease,
};
pub(super) use command::{command_can_remove, retire_command};
pub const GAP_MS: u64 = 30_000;
pub const MAX_WAITING: usize = 64;
pub const MAX_WAITING_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 512 * 1024;

/// No terminal response is fabricated for a buffered write. Results may include earlier
/// buffered writes drained by this arrival. Send all of them through P5's reliable lane.
/// On next_deadline_ms arm ONE deadline event, call expire(), replace/cancel that event
/// after each batch. No periodic polling, and an idle connection still expires its gap.
#[derive(Debug, Clone)]
pub struct WriteBatch {
    pub results: Vec<OrderedWriteResult>,
    pub next_seq: Seq,
    pub next_deadline_ms: Option<u64>,
}

#[derive(Clone)]
pub struct OrderedWriter {
    writer: WriterHandle,
    authority: Arc<dyn WriteAuthority>,
}
/// Core adapters already running on this writer use the SAME ordered ingress,
/// rather than submitting back to the writer (deadlock) or writing tables
/// directly (bypassing admission/history/dedup). No connection is opened here.
/// A completion publisher must require its own accepted result in the batch;
/// buffering/rejection is not a successfully published command completion.
pub fn submit_in_transaction(
    tx: &Transaction<'_>,
    participant: &str,
    write: &OrderedWrite,
    now_ms: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<WriteBatch> {
    ingest(tx, participant, write, now_ms, authority)
}
impl OrderedWriter {
    pub fn new(writer: WriterHandle, authority: Arc<dyn WriteAuthority>) -> Self {
        Self { writer, authority }
    }
    /// Admission, order, merge, document, _revs and both dedup ledgers commit atomically.
    pub async fn submit(
        &self,
        participant: String,
        write: OrderedWrite,
        now_ms: u64,
    ) -> Result<WriteBatch, StoreError> {
        let authority = self.authority.clone();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                submit_in_transaction(tx, &participant, &write, now_ms, authority.as_ref())
                    .map_err(Into::into)
            }))?
            .await
    }
    /// At startup enumerate the durable deadlines once, then arm the same
    /// one-shot events used by submit; no watch/poll recovery mechanism.
    pub async fn recover_deadlines(
        &self,
        after: Option<String>,
        limit: usize,
    ) -> Result<Vec<(String, u64)>, StoreError> {
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                initialize_operations(tx)?;
                // initialize schema without minting a participant or resetting a cursor.
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_write_pending')",
                    [],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Ok(Vec::new());
                }
                pending_deadlines(tx, after.as_deref(), limit).map_err(Into::into)
            }))?
            .await
    }
    pub async fn expire(&self, participant: String, now_ms: u64) -> Result<WriteBatch, StoreError> {
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                atomic(tx, || {
                    let (next, now) = initialize(tx, &participant, now_ms)?;
                    let (next, results) = expire_waiting(tx, &participant, next, now)?;
                    batch(tx, &participant, next, results)
                })
                .map_err(Into::into)
            }))?
            .await
    }
}
fn initialize(tx: &Transaction<'_>, participant: &str, now: u64) -> rusqlite::Result<(u64, u64)> {
    if participant.is_empty() || participant.len() > 512 {
        return Err(invalid("invalid participant"));
    }
    initialize_operations(tx)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _write_order(
        participant TEXT PRIMARY KEY NOT NULL,next_seq INTEGER NOT NULL CHECK(next_seq>0),last_now_ms INTEGER NOT NULL) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS _write_pending(
        participant TEXT NOT NULL,client_seq INTEGER NOT NULL,request_json TEXT NOT NULL,
        arrived_at_ms INTEGER NOT NULL,byte_len INTEGER NOT NULL,PRIMARY KEY(participant,client_seq)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS _write_results(
        participant TEXT NOT NULL,client_seq INTEGER NOT NULL,op_id TEXT NOT NULL,result_json TEXT NOT NULL,
        expires_at_ms INTEGER NOT NULL,PRIMARY KEY(participant,client_seq)) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS _write_results_expiry ON _write_results(expires_at_ms);")?;
    tx.execute(
        "INSERT OR IGNORE INTO _write_order VALUES(?1,1,?2)",
        params![participant, integer(now)?],
    )?;
    tx.execute(
        "UPDATE _write_order SET last_now_ms=MAX(last_now_ms,?2) WHERE participant=?1",
        params![participant, integer(now)?],
    )?;
    tx.query_row(
        "SELECT next_seq,last_now_ms FROM _write_order WHERE participant=?1",
        [participant],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}
fn record(
    tx: &Transaction<'_>,
    participant: &str,
    seq: u64,
    result: &WriteResult,
    now: u64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO _write_results VALUES(?1,?2,?3,?4,?5)",
        params![
            participant,
            integer(seq)?,
            result.op_id,
            serde_json::to_string(result).map_err(|e| invalid(e.to_string()))?,
            integer(
                now.checked_add(OP_HORIZON_MS)
                    .ok_or_else(|| invalid("expiry overflow"))?
            )?
        ],
    )?;
    Ok(())
}
fn prior(
    tx: &Transaction<'_>,
    participant: &str,
    seq: u64,
    now: u64,
) -> rusqlite::Result<Option<WriteResult>> {
    let row:Option<(String,u64)>=tx.query_row(
        "SELECT result_json,expires_at_ms FROM _write_results WHERE participant=?1 AND client_seq=?2",
        params![participant,integer(seq)?],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    row.map(|(json, expires)| {
        let mut result: WriteResult =
            serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?;
        let op_expired = operation_time(&result.op_id)
            .is_ok_and(|issued| now.saturating_sub(issued) > OP_HORIZON_MS);
        if now > expires || op_expired {
            result.status = WriteStatus::Rejected;
            result.reason = Some("expired".into());
            result.code = Some("expired".into());

            result.seq = None;
            result.rev = None;
        }
        Ok(result)
    })
    .transpose()
}
fn delivery(seq: u64, result: WriteResult) -> OrderedWriteResult {
    let requires_page = matches!(result.status, WriteStatus::Merged | WriteStatus::Migrated);
    OrderedWriteResult {
        client_seq: Seq(seq),
        requires_page,
        result,
    }
}
fn terminal(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    reason: &str,
    now: u64,
) -> rusqlite::Result<WriteResult> {
    // An expiry is terminal for op_id too; changing only client_seq cannot resurrect it.
    if operation_time(&request.op_id).is_ok_and(|issued| now.saturating_sub(issued) > OP_HORIZON_MS)
    {
        return Ok(rejected(request, "expired"));
    }
    let old: Option<(String, String, String)> = tx
        .query_row(
            "SELECT participant,request_json,result_json FROM _ops WHERE op_id=?1",
            [&request.op_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((owner, identity, json)) = old {
        if owner == participant && identity == request_identity(request)? {
            return serde_json::from_str(&json).map_err(|e| invalid(e.to_string()));
        }
        return Ok(rejected(request, "op_id_conflict"));
    }
    let result = rejected(request, reason);
    remember(tx, participant, request, &result, now)?;
    Ok(result)
}
fn set_next(tx: &Transaction<'_>, participant: &str, next: u64) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE _write_order SET next_seq=?2 WHERE participant=?1",
        params![participant, integer(next)?],
    )?;
    Ok(())
}
fn batch(
    tx: &Transaction<'_>,
    participant: &str,
    next: u64,
    results: Vec<OrderedWriteResult>,
) -> rusqlite::Result<WriteBatch> {
    let oldest: Option<u64> = tx.query_row(
        "SELECT MIN(arrived_at_ms) FROM _write_pending WHERE participant=?1",
        [participant],
        |r| r.get(0),
    )?;
    Ok(WriteBatch {
        results,
        next_seq: Seq(next),
        next_deadline_ms: oldest.map(|t| t.saturating_add(GAP_MS).saturating_add(1)),
    })
}
fn expire_waiting(
    tx: &Transaction<'_>,
    participant: &str,
    next: u64,
    now: u64,
) -> rusqlite::Result<(u64, Vec<OrderedWriteResult>)> {
    let oldest: Option<u64> = tx.query_row(
        "SELECT MIN(arrived_at_ms) FROM _write_pending WHERE participant=?1",
        [participant],
        |r| r.get(0),
    )?;
    if !oldest.is_some_and(|t| now.saturating_sub(t) > GAP_MS) {
        return Ok((next, vec![]));
    }
    let waiting:Vec<(u64,String)>=tx.prepare("SELECT client_seq,request_json FROM _write_pending WHERE participant=?1 ORDER BY client_seq")?
        .query_map([participant],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut results = Vec::new();
    let mut next = next;
    for (seq, json) in waiting {
        let request: Write = serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?;
        let result = terminal(tx, participant, &request, "expired", now)?;
        record(tx, participant, seq, &result, now)?;
        results.push(delivery(seq, result));
        next = seq
            .checked_add(1)
            .ok_or_else(|| invalid("client_seq exhausted"))?;
    }
    // Missing lower sequences are fenced forever, including after result GC.
    set_next(tx, participant, next)?;
    tx.execute(
        "DELETE FROM _write_pending WHERE participant=?1",
        [participant],
    )?;
    Ok((next, results))
}
fn ingest(
    tx: &Transaction<'_>,
    participant: &str,
    ordered: &OrderedWrite,
    now: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<WriteBatch> {
    atomic(tx, || {
        let (next, now) = initialize(tx, participant, now)?;
        let seq = ordered.client_seq.0;
        if seq == 0 || seq >= i64::MAX as u64 {
            return Err(invalid("invalid client_seq"));
        }
        let (mut next, mut results) = expire_waiting(tx, participant, next, now)?;
        if let Some(result) = prior(tx, participant, seq, now)? {
            results.push(delivery(seq, result));
            return batch(tx, participant, next, results);
        }
        let request = &ordered.write;
        if seq < next {
            results.push(delivery(seq, rejected(request, "expired")));
            return batch(tx, participant, next, results);
        }
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _write_pending WHERE participant=?1 AND client_seq=?2)",
            params![participant, integer(seq)?],
            |r| r.get(0),
        )?;
        if !pending {
            let json = serde_json::to_string(request).map_err(|e| invalid(e.to_string()))?;
            let (count,bytes):(usize,usize)=tx.query_row(
                "SELECT COUNT(*),COALESCE(SUM(byte_len),0) FROM _write_pending WHERE participant=?1",[participant],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let issued = operation_time(&request.op_id);
            let reason = if request.collection == "business_commands"
                && request
                    .body
                    .as_ref()
                    .is_some_and(|body| routes::validate_persisted_input(body).is_err())
            {
                // terminal()/remember() retain only request identity (no
                // body) and the typed rejection. Never enqueue these bytes.
                Some("credential_ref_required")
            } else if request.id.is_empty() || request.collection.is_empty() || request.gen == 0 {
                Some("invalid_input")
            } else if issued.is_err() {
                Some("invalid_op_id")
            } else if issued
                .as_ref()
                .is_ok_and(|t| *t > now.saturating_add(300_000))
            {
                Some("invalid_time")
            } else if issued
                .as_ref()
                .is_ok_and(|t| now.saturating_sub(*t) > OP_HORIZON_MS)
            {
                Some("expired")
            } else if json.len() > MAX_REQUEST_BYTES
                || (seq > next
                    && (count >= MAX_WAITING
                        || bytes.saturating_add(json.len()) > MAX_WAITING_BYTES))
            {
                Some("expired")
            } else {
                None
            };
            if let Some(reason) = reason {
                let result = terminal(tx, participant, request, reason, now)?;
                record(tx, participant, seq, &result, now)?;
                results.push(delivery(seq, result));
            } else {
                if let super::WritePermission::Reject(reason) =
                    authority.authorize(tx, participant, request)?
                {
                    let result = terminal(tx, participant, request, &reason, now)?;
                    record(tx, participant, seq, &result, now)?;
                    results.push(delivery(seq, result));
                } else {
                    tx.execute(
                        "INSERT INTO _write_pending VALUES(?1,?2,?3,?4,?5)",
                        params![
                            participant,
                            integer(seq)?,
                            json,
                            integer(now)?,
                            json.len() as i64
                        ],
                    )?;
                }
            }
        }
        loop {
            // Terminal rejections at a future client_seq still occupy their order slot.
            if prior(tx, participant, next, now)?.is_some() {
                next += 1;
                set_next(tx, participant, next)?;
                continue;
            }
            let json:Option<String>=tx.query_row("SELECT request_json FROM _write_pending WHERE participant=?1 AND client_seq=?2",
                params![participant,integer(next)?],|r|r.get(0)).optional()?;
            let Some(json) = json else { break };
            let request: Write = serde_json::from_str(&json).map_err(|e| invalid(e.to_string()))?;
            // In a strictly ordered inbox a never-recorded predecessor is not a
            // transport gap: that predecessor did not occupy an earlier valid slot.
            let known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM _ops WHERE op_id=?1)",
                [&request.op_id],
                |r| r.get(0),
            )?;
            let invalid_previous = if known {
                false
            } else if let Some(previous) = &request.prev_op_id {
                match operation_time(previous) {
                    Err(_) => true,
                    // Preserve the original expired classification even after _ops GC.
                    Ok(issued) if now.saturating_sub(issued) > OP_HORIZON_MS => false,
                    Ok(_) => !tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM _ops WHERE op_id=?1)",
                        [previous],
                        |r| r.get::<_, bool>(0),
                    )?,
                }
            } else {
                false
            };
            let result = if invalid_previous {
                terminal(tx, participant, &request, "invalid_predecessor", now)?
            } else {
                apply_write(tx, participant, &request, now, authority)?
            };
            record(tx, participant, next, &result, now)?;
            // Never clear a buffered input before its durable terminal result is stored.
            tx.execute(
                "DELETE FROM _write_pending WHERE participant=?1 AND client_seq=?2",
                params![participant, integer(next)?],
            )?;
            results.push(delivery(next, result));
            next += 1;
            set_next(tx, participant, next)?;
        }
        batch(tx, participant, next, results)
    })
}
/// Startup recovery is a bounded keyset walk, not a timer/poll loop.
pub fn pending_deadlines(
    tx: &Transaction<'_>,
    after: Option<&str>,
    limit: usize,
) -> rusqlite::Result<Vec<(String, u64)>> {
    if limit == 0 || limit > 500 {
        return Err(invalid("invalid deadline page"));
    }
    let mut stmt = tx.prepare(
        "SELECT participant,MIN(arrived_at_ms) FROM _write_pending
        WHERE participant>COALESCE(?1,'') GROUP BY participant ORDER BY participant LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![after, limit as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
    })?;
    rows.map(|r| r.map(|(p, t)| (p, t.saturating_add(GAP_MS).saturating_add(1))))
        .collect()
}
/// Bounded result GC never removes pending inputs or the permanent sequence fence.
pub fn purge_results(tx: &Transaction<'_>, now_ms: u64, limit: usize) -> rusqlite::Result<usize> {
    if limit == 0 || limit > 500 {
        return Err(invalid("invalid result purge batch"));
    }
    tx.execute("DELETE FROM _write_results WHERE (participant,client_seq) IN(
        SELECT participant,client_seq FROM _write_results WHERE expires_at_ms<?1 ORDER BY expires_at_ms LIMIT ?2)",
        params![integer(now_ms)?,limit as i64])
}

/// Recursive three-way field merge, including missing != null and field deletion.
/// A field changed on both sides keeps the server value. Arrays/scalars are atomic.
pub(super) fn merge_fields(base: &Value, server: &Value, client: &Value) -> Value {
    fn field(
        base: Option<&Value>,
        server: Option<&Value>,
        client: Option<&Value>,
    ) -> Option<Value> {
        if server == base {
            return client.cloned();
        }
        if client == base {
            return server.cloned();
        }
        match (base, server, client) {
            (Some(Value::Object(b)), Some(Value::Object(s)), Some(Value::Object(c))) => {
                let keys: std::collections::BTreeSet<_> =
                    b.keys().chain(s.keys()).chain(c.keys()).collect();
                let mut out = serde_json::Map::new();
                for key in keys {
                    if let Some(value) = field(b.get(key), s.get(key), c.get(key)) {
                        out.insert(key.clone(), value);
                    }
                }
                Some(Value::Object(out))
            }
            _ => server.cloned(),
        }
    }
    field(Some(base), Some(server), Some(client)).unwrap_or(Value::Null)
}

#[cfg(test)]
#[path = "write_contract_tests.rs"]
mod tests;
