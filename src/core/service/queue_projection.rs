//! Bounded queue projections with immutable, lossless Core-owned payloads.
//! Invoked inside the ACK transaction: content, intent and ACK commit together.
use ctox_store::StoreError;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) const INLINE_BYTES: usize = 64 * 1024;
pub(crate) const CHUNK_BYTES: usize = 16 * 1024;

pub(crate) fn capture(db: &Connection, document: Value) -> anyhow::Result<Value> {
    let bytes = serde_json::to_vec(&document)?;
    if bytes.len() <= INLINE_BYTES {
        return Ok(document);
    }
    let id = document["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("projection identity missing"))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    // Private source content is not a replicated collection. The reference is
    // not a read grant; only the same-user daemon IPC serves bounded chunks.
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS _queue_projection_content (
        hash TEXT PRIMARY KEY NOT NULL, content BLOB NOT NULL) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS _queue_projection_refs (
        record_id TEXT NOT NULL,hash TEXT NOT NULL,PRIMARY KEY(record_id,hash)) WITHOUT ROWID;",
    )?;
    db.execute(
        "INSERT OR IGNORE INTO _queue_projection_content VALUES(?1,?2)",
        params![hash, bytes],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO _queue_projection_refs VALUES(?1,?2)",
        params![id, hash],
    )?;
    let mut summary = serde_json::Map::new();
    // A summary is explicitly identified, never advertised as the full input.
    // Keep typed strings as strings; original JSON (including all nested fields)
    // remains immutable and hash-addressed, rather than applying the old clamp.
    for key in [
        "id",
        "command_id",
        "type",
        "command_type",
        "title",
        "status",
        "route_status",
        "module",
        "source_module",
        "inbound_channel",
        "priority",
        "thread_key",
        "updated_at_ms",
        "queue_projection_version",
        "projection_version",
        "prompt",
    ] {
        if let Some(value) = document.get(key) {
            if value.is_number() || value.is_boolean() || value.is_null() {
                summary.insert(key.into(), value.clone());
            } else if let Some(text) = value.as_str() {
                summary.insert(
                    key.into(),
                    Value::String(
                        text.chars()
                            .take(if key == "prompt" { 512 } else { 256 })
                            .collect(),
                    ),
                );
            }
        }
    }
    // Identity must never be shortened, even for a pre-existing unusual key.
    summary.insert("id".into(), Value::String(id.into()));
    summary.insert("projection_ref".into(), json!({"sha256":hash,"size":bytes.len(),"mime":"application/json","source":"core.queue-projection","record_id":id}));
    Ok(Value::Object(summary))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChunkRequest {
    pub record_id: String,
    pub sha256: String,
    pub offset: u64,
}
/// No arbitrary SQL, path, length or authority crosses the process boundary.
/// A small indexed chunk remains inside the ordinary reader execution budget.
pub(crate) fn read_chunk(db: &Connection, request: &ChunkRequest) -> Result<Vec<u8>, StoreError> {
    if request.record_id.is_empty()
        || request.record_id.len() > 512
        || request.sha256.len() != 64
        || !request
            .sha256
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || request.offset > i64::MAX as u64 - 1
    {
        return Err(StoreError::Invalid("invalid projection reference".into()));
    }
    db.query_row("SELECT substr(content,?3,?4) FROM _queue_projection_content AS c WHERE hash=?2 AND EXISTS(SELECT 1 FROM _queue_projection_refs WHERE record_id=?1 AND hash=c.hash)",params![request.record_id,request.sha256,request.offset as i64+1,CHUNK_BYTES as i64],|r|r.get(0)).optional()?.ok_or_else(||StoreError::Invalid("projection content not found".into()))
}
