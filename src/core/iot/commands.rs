// Origin: CTOX
// License: AGPL-3.0-only
//
// IoT Phase-2 command path: two thin adapters over ONE shared set of engine-op
// functions.
//
//   * `handle_iot_command`     — the `ctox iot …` CLI surface (parse argv, call
//     the shared op, print pretty JSON to stdout).
//   * `handle_business_command`— the RxDB `business_commands` executor surface
//     (typed payload in, structured outcome value out).
//
// Both surfaces funnel through the same `asset_*` / `attribute_* / datapoints_* /
// alarm_* / ruleset_* / agent_*` op functions, which wrap the already-green
// Phase-1 engine (`crate::iot::{store, datapoints, alarms, model}`) and mirror
// every authoritative mutation into `business_records` projection rows keyed by
// `(collection='iot_*', record_id)`. The existing RxDB machinery echoes those
// rows to the browser over WebRTC — there is NO HTTP data bridge, and this
// module reads/writes only the single core db (runtime/ctox.sqlite3) via
// `crate::paths::core_db(root)`.
//
// Ruleset/agent ops are real CRUD persisting to thin `iot_rulesets` /
// `iot_agents` / `iot_agent_status` tables created lazily here — rules/agents
// are not evaluated until later phases, but the records are real (no panics).
//
// Time model (see iot/mod.rs):
//   * attribute / datapoint / alarm domain time is i64 epoch-ms (§2A.13).
//   * projection `updated_at_ms` is `crate::iot::now_ms()`.
//   * `created_at` / `updated_at` audit columns on the thin stub tables are
//     RFC-3339 millis-precision UTC TEXT via `now_iso()` (CTOX house style).

use crate::iot::model::{Asset, AssetTypeInfo, AttributeEvent, AttributeValue};
use crate::iot::{alarms, datapoints, now_iso, now_ms, store, Result};
use anyhow::{anyhow, bail, Context};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

// ---------------------------------------------------------------------------
// Collection name constants (the 9 iot_* business_records collections)
// ---------------------------------------------------------------------------

const COLLECTION_ASSETS: &str = "iot_assets";
const COLLECTION_ATTRIBUTES: &str = "iot_attributes";
const COLLECTION_ASSET_TYPES: &str = "iot_asset_types";
const COLLECTION_REALMS: &str = "iot_realms";
const COLLECTION_DATAPOINTS: &str = "iot_datapoints";
const COLLECTION_ALARMS: &str = "iot_alarms";
const COLLECTION_RULESETS: &str = "iot_rulesets";
const COLLECTION_AGENTS: &str = "iot_agents";
const COLLECTION_AGENT_STATUS: &str = "iot_agent_status";

// ---------------------------------------------------------------------------
// EngineOutcome — the structured result both surfaces share
// ---------------------------------------------------------------------------

