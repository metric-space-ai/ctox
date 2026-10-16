//! Native provenance is private writer metadata, never a field accepted from a client.
use super::*;
use rusqlite::Transaction;

/// Only in-process owners can choose this source; no wire deserializer exists.
#[derive(Clone, Copy)]
pub(crate) enum CoreCommandSource {
    Supervisor,
    Scheduler,
    CommandPlane,
}
impl CoreCommandSource {
    fn name(self) -> &'static str {
        match self {
            Self::Supervisor => "supervisor",
            Self::Scheduler => "scheduler",
            Self::CommandPlane => "core_command_plane",
        }
    }
}
/// The caller borrows the existing sync writer transaction. No second store or writer.
/// Native scheduler/supervisor producers use this instead of a serialized origin field.
pub(crate) fn enqueue_core_command(
    tx: &Transaction<'_>,
    source: CoreCommandSource,
    id: &str,
    app: &str,
    kind: &str,
    actor_id: &str,
    payload: Value,
) -> Result<(), StoreError> {
    if document(tx, "business_commands", id)?.is_some() {
        return Err(invalid("conflict: command identity already exists"));
    }
    let _ = actor(tx, actor_id)?;
    let doc = json!({"id":id,"app_id":app,"type":kind,"actor_id":actor_id,"payload":payload,"status":"queued","created_at":chrono::Utc::now().to_rfc3339()});
    store_command(tx, &doc)?;
    stamp(tx, source.name(), &doc)?;
    Ok(())
}
pub(super) fn stamp(tx: &Transaction<'_>, source: &str, doc: &Value) -> Result<(), StoreError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _app_runtime_core_commands(gen INTEGER NOT NULL,id TEXT NOT NULL,source TEXT NOT NULL,intent TEXT NOT NULL,rev TEXT NOT NULL,PRIMARY KEY(gen,id))")?;
    let c = collection(tx, "business_commands")?;
    let id = text(doc, "id").map_err(sql_error)?;
    tx.execute(
        "INSERT INTO _app_runtime_core_commands(gen,id,source,intent,rev) VALUES(?1,?2,?3,?4,?5)",
        params![c.gen, id, source, intent(doc)?, command_rev(tx, &c, id)?],
    )?;
    Ok(())
}
pub(super) fn native_event(db: &Connection, doc: &Value) -> Result<bool, StoreError> {
    if doc["runtime_event"] != true || receipt(db, doc)?.is_none() {
        return Ok(false);
    }
    let c = collection(db, "business_commands")?;
    Ok(db.query_row(
        "SELECT source='runtime_event' FROM _app_runtime_core_commands WHERE gen=?1 AND id=?2",
        params![c.gen, text(doc, "id").map_err(sql_error)?],
        |r| r.get(0),
    )?)
}
fn intent(doc: &Value) -> Result<String, StoreError> {
    // Actor/app_ver are added by the host to the retained binding. Terminal fields
    // are owned by the command executor; every other input remains immutable.
    let mut v = doc.clone();
    for key in ["status", "result", "error", "actor", "app_ver"] {
        v.as_object_mut()
            .ok_or_else(|| invalid("invalid: command"))?
            .remove(key);
    }
    Ok(digest(&serde_json::to_vec(&v).map_err(sql_error)?))
}
fn command_rev(db: &Connection, c: &Collection, id: &str) -> Result<String, StoreError> {
    let rev: String = db.query_row(
        &format!(
            "SELECT rev FROM {} WHERE id=?1 AND deleted=0",
            quote(&c.table)
        ),
        [id],
        |r| r.get(0),
    )?;
    let seq: u64 = db.query_row(
        "SELECT seq FROM _changes WHERE collection='business_commands' AND gen=?1 AND id=?2",
        params![c.gen, id],
        |r| r.get(0),
    )?;
    Ok(format!("{rev}:{seq}"))
}
fn receipt(db: &Connection, doc: &Value) -> Result<Option<(u64, String)>, StoreError> {
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='_app_runtime_core_commands')",[],|r|r.get(0))?;
    if !exists {
        return Ok(None);
    }
    let c = collection(db, "business_commands")?;
    let id = text(doc, "id").map_err(sql_error)?;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT intent,rev FROM _app_runtime_core_commands WHERE gen=?1 AND id=?2",
            params![c.gen, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((hash, rev)) = row else {
        return Ok(None);
    };
    if hash != intent(doc)? || command_rev(db, &c, id)? != rev {
        return Err(invalid("forbidden: core command provenance changed"));
    }
    Ok(Some((c.gen, rev)))
}
/// Keep the private receipt in step only with native executor writes. A peer
/// rewrite changes the canonical revision without this metadata and fails closed.
pub(super) fn before_native_write(db: &Connection, doc: &Value) -> Result<Option<u64>, StoreError> {
    Ok(receipt(db, doc)?.map(|(gen, _)| gen))
}
pub(super) fn after_native_write(
    tx: &Transaction<'_>,
    doc: &Value,
    gen: Option<u64>,
) -> Result<(), StoreError> {
    if let Some(gen) = gen {
        let c = collection(tx, "business_commands")?;
        let id = text(doc, "id").map_err(sql_error)?;
        tx.execute(
            "UPDATE _app_runtime_core_commands SET rev=?1 WHERE gen=?2 AND id=?3",
            params![command_rev(tx, &c, id)?, gen, id],
        )?;
    }
    Ok(())
}
fn core_only(db: &Connection, app: &str, kind: &str) -> Result<bool, StoreError> {
    let manifests = serde_json::to_value(rights(db)?.manifests).map_err(sql_error)?;
    let Some(manifest) = manifests
        .as_array()
        .and_then(|all| all.iter().find(|m| m["app_id"] == app))
    else {
        return Ok(false);
    };
    let Some(rules) = manifest["command_rights"].as_object() else {
        return Ok(false);
    };
    let rule = rules.get(kind).or_else(|| {
        rules
            .iter()
            .filter_map(|(pattern, rule)| {
                pattern
                    .strip_suffix('*')
                    .filter(|prefix| kind.starts_with(prefix))
                    .map(|prefix| (prefix.len(), rule))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, rule)| rule)
    });
    Ok(rule.is_some_and(|rule| {
        rule["roles"].as_array().is_some_and(Vec::is_empty)
            && !rule["assigned_users"].as_bool().unwrap_or(false)
    }))
}
/// The exceptional right is delivery only. Core-origin commands retain the
/// current user's row/field policy and gain no synthetic P6 CommandPermit.
pub(super) fn delivery_permit(
    db: &Connection,
    doc: &Value,
    a: &BusinessOsActor,
) -> Result<bool, StoreError> {
    if a.id.as_deref() != doc["actor_id"].as_str() {
        return Err(invalid("forbidden: command actor differs"));
    }
    let app = text(doc, "app_id").map_err(sql_error)?;
    let kind = text(doc, "type").map_err(sql_error)?;
    if core_only(db, app, kind)? {
        if receipt(db, doc)?.is_none() {
            return Err(invalid("forbidden: core origin required"));
        }
        return Ok(true);
    }
    permit(db, app, a, kind)?;
    Ok(false)
}
pub(super) fn audit_rejection(
    tx: &Transaction<'_>,
    doc: &Value,
    code: &str,
) -> Result<(), StoreError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS _app_runtime_command_audit(gen INTEGER NOT NULL,id TEXT NOT NULL,app_id TEXT NOT NULL,command_type TEXT NOT NULL,actor_id TEXT NOT NULL,code TEXT NOT NULL,at_core_ms INTEGER NOT NULL,PRIMARY KEY(gen,id))")?;
    let c = collection(tx, "business_commands")?;
    let has_receipts:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='_app_runtime_core_commands')",[],|r|r.get(0))?;
    if has_receipts {
        tx.execute(
            "DELETE FROM _app_runtime_core_commands WHERE gen=?1 AND id=?2",
            params![c.gen, text(doc, "id").map_err(sql_error)?],
        )?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO _app_runtime_command_audit VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            c.gen,
            text(doc, "id").map_err(sql_error)?,
            text(doc, "app_id").map_err(sql_error)?,
            text(doc, "type").map_err(sql_error)?,
            text(doc, "actor_id").map_err(sql_error)?,
            code,
            now()
        ],
    )?;
    Ok(())
}
