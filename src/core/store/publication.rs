//! Durable, idempotent A -> B publication; no cross-file transaction or wait.
//! Native handlers are short synchronous SQLite operations. The dispatcher awaits
//! jobs only outside transactions. It is driven by the daemon, not by a polling
//! thread hidden in this crate.
use crate::{writer::WriterHandle, JobOptions, StoreError, WriteJob};
use rusqlite::{params, OptionalExtension, Transaction};
use std::sync::Arc;

const MAX_BODY: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    pub op_id: String,
    pub target: String,
    pub kind: String,
    /// Reference to current native authority, not a credential or cached grant.
    pub subject: String,
    pub payload: Vec<u8>,
}
impl Publication {
    fn validate(&self) -> Result<(), StoreError> {
        if [&self.op_id, &self.target, &self.kind, &self.subject]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256)
            || self.payload.len() > MAX_BODY
        {
            return Err(StoreError::Invalid("invalid publication envelope".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Forbidden,
    Revoked,
    Expired,
    Conflict,
    Invalid,
    RetryExhausted,
}
impl Rejection {
    fn code(self) -> &'static str {
        match self {
            Self::Forbidden => "forbidden",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::Conflict => "conflict",
            Self::Invalid => "invalid",
            Self::RetryExhausted => "retry_exhausted",
        }
    }
}

/// The host must acquire current policy/account/executor authority on B's writer.
/// If authority lives outside B, this guard MUST fence mutations until dropped.
/// Policy stored in B itself is already fenced by B's single IMMEDIATE writer.
/// It is retained in the job result through commit (also on rollback/panic).
pub struct AuthorityPermit {
    _guard: Box<dyn Send>,
}
impl AuthorityPermit {
    pub fn holding(guard: impl Send + 'static) -> Self {
        Self {
            _guard: Box::new(guard),
        }
    }
}
/// Unavailable is retryable: guard acquisition must not block a writer while
/// another authority/store queue needs to make progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    Rejected(Rejection),
    Unavailable,
}

pub trait CurrentAuthority: Send + Sync + 'static {
    fn acquire(
        &self,
        tx: &Transaction<'_>,
        publication: &Publication,
    ) -> Result<AuthorityPermit, AuthorityError>;
}
pub type Apply =
    Arc<dyn Fn(&Transaction<'_>, &Publication) -> Result<(), StoreError> + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: i64,
    pub max_delay_ms: i64,
    pub lease_ms: i64,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            base_delay_ms: 250,
            max_delay_ms: 30_000,
            lease_ms: 30_000,
        }
    }
}
impl RetryPolicy {
    fn validate(self) -> Result<(), StoreError> {
        if self.max_attempts == 0
            || self.base_delay_ms < 0
            || self.max_delay_ms < self.base_delay_ms
            || self.lease_ms <= 0
        {
            return Err(StoreError::Invalid(
                "invalid publication retry policy".into(),
            ));
        }
        Ok(())
    }
    fn delay(self, attempt: u32) -> i64 {
        self.base_delay_ms
            .saturating_mul(1i64 << attempt.saturating_sub(1).min(30))
            .min(self.max_delay_ms)
    }
}

#[derive(Clone, Debug)]
pub struct Claim {
    pub publication: Publication,
    pub created_at_ms: i64,
    /// Fencing token: monotonically increased in A, never reused for this op_id.
    pub attempt: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Idle,
    Applied,
    AlreadyApplied,
    Retrying,
    DeadLetter(Rejection),
    LostClaim,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Inserted,
    Existing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub state: String,
    pub attempts: u32,
    pub next_attempt_ms: i64,
    pub error_code: Option<String>,
}