/// Result of one mutating op: the domain object plus the list of
/// `(collection, record_id)` projection rows written to `business_records`
/// during execution (the rxdb_peer branch streams those rows to RxDB).
#[derive(Debug)]
pub(crate) struct EngineOutcome {
    /// Domain object: `{ "asset": {…} }` / `{ "alarm": {…} }` / …
    pub result: Value,
    /// `(collection, record_id)` pairs echoed to RxDB.
    pub projections: Vec<(&'static str, String)>,
}

impl EngineOutcome {
    /// Flatten into the wire value: the domain `result` object with a
    /// `projections` array appended.
    pub(crate) fn into_value(self) -> Value {
        let mut obj = match self.result {
            Value::Object(map) => map,
            other => {
                let mut m = serde_json::Map::new();
                m.insert("result".to_string(), other);
                m
            }
        };
        let projections: Vec<Value> = self
            .projections
            .iter()
            .map(|(collection, id)| json!({ "collection": collection, "id": id }))
            .collect();
        obj.insert("projections".to_string(), Value::Array(projections));
        Value::Object(obj)
    }
}

// ---------------------------------------------------------------------------
// Request shapes (deserialized from the executor payload; CLI builds them too)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct AssetUpsertReq {
    /// Omit to create a new asset (a 22-char Base62 id is generated).
    #[serde(default)]
    pub id: Option<String>,
    pub realm: String,
    pub asset_type: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Optional asset-type descriptor to register alongside the asset.
    #[serde(default)]
    pub asset_type_info: Option<AssetTypeInfo>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct AttributeWriteReq {
    pub asset_id: String,
    pub name: String,
    pub value: Value,
    /// Domain timestamp epoch-ms; `<= 0` (or omitted) → normalized to now (§2A.2).
    #[serde(default)]
    pub timestamp_ms: i64,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct DatapointsQueryReq {
    pub asset_id: String,
    pub attribute_name: String,
    pub from_ms: i64,
    pub to_ms: i64,
    /// `"all" | "interval" | "lttb"`.
    pub shape: String,
    #[serde(default)]
    pub interval_ms: Option<i64>,
    #[serde(default)]
    pub threshold: Option<usize>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct AlarmUpdateReq {
    pub alarm_id: String,
    /// `"ack" | "assign" | "resolve" | "close" | "progress" | "status"`.
    pub action: String,
    #[serde(default)]
    pub assignee: Option<String>,
    /// Required when `action == "status"`: the target status name.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct RulesetSaveReq {
    /// Omit to create (a generated id is assigned).
    #[serde(default)]
    pub id: Option<String>,
    pub realm: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Opaque rule JSON; evaluated by the native rules layer.
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct AgentConfigureReq {
    /// Omit to create (a generated id is assigned).
    #[serde(default)]
    pub id: Option<String>,
    pub realm: String,
    pub name: String,
    /// `"mqtt" | "http" | "websocket"`.
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Agent config JSON consumed by the native protocol supervisor.
    #[serde(default)]
    pub data: Value,
}

/// RFC 0011 — a dashboard groups automation widgets.
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct DashboardUpsertReq {
    #[serde(default)]
    pub id: Option<String>,
    pub realm: String,
    pub name: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub scope_ref: Option<String>,
    #[serde(default)]
    pub view_mode: Option<String>,
    #[serde(default)]
    pub sort_index: Option<i64>,
}

/// RFC 0011 — a widget = an automation, von CTOX in drei Teilen programmiert:
/// `cond_text` (Wenn, Freitext) → `trigger_code` (Rhai-Wächter, generiert),
/// `render_code` (Widget-Code/Visualisierung, generiert), `action_prompt` (Dann).
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct WidgetUpsertReq {
    #[serde(default)]
    pub id: Option<String>,
    pub dashboard_id: String,
    pub realm: String,
    pub signal_ref: String,
    #[serde(default)]
    pub cond_text: Option<String>,
    #[serde(default)]
    pub action_prompt: Option<String>,
    /// Generated by CTOX (`compile_trigger`). Opaque Rhai source.
    #[serde(default)]
    pub trigger_code: Option<String>,
    /// Generated by CTOX (`generate_render`). Opaque HTML/CSS/JS, sandboxed at render.
    #[serde(default)]
    pub render_code: Option<String>,
    #[serde(default)]
    pub x: Option<i64>,
    #[serde(default)]
    pub y: Option<i64>,
    #[serde(default)]
    pub w: Option<i64>,
    #[serde(default)]
    pub h: Option<i64>,
    #[serde(default)]
    pub sort_index: Option<i64>,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Thin ruleset / agent schema. Created lazily on the shared core db; the rows are
// real engine state and are also consumed by the runtime supervisor.
// ---------------------------------------------------------------------------

pub(crate) fn ensure_stub_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS iot_rulesets (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            name        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            data        TEXT NOT NULL,
            last_fired_ms INTEGER,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_rulesets_realm ON iot_rulesets(realm);

        CREATE TABLE IF NOT EXISTS iot_agents (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            name        TEXT NOT NULL,
            kind        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            data        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_agents_realm ON iot_agents(realm);

        -- SECRET REDACTION CONTRACT: the `error` column is intentionally never
        -- written from a connect/transport failure path. Agent connect errors are
        -- absorbed by the protocol agents (they may carry credentials from the
        -- secret store) and must NEVER be persisted here verbatim. If a future
        -- phase records a status error, it MUST sanitize the message first
        -- (no usernames, passwords, auth-header values, or tokens) — see the
        -- redact_auth helper in agents/ws_native.rs for the redaction pattern.
        CREATE TABLE IF NOT EXISTS iot_agent_status (
            id            TEXT PRIMARY KEY,
            agent_id      TEXT NOT NULL,
            realm         TEXT NOT NULL,
            link_state    TEXT NOT NULL,
            last_event_ms INTEGER,
            error         TEXT,
            data          TEXT NOT NULL,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL
        );

        -- RFC 0011 — Automation-Widgets: ein Dashboard gruppiert Widgets.
        CREATE TABLE IF NOT EXISTS iot_dashboards (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            name        TEXT NOT NULL,
            scope       TEXT,
            scope_ref   TEXT,
            view_mode   TEXT,
            sort_index  INTEGER,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_dashboards_realm ON iot_dashboards(realm);

        -- RFC 0011 — ein Widget = eine Automatisierung. trigger_code (Rhai-Wächter)
        -- und render_code (Widget-Code) werden von CTOX generiert.
        CREATE TABLE IF NOT EXISTS iot_widgets (
            id             TEXT PRIMARY KEY,
            dashboard_id   TEXT NOT NULL,
            realm          TEXT NOT NULL,
            signal_ref     TEXT NOT NULL,
            cond_text      TEXT,
            action_prompt  TEXT,
            trigger_code   TEXT,
            trigger_state  TEXT,
            trigger_status TEXT,
            render_code    TEXT,
            x              INTEGER,
            y              INTEGER,
            w              INTEGER,
            h              INTEGER,
            sort_index     INTEGER,
            created_at     TEXT NOT NULL,
            updated_at     TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_widgets_dashboard ON iot_widgets(dashboard_id);
        CREATE INDEX IF NOT EXISTS idx_iot_widgets_realm ON iot_widgets(realm);

        -- spec §5 — inbound webhook registry: token (in the secret store) → the
        -- one signal it may write. Backend-only (no RxDB projection); the service
        -- HTTP route looks rows up directly.
        CREATE TABLE IF NOT EXISTS iot_webhooks (
            id          TEXT PRIMARY KEY,
            realm       TEXT NOT NULL,
            signal_ref  TEXT NOT NULL,
            value_path  TEXT,
            secret_name TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_iot_webhooks_realm ON iot_webhooks(realm);",
    )
    .context("failed to create IoT ruleset/agent stub schema")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Projection write — mirror an authoritative engine row into business_records.
//
// This writes the same `(collection, record_id, rev, deleted, updated_at_ms,
// payload_json)` shape as business_os::store::upsert_business_record. We write
// it inline (rather than via the cross-module private helper) so this module
// stays self-contained against the one shared core db; the existing rxdb_peer
// projection branch echoes the rows to RxDB unchanged.
// ---------------------------------------------------------------------------

/// Ensure the `business_records` projection table exists. In the integrated
/// build the business_os store migration owns this table, but the iot store
/// connection (`open_iot_store`) may be opened first (CLI surface), so we
/// create it idempotently with the SAME schema as
/// `business_os::store::migrate` to keep the projection contract identical.
fn ensure_business_records_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS business_records (
            collection TEXT NOT NULL,
            record_id TEXT NOT NULL,
            rev TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY (collection, record_id)
        );
        CREATE INDEX IF NOT EXISTS idx_business_records_collection_updated
            ON business_records(collection, updated_at_ms, record_id);",
    )
    .context("failed to ensure business_records schema")?;
    Ok(())
}

fn project_record(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
    deleted: bool,
) -> Result<()> {
    ensure_business_records_schema(conn)?;
    let rev = format!("rev_{}", uuid::Uuid::new_v4());
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev.clone()));
        obj.insert("_deleted".to_string(), Value::Bool(deleted));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        params![
            collection,
            record_id,
            rev,
            if deleted { 1 } else { 0 },
            updated_at_ms,
            serde_json::to_string(&payload).context("failed to serialize projection payload")?,
        ],
    )
    .context("failed to write iot projection row")?;
    Ok(())
}

/// Read back a single projection payload (used by tests + the row builders'
/// self-checks). `None` if no row exists.
#[cfg(test)]
fn read_projection(conn: &Connection, collection: &str, record_id: &str) -> Result<Option<Value>> {
    let row: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM business_records WHERE collection = ?1 AND record_id = ?2",
            params![collection, record_id],
            |r| r.get(0),
        )
        .optional()
        .context("failed to read projection row")?;
    match row {
        Some(p) => Ok(Some(
            serde_json::from_str(&p).context("failed to parse projection payload")?,
        )),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Pure row builders — build the canonical `data` + light index envelope.
// `project_record` injects id/_rev/_deleted/updated_at_ms.
// ---------------------------------------------------------------------------

fn asset_row(asset: &Asset) -> (String, Value) {
    let payload = json!({
        "realm": asset.realm,
        "parent_id": asset.parent_id,
        "asset_type": asset.asset_type,
        "name": asset.name,
        "data": serde_json::to_value(asset).unwrap_or(Value::Null),
        "index_text": asset.name.to_lowercase(),
        "sort_key": asset.name,
        "created_at_ms": now_ms(),
    });
    (asset.id.clone(), payload)
}

fn attribute_record_id(asset_id: &str, name: &str) -> String {
    format!("{asset_id}:{name}")
}

fn attribute_row(asset: &Asset, name: &str) -> Option<(String, Value)> {
    let attr = asset.attributes.get(name)?;
    let value_type = attr
        .value_type
        .map(|t| format!("{t:?}"))
        .unwrap_or_default();
    let payload = json!({
        "realm": asset.realm,
        "asset_id": asset.id,
        "attribute_name": attr.name,
        "value_type": value_type,
        "timestamp_ms": attr.timestamp,
        "data": serde_json::to_value(attr).unwrap_or(Value::Null),
        "index_text": attr.name.to_lowercase(),
        "sort_key": attr.name,
        "status_key": value_type,
        "created_at_ms": now_ms(),
    });
    Some((attribute_record_id(asset.id.as_str(), name), payload))
}

fn asset_type_row(info: &AssetTypeInfo) -> (String, Value) {
    let payload = json!({
        "asset_type": info.asset_type,
        "attribute_count": info.attributes.len(),
        "data": serde_json::to_value(info).unwrap_or(Value::Null),
        "index_text": info.asset_type.to_lowercase(),
        "sort_key": info.asset_type,
        "created_at_ms": now_ms(),
    });
    (info.asset_type.clone(), payload)
}

fn alarm_row(alarm: &alarms::Alarm) -> (String, Value) {
    let status = format!("{:?}", alarm.status).to_uppercase_snake();
    let severity = format!("{:?}", alarm.severity).to_uppercase();
    let payload = json!({
        "realm": alarm.realm,
        "title": alarm.title,
        "severity": severity,
        "status": status,
        "assignee_id": alarm.assignee_id,
        "source": format!("{:?}", alarm.source),
        "created_ms": alarm.created,
        "data": serde_json::to_value(alarm).unwrap_or(Value::Null),
        "index_text": alarm.title.to_lowercase(),
        // Zero-padded created_ms, descending sort handled by the consumer.
        "sort_key": format!("{:020}", alarm.created),
        "status_key": status,
        "created_at_ms": alarm.created,
    });
    (alarm.id.clone(), payload)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

trait UpperSnake {
    fn to_uppercase_snake(&self) -> String;
}

impl UpperSnake for String {
    /// Map a Rust-debug enum variant (`InProgress`) to the upstream wire form
    /// (`IN_PROGRESS`).
    fn to_uppercase_snake(&self) -> String {
        let mut out = String::with_capacity(self.len() + 4);
        for (i, ch) in self.chars().enumerate() {
            if ch.is_uppercase() && i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
        }
        out
    }
}

fn value_as_attribute(value: Value) -> AttributeValue {
    AttributeValue(value)
}

fn parse_status(name: &str) -> Result<alarms::Status> {
    Ok(match name.to_ascii_uppercase().as_str() {
        "OPEN" => alarms::Status::Open,
        "ACKNOWLEDGED" | "ACK" => alarms::Status::Acknowledged,
        "IN_PROGRESS" | "INPROGRESS" | "PROGRESS" => alarms::Status::InProgress,
        "RESOLVED" => alarms::Status::Resolved,
        "CLOSED" => alarms::Status::Closed,
        other => bail!("unknown alarm status: {other}"),
    })
}

// ===========================================================================
// Multi-realm isolation (Phase 2 — basic read/write isolation).
//
// Phase 2 enforces BASIC realm isolation on every read/write/projection: a
// resource (asset / alarm / datapoint window / ruleset / agent) may only be
// touched when its realm matches the realm the CALLER is authorized for. The
// authorized realm is derived from the BusinessOsSession (see
// `session_realm`) — it is NEVER trusted from the client payload's `realm`
// field. Fine-grained per-realm ACL (multiple authorized realms per user, role
// scoping) is DEFERRED to the Phase 3 rules engine; Phase 2 still hard-enforces
// "resource.realm == authorized_realm" so no command can read or mutate across
// realms.
//
// The shared ops take `realm: Option<&str>`:
//   * `Some(r)` — enforce isolation against realm `r` (the executor/session path).
//   * `None`    — trusted local surface (the `ctox iot …` CLI is an operator
//     tool running with full host access); it bypasses realm scoping the same
//     way the CLI already bypasses the session ACL gate.
// ===========================================================================

/// Derive the realm a session is authorized for. Phase 2: a single authorized
/// realm. Placeholder for Phase 3 `session.get_authorized_realms()`:
///   * an admin session is authorized for the realm it operates in — which, for
///     a CTOX single-tenant runtime, is `master` unless a future ACL says
///     otherwise (admins are not auto-granted cross-realm reads here);
///   * a non-admin authenticated session is bound to its own realm, modeled for
///     now as `master` until the Phase 3 user→realm map lands.
/// Returns the realm string the executor enforces every op against.
fn session_realm(session: &crate::business_os::store::BusinessOsSession) -> String {
    // Phase 2 single-realm model. The user's role/id is the Phase-3 ACL key; we
    // intentionally do NOT read any realm from the client payload here.
    let _ = session.user.as_ref().map(|u| (&u.id, &u.role));
    "master".to_string()
}

/// Centralized realm enforcement for asset-scoped ops. With `Some(realm)` the
/// asset must exist AND belong to `realm` (cross-realm id → "asset not found");
/// with `None` (trusted CLI) it is an unscoped fetch.
fn validate_asset_realm(conn: &Connection, asset_id: &str, realm: Option<&str>) -> Result<Asset> {
    let asset = match realm {
        Some(r) => store::get_asset_in_realm(conn, asset_id, r)?,
        None => store::get_asset(conn, asset_id)?,
    };
    asset.ok_or_else(|| anyhow!("asset not found: {asset_id}"))
}

// ===========================================================================
// SHARED ENGINE OPS — the single code path both surfaces call.
// ===========================================================================

// ---- assets ----

pub(crate) fn asset_list(root: &Path, realm: &str, parent_id: Option<&str>) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    let assets = store::list_assets(&conn, realm, parent_id)?;
    Ok(json!({ "assets": serde_json::to_value(assets)? }))
}

pub(crate) fn asset_show(root: &Path, asset_id: &str, realm: Option<&str>) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    // Realm-scoped: a cross-realm id resolves to "not found".
    let asset = validate_asset_realm(&conn, asset_id, realm)?;
    Ok(json!({ "asset": serde_json::to_value(asset)? }))
}

pub(crate) fn asset_upsert(
    root: &Path,
    mut req: AssetUpsertReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;

    // Multi-realm isolation: the realm is NEVER trusted from the client payload.
    // When a session realm is supplied, force the asset's realm to it (overriding
    // whatever the payload claimed) and reject an UPDATE that targets an existing
    // asset owned by a different realm (cross-realm mutation).
    if let Some(r) = realm {
        req.realm = r.to_string();
        if let Some(id) = req.id.as_deref() {
            if let Some(existing) = store::get_asset(&conn, id)? {
                anyhow::ensure!(
                    existing.realm == r,
                    "asset not found: {id}" // do not leak cross-realm existence
                );
            }
        }
    }

    // Register the asset-type descriptor first if supplied (it must exist before
    // Asset::new_with_type pre-materializes the typed attribute rows).
    if let Some(info) = req.asset_type_info.as_ref() {
        store::upsert_asset_type(&conn, info)?;
    }

    let id = req.id.clone().unwrap_or_else(Asset::generate_id);
    // Build the asset, preserving any existing attributes on update.
    let mut asset = match store::get_asset(&conn, &id)? {
        Some(existing) => existing,
        None => match req.asset_type_info.as_ref() {
            Some(info) => Asset::new_with_type(
                id.clone(),
                req.realm.clone(),
                req.asset_type.clone(),
                req.name.clone(),
                info,
            ),
            None => Asset {
                id: id.clone(),
                parent_id: None,
                realm: req.realm.clone(),
                asset_type: req.asset_type.clone(),
                name: req.name.clone(),
                path: Vec::new(),
                attributes: std::collections::BTreeMap::new(),
            },
        },
    };
    asset.realm = req.realm.clone();
    asset.asset_type = req.asset_type.clone();
    asset.name = req.name.clone();
    asset.parent_id = req.parent_id.clone();

    store::upsert_asset(&conn, &asset)?;

    // Re-read so the projection reflects fully-rehydrated current state.
    let asset = store::get_asset(&conn, &id)?
        .ok_or_else(|| anyhow!("asset vanished after upsert: {id}"))?;

    let updated_at = now_ms();
    let mut projections: Vec<(&'static str, String)> = Vec::new();

    let (asset_record_id, asset_payload) = asset_row(&asset);
    project_record(
        &conn,
        COLLECTION_ASSETS,
        &asset_record_id,
        updated_at,
        asset_payload,
        false,
    )?;
    projections.push((COLLECTION_ASSETS, asset_record_id));

    if let Some(info) = req.asset_type_info.as_ref() {
        let (type_id, type_payload) = asset_type_row(info);
        project_record(
            &conn,
            COLLECTION_ASSET_TYPES,
            &type_id,
            updated_at,
            type_payload,
            false,
        )?;
        projections.push((COLLECTION_ASSET_TYPES, type_id));
    }

    // §2A.14 — bound the attribute fan-out (mirrors projector::project_asset).
    let attr_count = asset.attributes.len();
    if attr_count > crate::iot::projector::MAX_ATTRIBUTES_PER_ASSET {
        eprintln!(
            "CTOX IoT asset_upsert projection truncated to {} attribute rows for \
             asset_id={} (had {attr_count})",
            crate::iot::projector::MAX_ATTRIBUTES_PER_ASSET,
            asset.id
        );
    }
    for name in asset
        .attributes
        .keys()
        .take(crate::iot::projector::MAX_ATTRIBUTES_PER_ASSET)
    {
        if let Some((rid, payload)) = attribute_row(&asset, name) {
            project_record(
                &conn,
                COLLECTION_ATTRIBUTES,
                &rid,
                updated_at,
                payload,
                false,
            )?;
            projections.push((COLLECTION_ATTRIBUTES, rid));
        }
    }

    Ok(EngineOutcome {
        result: json!({ "asset": serde_json::to_value(&asset)? }),
        projections,
    })
}

pub(crate) fn asset_delete(
    root: &Path,
    asset_id: &str,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    // Realm-scoped: validate the asset belongs to the caller's realm BEFORE
    // deleting. A cross-realm id is rejected as "not found" — no cross-realm
    // delete. (CLI passes None and may delete any asset on the host.)
    let existing = match realm {
        Some(_) => Some(validate_asset_realm(&conn, asset_id, realm)?),
        None => store::get_asset(&conn, asset_id)?,
    };
    store::delete_asset(&conn, asset_id)?;

    let updated_at = now_ms();
    let mut projections: Vec<(&'static str, String)> = Vec::new();

    // Tombstone the asset projection row.
    project_record(
        &conn,
        COLLECTION_ASSETS,
        asset_id,
        updated_at,
        json!({ "data": {} }),
        true,
    )?;
    projections.push((COLLECTION_ASSETS, asset_id.to_string()));

    // Tombstone each attribute projection row too.
    if let Some(asset) = existing.as_ref() {
        for name in asset.attributes.keys() {
            let rid = attribute_record_id(asset_id, name);
            project_record(
                &conn,
                COLLECTION_ATTRIBUTES,
                &rid,
                updated_at,
                json!({ "data": {} }),
                true,
            )?;
            projections.push((COLLECTION_ATTRIBUTES, rid));
        }
    }

    Ok(EngineOutcome {
        result: json!({ "deleted": asset_id }),
        projections,
    })
}

// ---- attributes (device-write path; §2A semantics via process_attribute_event) ----

pub(crate) fn attribute_read(
    root: &Path,
    asset_id: &str,
    name: &str,
    realm: Option<&str>,
) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    // Realm-scoped: a cross-realm asset id resolves to "not found".
    let asset = validate_asset_realm(&conn, asset_id, realm)?;
    let attr = asset
        .attributes
        .get(name)
        .ok_or_else(|| anyhow!("attribute not found: {asset_id}:{name}"))?;
    Ok(json!({ "attribute": serde_json::to_value(attr)? }))
}

pub(crate) fn attribute_write(
    root: &Path,
    req: AttributeWriteReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;

    // Multi-realm isolation: the target asset must belong to the caller's realm
    // before any write. A cross-realm asset id is rejected as "not found".
    if let Some(r) = realm {
        validate_asset_realm(&conn, &req.asset_id, Some(r))?;
    }

    // Capture the prior value/realm BEFORE the write so condition predicates that
    // test `previousValue` (§2A.23) see the value the event actually replaced.
    let prior_asset = store::get_asset(&conn, &req.asset_id)?;
    let prior_value = prior_asset
        .as_ref()
        .and_then(|a| a.attributes.get(&req.name))
        .and_then(|attr| attr.value.clone());
    let realm = prior_asset.as_ref().map(|a| a.realm.clone());

    let event = AttributeEvent {
        asset_id: req.asset_id.clone(),
        attribute_name: req.name.clone(),
        value: value_as_attribute(req.value.clone()),
        timestamp: req.timestamp_ms,
        old_value: None,
        old_value_timestamp: 0,
    };
    let now = now_ms();
    let outcome = store::process_attribute_event(&conn, &event, now)?;

    // Route the just-applied change through the thin condition layer. An explicit
    // engine write is a live change (not a cold boot replay), so the engine is
    // warm for §2A.21 purposes. The condition layer raises alarms and emits one
    // durable, budget-bounded queue task per dedup key — CTOX's mission brain
    // does the firing; no second automation engine runs here.
    if let Some(realm) = realm.as_deref() {
        let eval_event = AttributeEvent {
            asset_id: req.asset_id.clone(),
            attribute_name: req.name.clone(),
            value: value_as_attribute(req.value.clone()),
            timestamp: req.timestamp_ms,
            old_value: prior_value,
            old_value_timestamp: 0,
        };
        crate::iot::conditions::evaluate_and_emit(root, realm, &eval_event, true, now)?;
    }

    // RFC 0011 — also drive the NEW automation-widget watchers bound to this
    // signal. Independent of the legacy ruleset path above: a new "Auftrag" uses
    // the CTOX-generated Rhai watcher (per-datapoint, stateful), not the
    // deterministic condition template. A bad watcher flips its own widget to
    // `needs_attention`; it never fails the device write.
    if let Err(err) =
        crate::iot::widget_runtime::tick_widgets_for_signal(root, &req.asset_id, &req.name, now)
    {
        eprintln!(
            "ctox::iot::widget_runtime: watcher dispatch failed for {}::{}: {err}",
            req.asset_id, req.name
        );
    }

    // Re-read the asset so the projection reflects the post-write current state.
    let asset = store::get_asset(&conn, &req.asset_id)?
        .ok_or_else(|| anyhow!("asset vanished after attribute write: {}", req.asset_id))?;

    let updated_at = now_ms();
    let mut projections: Vec<(&'static str, String)> = Vec::new();

    // Project the attribute row and refresh the asset summary projection.
    if let Some((rid, payload)) = attribute_row(&asset, &req.name) {
        project_record(
            &conn,
            COLLECTION_ATTRIBUTES,
            &rid,
            updated_at,
            payload,
            false,
        )?;
        projections.push((COLLECTION_ATTRIBUTES, rid));
    }
    let (asset_rid, asset_payload) = asset_row(&asset);
    project_record(
        &conn,
        COLLECTION_ASSETS,
        &asset_rid,
        updated_at,
        asset_payload,
        false,
    )?;
    projections.push((COLLECTION_ASSETS, asset_rid));

    Ok(EngineOutcome {
        result: json!({
            "outcome": format!("{outcome:?}"),
            "attribute": asset.attributes.get(&req.name).and_then(|a| serde_json::to_value(a).ok()),
        }),
        projections,
    })
}

// ---- datapoints (windowed; writes one iot_datapoints projection row) ----

pub(crate) fn datapoints_query(
    root: &Path,
    req: DatapointsQueryReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;

    // Multi-realm isolation: validate the asset exists AND belongs to the
    // caller's realm before reading any datapoints (iot_datapoints itself is
    // keyed by asset_id only, so the guard MUST live here). Cross-realm asset id
    // → "not found". The window's realm is stamped from the validated asset, not
    // from any client field.
    let scoped_realm = match realm {
        Some(r) => {
            let asset = validate_asset_realm(&conn, &req.asset_id, Some(r))?;
            asset.realm
        }
        None => store::get_asset(&conn, &req.asset_id)?
            .map(|a| a.realm)
            .unwrap_or_default(),
    };
    let shape = req.shape.to_ascii_lowercase();

    // Run the requested shape; capture the bounded point array + truncation flag.
    let (points, point_count, truncated): (Value, usize, bool) = match shape.as_str() {
        "all" => {
            let dps = datapoints::all(
                &conn,
                &req.asset_id,
                &req.attribute_name,
                req.from_ms,
                req.to_ms,
            )?;
            let truncated = dps.len() >= datapoints::DEFAULT_QUERY_LIMIT;
            let count = dps.len();
            (serde_json::to_value(dps)?, count, truncated)
        }
        "interval" => {
            let interval_ms = req
                .interval_ms
                .ok_or_else(|| anyhow!("interval shape requires interval_ms"))?;
            // interval() returns its own authoritative truncation flag (the bucket
            // window was clamped to DEFAULT_QUERY_LIMIT), matching all()/lttb().
            let (pts, truncated) = datapoints::interval(
                &conn,
                &req.asset_id,
                &req.attribute_name,
                req.from_ms,
                req.to_ms,
                interval_ms,
            )?;
            let count = pts.len();
            (serde_json::to_value(pts)?, count, truncated)
        }
        "lttb" => {
            let threshold = req
                .threshold
                .ok_or_else(|| anyhow!("lttb shape requires threshold"))?;
            let pts = datapoints::lttb_query(
                &conn,
                &req.asset_id,
                &req.attribute_name,
                req.from_ms,
                req.to_ms,
                threshold,
            )?;
            let truncated = pts.len() > threshold;
            let count = pts.len();
            (serde_json::to_value(pts)?, count, truncated)
        }
        other => bail!("unknown datapoints shape: {other} (expected all|interval|lttb)"),
    };

    let window_key = format!(
        "{}:{}:{}:{}:{}",
        req.asset_id, req.attribute_name, req.from_ms, req.to_ms, shape
    );
    let updated_at = now_ms();
    let payload = json!({
        "realm": scoped_realm,
        "asset_id": req.asset_id,
        "attribute_name": req.attribute_name,
        "from_ms": req.from_ms,
        "to_ms": req.to_ms,
        "shape": shape,
        "point_count": point_count,
        "truncated": truncated,
        "data": points,
        "index_text": format!("{}:{}", req.asset_id, req.attribute_name).to_lowercase(),
        "sort_key": window_key,
        "created_at_ms": updated_at,
    });
    project_record(
        &conn,
        COLLECTION_DATAPOINTS,
        &window_key,
        updated_at,
        payload.clone(),
        false,
    )?;

    Ok(EngineOutcome {
        result: json!({
            "window": window_key,
            "shape": shape,
            "point_count": point_count,
            "truncated": truncated,
            "points": payload.get("data").cloned().unwrap_or(Value::Null),
        }),
        projections: vec![(COLLECTION_DATAPOINTS, window_key)],
    })
}

// ---- alarms ----

pub(crate) fn alarm_list(
    root: &Path,
    realm: &str,
    status: Option<alarms::Status>,
) -> Result<Value> {
    let conn = alarms::open(root)?;
    let list = alarms::list(&conn, realm, status)?;
    Ok(json!({ "alarms": serde_json::to_value(list)? }))
}

pub(crate) fn alarm_update(
    root: &Path,
    req: AlarmUpdateReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = alarms::open(root)?;
    // Multi-realm isolation: with a session realm, every transition loads the
    // alarm through the realm-scoped getter, so a cross-realm alarm id is
    // rejected as "does not exist" before any mutation. CLI passes None.
    let alarm = match (req.action.to_ascii_lowercase().as_str(), realm) {
        ("ack" | "acknowledge", Some(r)) => {
            alarms::update_status_in_realm(&conn, &req.alarm_id, r, alarms::Status::Acknowledged)?
        }
        ("progress", Some(r)) => {
            alarms::update_status_in_realm(&conn, &req.alarm_id, r, alarms::Status::InProgress)?
        }
        ("resolve", Some(r)) => {
            alarms::update_status_in_realm(&conn, &req.alarm_id, r, alarms::Status::Resolved)?
        }
        ("close", Some(r)) => {
            alarms::update_status_in_realm(&conn, &req.alarm_id, r, alarms::Status::Closed)?
        }
        ("assign", Some(r)) => {
            alarms::assign_in_realm(&conn, &req.alarm_id, r, req.assignee.clone())?
        }
        ("status", Some(r)) => {
            let next = parse_status(
                req.status
                    .as_deref()
                    .ok_or_else(|| anyhow!("status action requires `status`"))?,
            )?;
            alarms::update_status_in_realm(&conn, &req.alarm_id, r, next)?
        }
        // Trusted CLI (realm == None): unscoped lifecycle transitions.
        ("ack" | "acknowledge", None) => alarms::acknowledge(&conn, &req.alarm_id)?,
        ("progress", None) => alarms::start_progress(&conn, &req.alarm_id)?,
        ("resolve", None) => alarms::resolve(&conn, &req.alarm_id)?,
        ("close", None) => alarms::close(&conn, &req.alarm_id)?,
        ("assign", None) => alarms::assign(&conn, &req.alarm_id, req.assignee.clone())?,
        ("status", None) => {
            let next = parse_status(
                req.status
                    .as_deref()
                    .ok_or_else(|| anyhow!("status action requires `status`"))?,
            )?;
            alarms::update_status(&conn, &req.alarm_id, next)?
        }
        (other, _) => bail!("unknown alarm action: {other}"),
    };

    let updated_at = now_ms();
    let (rid, payload) = alarm_row(&alarm);
    project_record(&conn, COLLECTION_ALARMS, &rid, updated_at, payload, false)?;

    Ok(EngineOutcome {
        result: json!({ "alarm": serde_json::to_value(&alarm)? }),
        projections: vec![(COLLECTION_ALARMS, rid)],
    })
}

// ---- rulesets ----

pub(crate) fn ruleset_save(
    root: &Path,
    mut req: RulesetSaveReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    // Multi-realm isolation: the realm is NEVER trusted from the client payload.
    // With a session realm, force the ruleset's realm to it AND reject an UPDATE
    // of an existing ruleset owned by another realm (cross-realm mutation).
    if let Some(r) = realm {
        req.realm = r.to_string();
        if let Some(id) = req.id.as_deref() {
            let existing_realm: Option<String> = conn
                .query_row(
                    "SELECT realm FROM iot_rulesets WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to realm-scope ruleset")?;
            if let Some(er) = existing_realm {
                anyhow::ensure!(er == r, "ruleset not found: {id}");
            }
        }
    }
    let id = req.id.clone().unwrap_or_else(Asset::generate_id);
    let now = now_iso();
    let data = serde_json::to_string(&req.data).context("failed to serialize ruleset data")?;
    conn.execute(
        "INSERT INTO iot_rulesets (id, realm, name, enabled, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
            realm = excluded.realm,
            name = excluded.name,
            enabled = excluded.enabled,
            data = excluded.data,
            updated_at = excluded.updated_at",
        params![
            id,
            req.realm,
            req.name,
            if req.enabled { 1 } else { 0 },
            data,
            now
        ],
    )
    .context("failed to upsert ruleset")?;

    let projection = project_ruleset(&conn, &id)?;
    Ok(EngineOutcome {
        result: json!({ "ruleset": { "id": id, "realm": req.realm, "name": req.name, "enabled": req.enabled } }),
        projections: vec![projection],
    })
}

pub(crate) fn ruleset_toggle(
    root: &Path,
    ruleset_id: &str,
    enabled: bool,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    // Multi-realm isolation: the UPDATE is realm-scoped when a session realm is
    // supplied, so a cross-realm ruleset id cannot be toggled (changed == 0 →
    // "not found"). CLI passes None for an unscoped toggle.
    let changed = match realm {
        Some(r) => conn
            .execute(
                "UPDATE iot_rulesets SET enabled = ?2, updated_at = ?3 WHERE id = ?1 AND realm = ?4",
                params![ruleset_id, if enabled { 1 } else { 0 }, now_iso(), r],
            )
            .context("failed to toggle ruleset")?,
        None => conn
            .execute(
                "UPDATE iot_rulesets SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
                params![ruleset_id, if enabled { 1 } else { 0 }, now_iso()],
            )
            .context("failed to toggle ruleset")?,
    };
    if changed == 0 {
        bail!("ruleset not found: {ruleset_id}");
    }
    let projection = project_ruleset(&conn, ruleset_id)?;
    Ok(EngineOutcome {
        result: json!({ "ruleset": { "id": ruleset_id, "enabled": enabled } }),
        projections: vec![projection],
    })
}

pub(crate) fn ruleset_list(root: &Path, realm: &str) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, realm, name, enabled, data FROM iot_rulesets
             WHERE realm = ?1 ORDER BY name ASC",
        )
        .context("failed to prepare ruleset list")?;
    let rows = stmt
        .query_map(params![realm], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "realm": r.get::<_, String>(1)?,
                "name": r.get::<_, String>(2)?,
                "enabled": r.get::<_, i64>(3)? != 0,
                "data": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
            }))
        })
        .context("failed to query rulesets")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("failed to read ruleset row")?);
    }
    Ok(json!({ "rulesets": out }))
}

