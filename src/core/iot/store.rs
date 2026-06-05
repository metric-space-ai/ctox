// Origin: CTOX
// License: AGPL-3.0-only
//
// Authoritative IoT asset / attribute / asset-type store plus the ported
// attribute-event write path (§2A.1-8). Domain semantics ported from
// OpenRemote (AGPL-3.0, archive/openremote, HEAD 22a42a7); persistence is
// CTOX-native SQLite (single runtime/ctox.sqlite3 via crate::paths::core_db).
//
// The pure event semantics (timestamp normalization, coercion, outdated check,
// descriptor-meta merge, lazy hydration) live in model.rs; this file is the
// stateful flow that reads old state under a per-asset lock, applies them in
// order, and persists current state + datapoints.
//
// Time model (see iot/mod.rs):
//   * attribute / event / datapoint time is `i64` epoch-ms UTC (the ported
//     domain dimension, §2A.13); the write path normalizes the event's supplied
//     timestamp against system_time_ms rather than reading the wall clock.
//   * created_at / updated_at audit columns are RFC-3339 millis-precision UTC
//     TEXT (CTOX house style) via now_iso().
//
// ref: AssetProcessingService.java:264-445  (the processing flow)
// ref: AssetStorageService.java:1404-1439    (read-modify-write under lock)