/// Run on each participating writer once during database registration. Hot-plug
/// callers can install these private tables in their normal schema transaction.
pub fn install(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _publication_outbox (
        op_id TEXT PRIMARY KEY, target TEXT NOT NULL, kind TEXT NOT NULL, subject TEXT NOT NULL, payload BLOB NOT NULL,
        created_at_ms INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('pending','inflight','delivered','dead_letter')),
        attempts INTEGER NOT NULL DEFAULT 0, next_attempt_ms INTEGER NOT NULL, lease_until_ms INTEGER NOT NULL DEFAULT 0,
        error_code TEXT);
        CREATE INDEX IF NOT EXISTS _publication_pending ON _publication_outbox(target,state,next_attempt_ms,op_id);
        CREATE INDEX IF NOT EXISTS _publication_leased ON _publication_outbox(target,state,lease_until_ms,op_id);
        CREATE TABLE IF NOT EXISTS _publication_inbox (
        op_id TEXT PRIMARY KEY, target TEXT NOT NULL, kind TEXT NOT NULL, subject TEXT NOT NULL, payload BLOB NOT NULL,
        created_at_ms INTEGER NOT NULL, applied_at_ms INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS _publication_age ON _publication_inbox(created_at_ms,op_id);
        CREATE TABLE IF NOT EXISTS _publication_horizon (id INTEGER PRIMARY KEY CHECK(id=1), accept_since_ms INTEGER NOT NULL);
        INSERT OR IGNORE INTO _publication_horizon VALUES(1,0);")?;
    Ok(())
}

/// Repeating an ID with identical content skips the domain mutation. The first
/// mutation and the outbox insert commit together, or both roll back.
pub async fn stage(
    source: &WriterHandle,
    options: JobOptions,
    publication: Publication,
    now_ms: i64,
    mutate: impl FnOnce(&Transaction<'_>) -> Result<(), StoreError> + Send + 'static,
) -> Result<Stage, StoreError> {
    publication.validate()?;
    source.submit(WriteJob::new(options, move |tx| {
        let existing = tx.query_row("SELECT target,kind,subject,payload FROM _publication_outbox WHERE op_id=?", [&publication.op_id], read_publication(&publication.op_id)).optional()?;
        if let Some(existing) = existing {
            return if existing == publication { Ok(Stage::Existing) } else { Err(StoreError::Invalid("publication op_id conflict".into())) };
        }
        mutate(tx)?;
        tx.execute("INSERT INTO _publication_outbox(op_id,target,kind,subject,payload,created_at_ms,state,next_attempt_ms) VALUES(?,?,?,?,?,?,'pending',?)",
            params![publication.op_id,publication.target,publication.kind,publication.subject,publication.payload,now_ms,now_ms])?;
        Ok(Stage::Inserted)
    }))?.await
}

fn read_publication(
    op_id: &str,
) -> impl FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<Publication> + '_ {
    move |row| {
        Ok(Publication {
            op_id: op_id.to_owned(),
            target: row.get(0)?,
            kind: row.get(1)?,
            subject: row.get(2)?,
            payload: row.get(3)?,
        })
    }
}