fn project_ruleset(conn: &Connection, ruleset_id: &str) -> Result<(&'static str, String)> {
    let row: Option<(String, String, i64, String)> = conn
        .query_row(
            "SELECT realm, name, enabled, data FROM iot_rulesets WHERE id = ?1",
            params![ruleset_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .context("failed to read ruleset for projection")?;
    let (realm, name, enabled, data) =
        row.ok_or_else(|| anyhow!("ruleset vanished: {ruleset_id}"))?;
    let enabled = enabled != 0;
    let payload = json!({
        "realm": realm,
        "name": name,
        "enabled": enabled,
        "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null),
        "index_text": name.to_lowercase(),
        "sort_key": name,
        "status_key": if enabled { "enabled" } else { "disabled" },
        "created_at_ms": now_ms(),
    });
    project_record(
        conn,
        COLLECTION_RULESETS,
        ruleset_id,
        now_ms(),
        payload,
        false,
    )?;
    Ok((COLLECTION_RULESETS, ruleset_id.to_string()))
}

// ---- dashboards & widgets (RFC 0011: CTOX-programmierte Automation-Widgets) ----

fn project_dashboard(conn: &Connection, id: &str) -> Result<(&'static str, String)> {
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>, Option<i64>)> =
        conn.query_row(
            "SELECT realm, name, scope, scope_ref, view_mode, sort_index FROM iot_dashboards WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()
        .context("failed to read dashboard for projection")?;
    let (realm, name, scope, scope_ref, view_mode, sort_index) =
        row.ok_or_else(|| anyhow!("dashboard vanished: {id}"))?;
    let payload = json!({
        "realm": realm, "name": name, "scope": scope, "scope_ref": scope_ref,
        "view_mode": view_mode, "sort_index": sort_index,
        "index_text": name.to_lowercase(), "sort_key": name, "created_at_ms": now_ms(),
    });
    project_record(conn, "iot_dashboards", id, now_ms(), payload, false)?;
    Ok(("iot_dashboards", id.to_string()))
}

pub(crate) fn dashboard_upsert(
    root: &Path,
    mut req: DashboardUpsertReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    if let Some(r) = realm {
        req.realm = r.to_string();
        if let Some(id) = req.id.as_deref() {
            let er: Option<String> = conn
                .query_row(
                    "SELECT realm FROM iot_dashboards WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to realm-scope dashboard")?;
            if let Some(er) = er {
                anyhow::ensure!(er == r, "dashboard not found: {id}");
            }
        }
    }
    let id = req.id.clone().unwrap_or_else(Asset::generate_id);
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_dashboards (id, realm, name, scope, scope_ref, view_mode, sort_index, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(id) DO UPDATE SET
            realm = excluded.realm, name = excluded.name, scope = excluded.scope,
            scope_ref = excluded.scope_ref, view_mode = excluded.view_mode,
            sort_index = excluded.sort_index, updated_at = excluded.updated_at",
        params![id, req.realm, req.name, req.scope, req.scope_ref, req.view_mode, req.sort_index, now],
    )
    .context("failed to upsert dashboard")?;
    let projection = project_dashboard(&conn, &id)?;
    Ok(EngineOutcome {
        result: json!({ "dashboard": { "id": id, "realm": req.realm, "name": req.name } }),
        projections: vec![projection],
    })
}

pub(crate) fn dashboard_delete(
    root: &Path,
    id: &str,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    if let Some(r) = realm {
        let er: Option<String> = conn
            .query_row(
                "SELECT realm FROM iot_dashboards WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .context("failed to realm-scope dashboard")?;
        if let Some(er) = er {
            anyhow::ensure!(er == r, "dashboard not found: {id}");
        }
    }
    conn.execute(
        "DELETE FROM iot_widgets WHERE dashboard_id = ?1",
        params![id],
    )
    .context("failed to delete dashboard widgets")?;
    conn.execute("DELETE FROM iot_dashboards WHERE id = ?1", params![id])
        .context("failed to delete dashboard")?;
    project_record(&conn, "iot_dashboards", id, now_ms(), json!({}), true)?;
    Ok(EngineOutcome {
        result: json!({ "deleted": id }),
        projections: vec![("iot_dashboards", id.to_string())],
    })
}

pub(crate) fn dashboard_list(root: &Path, realm: &str) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, name, scope, scope_ref, view_mode, sort_index FROM iot_dashboards WHERE realm = ?1 ORDER BY sort_index, name",
    )?;
    let rows = stmt
        .query_map(params![realm], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "realm": realm,
                "name": r.get::<_, String>(1)?,
                "scope": r.get::<_, Option<String>>(2)?,
                "scope_ref": r.get::<_, Option<String>>(3)?,
                "view_mode": r.get::<_, Option<String>>(4)?,
                "sort_index": r.get::<_, Option<i64>>(5)?,
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({ "dashboards": rows }))
}

pub(crate) fn project_widget(conn: &Connection, id: &str) -> Result<(&'static str, String)> {
    let row: Option<(
        String, String, String, Option<String>, Option<String>, Option<String>,
        Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>,
    )> = conn
        .query_row(
            "SELECT dashboard_id, realm, signal_ref, cond_text, action_prompt, trigger_status, trigger_code, render_code, x, y, w, h, sort_index FROM iot_widgets WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?)),
        )
        .optional()
        .context("failed to read widget for projection")?;
    let (
        dashboard_id,
        realm,
        signal_ref,
        cond_text,
        action_prompt,
        trigger_status,
        trigger_code,
        render_code,
        x,
        y,
        w,
        h,
        sort_index,
    ) = row.ok_or_else(|| anyhow!("widget vanished: {id}"))?;
    let status = trigger_status.clone().unwrap_or_else(|| "idle".to_string());
    let payload = json!({
        "dashboard_id": dashboard_id, "realm": realm, "signal_ref": signal_ref,
        "cond_text": cond_text, "action_prompt": action_prompt,
        "trigger_status": trigger_status, "trigger_code": trigger_code, "render_code": render_code,
        "x": x, "y": y, "w": w, "h": h, "sort_index": sort_index,
        "index_text": cond_text.clone().unwrap_or_default().to_lowercase(),
        "sort_key": format!("{:08}", sort_index.unwrap_or(0)),
        "status_key": status, "created_at_ms": now_ms(),
    });
    project_record(conn, "iot_widgets", id, now_ms(), payload, false)?;
    Ok(("iot_widgets", id.to_string()))
}

pub(crate) fn widget_upsert(
    root: &Path,
    mut req: WidgetUpsertReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    if let Some(r) = realm {
        req.realm = r.to_string();
        if let Some(id) = req.id.as_deref() {
            let er: Option<String> = conn
                .query_row(
                    "SELECT realm FROM iot_widgets WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to realm-scope widget")?;
            if let Some(er) = er {
                anyhow::ensure!(er == r, "widget not found: {id}");
            }
        }
    }
    let id = req.id.clone().unwrap_or_else(Asset::generate_id);
    let now = now_iso();
    conn.execute(
        "INSERT INTO iot_widgets (id, dashboard_id, realm, signal_ref, cond_text, action_prompt, trigger_code, render_code, x, y, w, h, sort_index, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
         ON CONFLICT(id) DO UPDATE SET
            dashboard_id = excluded.dashboard_id, realm = excluded.realm, signal_ref = excluded.signal_ref,
            cond_text = excluded.cond_text, action_prompt = excluded.action_prompt,
            trigger_code = excluded.trigger_code, render_code = excluded.render_code,
            x = excluded.x, y = excluded.y, w = excluded.w, h = excluded.h,
            sort_index = excluded.sort_index, updated_at = excluded.updated_at",
        params![id, req.dashboard_id, req.realm, req.signal_ref, req.cond_text, req.action_prompt, req.trigger_code, req.render_code, req.x, req.y, req.w, req.h, req.sort_index, now],
    )
    .context("failed to upsert widget")?;
    // If a watcher program was provided (typically written back by the codegen
    // agent), validate it up front and reflect the result in trigger_status. This
    // is the self-repair gate: invalid generated code lands as "needs_attention"
    // so CTOX regenerates it; a runnable program is "armed".
    if let Some(code) = req.trigger_code.as_deref() {
        if !code.trim().is_empty() {
            let status = match crate::iot::watcher::validate_program(code) {
                None => "armed",
                Some(_) => "needs_attention",
            };
            conn.execute(
                "UPDATE iot_widgets SET trigger_status = ?2 WHERE id = ?1",
                params![id, status],
            )
            .context("failed to set widget trigger_status")?;
        }
    }
    let projection = project_widget(&conn, &id)?;
    Ok(EngineOutcome {
        result: json!({ "widget": { "id": id, "dashboard_id": req.dashboard_id, "signal_ref": req.signal_ref } }),
        projections: vec![projection],
    })
}

pub(crate) fn widget_delete(root: &Path, id: &str, realm: Option<&str>) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    if let Some(r) = realm {
        let er: Option<String> = conn
            .query_row(
                "SELECT realm FROM iot_widgets WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .context("failed to realm-scope widget")?;
        if let Some(er) = er {
            anyhow::ensure!(er == r, "widget not found: {id}");
        }
    }
    conn.execute("DELETE FROM iot_widgets WHERE id = ?1", params![id])
        .context("failed to delete widget")?;
    project_record(&conn, "iot_widgets", id, now_ms(), json!({}), true)?;
    Ok(EngineOutcome {
        result: json!({ "deleted": id }),
        projections: vec![("iot_widgets", id.to_string())],
    })
}

pub(crate) fn widget_arrange(
    root: &Path,
    id: &str,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    if let Some(r) = realm {
        let er: Option<String> = conn
            .query_row(
                "SELECT realm FROM iot_widgets WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .context("failed to realm-scope widget")?;
        if let Some(er) = er {
            anyhow::ensure!(er == r, "widget not found: {id}");
        }
    }
    conn.execute(
        "UPDATE iot_widgets SET x = ?2, y = ?3, w = ?4, h = ?5, updated_at = ?6 WHERE id = ?1",
        params![id, x, y, w, h, now_iso()],
    )
    .context("failed to arrange widget")?;
    let projection = project_widget(&conn, id)?;
    Ok(EngineOutcome {
        result: json!({ "widget": { "id": id, "x": x, "y": y, "w": w, "h": h } }),
        projections: vec![projection],
    })
}

/// Pause/resume a widget's watcher. Paused → `tick_widget` skips it. Resuming
/// recomputes the status from the (re-validated) program so a previously broken
/// watcher does not silently come back "armed".
pub(crate) fn widget_set_pause(
    root: &Path,
    widget_id: &str,
    paused: bool,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT realm, trigger_code FROM iot_widgets WHERE id = ?1",
            params![widget_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .context("failed to load widget for pause")?;
    let (wrealm, trigger_code) = row.ok_or_else(|| anyhow!("widget not found: {widget_id}"))?;
    if let Some(r) = realm {
        anyhow::ensure!(wrealm == r, "widget not found: {widget_id}");
    }
    let status = if paused {
        "paused".to_string()
    } else {
        match trigger_code.as_deref().filter(|c| !c.trim().is_empty()) {
            None => "idle".to_string(),
            Some(c) => {
                if crate::iot::watcher::validate_program(c).is_none() {
                    "armed".to_string()
                } else {
                    "needs_attention".to_string()
                }
            }
        }
    };
    conn.execute(
        "UPDATE iot_widgets SET trigger_status = ?2, updated_at = ?3 WHERE id = ?1",
        params![widget_id, status, now_iso()],
    )
    .context("failed to set widget pause state")?;
    let projection = project_widget(&conn, widget_id)?;
    Ok(EngineOutcome {
        result: json!({ "widget": { "id": widget_id, "trigger_status": status } }),
        projections: vec![projection],
    })
}

pub(crate) fn widget_list(root: &Path, dashboard_id: &str) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, realm, signal_ref, cond_text, action_prompt, trigger_status, x, y, w, h, sort_index FROM iot_widgets WHERE dashboard_id = ?1 ORDER BY sort_index",
    )?;
    let rows = stmt
        .query_map(params![dashboard_id], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "dashboard_id": dashboard_id,
                "realm": r.get::<_, String>(1)?,
                "signal_ref": r.get::<_, String>(2)?,
                "cond_text": r.get::<_, Option<String>>(3)?,
                "action_prompt": r.get::<_, Option<String>>(4)?,
                "trigger_status": r.get::<_, Option<String>>(5)?,
                "x": r.get::<_, Option<i64>>(6)?,
                "y": r.get::<_, Option<i64>>(7)?,
                "w": r.get::<_, Option<i64>>(8)?,
                "h": r.get::<_, Option<i64>>(9)?,
                "sort_index": r.get::<_, Option<i64>>(10)?,
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({ "widgets": rows }))
}

// ---- widget codegen (RFC 0011: CTOX programs the watcher + render) ----
//
// The human writes prompts (Wenn/Dann); CTOX writes the code. Generation is an
// AGENT-TURN — never a heuristic template here. These commands enqueue a DURABLE
// codegen task (mission::channels::create_queue_task) that a model-capable agent
// leases and completes by writing the code back via `ctox iot widget upsert
// --trigger-code/--render-code`. No synchronous model call lives in the command
// path, so the request never blocks and survives a missing/slow model.

struct WidgetCodegenCtx {
    realm: String,
    dashboard_id: String,
    signal_ref: String,
    cond_text: Option<String>,
    action_prompt: Option<String>,
}

fn load_widget_codegen_ctx(
    conn: &Connection,
    widget_id: &str,
    realm: Option<&str>,
) -> Result<WidgetCodegenCtx> {
    let row = conn
        .query_row(
            "SELECT realm, dashboard_id, signal_ref, cond_text, action_prompt FROM iot_widgets WHERE id = ?1",
            params![widget_id],
            |r| {
                Ok(WidgetCodegenCtx {
                    realm: r.get(0)?,
                    dashboard_id: r.get(1)?,
                    signal_ref: r.get(2)?,
                    cond_text: r.get(3)?,
                    action_prompt: r.get(4)?,
                })
            },
        )
        .optional()
        .context("failed to load widget for codegen")?;
    let ctx = row.ok_or_else(|| anyhow!("widget not found: {widget_id}"))?;
    if let Some(r) = realm {
        anyhow::ensure!(ctx.realm == r, "widget not found: {widget_id}");
    }
    Ok(ctx)
}

fn enqueue_codegen(
    root: &Path,
    widget_id: &str,
    signal_ref: &str,
    title: String,
    prompt: String,
    kind: &str,
    priority: &str,
) -> Result<EngineOutcome> {
    let task = crate::mission::channels::create_queue_task(
        root,
        crate::mission::channels::QueueTaskCreateRequest {
            title,
            prompt,
            thread_key: format!("iot-codegen:{widget_id}"),
            workspace_root: None,
            priority: priority.to_string(),
            suggested_skill: Some("iot-operations".to_string()),
            parent_message_key: None,
            extra_metadata: Some(
                json!({ "kind": kind, "widget_id": widget_id, "signal_ref": signal_ref }),
            ),
        },
    )?;
    Ok(EngineOutcome {
        result: json!({ "queued": task.message_key, "widget_id": widget_id, "kind": kind }),
        projections: vec![],
    })
}

/// `compile_trigger` — ask CTOX to write the Rhai watcher from the widget's
/// free-text "Wenn". Enqueues a durable codegen task (agent-turn at lease time).
pub(crate) fn compile_trigger(
    root: &Path,
    widget_id: &str,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let ctx = load_widget_codegen_ctx(&conn, widget_id, realm)?;
    let cond = ctx.cond_text.clone().unwrap_or_default();
    anyhow::ensure!(
        !cond.trim().is_empty(),
        "widget {widget_id} has no condition (Wenn) to compile"
    );
    let prompt = build_trigger_codegen_prompt(widget_id, &ctx, &cond);
    enqueue_codegen(
        root,
        widget_id,
        &ctx.signal_ref,
        format!("IoT-Wächter programmieren: {}", ctx.signal_ref),
        prompt,
        "iot_trigger_code",
        "high",
    )
}

/// `generate_render` — ask CTOX to write the (subordinate) widget visualization.
pub(crate) fn generate_render(
    root: &Path,
    widget_id: &str,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let ctx = load_widget_codegen_ctx(&conn, widget_id, realm)?;
    let prompt = build_render_codegen_prompt(widget_id, &ctx);
    enqueue_codegen(
        root,
        widget_id,
        &ctx.signal_ref,
        format!("IoT-Widget-Code generieren: {}", ctx.signal_ref),
        prompt,
        "iot_render_code",
        "normal",
    )
}

fn build_trigger_codegen_prompt(widget_id: &str, ctx: &WidgetCodegenCtx, cond: &str) -> String {
    let action = ctx.action_prompt.as_deref().unwrap_or("(noch keine)");
    format!(
        "Du bist CTOX und programmierst die Trigger-Logik (den Wächter) eines IoT-Automatisierungs-Widgets.\n\n\
         Widget-ID: {widget_id}\n\
         Signal: {signal} (Form <asset_id>::<attribute_name>)\n\
         Bedingung (Wenn, Freitext): \"{cond}\"\n\
         Geplante Aktion (Dann): \"{action}\"\n\n\
         Schreibe ein kleines Rhai-Programm, das pro neuem Datenpunkt STATEFUL laeuft und `fire(grund)` aufruft, \
         sobald die Bedingung zutrifft. Nur-lesende Signal-API:\n\
         - signal.last() -> Zahl; signal.has_data() -> bool; signal.age_ms() -> Zahl\n\
         - signal.window(\"15m\") -> [Zahl]; signal.avg/min/max/count(\"15m\"); signal.rate(\"15m\") (pro Sekunde)\n\
         - signals(\"name\") -> weiteres gebundenes Signal\n\
         - state: persistente Map zwischen Aufrufen (\"seit X\", Hysterese, Zaehler), z.B. state.streak = (state.streak ?? 0) + 1\n\
         - fire(grund): meldet, dass die Bedingung haelt\n\
         Zeitfenster: ms/s/m/h/d. KEIN Datei-/Netz-Zugriff, kein eval; harte Op-/Zeitlimits.\n\n\
         Schreibe den Waechter zurueck (validiere vorher, dass er kompiliert):\n\
         ctox iot widget upsert --id {widget_id} --dashboard {dash} --realm {realm} --signal {signal} --trigger-code '<RHAI>'\n\
         Gib sonst nichts aus.",
        widget_id = widget_id,
        signal = ctx.signal_ref,
        cond = cond,
        action = action,
        dash = ctx.dashboard_id,
        realm = ctx.realm,
    )
}

fn build_render_codegen_prompt(widget_id: &str, ctx: &WidgetCodegenCtx) -> String {
    format!(
        "Du bist CTOX und programmierst den Widget-Code (die Visualisierung) eines IoT-Automatisierungs-Widgets. \
         Die Visualisierung ist dem Auftrag UNTERGEORDNET - schlicht, kein Grafana.\n\n\
         Widget-ID: {widget_id}\n\
         Signal: {signal}\n\
         Bedingung (Wenn): \"{cond}\"\n\n\
         Schreibe den Rumpf einer JS-Funktion `render(host, api)`, die in das eigene Kachel-Element `host` rendert. \
         Gesandboxte API (NUR diese): api.signal.last()/.window(\"15m\")/.rate(\"15m\"); api.draw.line/value/gauge/grid; api.fmt. \
         KEIN Zugriff auf window/document/parent/fetch/eval/import. Halte es minimal (Wert + Sparkline genuegt meist).\n\n\
         Schreibe den Code zurueck:\n\
         ctox iot widget upsert --id {widget_id} --dashboard {dash} --realm {realm} --signal {signal} --render-code '<JS>'\n\
         Gib sonst nichts aus.",
        widget_id = widget_id,
        signal = ctx.signal_ref,
        cond = ctx.cond_text.as_deref().unwrap_or(""),
        dash = ctx.dashboard_id,
        realm = ctx.realm,
    )
}

// ---- agents ----

pub(crate) fn agent_configure(
    root: &Path,
    mut req: AgentConfigureReq,
    realm: Option<&str>,
) -> Result<EngineOutcome> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    // Multi-realm isolation: the realm is NEVER trusted from the client payload.
    // With a session realm, force the agent's realm to it AND reject an UPDATE of
    // an existing agent owned by another realm (cross-realm mutation).
    if let Some(r) = realm {
        req.realm = r.to_string();
        if let Some(id) = req.id.as_deref() {
            let existing_realm: Option<String> = conn
                .query_row(
                    "SELECT realm FROM iot_agents WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .context("failed to realm-scope agent")?;
            if let Some(er) = existing_realm {
                anyhow::ensure!(er == r, "agent not found: {id}");
            }
        }
    }
    let id = req.id.clone().unwrap_or_else(Asset::generate_id);
    let now = now_iso();
    let data = serde_json::to_string(&req.data).context("failed to serialize agent data")?;
    conn.execute(
        "INSERT INTO iot_agents (id, realm, name, kind, enabled, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(id) DO UPDATE SET
            realm = excluded.realm,
            name = excluded.name,
            kind = excluded.kind,
            enabled = excluded.enabled,
            data = excluded.data,
            updated_at = excluded.updated_at",
        params![
            id,
            req.realm,
            req.name,
            req.kind,
            if req.enabled { 1 } else { 0 },
            data,
            now
        ],
    )
    .context("failed to upsert agent")?;

    // A newly configured agent starts as unconfigured until the native peer
    // supervisor observes links and publishes live status.
    conn.execute(
        "INSERT INTO iot_agent_status (id, agent_id, realm, link_state, data, created_at, updated_at)
         VALUES (?1, ?1, ?2, 'unconfigured', '{}', ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            realm = excluded.realm,
            updated_at = excluded.updated_at",
        params![id, req.realm, now],
    )
    .context("failed to upsert agent status")?;

    let mut projections = vec![project_agent(&conn, &id)?];
    projections.push(project_agent_status(&conn, &id)?);
    Ok(EngineOutcome {
        result: json!({ "agent": { "id": id, "realm": req.realm, "name": req.name, "kind": req.kind, "enabled": req.enabled } }),
        projections,
    })
}

pub(crate) fn agent_list(root: &Path, realm: &str) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, realm, name, kind, enabled, data FROM iot_agents
             WHERE realm = ?1 ORDER BY name ASC",
        )
        .context("failed to prepare agent list")?;
    let rows = stmt
        .query_map(params![realm], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "realm": r.get::<_, String>(1)?,
                "name": r.get::<_, String>(2)?,
                "kind": r.get::<_, String>(3)?,
                "enabled": r.get::<_, i64>(4)? != 0,
                "data": serde_json::from_str::<Value>(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
            }))
        })
        .context("failed to query agents")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("failed to read agent row")?);
    }
    Ok(json!({ "agents": out }))
}

