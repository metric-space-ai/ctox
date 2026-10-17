//! `audit` — append-only event journal (CATALOG Nr. 13) over the Core
//! `business_events` table. The writing app is the Core-bound ctx.app_id,
//! stamped under the reserved `_cap` key; a payload can never claim it.
//! Queries see only the caller's own events; platform callers see all.
use super::*;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;

const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const RESERVED: &str = "_cap";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    collection: String,
    #[serde(default)]
    record_id: String,
    event_type: String,
    #[serde(default)]
    payload: Value,
    observed_at_ms: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    command_type_prefix: Option<String>,
    collection: Option<String>,
    record_id: Option<String>,
    #[serde(default)]
    summary: serde_json::Map<String, Value>,
    order: Option<String>,
    limit: Option<u32>,
}

pub(super) fn execute(
    root: &Path,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
    now_ms: u64,
) -> Result<Value, CtoxError> {
    match function {
        "event" => event(root, ctx, decode(args)?, now_ms),
        "query" => query(root, ctx, decode(args)?),
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown audit function",
        )),
    }
}

fn event(root: &Path, ctx: &CapCallContext, e: Event, now_ms: u64) -> Result<Value, CtoxError> {
    text(&e.collection, 128, false, "$.collection")?;
    text(&e.record_id, 256, true, "$.record_id")?;
    text(&e.event_type, 256, false, "$.event_type")?;
    let mut payload = match e.payload {
        Value::Null => serde_json::Map::new(),
        Value::Object(map) => map,
        _ => return Err(invalid("$.payload")),
    };
    if payload.contains_key(RESERVED) {
        return Err(invalid("$.payload._cap"));
    }
    if json_size(&Value::Object(payload.clone())) > MAX_PAYLOAD_BYTES {
        return Err(CtoxError::new(
            ErrorCode::Quota,
            "audit payload exceeds 64 KiB",
        ));
    }
    payload.insert(
        RESERVED.into(),
        json!({"app_id":ctx.app_id,"app_ver":ctx.app_ver,"actor_id":ctx.actor.id,
               "actor_role":ctx.actor.role,"command_id":ctx.command_id}),
    );
    let observed = e
        .observed_at_ms
        .filter(|v| *v > 0)
        .unwrap_or(now_ms.min(i64::MAX as u64) as i64);
    // Same call_id = retry of the same append, never a second event.
    let identity =
        serde_json::to_vec(&(&ctx.app_id, &ctx.command_id, &ctx.call_id)).map_err(native_error)?;
    let event_id = format!("cap-audit:{}", sha256_hex(&identity));
    let body = serde_json::to_string(&payload).map_err(native_error)?;
    let conn = crate::business_os::store::open_store(root).map_err(native_error)?;
    let existing: Option<(String, String, String)> = conn
        .query_row(
            "SELECT collection, record_id, command_type FROM business_events WHERE event_id=?1",
            params![event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(native_error)?;
    if let Some(stored) = existing {
        if stored
            != (
                e.collection.clone(),
                e.record_id.clone(),
                e.event_type.clone(),
            )
        {
            return Err(CtoxError::new(
                ErrorCode::Conflict,
                "call_id already recorded a different audit event",
            ));
        }
        return Ok(json!({"event_id":event_id,"stored":false}));
    }
    conn.execute(
        "INSERT INTO business_events
            (event_id, collection, record_id, command_type, payload_json, observed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event_id,
            e.collection,
            e.record_id,
            e.event_type,
            body,
            observed
        ],
    )
    .map_err(native_error)?;
    Ok(json!({"event_id":event_id,"stored":true}))
}

fn query(root: &Path, ctx: &CapCallContext, q: Query) -> Result<Value, CtoxError> {
    if let Some(prefix) = &q.command_type_prefix {
        text(prefix, 256, true, "$.command_type_prefix")?;
    }
    if let Some(collection) = &q.collection {
        text(collection, 128, false, "$.collection")?;
    }
    if let Some(record_id) = &q.record_id {
        text(record_id, 256, true, "$.record_id")?;
    }
    if q.summary.len() > 8 {
        return Err(invalid("$.summary"));
    }
    for (key, value) in &q.summary {
        if key.is_empty()
            || key.len() > 64
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !(value.is_string() || value.is_number() || value.is_boolean())
        {
            return Err(invalid("$.summary"));
        }
    }
    let descending = match q.order.as_deref() {
        None | Some("observed_at_ms") => false,
        Some("-observed_at_ms") => true,
        Some(_) => return Err(invalid("$.order")),
    };
    let limit = q.limit.unwrap_or(500);
    if limit == 0 || limit > 1000 {
        return Err(invalid("$.limit"));
    }
    // Own events only; a platform caller (ctox-platform) audits every app.
    let scope_app = if is_platform(ctx) {
        None
    } else {
        Some(ctx.app_id.clone())
    };
    let prefix = q.command_type_prefix.clone().unwrap_or_default();
    let summary = serde_json::to_string(&q.summary).map_err(native_error)?;
    let sql = format!(
        "SELECT event_id, collection, record_id, command_type, payload_json, observed_at_ms
           FROM business_events
          WHERE (?1 IS NULL OR json_extract(payload_json,'$._cap.app_id') = ?1)
            AND json_extract(payload_json,'$._cap.app_id') IS NOT NULL
            AND substr(command_type,1,length(?2)) = ?2
            AND (?3 IS NULL OR collection = ?3)
            AND (?4 IS NULL OR record_id = ?4)
            AND NOT EXISTS (SELECT 1 FROM json_each(?5) AS s
                 WHERE json_extract(payload_json,'$.summary.' || s.key) IS NOT s.value)
          ORDER BY observed_at_ms {}, event_id
          LIMIT ?6",
        if descending { "DESC" } else { "ASC" }
    );
    let conn = crate::business_os::store::open_store(root).map_err(native_error)?;
    let mut statement = conn.prepare(&sql).map_err(native_error)?;
    let rows = statement
        .query_map(
            params![
                scope_app,
                prefix,
                q.collection,
                q.record_id,
                summary,
                limit as i64
            ],
            |row| {
                let body: String = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    body,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .map_err(native_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(native_error)?;
    let events = rows
        .into_iter()
        .map(|(event_id, collection, record_id, event_type, body, observed)| {
            let mut payload: serde_json::Map<String, Value> =
                serde_json::from_str(&body).unwrap_or_default();
            let app_id = payload
                .remove(RESERVED)
                .and_then(|cap| cap.get("app_id").cloned())
                .unwrap_or(Value::Null);
            json!({"event_id":event_id,"collection":collection,"record_id":record_id,
                   "event_type":event_type,"payload":payload,"observed_at_ms":observed,"app_id":app_id})
        })
        .collect::<Vec<_>>();
    Ok(json!({ "events": events }))
}