// Separate indexed top-one queries avoid OR + sorting every ready row while
// holding the only writer. Recoverable leases and pending work each cost O(log n).
const PENDING_SQL: &str = "SELECT op_id,target,kind,subject,payload,created_at_ms,attempts FROM _publication_outbox WHERE target=? AND state='pending' AND next_attempt_ms<=? ORDER BY next_attempt_ms,op_id LIMIT 1";
const LEASED_SQL: &str = "SELECT op_id,target,kind,subject,payload,created_at_ms,attempts FROM _publication_outbox WHERE target=? AND state='inflight' AND lease_until_ms<=? ORDER BY lease_until_ms,op_id LIMIT 1";
fn candidate(
    tx: &Transaction<'_>,
    sql: &str,
    target: &str,
    now_ms: i64,
) -> Result<Option<(Publication, i64, u32)>, StoreError> {
    Ok(tx
        .query_row(sql, params![target, now_ms], |r| {
            Ok((
                Publication {
                    op_id: r.get(0)?,
                    target: r.get(1)?,
                    kind: r.get(2)?,
                    subject: r.get(3)?,
                    payload: r.get(4)?,
                },
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .optional()?)
}

pub async fn claim_next(
    source: &WriterHandle,
    options: JobOptions,
    target: String,
    now_ms: i64,
    policy: RetryPolicy,
) -> Result<Option<Claim>, StoreError> {
    policy.validate()?;
    source.submit(WriteJob::new(options, move |tx| {
        let pending=candidate(tx,PENDING_SQL,&target,now_ms)?;
        let leased=candidate(tx,LEASED_SQL,&target,now_ms)?;
        let row=pending.into_iter().chain(leased).min_by_key(|(publication,created,_)|(*created,publication.op_id.clone()));
        let Some((publication, created_at_ms, previous)) = row else { return Ok(None); };
        // An abandoned attempt may have committed on B. It must still be
        // re-delivered once to inspect B's dedupe receipt, even at retry limit.
        let attempt = previous.checked_add(1).ok_or_else(|| StoreError::Invalid("attempt counter exhausted".into()))?;
        tx.execute("UPDATE _publication_outbox SET state='inflight',attempts=?,lease_until_ms=? WHERE op_id=?",
            params![attempt,now_ms.saturating_add(policy.lease_ms),publication.op_id])?;
        Ok(Some(Claim { publication, created_at_ms, attempt }))
    }))?.await
}

struct TargetResult {
    delivery: Delivery,
    _permit: Option<AuthorityPermit>,
}
async fn apply_claim(
    target: &WriterHandle,
    options: JobOptions,
    claim: Claim,
    authority: Arc<dyn CurrentAuthority>,
    apply: Apply,
    now_ms: i64,
) -> Result<Delivery, StoreError> {
    let result = target.submit(WriteJob::new(options, move |tx| {
        // This callback executes after dequeue, never on a captured pre-await
        // policy snapshot. The permit stays alive through the outer commit.
        let permit = match authority.acquire(tx, &claim.publication) {
            Ok(permit) => permit,
            Err(AuthorityError::Unavailable) => return Err(StoreError::Busy),
            Err(AuthorityError::Rejected(reason)) => return Ok(TargetResult { delivery: Delivery::DeadLetter(reason), _permit: None }),
        };
        let publication = &claim.publication;
        let floor: i64 = tx.query_row("SELECT accept_since_ms FROM _publication_horizon WHERE id=1", [], |r| r.get(0))?;
        if claim.created_at_ms < floor { return Ok(TargetResult { delivery: Delivery::DeadLetter(Rejection::Expired), _permit: Some(permit) }); }
        let existing = tx.query_row("SELECT target,kind,subject,payload,created_at_ms FROM _publication_inbox WHERE op_id=?", [&publication.op_id], |row| {
            Ok(((read_publication(&publication.op_id))(row)?,row.get::<_,i64>(4)?))
        }).optional()?;
        let delivery = if let Some((existing, created)) = existing {
            if existing != *publication || created != claim.created_at_ms { Delivery::DeadLetter(Rejection::Conflict) } else { Delivery::AlreadyApplied }
        } else {
            apply(tx, publication)?;
            tx.execute("INSERT INTO _publication_inbox VALUES(?,?,?,?,?,?,?)", params![publication.op_id,publication.target,publication.kind,publication.subject,publication.payload,claim.created_at_ms,now_ms])?;
            Delivery::Applied
        };
        Ok(TargetResult { delivery, _permit: Some(permit) })
    }))?.await?;
    // Drop the policy fence BEFORE submitting the receipt job on A. No fence
    // or transaction on B is held while awaiting A's writer.
    let delivery = result.delivery;
    drop(result);
    Ok(delivery)
}

pub async fn finish(
    source: &WriterHandle,
    options: JobOptions,
    claim: Claim,
    delivery: Delivery,
    now_ms: i64,
    policy: RetryPolicy,
) -> Result<Delivery, StoreError> {
    policy.validate()?;
    source.submit(WriteJob::new(options, move |tx| {
        let delivery = if delivery == Delivery::Retrying && claim.attempt >= policy.max_attempts { Delivery::DeadLetter(Rejection::RetryExhausted) } else { delivery };
        let (state,error) = match delivery {
            Delivery::Applied | Delivery::AlreadyApplied => ("delivered",None),
            Delivery::DeadLetter(reason) => ("dead_letter",Some(reason.code())),
            Delivery::Retrying => ("pending",Some("retryable")),
            _ => return Err(StoreError::Invalid("invalid delivery completion".into())),
        };
        let changed = tx.execute("UPDATE _publication_outbox SET state=?,error_code=?,next_attempt_ms=?,lease_until_ms=0 WHERE op_id=? AND state='inflight' AND attempts=?",
            params![state,error,now_ms.saturating_add(policy.delay(claim.attempt)),claim.publication.op_id,claim.attempt])?;
        Ok(if changed == 1 { delivery } else { Delivery::LostClaim })
    }))?.await
}

/// Native receiver bindings, resolved by the daemon rather than supplied over IPC.
pub struct Receiver {
    pub authority: Arc<dyn CurrentAuthority>,
    pub apply: Apply,
}

/// One bounded turn. The daemon schedules subsequent turns from source commit
/// notifications / persisted retry deadlines. Crashes after B's commit and before
/// A's receipt recover by lease expiry and B's idempotency record.
pub async fn deliver_one(
    source: &WriterHandle,
    target: &WriterHandle,
    target_name: String,
    options: JobOptions,
    receiver: Receiver,
    now_ms: i64,
    policy: RetryPolicy,
) -> Result<Delivery, StoreError> {
    let Some(claim) = claim_next(source, options.clone(), target_name, now_ms, policy).await?
    else {
        return Ok(Delivery::Idle);
    };
    let delivery = match apply_claim(
        target,
        options.clone(),
        claim.clone(),
        receiver.authority,
        receiver.apply,
        now_ms,
    )
    .await
    {
        Ok(delivery) => delivery,
        Err(StoreError::Invalid(_)) => Delivery::DeadLetter(Rejection::Invalid),
        Err(_) => Delivery::Retrying,
    };
    // If original cancellation/deadline expired after B committed, leave the
    // durable claim for recovery rather than acknowledging through a stale job.
    finish(source, options, claim, delivery, now_ms, policy).await
}

pub async fn status(
    source: &WriterHandle,
    options: JobOptions,
    op_id: String,
) -> Result<Option<Status>, StoreError> {
    source.submit(WriteJob::new(options, move |tx| Ok(tx.query_row("SELECT state,attempts,next_attempt_ms,error_code FROM _publication_outbox WHERE op_id=?", [op_id], |r| Ok(Status {state:r.get(0)?,attempts:r.get(1)?,next_attempt_ms:r.get(2)?,error_code:r.get(3)?})).optional()?)))?.await
}

/// Persist a monotonic acceptance floor before compacting old dedupe receipts.
/// Old IDs stay expired after compaction; never blindly execute them again.
pub async fn advance_horizon(
    target: &WriterHandle,
    options: JobOptions,
    accept_since_ms: i64,
) -> Result<(), StoreError> {
    target
        .submit(WriteJob::new(options, move |tx| {
            tx.execute(
                "UPDATE _publication_horizon SET accept_since_ms=MAX(accept_since_ms,?) WHERE id=1",
                [accept_since_ms],
            )?;
            Ok(())
        }))?
        .await
}

/// Bounded maintenance: at most 256 indexed receipts per transaction. Advancing
/// the durable horizon already fences old IDs; compaction never re-enables them.
pub async fn compact_receipts(
    target: &WriterHandle,
    options: JobOptions,
) -> Result<usize, StoreError> {
    target.submit(WriteJob::new(options,move |tx| Ok(tx.execute("DELETE FROM _publication_inbox WHERE op_id IN (SELECT op_id FROM _publication_inbox WHERE created_at_ms < (SELECT accept_since_ms FROM _publication_horizon WHERE id=1) ORDER BY created_at_ms,op_id LIMIT 256)",[])?)))?.await
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