pub(crate) fn agent_status(root: &Path, agent_id: &str) -> Result<Value> {
    let conn = store::open_iot_store(root)?;
    ensure_stub_schema(&conn)?;
    let row: Option<(String, String, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT realm, link_state, last_event_ms, error FROM iot_agent_status WHERE agent_id = ?1",
            params![agent_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .context("failed to query agent status")?;
    match row {
        Some((realm, link_state, last_event_ms, error)) => Ok(json!({
            "agent_status": {
                "agent_id": agent_id,
                "realm": realm,
                "link_state": link_state,
                "last_event_ms": last_event_ms,
                "error": error,
            }
        })),
        // No status row yet -> default link state.
        None => Ok(json!({
            "agent_status": { "agent_id": agent_id, "link_state": "unconfigured" }
        })),
    }
}

fn project_agent(conn: &Connection, agent_id: &str) -> Result<(&'static str, String)> {
    let row: Option<(String, String, String, i64, String)> = conn
        .query_row(
            "SELECT realm, name, kind, enabled, data FROM iot_agents WHERE id = ?1",
            params![agent_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .context("failed to read agent for projection")?;
    let (realm, name, kind, enabled, data) =
        row.ok_or_else(|| anyhow!("agent vanished: {agent_id}"))?;
    let enabled = enabled != 0;
    let payload = json!({
        "realm": realm,
        "name": name,
        "kind": kind,
        "enabled": enabled,
        "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null),
        "index_text": name.to_lowercase(),
        "sort_key": name,
        "status_key": kind,
        "created_at_ms": now_ms(),
    });
    project_record(conn, COLLECTION_AGENTS, agent_id, now_ms(), payload, false)?;
    Ok((COLLECTION_AGENTS, agent_id.to_string()))
}

fn project_agent_status(conn: &Connection, agent_id: &str) -> Result<(&'static str, String)> {
    let row: Option<(String, String, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT realm, link_state, last_event_ms, error FROM iot_agent_status WHERE agent_id = ?1",
            params![agent_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .context("failed to read agent status for projection")?;
    let (realm, link_state, last_event_ms, error) =
        row.ok_or_else(|| anyhow!("agent status vanished: {agent_id}"))?;
    let payload = json!({
        "realm": realm,
        "agent_id": agent_id,
        "link_state": link_state,
        "last_event_ms": last_event_ms,
        "error": error,
        "data": {},
        "status_key": link_state,
        "created_at_ms": now_ms(),
    });
    project_record(
        conn,
        COLLECTION_AGENT_STATUS,
        agent_id,
        now_ms(),
        payload,
        false,
    )?;
    Ok((COLLECTION_AGENT_STATUS, agent_id.to_string()))
}

// ===========================================================================
// SURFACE 1 — the `ctox iot …` CLI dispatcher.
// ===========================================================================

/// Parse the `ctox iot …` argv and invoke the matching shared op, printing
/// pretty JSON to stdout. Two-level dispatch on `args[0]` (the noun) then
/// `args[1]` (the verb). Flag parsing helpers are module-local by the dispatch
/// convention (NOT imported from main.rs).
pub fn handle_iot_command(root: &Path, args: &[String]) -> Result<()> {
    let noun = args.first().map(|s| s.as_str()).unwrap_or("");
    let verb = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let rest = if args.len() > 2 { &args[2..] } else { &[] };

    match (noun, verb) {
        // ---- assets ----
        ("asset", "list") => {
            let realm = required_flag_value(rest, "--realm")?;
            let parent = find_flag_value(rest, "--parent");
            print_json(&asset_list(root, &realm, parent.as_deref())?)
        }
        ("asset", "show") => {
            let id = required_flag_value(rest, "--id")?;
            // CLI is the trusted operator surface: realm = None (unscoped).
            print_json(&asset_show(root, &id, None)?)
        }
        ("asset", "upsert") => {
            let req = AssetUpsertReq {
                id: find_flag_value(rest, "--id"),
                realm: required_flag_value(rest, "--realm")?,
                asset_type: required_flag_value(rest, "--type")?,
                name: required_flag_value(rest, "--name")?,
                parent_id: find_flag_value(rest, "--parent"),
                asset_type_info: match find_flag_value(rest, "--type-info") {
                    Some(s) => {
                        Some(serde_json::from_str(&s).context("failed to parse --type-info JSON")?)
                    }
                    None => None,
                },
            };
            print_json(&asset_upsert(root, req, None)?.into_value())
        }
        ("asset", "delete") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&asset_delete(root, &id, None)?.into_value())
        }

        // ---- attributes ----
        ("attribute", "read") => {
            let asset = required_flag_value(rest, "--asset")?;
            let name = required_flag_value(rest, "--name")?;
            print_json(&attribute_read(root, &asset, &name, None)?)
        }
        ("attribute", "write") => {
            let raw = required_flag_value(rest, "--value")?;
            // Try JSON first; fall back to a plain string scalar.
            let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            let req = AttributeWriteReq {
                asset_id: required_flag_value(rest, "--asset")?,
                name: required_flag_value(rest, "--name")?,
                value,
                timestamp_ms: find_flag_value(rest, "--ts")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --ts")?
                    .unwrap_or(0),
            };
            print_json(&attribute_write(root, req, None)?.into_value())
        }

        // ---- datapoints ----
        ("datapoints", "query") => {
            let req = DatapointsQueryReq {
                asset_id: required_flag_value(rest, "--asset")?,
                attribute_name: required_flag_value(rest, "--name")?,
                from_ms: required_flag_value(rest, "--from")?
                    .parse()
                    .context("failed to parse --from")?,
                to_ms: required_flag_value(rest, "--to")?
                    .parse()
                    .context("failed to parse --to")?,
                shape: find_flag_value(rest, "--shape").unwrap_or_else(|| "all".to_string()),
                interval_ms: find_flag_value(rest, "--interval")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --interval")?,
                threshold: find_flag_value(rest, "--threshold")
                    .map(|s| s.parse::<usize>())
                    .transpose()
                    .context("failed to parse --threshold")?,
            };
            print_json(&datapoints_query(root, req, None)?.into_value())
        }

        // ---- alarms ----
        ("alarm", "list") => {
            let realm = required_flag_value(rest, "--realm")?;
            let status = match find_flag_value(rest, "--status") {
                Some(s) => Some(parse_status(&s)?),
                None => None,
            };
            print_json(&alarm_list(root, &realm, status)?)
        }
        ("alarm", "ack") => {
            let req = AlarmUpdateReq {
                alarm_id: required_flag_value(rest, "--id")?,
                action: "ack".to_string(),
                assignee: None,
                status: None,
            };
            print_json(&alarm_update(root, req, None)?.into_value())
        }
        ("alarm", "assign") => {
            let req = AlarmUpdateReq {
                alarm_id: required_flag_value(rest, "--id")?,
                action: "assign".to_string(),
                assignee: find_flag_value(rest, "--assignee"),
                status: None,
            };
            print_json(&alarm_update(root, req, None)?.into_value())
        }
        ("alarm", "resolve") => {
            let req = AlarmUpdateReq {
                alarm_id: required_flag_value(rest, "--id")?,
                action: "resolve".to_string(),
                assignee: None,
                status: None,
            };
            print_json(&alarm_update(root, req, None)?.into_value())
        }
        ("alarm", "close") => {
            let req = AlarmUpdateReq {
                alarm_id: required_flag_value(rest, "--id")?,
                action: "close".to_string(),
                assignee: None,
                status: None,
            };
            print_json(&alarm_update(root, req, None)?.into_value())
        }

        // ---- rules (stubs) ----
        ("rules", "list") => {
            let realm = required_flag_value(rest, "--realm")?;
            print_json(&ruleset_list(root, &realm)?)
        }
        ("rules", "save") => {
            let req = RulesetSaveReq {
                id: find_flag_value(rest, "--id"),
                realm: required_flag_value(rest, "--realm")?,
                name: required_flag_value(rest, "--name")?,
                enabled: find_flag_value(rest, "--enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true),
                data: match find_flag_value(rest, "--data") {
                    Some(s) => serde_json::from_str(&s).context("failed to parse --data JSON")?,
                    None => Value::Null,
                },
            };
            print_json(&ruleset_save(root, req, None)?.into_value())
        }
        ("rules", "toggle") => {
            let id = required_flag_value(rest, "--id")?;
            let enabled = required_flag_value(rest, "--enabled")?;
            let enabled = enabled != "false" && enabled != "0";
            print_json(&ruleset_toggle(root, &id, enabled, None)?.into_value())
        }

        // ---- agents (stubs) ----
        ("agent", "list") => {
            let realm = required_flag_value(rest, "--realm")?;
            print_json(&agent_list(root, &realm)?)
        }
        ("agent", "configure") => {
            let req = AgentConfigureReq {
                id: find_flag_value(rest, "--id"),
                realm: required_flag_value(rest, "--realm")?,
                name: required_flag_value(rest, "--name")?,
                kind: required_flag_value(rest, "--kind")?,
                enabled: find_flag_value(rest, "--enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true),
                data: match find_flag_value(rest, "--data") {
                    Some(s) => serde_json::from_str(&s).context("failed to parse --data JSON")?,
                    None => Value::Null,
                },
            };
            print_json(&agent_configure(root, req, None)?.into_value())
        }
        ("agent", "status") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&agent_status(root, &id)?)
        }

        // ---- dashboards (RFC 0011) ----
        ("dashboard", "list") => {
            let realm = required_flag_value(rest, "--realm")?;
            print_json(&dashboard_list(root, &realm)?)
        }
        ("dashboard", "upsert") => {
            let req = DashboardUpsertReq {
                id: find_flag_value(rest, "--id"),
                realm: required_flag_value(rest, "--realm")?,
                name: required_flag_value(rest, "--name")?,
                scope: find_flag_value(rest, "--scope"),
                scope_ref: find_flag_value(rest, "--scope-ref"),
                view_mode: find_flag_value(rest, "--view-mode"),
                sort_index: find_flag_value(rest, "--sort")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --sort")?,
            };
            print_json(&dashboard_upsert(root, req, None)?.into_value())
        }
        ("dashboard", "delete") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&dashboard_delete(root, &id, None)?.into_value())
        }

        // ---- widgets (RFC 0011: ein Widget = eine Automatisierung) ----
        ("widget", "list") => {
            let dashboard = required_flag_value(rest, "--dashboard")?;
            print_json(&widget_list(root, &dashboard)?)
        }
        ("widget", "upsert") => {
            let req = WidgetUpsertReq {
                id: find_flag_value(rest, "--id"),
                dashboard_id: required_flag_value(rest, "--dashboard")?,
                realm: required_flag_value(rest, "--realm")?,
                signal_ref: required_flag_value(rest, "--signal")?,
                cond_text: find_flag_value(rest, "--when"),
                action_prompt: find_flag_value(rest, "--then"),
                trigger_code: find_flag_value(rest, "--trigger-code"),
                render_code: find_flag_value(rest, "--render-code"),
                x: find_flag_value(rest, "--x")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --x")?,
                y: find_flag_value(rest, "--y")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --y")?,
                w: find_flag_value(rest, "--w")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --w")?,
                h: find_flag_value(rest, "--h")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --h")?,
                sort_index: find_flag_value(rest, "--sort")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .context("failed to parse --sort")?,
            };
            print_json(&widget_upsert(root, req, None)?.into_value())
        }
        ("widget", "delete") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&widget_delete(root, &id, None)?.into_value())
        }
        ("widget", "arrange") => {
            let id = required_flag_value(rest, "--id")?;
            let x = required_flag_value(rest, "--x")?
                .parse::<i64>()
                .context("failed to parse --x")?;
            let y = required_flag_value(rest, "--y")?
                .parse::<i64>()
                .context("failed to parse --y")?;
            let w = required_flag_value(rest, "--w")?
                .parse::<i64>()
                .context("failed to parse --w")?;
            let h = required_flag_value(rest, "--h")?
                .parse::<i64>()
                .context("failed to parse --h")?;
            print_json(&widget_arrange(root, &id, x, y, w, h, None)?.into_value())
        }
        ("widget", "compile-trigger") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&compile_trigger(root, &id, None)?.into_value())
        }
        ("widget", "generate-render") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&generate_render(root, &id, None)?.into_value())
        }
        ("widget", "pause") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&widget_set_pause(root, &id, true, None)?.into_value())
        }
        ("widget", "resume") => {
            let id = required_flag_value(rest, "--id")?;
            print_json(&widget_set_pause(root, &id, false, None)?.into_value())
        }

        // ---- webhooks (spec §5: rein & raus) ----
        ("webhook", "ingest") => {
            let signal = required_flag_value(rest, "--signal")?;
            let payload: Value = serde_json::from_str(&required_flag_value(rest, "--payload")?)
                .context("failed to parse --payload JSON")?;
            let path = find_flag_value(rest, "--path");
            let ts = find_flag_value(rest, "--ts")
                .map(|s| s.parse::<i64>())
                .transpose()
                .context("failed to parse --ts")?
                .unwrap_or(0);
            // CLI is the trusted operator surface (the HTTP front authenticates the
            // caller via the webhook secret before reaching here): realm = None.
            print_json(&crate::iot::webhook::ingest(
                root,
                &signal,
                &payload,
                path.as_deref(),
                ts,
                None,
            )?)
        }
        ("webhook", "send") => {
            let url = required_flag_value(rest, "--url")?;
            let payload: Value = serde_json::from_str(&required_flag_value(rest, "--payload")?)
                .context("failed to parse --payload JSON")?;
            let secret = find_flag_value(rest, "--secret-ref");
            // Repeatable `--header key=value`.
            let mut headers = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                if rest[i] == "--header" {
                    if let Some((k, v)) = rest.get(i + 1).and_then(|kv| kv.split_once('=')) {
                        headers.push((k.to_string(), v.to_string()));
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            print_json(&crate::iot::webhook::send(
                root,
                &url,
                &payload,
                secret.as_deref(),
                &headers,
            )?)
        }
        ("webhook", "register") => {
            let req = crate::iot::webhook::WebhookRegisterReq {
                id: find_flag_value(rest, "--id"),
                realm: required_flag_value(rest, "--realm")?,
                signal_ref: required_flag_value(rest, "--signal")?,
                value_path: find_flag_value(rest, "--path"),
            };
            print_json(&crate::iot::webhook::register(root, req)?)
        }

        // ---- resync ----
        ("project", "all") => {
            // Full idempotent resync: scan EVERY projectable engine row via
            // projector::project_all and upsert it into the RxDB-visible
            // business-os store so CLI mutations actually reach the apps over
            // RxDB/WebRTC. No HTTP bridge: engine -> projector -> business_records.
            // `None` realm: the CLI is a trusted operator surface (full host
            // access), so it resyncs every realm — same way the CLI bypasses the
            // session ACL gate. The session/executor projection path uses the
            // realm-scoped form (`project_all_iot(root, Some(realm))`).
            let pairs = crate::business_os::store::project_all_iot(root, None)
                .context("iot project all: full resync failed")?;
            let projected: Vec<Value> = pairs
                .into_iter()
                .map(|(collection, record_id)| json!({ "collection": collection, "id": record_id }))
                .collect();
            print_json(&json!({
                "status": "ok",
                "projected_count": projected.len(),
                "projected": projected,
            }))
        }

        _ => bail!(
            "unknown iot command: `{noun} {verb}` (expected \
             asset|attribute|datapoints|alarm|rules|agent <subcommand>)"
        ),
    }
}

