//! P9: command invariants run inside OrderedWriter's existing transaction.
use super::super::WritePermission;
use super::*;
use serde::{Deserialize, Serialize};

/// Canonical V4 document schema for the platform release publisher. This is an
/// incompatible replacement of the V3 module/command_type shape: publish via
/// the normal generation migration, never silently rewrite an active registry.
pub fn command_document_schema() -> Value {
    serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/business-command-document-v4.json"
    ))
    .expect("checked-in command schema")
}

const TERMINAL: &[&str] = &["done", "failed", "cancelled", "rejected"];
const NATIVE_INITIAL: &[&str] = &[
    "execution_mode",
    "execution_task_id",
    "target_task_id",
    "target_record_id",
    "replication_phase",
    "execution_phase",
    "terminal_status",
    "projection_version",
    "attempt",
    "result_ref",
    "error_code",
    "error_message",
    "retryable",
    "progress",
];
pub fn command_can_remove(body: Option<&str>) -> rusqlite::Result<bool> {
    let value: Option<Value> = body
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| invalid(e.to_string()))?;
    Ok(value
        .as_ref()
        .is_some_and(|doc| TERMINAL.contains(&state(doc))))
}
pub fn retire_command(
    tx: &Transaction<'_>,
    id: &str,
    gen: u64,
    rev: &str,
    seq: u64,
) -> rusqlite::Result<()> {
    initialize_commands(tx)?;
    tx.execute("UPDATE _command_projections SET source_seq=?2,source_rev=?3 WHERE command_id=?1 AND gen=?4",params![id,integer(seq)?,rev,integer(gen)?])?;
    tx.execute(
        "UPDATE _command_leases SET expires_at_core=0 WHERE command_id=?1",
        [id],
    )?;
    Ok(())
}
/// Explicit command-reader cutover, called by the integrator in the ONE writer.
/// Registry presence is not authority: P3 may prepare tables while legacy still owns writes.
/// Backfill batches first; this final CAS verifies coverage without reading bodies.
pub fn activate_command_projections(tx: &Transaction<'_>, gen: u64) -> rusqlite::Result<()> {
    atomic(tx, || {
        initialize_commands(tx)?;
        let current = crate::schema::active_collection(tx, "business_commands")?
            .ok_or_else(|| invalid("command collection unavailable"))?;
        if current.gen != gen {
            return Err(invalid("projection generation changed"));
        }
        let table = crate::change_log::identifier(&current.table)?;
        let missing:bool=tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} d LEFT JOIN _changes c ON c.collection='business_commands' AND c.gen=?1 AND c.id=d.id LEFT JOIN _command_projections p ON p.command_id=d.id AND p.gen=?1 WHERE d.deleted=0 AND (c.seq IS NULL OR p.source_seq IS NULL OR p.source_seq!=c.seq OR p.source_rev!=d.rev))"),[integer(gen)?],|r|r.get(0))?;
        if missing {
            return Err(invalid("projection_backfill_required"));
        }
        tx.execute("INSERT INTO _command_projection_authority VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET gen=excluded.gen",[integer(gen)?])?;
        Ok(())
    })
}
/// Native queue projection input. No command body, prompt, actor or secret is copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandProjection {
    pub module: String,
    pub status: String,
    pub gen: u64,
    pub source_seq: Seq,
    pub source_rev: String,
}
/// None = pre-cutover command authority; Some(None) = activated V4 with no current projection.
/// Once explicitly activated, never fall back to the old command ledger.
/// Call on the consumer's existing read snapshot; this never opens another database.
pub fn read_command_projection(
    conn: &rusqlite::Connection,
    command_id: &str,
) -> rusqlite::Result<Option<Option<CommandProjection>>> {
    let configured:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_command_projection_authority' AND type='table')",[],|r|r.get(0))?;
    if !configured {
        return Ok(None);
    }
    let active: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM _command_projection_authority WHERE singleton=1)",
        [],
        |r| r.get(0),
    )?;
    if !active {
        return Ok(None);
    }
    let projection=conn.query_row("SELECT p.module,p.status,p.gen,p.source_seq,p.source_rev FROM _command_projections p JOIN _collections c ON c.name='business_commands' AND c.gen=p.gen AND c.state='active' JOIN _command_projection_authority a ON a.singleton=1 AND a.gen=p.gen WHERE p.command_id=?1",[command_id],|r|Ok(CommandProjection{module:r.get(0)?,status:r.get(1)?,gen:r.get(2)?,source_seq:Seq(r.get(3)?),source_rev:r.get(4)?})).optional()?;
    Ok(Some(projection))
}
/// Same writer transaction as the accepted command mutation; source_seq is the
/// command's global ordering point, not a second independent clock.
pub fn project_command(
    tx: &Transaction<'_>,
    command_id: &str,
    gen: u64,
    rev: &str,
    seq: u64,
    document: &Value,
) -> rusqlite::Result<()> {
    initialize_commands(tx)?;
    let status = match state(document) {
        "done" => "completed",
        "rejected" => "failed",
        s => s,
    };
    let module = document["app_id"]
        .as_str()
        .ok_or_else(|| invalid("command app_id missing"))?;
    tx.execute("INSERT INTO _command_projections VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(command_id) DO UPDATE SET module=excluded.module,status=excluded.status,gen=excluded.gen,source_seq=excluded.source_seq,source_rev=excluded.source_rev WHERE excluded.source_seq>_command_projections.source_seq",params![command_id,module,status,integer(gen)?,integer(seq)?,rev])?;
    Ok(())
}
/// Bootstrap/rebuild an existing V4 command generation in bounded writer batches.
/// Retry is idempotent (source seq CAS); the integrator persists the returned ID cursor.
/// During activation use this inside the migration/publication writer transaction.
pub fn backfill_command_projections(
    tx: &Transaction<'_>,
    gen: u64,
    after: Option<&str>,
    limit: usize,
) -> rusqlite::Result<(Option<String>, bool)> {
    atomic(tx, || {
        initialize_commands(tx)?;
        if limit == 0 || limit > 500 {
            return Err(invalid("projection batch limit"));
        }
        let collection = crate::schema::active_collection(tx, "business_commands")?
            .ok_or_else(|| invalid("command collection unavailable"))?;
        if collection.gen != gen {
            return Err(invalid("projection generation changed"));
        }
        let table = crate::change_log::identifier(&collection.table)?;
        let rows:Vec<(String,String,u64,String)>=tx.prepare(&format!("SELECT d.id,d.rev,c.seq,d.data FROM {table} d JOIN _changes c ON c.collection='business_commands' AND c.gen=?1 AND c.id=d.id WHERE d.deleted=0 AND (?2 IS NULL OR d.id>?2) ORDER BY d.id LIMIT ?3"))?.query_map(params![integer(gen)?,after,limit as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        let cursor = rows
            .last()
            .map(|r| r.0.clone())
            .or_else(|| after.map(str::to_owned));
        for (id, rev, seq, json) in &rows {
            let body: Value = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
            project_command(tx, id, gen, rev, *seq, &body)?;
        }
        Ok((cursor, rows.len() < limit))
    })
}
/// Fenced Store-local effect: admission, fence check and the mutation share the
/// ONE writer transaction. Never perform network/process/model/file I/O in effect.
/// Those execution points must validate the same fence at their native boundary.
pub fn with_command_fence<T>(
    tx: &Transaction<'_>,
    participant: &str,
    command_id: &str,
    fence: Seq,
    now_ms: u64,
    authority: &dyn WriteAuthority,
    effect: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Result<T, CommandError>> {
    atomic(tx, || {
        let Some(doc) = command_document(tx, command_id)? else {
            return Ok(Err(failure("not_found")));
        };
        if let WritePermission::Reject(reason) =
            authority.authorize_command_executor(tx, participant, &doc)?
        {
            return Ok(Err(failure(&reason)));
        }
        if let Err(error) = check_executor_fence(tx, participant, command_id, fence, now_ms)? {
            return Ok(Err(error));
        }
        Ok(Ok(effect(tx)?))
    })
}
const INTENT: &[&str] = &[
    "id",
    "type",
    "app_id",
    "payload",
    "record_id",
    "client_context",
    "actor_id",
    "created_at",
    "on_complete",
    "on_failed",
    "command_id",
    "idempotency_key",
    "payload_hash",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLease {
    pub command_id: String,
    pub participant: String,
    pub fence: Seq,
    pub expires_at_core: u64,
}
/// Native lease inspection for CAS recovery after a lost claim response. This is
/// metadata only and uses the same current executor admission as claim/renew.
pub fn command_lease(
    tx: &Transaction<'_>,
    participant: &str,
    command_id: &str,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<Result<Option<CommandLease>, CommandError>> {
    initialize_commands(tx)?;
    let Some(doc) = command_document(tx, command_id)? else {
        return Ok(Err(failure("not_found")));
    };
    if let WritePermission::Reject(reason) =
        authority.authorize_command_executor(tx, participant, &doc)?
    {
        return Ok(Err(failure(&reason)));
    }
    let row = tx
        .query_row(
            "SELECT participant,fence,expires_at_core FROM _command_leases WHERE command_id=?1",
            [command_id],
            |r| {
                Ok(CommandLease {
                    command_id: command_id.into(),
                    participant: r.get(0)?,
                    fence: Seq(r.get(1)?),
                    expires_at_core: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(Ok(row))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandError {
    pub status: String,
    pub code: String,
    pub reason: String,
}
fn failure(reason: &str) -> CommandError {
    CommandError {
        status: "rejected".into(),
        code: rejection_code(reason).into(),
        reason: reason.into(),
    }
}
fn initialize_commands(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _command_leases(command_id TEXT PRIMARY KEY NOT NULL, participant TEXT NOT NULL, fence INTEGER NOT NULL CHECK(fence>0), expires_at_core INTEGER NOT NULL, last_now_ms INTEGER NOT NULL,binding TEXT NOT NULL DEFAULT '') WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS _command_intents(command_id TEXT PRIMARY KEY NOT NULL, participant TEXT NOT NULL) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS _command_projection_authority(singleton INTEGER PRIMARY KEY CHECK(singleton=1),gen INTEGER NOT NULL) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS _command_projections(command_id TEXT PRIMARY KEY NOT NULL,module TEXT NOT NULL,status TEXT NOT NULL,gen INTEGER NOT NULL,source_seq INTEGER NOT NULL,source_rev TEXT NOT NULL) WITHOUT ROWID;")?;
    // Upgrade retained P9 metadata in the same writer transaction. Old leases
    // have no release binding and are retired, never silently re-authorized.
    let bound: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('_command_leases') WHERE name='binding')",
        [],
        |r| r.get(0),
    )?;
    if !bound {
        tx.execute_batch(
            "ALTER TABLE _command_leases ADD COLUMN binding TEXT NOT NULL DEFAULT ''",
        )?;
    }
    Ok(())
}
fn command_binding(tx: &Transaction<'_>) -> rusqlite::Result<Option<String>> {
    crate::schema::active_collection(tx, "business_commands")?
        .map(|c| {
            serde_json::to_string(&(
                crate::change_log::head(tx)?.epoch,
                c.definition.app_id,
                c.gen,
                c.definition.schema_ver,
                c.definition.app_ver,
            ))
            .map_err(|e| invalid(e.to_string()))
        })
        .transpose()
}
fn state(doc: &Value) -> &str {
    doc["status"].as_str().unwrap_or("")
}
fn executable_command(doc: &Value, id: &str) -> bool {
    doc["id"].as_str() == Some(id)
        && doc["app_id"].as_str().is_some_and(|s| !s.is_empty())
        && doc["type"].as_str().is_some_and(|s| !s.is_empty())
        && doc["payload"].is_object()
        && transition(state(doc), state(doc))
        && !TERMINAL.contains(&state(doc))
}
fn lifecycle_rejection(old: &Value, next: &Value) -> Option<String> {
    if !transition(state(old), state(next)) {
        return Some("invalid_transition".into());
    }
    if TERMINAL.contains(&state(old)) && old != next {
        return Some("command_terminal".into());
    }
    None
}
/// Validate the submitted intent before field merge can mask an invalid state or stale token.
pub fn check_command_input(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    old: &Value,
    now: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<Option<String>> {
    let Some(input) = request.body.as_ref() else {
        return Ok(None);
    };
    if command_intent_changed(old, input, request.op) {
        return Ok(Some("idempotency_conflict".into()));
    }
    let mut proposed = old.clone();
    if request.op == WriteOp::Patch {
        super::super::merge_patch(&mut proposed, input)
    } else {
        proposed = input.clone()
    }
    if let Some(reason) = lifecycle_rejection(old, &proposed) {
        return Ok(Some(reason));
    }
    if old == &proposed {
        return Ok(None);
    }
    if let WritePermission::Reject(reason) =
        authority.authorize_command_executor(tx, participant, &proposed)?
    {
        return Ok(Some(reason));
    }
    let Some(fence) = serde_json::from_value::<Seq>(proposed["executor_fence"].clone()).ok() else {
        return Ok(Some("stale_fence".into()));
    };
    Ok(
        check_executor_fence(tx, participant, &request.id, fence, now)?
            .err()
            .map(|e| e.reason),
    )
}
fn intent(doc: &Value) -> Value {
    Value::Object(
        INTENT
            .iter()
            .filter_map(|k| doc.get(*k).map(|v| ((*k).into(), v.clone())))
            .collect(),
    )
}
pub fn command_intent_changed(old: &Value, input: &Value, op: WriteOp) -> bool {
    INTENT.iter().any(|field| match input.get(*field) {
        Some(value) => old.get(*field) != Some(value),
        None => op == WriteOp::Put && old.get(*field).is_some(),
    })
}
// Exact projection of business-command-lifecycle-v2.json, as in kernel/command-state.mjs.
pub(super) fn transition(from: &str, to: &str) -> bool {
    match from {
        "queued" => [
            "queued",
            "accepted",
            "running",
            "done",
            "failed",
            "cancelled",
            "rejected",
        ]
        .contains(&to),
        "accepted" => [
            "accepted",
            "queued",
            "running",
            "done",
            "failed",
            "cancelled",
        ]
        .contains(&to),
        "running" => ["running", "queued", "done", "failed", "cancelled"].contains(&to),
        "done" | "failed" | "cancelled" | "rejected" => from == to,
        _ => false,
    }
}
fn command_document(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<Option<Value>> {
    let Some(c) = crate::schema::active_collection(tx, "business_commands")? else {
        return Ok(None);
    };
    let table = crate::change_log::identifier(&c.table)?;
    let json: Option<String> = tx
        .query_row(
            &format!("SELECT data FROM {table} WHERE id=?1 AND deleted=0"),
            [id],
            |r| r.get(0),
        )
        .optional()?;
    json.map(|s| serde_json::from_str(&s).map_err(|e| invalid(e.to_string())))
        .transpose()
}
/// CAS acquisition/renewal. The native policy must admit this authenticated executor.
/// An expired or generation/release-retired lease may change owner; every new acquisition increments the fence.
pub fn claim_command(
    tx: &Transaction<'_>,
    participant: &str,
    command_id: &str,
    expected: Option<Seq>,
    now_ms: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<Result<CommandLease, CommandError>> {
    atomic(tx, || {
        initialize_commands(tx)?;
        if participant.is_empty() || participant.len() > 512 || command_id.is_empty() {
            return Ok(Err(failure("invalid_command")));
        }
        let Some(doc) = command_document(tx, command_id)? else {
            return Ok(Err(failure("not_found")));
        };
        if let WritePermission::Reject(reason) =
            authority.authorize_command_executor(tx, participant, &doc)?
        {
            return Ok(Err(failure(&reason)));
        }
        if TERMINAL.contains(&state(&doc)) {
            return Ok(Err(failure("command_terminal")));
        }
        if !executable_command(&doc, command_id) {
            return Ok(Err(failure("invalid_command")));
        }
        let binding = command_binding(tx)?.ok_or_else(|| invalid("command generation missing"))?;
        let old: Option<(String,u64,u64,u64,String)> = tx.query_row("SELECT participant,fence,expires_at_core,last_now_ms,binding FROM _command_leases WHERE command_id=?1",[command_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let now = old.as_ref().map_or(now_ms, |r| now_ms.max(r.3));
        let fence = match old {
            None if expected.is_none() => 1,
            Some((owner, fence, expires, _, old_binding)) if expected == Some(Seq(fence)) => {
                if now < expires && old_binding == binding && owner != participant {
                    return Ok(Err(failure("stale_fence")));
                }
                if now < expires && old_binding == binding {
                    fence
                } else {
                    fence
                        .checked_add(1)
                        .ok_or_else(|| invalid("fence exhausted"))?
                }
            }
            _ => return Ok(Err(failure("stale_fence"))),
        };
        let expires = now
            .checked_add(30_000)
            .ok_or_else(|| invalid("lease expiry overflow"))?;
        tx.execute("INSERT INTO _command_leases(command_id,participant,fence,expires_at_core,last_now_ms,binding) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(command_id) DO UPDATE SET participant=excluded.participant,fence=excluded.fence,expires_at_core=excluded.expires_at_core,last_now_ms=excluded.last_now_ms,binding=excluded.binding",params![command_id,participant,integer(fence)?,integer(expires)?,integer(now)?,binding])?;
        Ok(Ok(CommandLease {
            command_id: command_id.into(),
            participant: participant.into(),
            fence: Seq(fence),
            expires_at_core: expires,
        }))
    })
}
/// Call immediately before an effect in its authoritative transaction. Never treat a lease
/// cached by the caller as proof. This does not make an external non-idempotent effect atomic.
pub fn check_executor_fence(
    tx: &Transaction<'_>,
    participant: &str,
    command_id: &str,
    fence: Seq,
    now_ms: u64,
) -> rusqlite::Result<Result<(), CommandError>> {
    initialize_commands(tx)?;
    tx.execute(
        "UPDATE _command_leases SET last_now_ms=MAX(last_now_ms,?2) WHERE command_id=?1",
        params![command_id, integer(now_ms)?],
    )?;
    if command_document(tx, command_id)?
        .as_ref()
        .is_none_or(|doc| !executable_command(doc, command_id))
    {
        return Ok(Err(failure("stale_fence")));
    }
    let binding = command_binding(tx)?.unwrap_or_default();
    let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM _command_leases WHERE command_id=?1 AND participant=?2 AND fence=?3 AND expires_at_core>MAX(?4,last_now_ms) AND binding=?5)",params![command_id,participant,integer(fence.0)?,integer(now_ms)?,binding],|r|r.get(0))?;
    Ok(if valid {
        Ok(())
    } else {
        Err(failure("stale_fence"))
    })
}
/// Final post-merge validation; the client cannot use field merge to undo a terminal outcome.
pub fn check_command(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    previous: Option<&Value>,
    next: &Value,
    now: u64,
    authority: &dyn WriteAuthority,
) -> rusqlite::Result<Option<String>> {
    initialize_commands(tx)?;
    if next["id"].as_str() != Some(request.id.as_str())
        || next
            .get("command_id")
            .is_some_and(|v| v.as_str() != Some(request.id.as_str()))
        || next["type"].as_str().is_none_or(str::is_empty)
        || next["app_id"].as_str().is_none_or(str::is_empty)
        || !next["payload"].is_object()
        || ![
            "queued",
            "accepted",
            "running",
            "done",
            "failed",
            "cancelled",
            "rejected",
        ]
        .contains(&state(next))
    {
        return Ok(Some("invalid_command".into()));
    }
    let payload_bytes = serde_json::to_vec(&next["payload"])
        .map_err(|e| invalid(e.to_string()))?
        .len();
    if payload_bytes > 8192 {
        return Ok(Some("command_payload_quota".into()));
    }
    let native = matches!(
        authority.authorize_command_executor(tx, participant, next)?,
        WritePermission::Permit
    );
    if let Some(old) = previous {
        if intent(old) != intent(next) {
            return Ok(Some("idempotency_conflict".into()));
        }
        if let Some(reason) = lifecycle_rejection(old, next) {
            return Ok(Some(reason));
        }
        if old == next {
            return Ok(None);
        }
        if !native {
            return Ok(Some("forbidden".into()));
        }
        let fence = serde_json::from_value::<Seq>(next["executor_fence"].clone()).ok();
        let Some(fence) = fence else {
            return Ok(Some("stale_fence".into()));
        };
        if let Err(error) = check_executor_fence(tx, participant, &request.id, fence, now)? {
            return Ok(Some(error.reason));
        }
    } else {
        if let WritePermission::Reject(reason) =
            authority.authorize_command_intent(tx, participant, next)?
        {
            return Ok(Some(reason));
        }
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM _command_intents WHERE command_id=?1)",
            [&request.id],
            |r| r.get(0),
        )?;
        if used {
            return Ok(Some("command_terminal".into()));
        }
        if NATIVE_INITIAL
            .iter()
            .any(|field| next.get(*field).is_some())
        {
            return Ok(Some("invalid_transition".into()));
        }
        if state(next) != "queued"
            || next.get("result").is_some_and(|v| !v.is_null())
            || next.get("error").is_some_and(|v| !v.is_null())
            || next.get("executor_fence").is_some()
        {
            return Ok(Some("invalid_transition".into()));
        }
        tx.execute(
            "INSERT INTO _command_intents VALUES(?1,?2)",
            params![request.id, participant],
        )?;
    }
    Ok(None)
}
pub fn duplicate_command(
    tx: &Transaction<'_>,
    participant: &str,
    request: &Write,
    previous: Option<&str>,
) -> rusqlite::Result<bool> {
    initialize_commands(tx)?;
    let Some(body) = request.body.as_ref() else {
        return Ok(false);
    };
    if state(body) != "queued" {
        return Ok(false);
    }
    let owner: Option<String> = tx
        .query_row(
            "SELECT participant FROM _command_intents WHERE command_id=?1",
            [&request.id],
            |r| r.get(0),
        )
        .optional()?;
    let prior: Option<Value> = previous
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| invalid(e.to_string()))?;
    Ok(owner.as_deref() == Some(participant)
        && prior
            .as_ref()
            .is_some_and(|old| intent(old) == intent(body)))
}
pub fn rejection_code(reason: &str) -> &str {
    match reason {
        "stale_fence"
        | "conflict"
        | "idempotency_conflict"
        | "invalid_transition"
        | "command_terminal"
        | "op_id_conflict" => "conflict",
        "forbidden" | "command_delete_forbidden" => "forbidden",
        "expired" | "revoked" | "migration_required" | "not_found" => reason,
        "command_payload_quota" => "quota",
        "collection_unavailable" => "unavailable",
        _ => "invalid",
    }
}
impl OrderedWriter {
    pub async fn claim_command(
        &self,
        participant: String,
        command_id: String,
        expected: Option<Seq>,
        now_ms: u64,
    ) -> Result<Result<CommandLease, CommandError>, StoreError> {
        let authority = self.authority.clone();
        self.writer
            .submit(WriteJob::new(JobOptions::default(), move |tx| {
                claim_command(
                    tx,
                    &participant,
                    &command_id,
                    expected,
                    now_ms,
                    authority.as_ref(),
                )
                .map_err(Into::into)
            }))?
            .await
    }
}