use crate::iot::model::*;
use crate::iot::{datapoints, now_iso, Result};
use anyhow::{bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

// ---------------------------------------------------------------------------
// Store: open + schema
// ---------------------------------------------------------------------------

/// Open the shared CTOX runtime store and ensure the full IoT schema exists.
/// Mirrors business_os::store::open_store (WAL + busy_timeout house idiom) and
/// targets the core db (runtime/ctox.sqlite3) per CTOX's single-store rule.
///
/// This is the one clear init path: it creates the asset/attribute/asset-type
/// tables here AND chains the datapoints schema init so a single
/// `open_iot_store` yields a fully-usable store (the alarm schema is owned by
/// alarms::open / alarms::init_schema and ensured lazily there).
pub(crate) fn open_iot_store(root: &Path) -> Result<Connection> {
    let path = crate::paths::core_db(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create runtime dir {}", parent.display()))?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open IoT core store {}", path.display()))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())
        .context("failed to configure IoT SQLite busy_timeout")?;
    let ms = crate::persistence::sqlite_busy_timeout_millis();
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout={ms};"
    ))
    .context("failed to configure IoT SQLite pragmas")?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Create the asset / attribute / asset-type tables (and the datapoints table
/// via datapoints::init_schema) if absent. Canonical JSON lives in a `data`
/// TEXT column; light index columns (id/asset_id/name/realm/timestamp) sit
/// alongside for querying.
fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_asset_types (
            asset_type  TEXT PRIMARY KEY,
            data        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS iot_assets (
            id          TEXT PRIMARY KEY,
            parent_id   TEXT,
            realm       TEXT NOT NULL,
            asset_type  TEXT NOT NULL,
            name        TEXT NOT NULL,
            data        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_assets_realm  ON iot_assets(realm);
        CREATE INDEX IF NOT EXISTS idx_iot_assets_parent ON iot_assets(parent_id);

        CREATE TABLE IF NOT EXISTS iot_attributes (
            asset_id      TEXT    NOT NULL,
            name          TEXT    NOT NULL,
            value         TEXT,
            value_type    TEXT,
            timestamp_ms  INTEGER NOT NULL,
            meta          TEXT    NOT NULL,
            created_at    TEXT    NOT NULL,
            updated_at    TEXT    NOT NULL,
            PRIMARY KEY (asset_id, name)
        );",
    )
    .context("failed to create IoT asset/attribute schema")?;
    // One clear init path: ensure the datapoints schema too (§2A write path
    // records both outdated and current events as datapoints).
    datapoints::init_schema(conn)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ValueBaseType <-> column string (light index column; canonical form is JSON)
// ---------------------------------------------------------------------------

fn value_base_type_str(t: ValueBaseType) -> &'static str {
    match t {
        ValueBaseType::Number => "Number",
        ValueBaseType::Boolean => "Boolean",
        ValueBaseType::Text => "Text",
        ValueBaseType::Object => "Object",
        ValueBaseType::Array => "Array",
        ValueBaseType::GeoPoint => "GeoPoint",
    }
}

fn value_base_type_from_str(s: &str) -> Option<ValueBaseType> {
    Some(match s {
        "Number" => ValueBaseType::Number,
        "Boolean" => ValueBaseType::Boolean,
        "Text" => ValueBaseType::Text,
        "Object" => ValueBaseType::Object,
        "Array" => ValueBaseType::Array,
        "GeoPoint" => ValueBaseType::GeoPoint,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Asset-type registry CRUD
// ---------------------------------------------------------------------------

/// Insert or replace an asset-type descriptor record. The descriptor registry
/// is what supplies the declared attribute base types + descriptor-meta
/// defaults consumed by the write path (§2A.6) and Asset::new_with_type.
pub(crate) fn upsert_asset_type(conn: &Connection, info: &AssetTypeInfo) -> Result<()> {
    let data = serde_json::to_string(info).context("failed to serialize asset type")?;
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_asset_types (asset_type, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(asset_type) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
        params![info.asset_type, data, now],
    )
    .context("failed to upsert asset type")?;
    Ok(())
}

/// Fetch an asset-type descriptor record, or None if not registered.
pub(crate) fn get_asset_type(conn: &Connection, asset_type: &str) -> Result<Option<AssetTypeInfo>> {
    let row: Option<String> = conn
        .query_row(
            "SELECT data FROM iot_asset_types WHERE asset_type = ?1",
            params![asset_type],
            |r| r.get(0),
        )
        .optional()
        .context("failed to query asset type")?;
    match row {
        Some(data) => Ok(Some(
            serde_json::from_str(&data).context("failed to deserialize asset type")?,
        )),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Asset CRUD
// ---------------------------------------------------------------------------

/// Insert or replace an asset and its attribute rows. Canonical JSON of the
/// asset lives in `iot_assets.data`; each attribute is materialized into a row
/// in `iot_attributes` (latest value + timestamp per (asset_id, name)) so the
/// write path can read old state without rehydrating the whole asset.
pub(crate) fn upsert_asset(conn: &Connection, asset: &Asset) -> Result<()> {
    let data = serde_json::to_string(asset).context("failed to serialize asset")?;
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_assets (id, parent_id, realm, asset_type, name, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(id) DO UPDATE SET
            parent_id = excluded.parent_id,
            realm = excluded.realm,
            asset_type = excluded.asset_type,
            name = excluded.name,
            data = excluded.data,
            updated_at = excluded.updated_at",
        params![
            asset.id,
            asset.parent_id,
            asset.realm,
            asset.asset_type,
            asset.name,
            data,
            now,
        ],
    )
    .context("failed to upsert asset")?;
    for attr in asset.attributes.values() {
        upsert_attribute_row(conn, &asset.id, attr)?;
    }
    Ok(())
}

/// Write a single attribute row (latest current state). `value` holds the
/// canonical JSON of the AttributeValue; `meta` the canonical JSON of the
/// MetaMap; `value_type` a light index string (canonical type lives inside the
/// attribute row's value/meta JSON form too).
fn upsert_attribute_row(conn: &Connection, asset_id: &str, attr: &Attribute) -> Result<()> {
    let value_json = match &attr.value {
        Some(v) => Some(serde_json::to_string(v).context("failed to serialize attribute value")?),
        None => None,
    };
    let meta_json =
        serde_json::to_string(&attr.meta).context("failed to serialize attribute meta")?;
    let vtype = attr.value_type.map(value_base_type_str);
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_attributes
            (asset_id, name, value, value_type, timestamp_ms, meta, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(asset_id, name) DO UPDATE SET
            value = excluded.value,
            value_type = excluded.value_type,
            timestamp_ms = excluded.timestamp_ms,
            meta = excluded.meta,
            updated_at = excluded.updated_at",
        params![
            asset_id,
            attr.name,
            value_json,
            vtype,
            attr.timestamp,
            meta_json,
            now,
        ],
    )
    .context("failed to upsert attribute row")?;
    Ok(())
}

/// Load a single attribute's current state (or None if no row exists yet).
///
/// Lazy hydration (§2A.8): the row stores the value as canonical JSON; we parse
/// it back into an AttributeValue on load. value_str is not used at the row
/// level because the row already keeps the parsed JSON, but Attribute carries
/// the field for callers that hydrate from a descriptor later.
fn load_attribute_row(conn: &Connection, asset_id: &str, name: &str) -> Result<Option<Attribute>> {
    let row: Option<(Option<String>, Option<String>, i64, String)> = conn
        .query_row(
            "SELECT value, value_type, timestamp_ms, meta
             FROM iot_attributes WHERE asset_id = ?1 AND name = ?2",
            params![asset_id, name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .context("failed to query attribute row")?;
    let Some((value_json, vtype_str, timestamp, meta_json)) = row else {
        return Ok(None);
    };
    let value = match value_json {
        Some(s) => {
            Some(serde_json::from_str::<AttributeValue>(&s).context("failed to parse attr value")?)
        }
        None => None,
    };
    let value_type = vtype_str.as_deref().and_then(value_base_type_from_str);
    let meta: MetaMap = serde_json::from_str(&meta_json).context("failed to parse attr meta")?;
    Ok(Some(Attribute {
        name: name.to_string(),
        value_type,
        value,
        value_str: None,
        timestamp,
        meta,
    }))
}

/// Fetch a full asset by id (with its attributes rehydrated from the attribute
/// rows so current state reflects the latest write-path updates).
pub(crate) fn get_asset(conn: &Connection, id: &str) -> Result<Option<Asset>> {
    let row: Option<String> = conn
        .query_row(
            "SELECT data FROM iot_assets WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .context("failed to query asset")?;
    let Some(data) = row else {
        return Ok(None);
    };
    let mut asset: Asset = serde_json::from_str(&data).context("failed to deserialize asset")?;
    // Rehydrate attribute current-state from the authoritative attribute rows
    // (the asset `data` blob is a snapshot; the rows hold the latest writes).
    let mut stmt = conn
        .prepare(
            "SELECT name, value, value_type, timestamp_ms, meta
             FROM iot_attributes WHERE asset_id = ?1",
        )
        .context("failed to prepare attribute load")?;
    let rows = stmt
        .query_map(params![id], |r| {
            let name: String = r.get(0)?;
            let value: Option<String> = r.get(1)?;
            let vtype: Option<String> = r.get(2)?;
            let ts: i64 = r.get(3)?;
            let meta: String = r.get(4)?;
            Ok((name, value, vtype, ts, meta))
        })
        .context("failed to query attribute rows")?;
    for row in rows {
        let (name, value_json, vtype_str, timestamp, meta_json) =
            row.context("failed to read attribute row")?;
        let value = match value_json {
            Some(s) => Some(
                serde_json::from_str::<AttributeValue>(&s).context("failed to parse attr value")?,
            ),
            None => None,
        };
        let value_type = vtype_str.as_deref().and_then(value_base_type_from_str);
        let meta: MetaMap =
            serde_json::from_str(&meta_json).context("failed to parse attr meta")?;
        asset.attributes.insert(
            name.clone(),
            Attribute {
                name,
                value_type,
                value,
                value_str: None,
                timestamp,
                meta,
            },
        );
    }
    Ok(Some(asset))
}

/// Realm-scoped asset fetch (multi-realm isolation, Phase 2). Returns the asset
/// ONLY when it both exists AND belongs to `realm`; a cross-realm id resolves to
/// `None`, identical to "not found", so a caller cannot read another realm's
/// asset by guessing its id. The query enforces the realm at the SQL layer.
pub(crate) fn get_asset_in_realm(
    conn: &Connection,
    id: &str,
    realm: &str,
) -> Result<Option<Asset>> {
    // Realm gate at the store layer: SELECT ... WHERE id = ?1 AND realm = ?2.
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM iot_assets WHERE id = ?1 AND realm = ?2",
            params![id, realm],
            |r| r.get(0),
        )
        .optional()
        .context("failed to realm-scope asset")?;
    if exists.is_none() {
        return Ok(None);
    }
    // Realm matches — rehydrate the full asset (attributes from their rows).
    get_asset(conn, id)
}

/// List assets in a realm, optionally filtered to a parent id. Returns the
/// snapshot `data` form (attributes as last persisted in the blob); use
/// get_asset for fully-rehydrated current state.
pub(crate) fn list_assets(
    conn: &Connection,
    realm: &str,
    parent_id: Option<&str>,
) -> Result<Vec<Asset>> {
    let mut sql = String::from("SELECT data FROM iot_assets WHERE realm = ?1");
    if parent_id.is_some() {
        sql.push_str(" AND parent_id = ?2");
    }
    sql.push_str(" ORDER BY name ASC");
    let mut stmt = conn.prepare(&sql).context("failed to prepare asset list")?;
    let map_row = |r: &rusqlite::Row| -> rusqlite::Result<String> { r.get(0) };
    let rows: Vec<String> = match parent_id {
        Some(p) => stmt
            .query_map(params![realm, p], map_row)?
            .collect::<rusqlite::Result<_>>()?,
        None => stmt
            .query_map(params![realm], map_row)?
            .collect::<rusqlite::Result<_>>()?,
    };
    rows.iter()
        .map(|d| serde_json::from_str::<Asset>(d).context("failed to deserialize asset"))
        .collect()
}

/// Delete an asset and its attribute rows. Datapoints are append-only history
/// and are intentionally retained.
pub(crate) fn delete_asset(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM iot_attributes WHERE asset_id = ?1",
        params![id],
    )
    .context("failed to delete attribute rows")?;
    conn.execute("DELETE FROM iot_assets WHERE id = ?1", params![id])
        .context("failed to delete asset")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// §2A.7 — per-asset write serialization (replaces Hibernate @Version +
// withAssetLock). A process-wide map of per-asset mutexes; the write path
// holds the asset's mutex across read-old → outdated-check → write so
// concurrent writers to the same asset cannot interleave their
// read-modify-write cycles.
// ref: AssetStorageService.java:1404-1439 (withAssetLock)
// ---------------------------------------------------------------------------

static ASSET_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Acquire (creating if necessary) the per-asset lock handle.
fn asset_lock(asset_id: &str) -> Arc<Mutex<()>> {
    let mut map = ASSET_LOCKS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    map.entry(asset_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

// ---------------------------------------------------------------------------
// THE WRITE PATH — process_attribute_event (§2A.1-8)
// ref: AssetProcessingService.java:264-445
// ---------------------------------------------------------------------------

/// Outcome of processing one attribute event (for caller telemetry / tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventOutcome {
    /// Not outdated: current attribute state updated AND a datapoint recorded.
    Updated,
    /// Outdated (oldValueTimestamp > eventTimestamp): current state left
    /// UNCHANGED, but a datapoint was STILL recorded (§2A.3).
    OutdatedRecordedOnly,
}

/// Apply one inbound attribute event to the store, following the ported
/// AssetProcessingService flow IN ORDER (§2A.1-8):
///
///   1. Resolve the attribute's declared base type from the stored attribute
///      row (if any) else the asset-type descriptor registry.
///   2. Coerce the event value to that declared type BEFORE validation;
///      coercion failure rejects the WHOLE event (§2A.5).
///   3. Normalize the event timestamp against system time (§2A.2).
///   4. Read the old value + timestamp under a PER-ASSET write lock so
///      concurrent writers cannot interleave their read-modify-write (§2A.7).
///   5. Outdated check (STRICT oldTs > eventTs, §2A.3): if outdated, DO NOT
///      update current state but STILL record the datapoint; else update
///      current attribute state (value/type/timestamp/meta) AND record the
///      datapoint.
///
/// Descriptor meta is merged ONCE at attribute creation only (§2A.6) — never
/// re-merged on update. Lazy hydration (§2A.8) is honored on load.
///
/// ref: AssetProcessingService.java:264-445
pub(crate) fn process_attribute_event(
    conn: &Connection,
    event: &AttributeEvent,
    system_time_ms: i64,
) -> Result<EventOutcome> {
    // The asset must exist before an event can be processed (upstream rejects
    // events for unknown assets). ref: AssetProcessingService.java:300-312
    let asset = match get_asset(conn, &event.asset_id)? {
        Some(a) => a,
        None => bail!("attribute event for unknown asset: {}", event.asset_id),
    };

    // -- (1) Resolve the declared base type -------------------------------
    // Prefer the stored attribute's resolved type; fall back to the asset-type
    // descriptor registry. A brand-new attribute with no descriptor stays
    // untyped (None) and the raw value passes through without coercion.
    let stored = asset.attributes.get(&event.attribute_name);
    // Resolve the asset-type descriptor for this attribute once; it supplies
    // BOTH the declared base type AND the descriptor-meta defaults that must be
    // merged on event-driven attribute creation (§2A.6, symmetric with
    // value_type recovery below).
    let asset_type_info = if stored.and_then(|a| a.value_type).is_none() || stored.is_none() {
        get_asset_type(conn, &asset.asset_type)?
    } else {
        None
    };
    let descriptor = asset_type_info
        .as_ref()
        .and_then(|info| info.descriptor(&event.attribute_name));
    let declared_type: Option<ValueBaseType> = match stored.and_then(|a| a.value_type) {
        Some(t) => Some(t),
        None => descriptor.map(|d| d.value_descriptor.base_type),
    };

    // -- (2) Coerce BEFORE validation; failure rejects the whole event ----
    // (§2A.5) — ref: AssetProcessingService.java:362-370 (getValueCoerced)
    let coerced_value = match declared_type {
        Some(t) => coerce_value(&event.value, t)?,
        None => event.value.clone(),
    };

    // -- (3) Normalize the event timestamp (§2A.2) ------------------------
    // ref: AssetProcessingService.java:279-285
    let event_ts = normalize_event_timestamp(event.timestamp, system_time_ms);

    // -- (4) Per-asset write lock around read-old → write (§2A.7) ---------
    // ref: AssetStorageService.java:1404-1439 (withAssetLock)
    let lock = asset_lock(&event.asset_id);
    let _guard = lock.lock().unwrap_or_else(|poison| poison.into_inner());

    // Read the CURRENT old value + timestamp under the lock so a concurrent
    // writer cannot have slipped a newer value in between resolve and write.
    let old = load_attribute_row(conn, &event.asset_id, &event.attribute_name)?;
    let old_value_timestamp = old.as_ref().map(|a| a.timestamp).unwrap_or(0);

    // Build the event with the authoritative old value/timestamp for the
    // outdated comparison (§2A.3 compares against current persisted state).
    let resolved = AttributeEvent {
        asset_id: event.asset_id.clone(),
        attribute_name: event.attribute_name.clone(),
        value: coerced_value.clone(),
        timestamp: event_ts,
        old_value: old.as_ref().and_then(|a| a.value.clone()),
        old_value_timestamp,
    };

    // The recorded type is the declared type, else the existing stored type,
    // else inferred from the (coerced) value's JSON shape.
    let recorded_type = declared_type
        .or_else(|| old.as_ref().and_then(|a| a.value_type))
        .or_else(|| infer_base_type(&coerced_value));

    // -- (5) Outdated check (STRICT >, §2A.3) -----------------------------
    // ref: AttributeEvent.java:219 (oldValueTimestamp - eventTimestamp > 0)
    if resolved.is_outdated() {
        // Outdated: DO NOT update current state, but STILL record the datapoint.
        datapoints::record_datapoint(
            conn,
            &event.asset_id,
            &event.attribute_name,
            &coerced_value,
            event_ts,
        )?;
        return Ok(EventOutcome::OutdatedRecordedOnly);
    }

    // Not outdated: update current attribute state (value/type/timestamp/meta)
    // AND record the datapoint. If the attribute already exists, carry its meta
    // forward — descriptor meta was merged ONCE at creation (§2A.6) and is never
    // re-merged on update. If this event is the FIRST writer of the attribute
    // (event-driven creation), this IS the attribute creation, so merge the
    // descriptor meta ONCE here — symmetric with declared-type recovery above.
    // ref: AssetProcessingService.java:264-445 (event materializes the attribute)
    let mut updated = Attribute {
        name: event.attribute_name.clone(),
        value_type: recorded_type,
        value: Some(coerced_value.clone()),
        value_str: None,
        timestamp: event_ts,
        meta: old.as_ref().map(|a| a.meta.clone()).unwrap_or_default(),
    };
    if old.is_none() {
        if let Some(d) = descriptor {
            // §2A.6 — merged ONCE at (event-driven) attribute creation.
            updated.merge_descriptor_meta_once(d);
        }
    }
    upsert_attribute_row(conn, &event.asset_id, &updated)?;
    datapoints::record_datapoint(
        conn,
        &event.asset_id,
        &event.attribute_name,
        &coerced_value,
        event_ts,
    )?;
    Ok(EventOutcome::Updated)
}

/// Infer a base type from a JSON value's shape (used only when neither the
/// stored attribute nor a descriptor declared one).
fn infer_base_type(value: &AttributeValue) -> Option<ValueBaseType> {
    use serde_json::Value;
    match &value.0 {
        Value::Number(_) => Some(ValueBaseType::Number),
        Value::Bool(_) => Some(ValueBaseType::Boolean),
        Value::String(_) => Some(ValueBaseType::Text),
        Value::Array(_) => Some(ValueBaseType::Array),
        Value::Object(_) => Some(ValueBaseType::Object),
        Value::Null => None,
    }
}

// ---------------------------------------------------------------------------
// Tests (§2A.1-8 binding acceptance criteria)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc as StdArc;
    use std::thread;

    fn descriptor(name: &str, base: ValueBaseType, meta: MetaMap) -> AttributeDescriptor {
        AttributeDescriptor {
            name: name.to_string(),
            value_descriptor: ValueDescriptor {
                name: format!("{name}_vd"),
                base_type: base,
                array_dimensions: 0,
                constraints: vec![],
                units: None,
                format: None,
            },
            meta,
        }
    }

    fn thermostat_type() -> AssetTypeInfo {
        let mut unit_meta = MetaMap::new();
        unit_meta.insert("unit".into(), json!("celsius"));
        AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![descriptor("temp", ValueBaseType::Number, unit_meta)],
        }
    }

    fn seed_thermostat(conn: &Connection, id: &str) -> Asset {
        let info = thermostat_type();
        upsert_asset_type(conn, &info).unwrap();
        let asset = Asset::new_with_type(
            id.to_string(),
            "master".into(),
            "Thermostat".into(),
            "Living room".into(),
            &info,
        );
        upsert_asset(conn, &asset).unwrap();
        asset
    }

    fn event(asset: &str, attr: &str, value: serde_json::Value, ts: i64) -> AttributeEvent {
        AttributeEvent {
            asset_id: asset.to_string(),
            attribute_name: attr.to_string(),
            value: AttributeValue(value),
            timestamp: ts,
            old_value: None,
            old_value_timestamp: 0,
        }
    }

    // Model round-trip: asset + typed attrs + meta + descriptors through SQLite.
    #[test]
    fn model_round_trip_through_sqlite() {
        let tmp = tempfile::tempdir().unwrap();
        let id = "asset-rt-1";
        // Persist with one connection...
        {
            let conn = open_iot_store(tmp.path()).unwrap();
            seed_thermostat(&conn, id);
            // The asset-type descriptor round-trips.
            let info = get_asset_type(&conn, "Thermostat").unwrap().unwrap();
            assert_eq!(info.asset_type, "Thermostat");
            assert_eq!(info.attributes.len(), 1);
            assert_eq!(
                info.attributes[0].value_descriptor.base_type,
                ValueBaseType::Number
            );
        }
        // ...and re-read with a fresh connection on the same core db.
        let conn2 = open_iot_store(tmp.path()).unwrap();
        let asset = get_asset(&conn2, id).unwrap().expect("asset present");
        assert_eq!(asset.realm, "master");
        assert_eq!(asset.asset_type, "Thermostat");
        assert_eq!(asset.name, "Living room");
        let temp = asset.attributes.get("temp").expect("temp attr");
        // Descriptor meta merged once at creation (§2A.6) survives the round-trip.
        assert_eq!(temp.meta.get("unit"), Some(&json!("celsius")));
        assert_eq!(temp.value_type, Some(ValueBaseType::Number));

        // list_assets by realm + by parent.
        let in_realm = list_assets(&conn2, "master", None).unwrap();
        assert_eq!(in_realm.len(), 1);
        assert_eq!(in_realm[0].id, id);
        assert!(list_assets(&conn2, "master", Some("no-such-parent"))
            .unwrap()
            .is_empty());

        // delete removes asset + attribute rows.
        delete_asset(&conn2, id).unwrap();
        assert!(get_asset(&conn2, id).unwrap().is_none());
    }

    // §2A.3 — outdated event leaves current state UNCHANGED but STILL records a
    // datapoint (asserted via the datapoints query).
    #[test]
    fn outdated_event_unchanged_state_but_datapoint_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_iot_store(tmp.path()).unwrap();
        let id = "asset-2a3";
        seed_thermostat(&conn, id);

        // First, a current write at ts=200 establishes current state = 22.0.
        let out =
            process_attribute_event(&conn, &event(id, "temp", json!(22.0), 200), 1_000).unwrap();
        assert_eq!(out, EventOutcome::Updated);

        // Now an OUTDATED event (ts=100 < current 200): strict outdated rule.
        let out2 =
            process_attribute_event(&conn, &event(id, "temp", json!(99.0), 100), 1_000).unwrap();
        assert_eq!(out2, EventOutcome::OutdatedRecordedOnly);

        // Current state must be UNCHANGED (still 22.0 @ ts=200).
        let asset = get_asset(&conn, id).unwrap().unwrap();
        let temp = asset.attributes.get("temp").unwrap();
        assert_eq!(
            temp.value.as_ref().unwrap().as_numeric(),
            Some(22.0),
            "state unchanged"
        );
        assert_eq!(temp.timestamp, 200);

        // ...but the outdated datapoint WAS recorded (both samples present).
        let dps = datapoints::all(&conn, id, "temp", 0, 10_000).unwrap();
        let ts: Vec<i64> = dps.iter().map(|d| d.timestamp_ms).collect();
        assert_eq!(
            ts,
            vec![100, 200],
            "both current + outdated datapoints recorded"
        );
        // The outdated sample carries its (coerced) value.
        let outdated_dp = dps.iter().find(|d| d.timestamp_ms == 100).unwrap();
        assert_eq!(outdated_dp.value.as_numeric(), Some(99.0));
    }

    // §2A.5 — coercion failure rejects the WHOLE event (no state change, no
    // datapoint).
    #[test]
    fn coercion_failure_rejects_event() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_iot_store(tmp.path()).unwrap();
        let id = "asset-2a5";
        seed_thermostat(&conn, id);

        // "notnum" cannot coerce to the declared Number type → whole event err.
        let err = process_attribute_event(&conn, &event(id, "temp", json!("notnum"), 100), 1_000)
            .unwrap_err();
        assert!(err.to_string().contains("coerce"), "got: {err}");

        // No current state and no datapoint were written.
        let asset = get_asset(&conn, id).unwrap().unwrap();
        let temp = asset.attributes.get("temp").unwrap();
        assert!(temp.value.is_none(), "no value written on rejected event");
        let dps = datapoints::all(&conn, id, "temp", 0, 10_000).unwrap();
        assert!(dps.is_empty(), "no datapoint on rejected event");

        // A coercible string ("21.5") succeeds (coercion happens before write).
        let out =
            process_attribute_event(&conn, &event(id, "temp", json!("21.5"), 200), 1_000).unwrap();
        assert_eq!(out, EventOutcome::Updated);
        let asset = get_asset(&conn, id).unwrap().unwrap();
        assert_eq!(
            asset
                .attributes
                .get("temp")
                .unwrap()
                .value
                .as_ref()
                .unwrap()
                .as_numeric(),
            Some(21.5),
            "coerced before persistence"
        );
    }

    // §2A.2 — future / zero timestamps are normalized on the STORED attribute.
    #[test]
    fn future_and_zero_timestamps_normalized_on_store() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_iot_store(tmp.path()).unwrap();
        let id = "asset-2a2";
        seed_thermostat(&conn, id);
        let sys = 1_000_000i64;

        // ts = 0 → normalized to system time on the stored attribute.
        process_attribute_event(&conn, &event(id, "temp", json!(1.0), 0), sys).unwrap();
        let temp = get_asset(&conn, id)
            .unwrap()
            .unwrap()
            .attributes
            .remove("temp")
            .unwrap();
        assert_eq!(temp.timestamp, sys, "zero ts → system time");

        // ts in the future → clamped to system time. Use a later system time so
        // the clamped value is not treated as outdated relative to the prior write.
        let sys2 = 2_000_000i64;
        process_attribute_event(&conn, &event(id, "temp", json!(2.0), sys2 + 5_000), sys2).unwrap();
        let temp = get_asset(&conn, id)
            .unwrap()
            .unwrap()
            .attributes
            .remove("temp")
            .unwrap();
        assert_eq!(temp.timestamp, sys2, "future ts → clamped to system time");
        assert_eq!(temp.value.unwrap().as_numeric(), Some(2.0));
    }

    // §2A.7 — N threads writing one asset serialize with no lost update.
    //
    // Each thread does a read-modify-write (increment a counter attribute) under
    // the per-asset lock. With proper serialization the final value equals the
    // number of successful increments and the highest timestamp wins as current
    // state; without the lock, concurrent read-old → write would lose updates.
    #[test]
    fn concurrent_writers_serialize_no_lost_update() {
        let tmp = tempfile::tempdir().unwrap();
        // Register an untyped "counter" asset (no declared type → raw numbers).
        let id = "asset-2a7";
        {
            let conn = open_iot_store(tmp.path()).unwrap();
            let info = AssetTypeInfo {
                asset_type: "Counter".into(),
                attributes: vec![descriptor("count", ValueBaseType::Number, MetaMap::new())],
            };
            upsert_asset_type(&conn, &info).unwrap();
            let asset = Asset::new_with_type(
                id.into(),
                "master".into(),
                "Counter".into(),
                "c".into(),
                &info,
            );
            upsert_asset(&conn, &asset).unwrap();
        }

        const THREADS: usize = 8;
        const PER_THREAD: usize = 25;
        let root = StdArc::new(tmp.path().to_path_buf());
        let mut handles = Vec::new();
        for t in 0..THREADS {
            let root = StdArc::clone(&root);
            handles.push(thread::spawn(move || {
                // Each thread opens its own connection on the same core db.
                let conn = open_iot_store(&root).unwrap();
                for i in 0..PER_THREAD {
                    // Read-modify-write under the per-asset lock: read current
                    // count, increment, write at a strictly increasing ts so the
                    // write is never treated as outdated.
                    let lock = asset_lock(id);
                    let guard = lock.lock().unwrap_or_else(|p| p.into_inner());
                    let cur = load_attribute_row(&conn, id, "count")
                        .unwrap()
                        .and_then(|a| a.value)
                        .and_then(|v| v.as_numeric())
                        .unwrap_or(0.0);
                    let next = cur + 1.0;
                    // Monotonic ts unique per (thread,i) and strictly increasing
                    // relative to whatever is stored, so it is never outdated.
                    let ts = (next as i64) + (t * PER_THREAD + i) as i64;
                    let attr = Attribute {
                        name: "count".into(),
                        value_type: Some(ValueBaseType::Number),
                        value: Some(AttributeValue(json!(next))),
                        value_str: None,
                        timestamp: ts,
                        meta: MetaMap::new(),
                    };
                    upsert_attribute_row(&conn, id, &attr).unwrap();
                    drop(guard);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        // No lost update: the counter reached exactly THREADS * PER_THREAD.
        let conn = open_iot_store(&root).unwrap();
        let count = load_attribute_row(&conn, id, "count")
            .unwrap()
            .unwrap()
            .value
            .unwrap()
            .as_numeric()
            .unwrap();
        assert_eq!(
            count,
            (THREADS * PER_THREAD) as f64,
            "serialized read-modify-write lost no increments"
        );
    }

    // §2A.7 — process_attribute_event itself must hold the per-asset lock across
    // its OWN read-old → outdated-check → write, so the production write path is
    // serialized for the same asset. This drives process_attribute_event from
    // many threads (the REAL write path, no test-side locking) and asserts a
    // lost-update-sensitive invariant: with globally-unique strictly-increasing
    // timestamps, the final persisted current state MUST be the event carrying
    // the GLOBAL MAXIMUM timestamp. Without the per-asset lock, a lower-ts event
    // could read old state (seeing a yet-lower ts), decide "not outdated", and
    // then write AFTER a higher-ts writer already committed — clobbering the
    // newer value and leaving an out-of-order final state. The "records all
    // datapoints" count is also checked, but the value/timestamp invariant is
    // what actually exercises serialization of the production RMW.
    #[test]
    fn concurrent_process_attribute_event_serializes_no_lost_update() {
        let tmp = tempfile::tempdir().unwrap();
        let id = "asset-2a7b";
        {
            let conn = open_iot_store(tmp.path()).unwrap();
            seed_thermostat(&conn, id);
        }
        const THREADS: usize = 6;
        const PER_THREAD: usize = 20;
        const TOTAL: usize = THREADS * PER_THREAD;
        let root = StdArc::new(tmp.path().to_path_buf());
        let mut handles = Vec::new();
        for t in 0..THREADS {
            let root = StdArc::clone(&root);
            handles.push(thread::spawn(move || {
                let conn = open_iot_store(&root).unwrap();
                for i in 0..PER_THREAD {
                    // Globally-unique strictly-positive ts per event; the value
                    // mirrors the ts so the final current value is identifiable.
                    let ts = (t * PER_THREAD + i + 1) as i64;
                    process_attribute_event(
                        &conn,
                        &event(id, "temp", json!(ts as f64), ts),
                        1_000_000,
                    )
                    .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let conn = open_iot_store(&root).unwrap();
        // Lost-update / out-of-order invariant: the persisted current state is
        // the GLOBAL-MAX-ts event. If the lock were missing, a stale lower-ts
        // writer could have committed last and this would fail.
        let temp = load_attribute_row(&conn, id, "temp").unwrap().unwrap();
        assert_eq!(
            temp.timestamp, TOTAL as i64,
            "current state is the global-max-ts event (serialized RMW)"
        );
        assert_eq!(
            temp.value.unwrap().as_numeric(),
            Some(TOTAL as f64),
            "current value matches the global-max-ts event"
        );
        // And every event still recorded exactly one datapoint.
        let dps = datapoints::all(&conn, id, "temp", 0, 10_000_000).unwrap();
        assert_eq!(
            dps.len(),
            TOTAL,
            "every concurrent event recorded a datapoint"
        );
    }

    // §2A.6 (event-driven creation) — when an inbound event is the FIRST writer
    // of a descriptor-backed attribute (the attribute row does not yet exist),
    // that event materializes the attribute and the descriptor meta must be
    // merged ONCE at this creation — symmetric with declared value_type recovery.
    // This covers the path seed_thermostat + Asset::new_with_type pre-materializes
    // away (and which previously dropped descriptor meta).
    #[test]
    fn descriptor_meta_merged_on_event_driven_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_iot_store(tmp.path()).unwrap();
        let id = "asset-2a6-evt";

        // Register a Thermostat type whose 'temp' descriptor carries meta
        // {unit: celsius}, but persist an asset with NO pre-materialized
        // attribute rows, so the event is the first writer of 'temp'.
        let info = thermostat_type();
        upsert_asset_type(&conn, &info).unwrap();
        let asset = Asset {
            id: id.into(),
            parent_id: None,
            realm: "master".into(),
            asset_type: "Thermostat".into(),
            name: "Living room".into(),
            path: Vec::new(),
            attributes: std::collections::BTreeMap::new(),
        };
        upsert_asset(&conn, &asset).unwrap();
        // Sanity: no attribute row exists yet.
        assert!(load_attribute_row(&conn, id, "temp").unwrap().is_none());

        // The event creates the attribute.
        let out =
            process_attribute_event(&conn, &event(id, "temp", json!(22.0), 100), 1_000).unwrap();
        assert_eq!(out, EventOutcome::Updated);

        let temp = get_asset(&conn, id)
            .unwrap()
            .unwrap()
            .attributes
            .remove("temp")
            .unwrap();
        // value_type recovered from the descriptor (the previously-working half)...
        assert_eq!(temp.value_type, Some(ValueBaseType::Number));
        assert_eq!(temp.value.unwrap().as_numeric(), Some(22.0));
        // ...AND descriptor meta merged ONCE at this event-driven creation
        // (the previously-dropped half — the fix under test).
        assert_eq!(
            temp.meta.get("unit"),
            Some(&json!("celsius")),
            "descriptor meta merged once at event-driven attribute creation"
        );

        // A subsequent event must NOT re-merge / disturb the meta (§2A.6 once).
        process_attribute_event(&conn, &event(id, "temp", json!(23.0), 200), 1_000).unwrap();
        let temp2 = get_asset(&conn, id)
            .unwrap()
            .unwrap()
            .attributes
            .remove("temp")
            .unwrap();
        assert_eq!(temp2.value.unwrap().as_numeric(), Some(23.0));
        assert_eq!(temp2.meta.get("unit"), Some(&json!("celsius")));
    }

    // §2A.6 — descriptor meta is merged ONCE at creation and not re-merged on a
    // subsequent event update (the write path carries existing meta forward).
    #[test]
    fn descriptor_meta_not_remerged_on_update() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_iot_store(tmp.path()).unwrap();
        let id = "asset-2a6";
        let asset = seed_thermostat(&conn, id);
        // Sanity: created with descriptor meta.
        assert_eq!(
            asset.attributes.get("temp").unwrap().meta.get("unit"),
            Some(&json!("celsius"))
        );

        // An event update keeps meta but never re-applies descriptor defaults
        // (we prove meta is preserved across the value update).
        process_attribute_event(&conn, &event(id, "temp", json!(18.0), 500), 1_000).unwrap();
        let temp = get_asset(&conn, id)
            .unwrap()
            .unwrap()
            .attributes
            .remove("temp")
            .unwrap();
        assert_eq!(temp.value.unwrap().as_numeric(), Some(18.0));
        assert_eq!(
            temp.meta.get("unit"),
            Some(&json!("celsius")),
            "meta preserved, not re-merged"
        );
    }
}