// ---------------------------------------------------------------------------
// Module-local flag parsing + output (copied locally per the dispatch
// convention — NOT imported from main.rs).
// ---------------------------------------------------------------------------

/// Return the value following `flag` in `args` (`--flag value`), or None.
fn find_flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == flag {
            return it.next().cloned();
        }
        if let Some(rest) = a.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Like `find_flag_value` but errors if the flag is absent.
fn required_flag_value(args: &[String], flag: &str) -> Result<String> {
    find_flag_value(args, flag).ok_or_else(|| anyhow!("missing required flag {flag}"))
}

/// Print pretty JSON to stdout (the CLI surface's only output channel).
fn print_json(value: &Value) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("failed to serialize iot command output")?
    );
    Ok(())
}

// ===========================================================================
// SURFACE 2 — the RxDB business_commands executor entry.
// ===========================================================================

/// Executor entry for `ctox.iot.*` commands. Deserializes `payload` into the
/// matching request, applies the auth + realm-isolation gate against `session`,
/// calls the same shared op the CLI calls (with the SESSION realm, never the
/// payload realm), and returns the flattened outcome value (domain result +
/// `projections` array) that the rxdb_peer branch echoes.
///
/// Multi-realm isolation (Phase 2): every read/write/projection is scoped to
/// `session_realm(session)`. The payload's `realm` field is IGNORED for
/// authorization — it is overridden by the session realm on create and validated
/// against the resource's realm on update/read. Fine-grained per-realm ACL
/// (multiple realms per user, role scoping) is DEFERRED to the Phase 3 rules
/// engine; Phase 2 still hard-enforces basic resource.realm == session_realm
/// isolation so no command can read or mutate across realms.
pub(crate) fn handle_business_command(
    root: &Path,
    command_type: &str,
    payload: &Value,
    session: &crate::business_os::store::BusinessOsSession,
) -> Result<Value> {
    // Auth gate: an admin role, or an authenticated non-auth-required session.
    let is_admin = session.user.as_ref().map(|u| u.is_admin).unwrap_or(false);
    anyhow::ensure!(
        is_admin || (session.authenticated && !session.auth_required),
        "iot: forbidden"
    );

    // Realm the session is authorized for. Derived from the session ONLY — never
    // from the client payload. Every op below is enforced against this realm.
    let realm = session_realm(session);
    let realm = Some(realm.as_str());

    let outcome = match command_type {
        "ctox.iot.asset.upsert" => asset_upsert(root, parse_payload(payload)?, realm)?,
        "ctox.iot.asset.delete" => {
            let asset_id = payload
                .get("asset_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.asset.delete requires asset_id"))?;
            asset_delete(root, asset_id, realm)?
        }
        "ctox.iot.attribute.write" => attribute_write(root, parse_payload(payload)?, realm)?,
        "ctox.iot.datapoints.query" => datapoints_query(root, parse_payload(payload)?, realm)?,
        "ctox.iot.alarm.update" => alarm_update(root, parse_payload(payload)?, realm)?,
        "ctox.iot.ruleset.save" => ruleset_save(root, parse_payload(payload)?, realm)?,
        "ctox.iot.ruleset.toggle" => {
            let id = payload
                .get("ruleset_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.ruleset.toggle requires ruleset_id"))?;
            let enabled = payload
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| anyhow!("ctox.iot.ruleset.toggle requires enabled"))?;
            ruleset_toggle(root, id, enabled, realm)?
        }
        "ctox.iot.agent.configure" => agent_configure(root, parse_payload(payload)?, realm)?,
        // RFC 0011 — Automation-Widgets. Reads go via iot_dashboards/iot_widgets
        // projections (like assets), so only mutations are routed here.
        "ctox.iot.dashboard.upsert" => dashboard_upsert(root, parse_payload(payload)?, realm)?,
        "ctox.iot.dashboard.delete" => {
            let id = payload
                .get("dashboard_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.dashboard.delete requires dashboard_id"))?;
            dashboard_delete(root, id, realm)?
        }
        "ctox.iot.widget.upsert" => widget_upsert(root, parse_payload(payload)?, realm)?,
        "ctox.iot.widget.delete" => {
            let id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.widget.delete requires widget_id"))?;
            widget_delete(root, id, realm)?
        }
        "ctox.iot.widget.arrange" => {
            let id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.widget.arrange requires widget_id"))?;
            let coord = |k: &str| -> Result<i64> {
                payload
                    .get(k)
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| anyhow!("ctox.iot.widget.arrange requires {k}"))
            };
            widget_arrange(
                root,
                id,
                coord("x")?,
                coord("y")?,
                coord("w")?,
                coord("h")?,
                realm,
            )?
        }
        "ctox.iot.widget.compile_trigger" => {
            let id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.widget.compile_trigger requires widget_id"))?;
            compile_trigger(root, id, realm)?
        }
        "ctox.iot.widget.generate_render" => {
            let id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.widget.generate_render requires widget_id"))?;
            generate_render(root, id, realm)?
        }
        "ctox.iot.widget.pause" => {
            let id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("ctox.iot.widget.pause requires widget_id"))?;
            let paused = payload
                .get("paused")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            widget_set_pause(root, id, paused, realm)?
        }
        // Returns the token + ingest path directly (not an EngineOutcome) so the
        // operator surface can show the one-time token.
        "ctox.iot.webhook.register" => {
            let mut req: crate::iot::webhook::WebhookRegisterReq = parse_payload(payload)?;
            if let Some(r) = realm {
                req.realm = r.to_string();
            }
            return crate::iot::webhook::register(root, req);
        }
        other => bail!("unknown iot command_type: {other}"),
    };

    Ok(outcome.into_value())
}

fn parse_payload<T: serde::de::DeserializeOwned>(payload: &Value) -> Result<T> {
    serde_json::from_value(payload.clone()).context("failed to deserialize iot command payload")
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn thermostat_type_info() -> AssetTypeInfo {
        use crate::iot::model::{AttributeDescriptor, MetaMap, ValueBaseType, ValueDescriptor};
        let mut meta = MetaMap::new();
        meta.insert("unit".into(), json!("celsius"));
        AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![AttributeDescriptor {
                name: "temp".into(),
                value_descriptor: ValueDescriptor {
                    name: "number".into(),
                    base_type: ValueBaseType::Number,
                    array_dimensions: 0,
                    constraints: vec![],
                    units: None,
                    format: None,
                },
                meta,
            }],
        }
    }

    fn admin_session() -> crate::business_os::store::BusinessOsSession {
        crate::business_os::store::BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(crate::business_os::store::BusinessOsSessionUser {
                id: "admin".into(),
                display_name: "Admin".into(),
                role: "admin".into(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        }
    }

    // handle_iot_command: asset upsert -> list -> show round-trip.
    #[test]
    fn cli_asset_upsert_list_show_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let outcome = asset_upsert(
            root,
            AssetUpsertReq {
                id: Some("asset-cli-1".into()),
                realm: "master".into(),
                asset_type: "Thermostat".into(),
                name: "Living room".into(),
                parent_id: None,
                asset_type_info: Some(thermostat_type_info()),
            },
            None,
        )
        .unwrap();
        let v = outcome.into_value();
        assert_eq!(v["asset"]["id"], "asset-cli-1");
        // The projection set includes the asset and the pre-materialized attr.
        let projections = v["projections"].as_array().unwrap();
        assert!(projections
            .iter()
            .any(|p| p["collection"] == "iot_assets" && p["id"] == "asset-cli-1"));

        // list by realm finds it.
        let listed = asset_list(root, "master", None).unwrap();
        let assets = listed["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0]["id"], "asset-cli-1");

        // show returns the same asset with the typed attribute.
        let shown = asset_show(root, "asset-cli-1", None).unwrap();
        assert_eq!(shown["asset"]["name"], "Living room");
        assert_eq!(shown["asset"]["attributes"]["temp"]["value_type"], "Number");

        // CLI dispatcher path also succeeds (prints JSON, returns Ok).
        handle_iot_command(
            root,
            &[
                "asset".into(),
                "show".into(),
                "--id".into(),
                "asset-cli-1".into(),
            ],
        )
        .unwrap();
    }

    // attribute write -> read round-trip + projection row asserted.
    #[test]
    fn attribute_write_then_read_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        asset_upsert(
            root,
            AssetUpsertReq {
                id: Some("asset-attr-1".into()),
                realm: "master".into(),
                asset_type: "Thermostat".into(),
                name: "Lab".into(),
                parent_id: None,
                asset_type_info: Some(thermostat_type_info()),
            },
            None,
        )
        .unwrap();

        let written = attribute_write(
            root,
            AttributeWriteReq {
                asset_id: "asset-attr-1".into(),
                name: "temp".into(),
                value: json!(22.0),
                timestamp_ms: 1_000,
            },
            None,
        )
        .unwrap();
        let wv = written.into_value();
        assert_eq!(wv["outcome"], "Updated");

        // attribute_read returns the coerced numeric value.
        let read = attribute_read(root, "asset-attr-1", "temp", None).unwrap();
        assert_eq!(read["attribute"]["value"], json!(22.0));

        // A business_records projection row exists for iot_attributes id
        // "asset-attr-1:temp" with data.value == 22.0.
        let conn = store::open_iot_store(root).unwrap();
        let payload = read_projection(&conn, "iot_attributes", "asset-attr-1:temp")
            .unwrap()
            .expect("attribute projection present");
        assert_eq!(payload["data"]["value"], json!(22.0));
        assert_eq!(payload["asset_id"], "asset-attr-1");
        assert_eq!(payload["_deleted"], json!(false));
    }

    // executor and CLI share one path: equivalent inputs → identical asset state.
    #[test]
    fn executor_and_cli_share_one_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session();

        let payload = json!({
            "id": "asset-shared-1",
            "realm": "master",
            "asset_type": "Sensor",
            "name": "Shared",
        });
        let exec_out =
            handle_business_command(root, "ctox.iot.asset.upsert", &payload, &session).unwrap();
        assert_eq!(exec_out["asset"]["id"], "asset-shared-1");
        assert!(exec_out["projections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["collection"] == "iot_assets"));

        // The projected iot_assets row matches what the CLI op would produce.
        let conn = store::open_iot_store(root).unwrap();
        let row = read_projection(&conn, "iot_assets", "asset-shared-1")
            .unwrap()
            .expect("asset projection present");
        assert_eq!(row["name"], "Shared");
        assert_eq!(row["asset_type"], "Sensor");
    }

    // RFC 0011 — a dashboard + an automation widget round-trip through the
    // business-command executor and land as iot_dashboards / iot_widgets
    // projections. The widget carries the three CTOX-programmed parts
    // (cond_text/Wenn, action_prompt/Dann, plus the generated code slots).
    #[test]
    fn dashboard_and_widget_roundtrip_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session();

        // 1) Dashboard.
        let dash_out = handle_business_command(
            root,
            "ctox.iot.dashboard.upsert",
            &json!({ "id": "dash-1", "realm": "master", "name": "Serverraum" }),
            &session,
        )
        .unwrap();
        assert_eq!(dash_out["dashboard"]["id"], "dash-1");
        assert!(dash_out["projections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["collection"] == "iot_dashboards"));

        // 2) Widget = an automation (Wenn/Dann + signal binding).
        let wid_out = handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({
                "id": "wid-1",
                "dashboard_id": "dash-1",
                "realm": "master",
                "signal_ref": "serverraum.temp",
                "cond_text": "wenn es zu lange zu heiß wird",
                "action_prompt": "Kühlung hochfahren und melden",
            }),
            &session,
        )
        .unwrap();
        assert_eq!(wid_out["widget"]["id"], "wid-1");

        let conn = store::open_iot_store(root).unwrap();
        let row = read_projection(&conn, "iot_widgets", "wid-1")
            .unwrap()
            .expect("widget projection present");
        assert_eq!(row["dashboard_id"], "dash-1");
        assert_eq!(row["signal_ref"], "serverraum.temp");
        assert_eq!(row["cond_text"], "wenn es zu lange zu heiß wird");
        assert_eq!(row["action_prompt"], "Kühlung hochfahren und melden");
        // No generated code yet → status idle (compile_trigger/generate_render pending).
        assert_eq!(row["status_key"], "idle");

        // 3) Arrange persists grid geometry.
        handle_business_command(
            root,
            "ctox.iot.widget.arrange",
            &json!({ "widget_id": "wid-1", "x": 2, "y": 1, "w": 4, "h": 3 }),
            &session,
        )
        .unwrap();
        let row = read_projection(&conn, "iot_widgets", "wid-1")
            .unwrap()
            .expect("widget projection present");
        assert_eq!(row["x"], 2);
        assert_eq!(row["w"], 4);

        // 4) Delete tombstones the widget projection (RxDB-replicated soft delete).
        handle_business_command(
            root,
            "ctox.iot.widget.delete",
            &json!({ "widget_id": "wid-1" }),
            &session,
        )
        .unwrap();
        let row = read_projection(&conn, "iot_widgets", "wid-1")
            .unwrap()
            .expect("tombstone row present");
        assert_eq!(row["_deleted"], true, "widget should be soft-deleted");
        // And it is gone from the engine table.
        let live: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM iot_widgets WHERE id = ?1",
                params!["wid-1"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 0, "widget row should be deleted from engine table");
    }

    // compile_trigger enqueues a durable codegen task (CTOX writes the watcher);
    // it never generates code synchronously here.
    #[test]
    fn compile_trigger_enqueues_durable_codegen_task() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session();
        handle_business_command(
            root,
            "ctox.iot.dashboard.upsert",
            &json!({ "id": "d1", "realm": "master", "name": "D" }),
            &session,
        )
        .unwrap();
        handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({ "id": "w1", "dashboard_id": "d1", "realm": "master", "signal_ref": "a::temp", "cond_text": "wenn zu heiß" }),
            &session,
        )
        .unwrap();

        let out = handle_business_command(
            root,
            "ctox.iot.widget.compile_trigger",
            &json!({ "widget_id": "w1" }),
            &session,
        )
        .unwrap();
        assert_eq!(out["kind"], "iot_trigger_code");
        assert!(
            out["queued"].as_str().unwrap_or("").starts_with("queue:"),
            "expected a queue task key, got: {out}"
        );

        // A widget without a Wenn cannot be compiled.
        handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({ "id": "w2", "dashboard_id": "d1", "realm": "master", "signal_ref": "a::temp" }),
            &session,
        )
        .unwrap();
        let err = handle_business_command(
            root,
            "ctox.iot.widget.compile_trigger",
            &json!({ "widget_id": "w2" }),
            &session,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no condition"), "got: {err}");
    }

    // Writing back a watcher program validates it: runnable → armed, broken →
    // needs_attention (the self-repair gate).
    #[test]
    fn widget_upsert_validates_trigger_code_into_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session();
        handle_business_command(
            root,
            "ctox.iot.dashboard.upsert",
            &json!({ "id": "d1", "realm": "master", "name": "D" }),
            &session,
        )
        .unwrap();

        // Valid program → armed.
        handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({ "id": "w1", "dashboard_id": "d1", "realm": "master", "signal_ref": "a::t",
                     "trigger_code": "if signal.last() > 30.0 { fire(\"x\"); }" }),
            &session,
        )
        .unwrap();
        let conn = store::open_iot_store(root).unwrap();
        let status: String = conn
            .query_row(
                "SELECT trigger_status FROM iot_widgets WHERE id = 'w1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "armed");

        // Broken program → needs_attention.
        handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({ "id": "w1", "dashboard_id": "d1", "realm": "master", "signal_ref": "a::t",
                     "trigger_code": "this is @@@ not rhai" }),
            &session,
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT trigger_status FROM iot_widgets WHERE id = 'w1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "needs_attention");
    }

    // Pause stops the watcher; resume recomputes status from the (re-validated)
    // program so a runnable one comes back "armed".
    #[test]
    fn widget_pause_and_resume_recomputes_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session();
        handle_business_command(
            root,
            "ctox.iot.dashboard.upsert",
            &json!({ "id": "d1", "realm": "master", "name": "D" }),
            &session,
        )
        .unwrap();
        handle_business_command(
            root,
            "ctox.iot.widget.upsert",
            &json!({ "id": "w1", "dashboard_id": "d1", "realm": "master", "signal_ref": "a::t",
                     "trigger_code": "if signal.last() > 30.0 { fire(\"x\"); }" }),
            &session,
        )
        .unwrap();
        let conn = store::open_iot_store(root).unwrap();
        let status = || -> String {
            conn.query_row(
                "SELECT trigger_status FROM iot_widgets WHERE id = 'w1'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(status(), "armed");

        handle_business_command(
            root,
            "ctox.iot.widget.pause",
            &json!({ "widget_id": "w1", "paused": true }),
            &session,
        )
        .unwrap();
        assert_eq!(status(), "paused");

        handle_business_command(
            root,
            "ctox.iot.widget.pause",
            &json!({ "widget_id": "w1", "paused": false }),
            &session,
        )
        .unwrap();
        assert_eq!(status(), "armed");
    }

    // The webhook.register business command returns the one-time token + path so
    // the operator UI can show them.
    #[test]
    fn webhook_register_command_returns_token_and_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let out = handle_business_command(
            root,
            "ctox.iot.webhook.register",
            &json!({ "realm": "master", "signal_ref": "asset-1::temperature", "value_path": "data.temp" }),
            &admin_session(),
        )
        .unwrap();
        assert!(out["token"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false));
        assert!(out["ingest_path"]
            .as_str()
            .unwrap_or("")
            .starts_with("/ctox/iot/webhook/"));
        assert_eq!(out["header"], "X-Webhook-Token");
    }

    // Forbidden when the session is neither admin nor an authenticated open one.
    #[test]
    fn executor_rejects_unauthorized_session() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = crate::business_os::store::BusinessOsSession {
            ok: false,
            authenticated: false,
            auth_required: true,
            user: None,
            login_url: None,
            reason: None,
        };
        let payload = json!({ "realm": "master", "asset_type": "X", "name": "n" });
        let err =
            handle_business_command(root, "ctox.iot.asset.upsert", &payload, &session).unwrap_err();
        assert!(err.to_string().contains("forbidden"), "got: {err}");
    }

    // alarm update lifecycle projects the new status; illegal transition errors.
    #[test]
    fn alarm_update_lifecycle_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create an alarm directly via the engine (creation is Phase-1).
        let conn = alarms::open(root).unwrap();
        let alarm = alarms::create(
            &conn,
            alarms::NewAlarm {
                realm: "master".into(),
                title: "High CPU".into(),
                content: None,
                severity: alarms::Severity::High,
                assignee_id: None,
                source: alarms::Source::Agent,
                source_id: "agent-1".into(),
            },
            vec![],
        )
        .unwrap();
        drop(conn);

        let acked = alarm_update(
            root,
            AlarmUpdateReq {
                alarm_id: alarm.id.clone(),
                action: "ack".into(),
                assignee: None,
                status: None,
            },
            None,
        )
        .unwrap();
        let v = acked.into_value();
        assert_eq!(v["alarm"]["status"], "Acknowledged");

        // Projection row carries the upstream wire status_key.
        let conn = store::open_iot_store(root).unwrap();
        let row = read_projection(&conn, "iot_alarms", &alarm.id)
            .unwrap()
            .expect("alarm projection present");
        assert_eq!(row["status_key"], "ACKNOWLEDGED");

        // Closing then acking is illegal (CLOSED is terminal) → Err.
        alarm_update(
            root,
            AlarmUpdateReq {
                alarm_id: alarm.id.clone(),
                action: "close".into(),
                assignee: None,
                status: None,
            },
            None,
        )
        .unwrap();
        let err = alarm_update(
            root,
            AlarmUpdateReq {
                alarm_id: alarm.id.clone(),
                action: "ack".into(),
                assignee: None,
                status: None,
            },
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("illegal"), "got: {err}");
    }

    // datapoints query writes a bounded-window projection row.
    #[test]
    fn datapoints_query_projects_bounded_window() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        asset_upsert(
            root,
            AssetUpsertReq {
                id: Some("asset-dp-1".into()),
                realm: "master".into(),
                asset_type: "Thermostat".into(),
                name: "DP".into(),
                parent_id: None,
                asset_type_info: Some(thermostat_type_info()),
            },
            None,
        )
        .unwrap();
        // Record several samples through the write path.
        for (i, ts) in [(0.0, 100), (10.0, 200), (20.0, 300), (30.0, 400)] {
            attribute_write(
                root,
                AttributeWriteReq {
                    asset_id: "asset-dp-1".into(),
                    name: "temp".into(),
                    value: json!(i),
                    timestamp_ms: ts,
                },
                None,
            )
            .unwrap();
        }

        let out = datapoints_query(
            root,
            DatapointsQueryReq {
                asset_id: "asset-dp-1".into(),
                attribute_name: "temp".into(),
                from_ms: 0,
                to_ms: 1_000,
                shape: "lttb".into(),
                interval_ms: None,
                threshold: Some(3),
            },
            None,
        )
        .unwrap();
        let v = out.into_value();
        assert_eq!(v["shape"], "lttb");
        assert!(v["point_count"].as_u64().unwrap() <= 3);

        let conn = store::open_iot_store(root).unwrap();
        let key = "asset-dp-1:temp:0:1000:lttb";
        let row = read_projection(&conn, "iot_datapoints", key)
            .unwrap()
            .expect("datapoint window projection present");
        assert!(row["point_count"].as_u64().unwrap() <= 3);
        assert_eq!(row["truncated"], json!(false));
    }

    // ruleset + agent rows persist and project.
    #[test]
    fn ruleset_and_agent_stubs_persist_and_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let saved = ruleset_save(
            root,
            RulesetSaveReq {
                id: Some("rule-1".into()),
                realm: "master".into(),
                name: "High temp".into(),
                enabled: true,
                data: json!({ "when": "temp > 30" }),
            },
            None,
        )
        .unwrap();
        assert!(!saved.projections.is_empty());

        ruleset_toggle(root, "rule-1", false, None).unwrap();
        let conn = store::open_iot_store(root).unwrap();
        let row = read_projection(&conn, "iot_rulesets", "rule-1")
            .unwrap()
            .expect("ruleset projection present");
        assert_eq!(row["status_key"], "disabled");
        drop(conn);

        // ruleset_list reflects the persisted record.
        let listed = ruleset_list(root, "master").unwrap();
        assert_eq!(listed["rulesets"].as_array().unwrap().len(), 1);

        // agent configure + status default.
        agent_configure(
            root,
            AgentConfigureReq {
                id: Some("agent-1".into()),
                realm: "master".into(),
                name: "MQTT broker".into(),
                kind: "mqtt".into(),
                enabled: true,
                data: json!({ "url": "mqtt://broker" }),
            },
            None,
        )
        .unwrap();
        let conn = store::open_iot_store(root).unwrap();
        let agent_row = read_projection(&conn, "iot_agents", "agent-1")
            .unwrap()
            .expect("agent projection present");
        assert_eq!(agent_row["status_key"], "mqtt");
        drop(conn);

        let status = agent_status(root, "agent-1").unwrap();
        assert_eq!(status["agent_status"]["link_state"], "unconfigured");
    }

    // -------------------------------------------------------------------
    // Multi-realm isolation (Phase 2): the session realm is enforced on every
    // read/write; a cross-realm resource is invisible; the client payload realm
    // is never trusted.
    // -------------------------------------------------------------------

    #[test]
    fn realm_isolation_blocks_cross_realm_read_and_write() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session(); // session_realm == "master"

        // Create an asset in a DIFFERENT realm directly via the engine (so the
        // payload-realm override on upsert does not mask the test).
        {
            let conn = store::open_iot_store(root).unwrap();
            let asset = Asset {
                id: "other-realm-asset".into(),
                parent_id: None,
                realm: "tenant-b".into(),
                asset_type: "Sensor".into(),
                name: "Hidden".into(),
                path: Vec::new(),
                attributes: std::collections::BTreeMap::new(),
            };
            store::upsert_asset(&conn, &asset).unwrap();
        }

        // asset_show through the session realm ("master") must NOT see it.
        let err = asset_show(root, "other-realm-asset", Some("master")).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");

        // attribute_write to the cross-realm asset is rejected (not found).
        let err = attribute_write(
            root,
            AttributeWriteReq {
                asset_id: "other-realm-asset".into(),
                name: "x".into(),
                value: json!(1.0),
                timestamp_ms: 0,
            },
            Some("master"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");

        // datapoints_query on the cross-realm asset is rejected.
        let err = datapoints_query(
            root,
            DatapointsQueryReq {
                asset_id: "other-realm-asset".into(),
                attribute_name: "x".into(),
                from_ms: 0,
                to_ms: 1,
                shape: "all".into(),
                interval_ms: None,
                threshold: None,
            },
            Some("master"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");

        // asset_delete on the cross-realm asset is rejected.
        let err = asset_delete(root, "other-realm-asset", Some("master")).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");

        // The cross-realm asset still exists (delete did not slip through).
        let conn = store::open_iot_store(root).unwrap();
        assert!(store::get_asset(&conn, "other-realm-asset")
            .unwrap()
            .is_some());
        drop(conn);

        // And the executor path produces the same isolation for asset.delete.
        let del_err = handle_business_command(
            root,
            "ctox.iot.asset.delete",
            &json!({ "asset_id": "other-realm-asset" }),
            &session,
        )
        .unwrap_err();
        assert!(del_err.to_string().contains("not found"), "got: {del_err}");
    }

    #[test]
    fn realm_is_not_trusted_from_payload_on_upsert() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let session = admin_session(); // session_realm == "master"

        // The client claims realm "tenant-evil" in the payload; the executor must
        // override it with the SESSION realm ("master").
        let out = handle_business_command(
            root,
            "ctox.iot.asset.upsert",
            &json!({
                "id": "claimed-asset",
                "realm": "tenant-evil",
                "asset_type": "Sensor",
                "name": "Claimed",
            }),
            &session,
        )
        .unwrap();
        assert_eq!(out["asset"]["realm"], "master", "payload realm overridden");

        // Persisted realm is the session realm, not the claimed one.
        let conn = store::open_iot_store(root).unwrap();
        let asset = store::get_asset(&conn, "claimed-asset").unwrap().unwrap();
        assert_eq!(asset.realm, "master");
    }
}
