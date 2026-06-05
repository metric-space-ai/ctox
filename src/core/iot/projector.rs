// Origin: CTOX
// License: AGPL-3.0-only
//
// IoT → business_records projection layer (Phase 2). Reads the authoritative
// `iot_*` engine tables (asset / attribute / asset-type / alarm / datapoints,
// plus the ruleset / agent / agent-status stub tables) out of the single core
// db (runtime/ctox.sqlite3) and builds the canonical projection rows that the
// rxdb_peer projection branch upserts into the read-only `iot_*` RxDB
// collections. Domain semantics ported from OpenRemote (AGPL-3.0,
// archive/openremote, HEAD 22a42a7); persistence/transport are CTOX-native.
//
// Hard rules honored here:
//   * native Rust only; no new heavy deps.
//   * engine state lives ONLY in runtime/ctox.sqlite3 via crate::paths::core_db
//     (reached through the Phase-1 store/datapoints/alarms `open_*` helpers).
//   * this module is a PURE producer of projection rows: it NEVER calls
//     rxdb_peer, never opens an HTTP bridge, never edits shared files. The
//     integrator wires `ProjectionRow` output into the rxdb_peer `projections`
//     branch, which is the only thing allowed to write `business_records` /
//     stream to RxDB over WebRTC.
//   * the module surface reads `iot_*` only; writes flow through
//     business_commands. This file produces the projections those writes echo.
//
// Row shape: each `ProjectionRow` carries the target `collection`, the
// `record_id` (the collection primary key), the domain `updated_at_ms`, and the
// canonical `payload` envelope. The envelope matches the schema-only contract in
// `src/apps/business-os/modules/iot/schema.js`: canonical engine JSON under
// `data`, plus light `index_text` (search), `sort_key` (ordering) and
// `status_key` (filter facet) columns, the light per-collection index columns,
// and `realm` / `created_at_ms` / `updated_at_ms`. The id / `_rev` / `_deleted`
// / `updated_at_ms` columns are injected by `store::upsert_business_record` at
// the integrator's write site, so the builders here deliberately do NOT set
// `id`/`_rev`/`_deleted` (a non-deletion row leaves `_deleted` for injection;
// a tombstone sets `_deleted: true` explicitly so the projection-tombstone path
// recognizes it).
//
// ref: AssetProcessingService.java (engine source of truth for asset/attr state)
// ref: AlarmService.java (alarm lifecycle source of truth)

use crate::iot::alarms::{self, Alarm, Severity, Source, Status};
use crate::iot::model::{Asset, Attribute, AttributeValue, ValueBaseType};
use crate::iot::{now_ms, Result};
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Collection names (must match modules/iot/schema.js + the rxdb_peer whitelist)
// ---------------------------------------------------------------------------

pub(crate) const COLLECTION_REALMS: &str = "iot_realms";
pub(crate) const COLLECTION_ASSET_TYPES: &str = "iot_asset_types";
pub(crate) const COLLECTION_ASSETS: &str = "iot_assets";
pub(crate) const COLLECTION_ATTRIBUTES: &str = "iot_attributes";
pub(crate) const COLLECTION_DATAPOINTS: &str = "iot_datapoints";
pub(crate) const COLLECTION_ALARMS: &str = "iot_alarms";
pub(crate) const COLLECTION_RULESETS: &str = "iot_rulesets";
pub(crate) const COLLECTION_AGENTS: &str = "iot_agents";
pub(crate) const COLLECTION_AGENT_STATUS: &str = "iot_agent_status";

/// §2A.14 — hard upper bound on how many `iot_attributes` projection rows a
/// single asset may fan out into per reprojection. Event-driven attribute
/// creation (§2A.6) could in principle let an asset accumulate an unbounded
/// attribute set; projecting it would emit N+1 rows into RxDB with no cap. Real
/// assets carry well under this (typical < 50). On overflow we project the
/// bounded prefix and LOG (never silent), matching the datapoints query bound.
pub(crate) const MAX_ATTRIBUTES_PER_ASSET: usize = 1000;

// ---------------------------------------------------------------------------
// ProjectionRow — the unit produced by every project_* fn
// ---------------------------------------------------------------------------

/// One projection row destined for a single `iot_*` collection. The integrator
/// upserts it via `store::upsert_business_record(conn, &row.collection,
/// &row.record_id, row.updated_at_ms, row.payload)`; `upsert_business_record`
/// injects `id` / `_rev` / `_deleted` / `updated_at_ms`, so this struct's
/// `payload` carries only the canonical envelope (`data` + light columns).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectionRow {
    /// Target `iot_*` collection (one of the COLLECTION_* constants).
    pub collection: &'static str,
    /// Collection primary key (`id`). Matches the per-collection record-id rule
    /// in the spec (asset id, `{asset}:{name}`, alarm id, window key, …).
    pub record_id: String,
    /// Domain `updated_at_ms` (epoch-ms, §2A.13) — `now_ms()` at projection time.
    pub updated_at_ms: i64,
    /// Canonical envelope WITHOUT id/_rev/_deleted/updated_at_ms (injected at the
    /// write site). A tombstone sets `_deleted: true` + `data: {}` explicitly.
    pub payload: Value,
}

impl ProjectionRow {
    fn new(
        collection: &'static str,
        record_id: impl Into<String>,
        payload: Value,
    ) -> ProjectionRow {
        ProjectionRow {
            collection,
            record_id: record_id.into(),
            updated_at_ms: now_ms(),
            payload,
        }
    }

    /// Build a tombstone row: `_deleted: true`, empty `data`. The rxdb_peer
    /// projection-tombstone path recognizes this and deletes the RxDB doc.
    fn tombstone(collection: &'static str, record_id: impl Into<String>) -> ProjectionRow {
        let now = now_ms();
        ProjectionRow {
            collection,
            record_id: record_id.into(),
            updated_at_ms: now,
            payload: json!({
                "_deleted": true,
                "data": {},
                "updated_at_ms": now,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Light helpers (light index columns are derived; `data` is canonical engine JSON)
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

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Low => "LOW",
        Severity::Medium => "MEDIUM",
        Severity::High => "HIGH",
    }
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Open => "OPEN",
        Status::Acknowledged => "ACKNOWLEDGED",
        Status::InProgress => "IN_PROGRESS",
        Status::Resolved => "RESOLVED",
        Status::Closed => "CLOSED",
    }
}

fn source_str(s: Source) -> &'static str {
    match s {
        Source::Manual => "MANUAL",
        Source::Client => "CLIENT",
        Source::GlobalRuleset => "GLOBAL_RULESET",
        Source::RealmRuleset => "REALM_RULESET",
        Source::AssetRuleset => "ASSET_RULESET",
        Source::Agent => "AGENT",
    }
}

/// Zero-pad a non-negative epoch-ms so lexical ordering matches numeric
/// ordering; used as the descending sort key for time-ordered collections
/// (alarms). `MAX - created` yields newest-first under ascending lexical sort.
fn sort_key_desc_ms(ms: i64) -> String {
    // i64 epoch-ms fits well within 19 decimal digits; pad to a stable width.
    let clamped = ms.max(0);
    let inverted = i64::MAX - clamped;
    format!("{inverted:019}")
}

/// Stable ascending zero-padded epoch-ms key (used for datapoint windows).
fn sort_key_asc_ms(ms: i64) -> String {
    let clamped = ms.max(0);
    format!("{clamped:019}")
}

/// Build the canonical `data` JSON for an attribute (full Attribute incl.
/// value/meta/type/timestamp), independent of how it was loaded.
fn attribute_data(attr: &Attribute) -> Value {
    serde_json::to_value(attr).unwrap_or_else(|_| json!({}))
}

// ---------------------------------------------------------------------------
// (b) Pure row builders — one per collection
// ---------------------------------------------------------------------------

/// `iot_assets` row. record_id = asset id; sort_key = name.
fn asset_row(asset: &Asset) -> ProjectionRow {
    // attribute_summary: a light name → current-value map for list rendering.
    let mut attribute_summary = serde_json::Map::new();
    for (name, attr) in &asset.attributes {
        let v = attr
            .value
            .as_ref()
            .map(|AttributeValue(v)| v.clone())
            .unwrap_or(Value::Null);
        attribute_summary.insert(name.clone(), v);
    }
    // location: lifted from a "location" attribute if present (GeoPoint/object).
    let location = asset
        .attributes
        .get("location")
        .and_then(|a| a.value.as_ref())
        .map(|AttributeValue(v)| v.clone())
        .unwrap_or(Value::Null);

    let index_text = format!(
        "{} {} {}",
        asset.name.to_lowercase(),
        asset.asset_type.to_lowercase(),
        asset.id.to_lowercase()
    );
    let payload = json!({
        "realm": asset.realm,
        "parent_id": asset.parent_id,
        "asset_type": asset.asset_type,
        "name": asset.name,
        "attribute_summary": Value::Object(attribute_summary),
        "location": location,
        "data": serde_json::to_value(asset).unwrap_or_else(|_| json!({})),
        "index_text": index_text,
        "sort_key": asset.name,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_ASSETS, asset.id.clone(), payload)
}

/// `iot_attributes` row. record_id = "{asset_id}:{attribute_name}";
/// status_key = value_type.
fn attribute_row(asset_id: &str, realm: &str, attr: &Attribute) -> ProjectionRow {
    let id = format!("{asset_id}:{}", attr.name);
    let value_type = attr.value_type.map(value_base_type_str).unwrap_or("");
    let payload = json!({
        "realm": realm,
        "asset_id": asset_id,
        "attribute_name": attr.name,
        "value_type": value_type,
        "timestamp_ms": attr.timestamp,
        "data": attribute_data(attr),
        "index_text": attr.name.to_lowercase(),
        "sort_key": attr.name,
        "status_key": value_type,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_ATTRIBUTES, id, payload)
}

/// `iot_asset_types` row. record_id = asset_type; realm is NOT required
/// (global types). attribute_count is a light convenience column.
fn asset_type_row(info: &crate::iot::model::AssetTypeInfo) -> ProjectionRow {
    let payload = json!({
        "asset_type": info.asset_type,
        "attribute_count": info.attributes.len(),
        "data": serde_json::to_value(info).unwrap_or_else(|_| json!({})),
        "index_text": info.asset_type.to_lowercase(),
        "sort_key": info.asset_type,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_ASSET_TYPES, info.asset_type.clone(), payload)
}

/// `iot_realms` row. record_id = realm name; status_key = "active"|"disabled".
fn realm_row(realm: &RealmRecord) -> ProjectionRow {
    let payload = json!({
        "realm": realm.name,
        "name": realm.name,
        "parent_realm": realm.parent_realm,
        "data": serde_json::to_value(realm).unwrap_or_else(|_| json!({})),
        "index_text": realm.name.to_lowercase(),
        "sort_key": realm.name,
        "status_key": if realm.enabled { "active" } else { "disabled" },
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_REALMS, realm.name.clone(), payload)
}

/// `iot_alarms` row. record_id = alarm id; status_key = lifecycle status;
/// sort_key = inverted created_ms (newest first under ascending sort).
fn alarm_row(alarm: &Alarm) -> ProjectionRow {
    let index_text = format!(
        "{} {}",
        alarm.title.to_lowercase(),
        alarm.content.as_deref().unwrap_or("").to_lowercase()
    );
    let payload = json!({
        "realm": alarm.realm,
        "title": alarm.title,
        "severity": severity_str(alarm.severity),
        "status": status_str(alarm.status),
        "assignee_id": alarm.assignee_id,
        "source": source_str(alarm.source),
        "created_ms": alarm.created,
        "data": serde_json::to_value(alarm).unwrap_or_else(|_| json!({})),
        "index_text": index_text,
        "sort_key": sort_key_desc_ms(alarm.created),
        "status_key": status_str(alarm.status),
        "created_at_ms": alarm.created,
    });
    ProjectionRow::new(COLLECTION_ALARMS, alarm.id.clone(), payload)
}

/// `iot_datapoints` window row. record_id = window key; never per-sample.
/// `data` carries the bounded point array; `truncated` is plumbed.
#[allow(clippy::too_many_arguments)]
fn datapoint_window_row(
    window_key: &str,
    realm: &str,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
    shape: &str,
    point_count: usize,
    truncated: bool,
    data: Value,
) -> ProjectionRow {
    let payload = json!({
        "realm": realm,
        "asset_id": asset_id,
        "attribute_name": attribute_name,
        "from_ms": from_ms,
        "to_ms": to_ms,
        "shape": shape,
        "point_count": point_count,
        "truncated": truncated,
        "data": data,
        "index_text": format!("{} {}", asset_id.to_lowercase(), attribute_name.to_lowercase()),
        "sort_key": sort_key_asc_ms(from_ms),
        "status_key": shape,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_DATAPOINTS, window_key.to_string(), payload)
}

/// `iot_rulesets` row. status_key = "enabled"|"disabled". `data` is native rule
/// JSON consumed by the rules layer.
fn ruleset_row(rs: &RulesetRecord) -> ProjectionRow {
    let payload = json!({
        "realm": rs.realm,
        "name": rs.name,
        "enabled": rs.enabled,
        "last_fired_ms": rs.last_fired_ms,
        "data": rs.data.clone(),
        "index_text": rs.name.to_lowercase(),
        "sort_key": rs.name,
        "status_key": if rs.enabled { "enabled" } else { "disabled" },
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_RULESETS, rs.id.clone(), payload)
}

/// `iot_agents` row. status_key = kind. `data` is native protocol-agent config.
fn agent_row(agent: &AgentRecord) -> ProjectionRow {
    let payload = json!({
        "realm": agent.realm,
        "name": agent.name,
        "kind": agent.kind,
        "enabled": agent.enabled,
        "data": agent.data.clone(),
        "index_text": format!("{} {}", agent.name.to_lowercase(), agent.kind.to_lowercase()),
        "sort_key": agent.name,
        "status_key": agent.kind,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_AGENTS, agent.id.clone(), payload)
}

/// `iot_agent_status` row. record_id = agent id. Projects the engine-internal
/// link health (default link_state "unconfigured").
fn agent_status_row(status: &AgentStatusRecord) -> ProjectionRow {
    let payload = json!({
        "realm": status.realm,
        "agent_id": status.agent_id,
        "link_state": status.link_state,
        "last_event_ms": status.last_event_ms,
        "error": status.error,
        "data": serde_json::to_value(status).unwrap_or_else(|_| json!({})),
        "index_text": status.agent_id.to_lowercase(),
        "sort_key": status.agent_id,
        "status_key": status.link_state,
        "created_at_ms": now_ms(),
    });
    ProjectionRow::new(COLLECTION_AGENT_STATUS, status.agent_id.clone(), payload)
}

// ---------------------------------------------------------------------------
// Table record types (realm / ruleset / agent / agent_status)
//
// These mirror the lazily-created ruleset/agent tables described in the spec.
// The projector reads them if present; if a table does not yet exist (the
// integrator may create it in commands.rs / store init), the readers return an
// empty set rather than erroring, so `project_all` is robust during bring-up.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RealmRecord {
    pub name: String,
    #[serde(default)]
    pub parent_realm: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RulesetRecord {
    pub id: String,
    pub realm: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_fired_ms: Option<i64>,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AgentRecord {
    pub id: String,
    pub realm: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AgentStatusRecord {
    pub agent_id: String,
    pub realm: String,
    pub link_state: String,
    #[serde(default)]
    pub last_event_ms: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

/// True iff a table with this name exists in the core db.
fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let found: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |r| r.get(0),
        )
        .optional()
        .with_context(|| format!("failed to probe for table {name}"))?;
    Ok(found.is_some())
}

// ---------------------------------------------------------------------------
// Stub-table readers (tolerate a not-yet-created table)
// ---------------------------------------------------------------------------

fn read_realm(conn: &Connection, realm: &str) -> Result<Option<RealmRecord>> {
    if !table_exists(conn, "iot_realms")? {
        return Ok(None);
    }
    let row: Option<(String, Option<String>, i64)> = conn
        .query_row(
            "SELECT name, parent_realm, enabled FROM iot_realms WHERE name = ?1",
            params![realm],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .context("failed to query iot_realms")?;
    Ok(row.map(|(name, parent_realm, enabled)| RealmRecord {
        name,
        parent_realm,
        enabled: enabled != 0,
    }))
}

fn read_ruleset(conn: &Connection, id: &str) -> Result<Option<RulesetRecord>> {
    if !table_exists(conn, "iot_rulesets")? {
        return Ok(None);
    }
    let row: Option<(String, String, String, i64, Option<i64>, String)> = conn
        .query_row(
            "SELECT id, realm, name, enabled, last_fired_ms, data
             FROM iot_rulesets WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .context("failed to query iot_rulesets")?;
    Ok(match row {
        Some((id, realm, name, enabled, last_fired_ms, data)) => Some(RulesetRecord {
            id,
            realm,
            name,
            enabled: enabled != 0,
            last_fired_ms,
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
        }),
        None => None,
    })
}

fn read_agent(conn: &Connection, id: &str) -> Result<Option<AgentRecord>> {
    if !table_exists(conn, "iot_agents")? {
        return Ok(None);
    }
    let row: Option<(String, String, String, String, i64, String)> = conn
        .query_row(
            "SELECT id, realm, name, kind, enabled, data FROM iot_agents WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .context("failed to query iot_agents")?;
    Ok(match row {
        Some((id, realm, name, kind, enabled, data)) => Some(AgentRecord {
            id,
            realm,
            name,
            kind,
            enabled: enabled != 0,
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
        }),
        None => None,
    })
}

fn read_agent_status(conn: &Connection, agent_id: &str) -> Result<Option<AgentStatusRecord>> {
    if !table_exists(conn, "iot_agent_status")? {
        return Ok(None);
    }
    let row: Option<(String, String, String, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT agent_id, realm, link_state, last_event_ms, error
             FROM iot_agent_status WHERE agent_id = ?1",
            params![agent_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .context("failed to query iot_agent_status")?;
    Ok(row.map(
        |(agent_id, realm, link_state, last_event_ms, error)| AgentStatusRecord {
            agent_id,
            realm,
            link_state,
            last_event_ms,
            error,
        },
    ))
}

// ---------------------------------------------------------------------------
// (c) project_* fns — read engine state, return the projection row(s)
//
// All readers take a `&Connection` on the core db (open via
// `crate::iot::store::open_iot_store(root)` / `alarms::open(root)` /
// `datapoints::open(root)`, which all target runtime/ctox.sqlite3). They return
// the row(s) for the integrator to upsert; they never write.
// ---------------------------------------------------------------------------

/// Project a single asset: one `iot_assets` row plus one `iot_attributes` row
/// per attribute. Returns an empty Vec if the asset does not exist (the
/// integrator treats "no rows" as nothing to upsert).
pub(crate) fn project_asset(conn: &Connection, asset_id: &str) -> Result<Vec<ProjectionRow>> {
    let Some(asset) = crate::iot::store::get_asset(conn, asset_id)? else {
        return Ok(Vec::new());
    };
    // §2A.14 — bound the attribute fan-out. An asset with more attributes than
    // MAX_ATTRIBUTES_PER_ASSET projects only the bounded prefix; the overflow is
    // LOGGED (never silent), so a runaway event-driven attribute set cannot
    // unbounded-fan-out into RxDB.
    let attr_count = asset.attributes.len();
    let capped = attr_count.min(MAX_ATTRIBUTES_PER_ASSET);
    if attr_count > MAX_ATTRIBUTES_PER_ASSET {
        eprintln!(
            "CTOX IoT asset projection truncated to {MAX_ATTRIBUTES_PER_ASSET} attribute rows \
             for asset_id={} (had {attr_count})",
            asset.id
        );
    }
    let mut rows = Vec::with_capacity(1 + capped);
    rows.push(asset_row(&asset));
    for attr in asset.attributes.values().take(MAX_ATTRIBUTES_PER_ASSET) {
        rows.push(attribute_row(&asset.id, &asset.realm, attr));
    }
    Ok(rows)
}

/// Project a single attribute of an asset (`iot_attributes`). Empty Vec if the
/// asset or attribute is absent.
pub(crate) fn project_attribute(
    conn: &Connection,
    asset_id: &str,
    name: &str,
) -> Result<Vec<ProjectionRow>> {
    let Some(asset) = crate::iot::store::get_asset(conn, asset_id)? else {
        return Ok(Vec::new());
    };
    let Some(attr) = asset.attributes.get(name) else {
        return Ok(Vec::new());
    };
    Ok(vec![attribute_row(&asset.id, &asset.realm, attr)])
}

/// Project a single asset-type descriptor (`iot_asset_types`). Empty if absent.
pub(crate) fn project_asset_type(
    conn: &Connection,
    asset_type: &str,
) -> Result<Vec<ProjectionRow>> {
    let Some(info) = crate::iot::store::get_asset_type(conn, asset_type)? else {
        return Ok(Vec::new());
    };
    Ok(vec![asset_type_row(&info)])
}

/// Project a single realm (`iot_realms`). Empty if no realm row exists.
pub(crate) fn project_realm(conn: &Connection, realm: &str) -> Result<Vec<ProjectionRow>> {
    match read_realm(conn, realm)? {
        Some(r) => Ok(vec![realm_row(&r)]),
        None => Ok(Vec::new()),
    }
}

/// Project a single alarm (`iot_alarms`). Empty if absent.
pub(crate) fn project_alarm(conn: &Connection, alarm_id: &str) -> Result<Vec<ProjectionRow>> {
    match alarms::get(conn, alarm_id) {
        Ok(alarm) => Ok(vec![alarm_row(&alarm)]),
        // alarms::get returns Err for a missing alarm; treat as "nothing to
        // project" rather than propagating (idempotent resync robustness).
        Err(_) => Ok(Vec::new()),
    }
}

/// Project a single ruleset (`iot_rulesets`). Empty if absent.
pub(crate) fn project_ruleset(conn: &Connection, ruleset_id: &str) -> Result<Vec<ProjectionRow>> {
    match read_ruleset(conn, ruleset_id)? {
        Some(rs) => Ok(vec![ruleset_row(&rs)]),
        None => Ok(Vec::new()),
    }
}

/// Project a single agent (`iot_agents`). Empty if absent.
pub(crate) fn project_agent(conn: &Connection, agent_id: &str) -> Result<Vec<ProjectionRow>> {
    match read_agent(conn, agent_id)? {
        Some(agent) => Ok(vec![agent_row(&agent)]),
        None => Ok(Vec::new()),
    }
}

/// Project a single agent-status row (`iot_agent_status`). When no engine row
/// exists yet, project a default `link_state:"unconfigured"` row so the
/// collection mirrors a configured agent before the supervisor reports live
/// link health.
pub(crate) fn project_agent_status(
    conn: &Connection,
    agent_id: &str,
) -> Result<Vec<ProjectionRow>> {
    let status = match read_agent_status(conn, agent_id)? {
        Some(s) => s,
        None => {
            // Derive realm from the agent config if present; default link state.
            let realm = read_agent(conn, agent_id)?
                .map(|a| a.realm)
                .unwrap_or_default();
            AgentStatusRecord {
                agent_id: agent_id.to_string(),
                realm,
                link_state: "unconfigured".to_string(),
                last_event_ms: None,
                error: None,
            }
        }
    };
    Ok(vec![agent_status_row(&status)])
}

/// Project a bounded datapoint WINDOW (`iot_datapoints`). The caller (the
/// datapoints query op) supplies the already-bounded `data` array, the window
/// bounds, the shape, the point count, and the truncation flag — this projector
/// never reads raw per-sample rows itself (unbounded series stay in SQLite).
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_datapoint_window(
    window_key: &str,
    realm: &str,
    asset_id: &str,
    attribute_name: &str,
    from_ms: i64,
    to_ms: i64,
    shape: &str,
    point_count: usize,
    truncated: bool,
    data: Value,
) -> ProjectionRow {
    datapoint_window_row(
        window_key,
        realm,
        asset_id,
        attribute_name,
        from_ms,
        to_ms,
        shape,
        point_count,
        truncated,
        data,
    )
}

/// Deletion projection for an asset: a tombstone for the `iot_assets` row plus
/// a tombstone for each currently-known attribute row. Because `delete_asset`
/// removes the attribute rows from the engine, the caller should compute the
/// attribute set BEFORE deletion; this helper accepts the attribute names so it
/// can emit per-attribute tombstones deterministically.
pub(crate) fn project_asset_deleted(
    asset_id: &str,
    attribute_names: &[String],
) -> Vec<ProjectionRow> {
    let mut rows = Vec::with_capacity(1 + attribute_names.len());
    rows.push(ProjectionRow::tombstone(COLLECTION_ASSETS, asset_id));
    for name in attribute_names {
        rows.push(ProjectionRow::tombstone(
            COLLECTION_ATTRIBUTES,
            format!("{asset_id}:{name}"),
        ));
    }
    rows
}

/// Tombstone for a single alarm row (`iot_alarms`).
pub(crate) fn project_alarm_deleted(alarm_id: &str) -> ProjectionRow {
    ProjectionRow::tombstone(COLLECTION_ALARMS, alarm_id)
}

// ---------------------------------------------------------------------------
// Outcome reprojection — the rxdb_peer integration entry (§4A one code path)
// ---------------------------------------------------------------------------

/// A reprojection result for one reported `(collection, record_id)` pair.
#[derive(Clone, Debug)]
pub(crate) enum ReprojectedRecord {
    /// Canonical engine-derived rows (one pair may fan out, e.g. asset + attrs).
    Rows(Vec<ProjectionRow>),
    /// `iot_datapoints` window rows are query-scoped and not reconstructable
    /// from a record id alone — the integrator echoes the executor's row as-is.
    EchoOnly {
        collection: &'static str,
        record_id: String,
    },
}

/// Re-derive the canonical projection rows for the `(collection, record_id)`
/// pairs an `iot::commands` op reported in `EngineOutcome.into_value()` (the
/// `projections` array of the executor result), from authoritative engine state
/// in the core db. PURE producer: this reads `runtime/ctox.sqlite3` (the engine
/// tables) but never writes — the business_os integrator owns the
/// `business_records` write into the RxDB-visible store and the RxDB stream.
///
/// This makes the projector the canonical envelope builder in the sync path: the
/// integrator writes the richer projector envelope (asset attribute summaries,
/// full index text, …) rather than the executor's lighter inline row.
///
/// Idempotent: re-running over identical engine state yields identical
/// envelopes. A pair whose engine row no longer exists (asset/attribute
/// deletion) yields a tombstone `ProjectionRow` so the integrator removes the
/// RxDB doc. `iot_datapoints` pairs yield `EchoOnly`.
pub(crate) fn reproject_business_command_outcome(
    root: &std::path::Path,
    result: &Value,
) -> Result<Vec<ReprojectedRecord>> {
    let Some(projections) = result.get("projections").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if projections.is_empty() {
        return Ok(Vec::new());
    }
    let conn = crate::iot::store::open_iot_store(root)?;

    let mut out: Vec<ReprojectedRecord> = Vec::new();
    for projection in projections {
        let collection = projection
            .get("collection")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let record_id = projection
            .get("id")
            .or_else(|| projection.get("record_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if collection.is_empty() || record_id.is_empty() {
            continue;
        }
        let Some(canonical) = canonical_collection(collection) else {
            // Unknown collection — ignore (never widen the iot whitelist here).
            continue;
        };

        if canonical == COLLECTION_DATAPOINTS {
            out.push(ReprojectedRecord::EchoOnly {
                collection: canonical,
                record_id: record_id.to_string(),
            });
            continue;
        }

        let rows = reproject_one(&conn, canonical, record_id)?;
        if rows.is_empty() {
            // Engine row absent (deletion): emit a tombstone so RxDB drops it.
            out.push(ReprojectedRecord::Rows(vec![ProjectionRow::tombstone(
                canonical, record_id,
            )]));
        } else {
            out.push(ReprojectedRecord::Rows(rows));
        }
    }
    Ok(out)
}

/// Map a reported collection name to the canonical static constant (and gate it
/// to the iot whitelist). Returns `None` for anything that is not an iot
/// collection so callers never widen the projection surface.
fn canonical_collection(collection: &str) -> Option<&'static str> {
    match collection {
        COLLECTION_REALMS => Some(COLLECTION_REALMS),
        COLLECTION_ASSET_TYPES => Some(COLLECTION_ASSET_TYPES),
        COLLECTION_ASSETS => Some(COLLECTION_ASSETS),
        COLLECTION_ATTRIBUTES => Some(COLLECTION_ATTRIBUTES),
        COLLECTION_DATAPOINTS => Some(COLLECTION_DATAPOINTS),
        COLLECTION_ALARMS => Some(COLLECTION_ALARMS),
        COLLECTION_RULESETS => Some(COLLECTION_RULESETS),
        COLLECTION_AGENTS => Some(COLLECTION_AGENTS),
        COLLECTION_AGENT_STATUS => Some(COLLECTION_AGENT_STATUS),
        _ => None,
    }
}

/// Re-derive the projection row(s) for one `(collection, record_id)` pair from
/// engine state. Empty Vec means "no engine row" (the caller tombstones).
fn reproject_one(
    conn: &Connection,
    collection: &'static str,
    record_id: &str,
) -> Result<Vec<ProjectionRow>> {
    match collection {
        COLLECTION_ASSETS => project_asset(conn, record_id),
        COLLECTION_ATTRIBUTES => {
            // record_id == "{asset_id}:{attribute_name}".
            match record_id.split_once(':') {
                Some((asset_id, name)) => project_attribute(conn, asset_id, name),
                None => Ok(Vec::new()),
            }
        }
        COLLECTION_ASSET_TYPES => project_asset_type(conn, record_id),
        COLLECTION_REALMS => project_realm(conn, record_id),
        COLLECTION_ALARMS => project_alarm(conn, record_id),
        COLLECTION_RULESETS => project_ruleset(conn, record_id),
        COLLECTION_AGENTS => project_agent(conn, record_id),
        COLLECTION_AGENT_STATUS => project_agent_status(conn, record_id),
        // Datapoints handled by the caller; nothing else is projectable here.
        _ => Ok(Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// Full resync (idempotent; boot / repair / `ctox iot project --all`)
// ---------------------------------------------------------------------------

/// Resync EVERY projectable engine row into projection rows, ACROSS ALL REALMS.
///
/// This is the trusted/operator entry (`ctox iot project all` runs with full
/// host access, same as the CLI bypasses the session ACL gate). The
/// realm-scoped projection/sync surface that the session executor must use is
/// `project_all_in_realm(conn, Some(realm))` — see that function's contract.
pub(crate) fn project_all(conn: &Connection) -> Result<Vec<ProjectionRow>> {
    project_all_in_realm(conn, None)
}

/// Resync the projectable engine rows for ONE realm (or all realms when `realm`
/// is `None`) into projection rows. Idempotent: an `upsert_business_record` of
/// an identical envelope is a no-op-ish overwrite (only `_rev`/updated_at
/// change). Datapoints are NOT resynced here (only windows produced by an
/// explicit query are ever projected).
///
/// REALM ISOLATION (Phase 2): when `realm` is `Some(r)`, the realm-bearing
/// scans (assets+attributes, alarms, realms, rulesets, agents+status) are
/// filtered with `WHERE realm = ?1` so only rows owned by `r` are projected
/// into the shared `business_records` collections that WebRTC replicates to
/// paired peers. This is the projection/sync-side enforcement that matches the
/// write/command/condition-path enforcement (`session_realm()` →
/// `commands.rs`): the read path the module consumes (RxDB-projected `iot_*`
/// collections) no longer carries other realms' rows. Asset TYPES are global
/// (no realm column) and are always projected. With `realm: None` the scan is
/// unscoped (the trusted CLI / `project_all`).
pub(crate) fn project_all_in_realm(
    conn: &Connection,
    realm: Option<&str>,
) -> Result<Vec<ProjectionRow>> {
    let mut rows = Vec::new();

    // Helper: scan one realm-bearing table's id/name column, optionally filtered
    // to a single realm. Returns the ordered ids. `realm_col` is the column that
    // carries the owning realm (`realm` for most tables; `name` for iot_realms).
    fn scan_ids(
        conn: &Connection,
        table: &str,
        id_col: &str,
        realm_col: &str,
        realm: Option<&str>,
    ) -> Result<Vec<String>> {
        let ids: Vec<String> = match realm {
            Some(r) => {
                let sql = format!(
                    "SELECT {id_col} FROM {table} WHERE {realm_col} = ?1 ORDER BY {id_col} ASC"
                );
                let mut stmt = conn
                    .prepare(&sql)
                    .with_context(|| format!("failed to prepare {table} realm scan"))?;
                let ids = stmt
                    .query_map(params![r], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<_>>()?;
                ids
            }
            None => {
                let sql = format!("SELECT {id_col} FROM {table} ORDER BY {id_col} ASC");
                let mut stmt = conn
                    .prepare(&sql)
                    .with_context(|| format!("failed to prepare {table} scan"))?;
                let ids = stmt
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<_>>()?;
                ids
            }
        };
        Ok(ids)
    }

    // Asset types (GLOBAL — no realm column). Tables are created lazily by the
    // engine ops (open_iot_store / alarms::open), so a fresh root may not have
    // every table yet; tolerate absence rather than crashing the full resync.
    if table_exists(conn, "iot_asset_types")? {
        let mut type_stmt = conn
            .prepare("SELECT asset_type FROM iot_asset_types ORDER BY asset_type ASC")
            .context("failed to prepare asset-type scan")?;
        let type_ids: Vec<String> = type_stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?;
        for asset_type in &type_ids {
            rows.extend(project_asset_type(conn, asset_type)?);
        }
    }

    // Assets (+ their attributes), scoped to the requested realm (or all).
    if table_exists(conn, "iot_assets")? {
        for id in scan_ids(conn, "iot_assets", "id", "realm", realm)? {
            rows.extend(project_asset(conn, &id)?);
        }
    }

    // Alarms (table created lazily by alarms::open).
    if table_exists(conn, "iot_alarms")? {
        for id in scan_ids(conn, "iot_alarms", "id", "realm", realm)? {
            rows.extend(project_alarm(conn, &id)?);
        }
    }

    // Realms (stub table; tolerate absence). The realm's own identity column IS
    // its name, so a realm-scoped resync projects only that realm's row.
    if table_exists(conn, "iot_realms")? {
        for name in scan_ids(conn, "iot_realms", "name", "name", realm)? {
            rows.extend(project_realm(conn, &name)?);
        }
    }

    // Rulesets (stub table; tolerate absence).
    if table_exists(conn, "iot_rulesets")? {
        for id in scan_ids(conn, "iot_rulesets", "id", "realm", realm)? {
            rows.extend(project_ruleset(conn, &id)?);
        }
    }

    // Agents + their status (stub tables; tolerate absence).
    if table_exists(conn, "iot_agents")? {
        for id in scan_ids(conn, "iot_agents", "id", "realm", realm)? {
            rows.extend(project_agent(conn, &id)?);
            rows.extend(project_agent_status(conn, &id)?);
        }
    }

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Tests — projection row shape over a seeded core db
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iot::alarms;
    use crate::iot::model::{
        AssetTypeInfo, AttributeDescriptor, MetaMap, ValueBaseType, ValueDescriptor,
    };
    use crate::iot::store;
    use serde_json::json;

    fn descriptor(name: &str, base: ValueBaseType) -> AttributeDescriptor {
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
            meta: MetaMap::new(),
        }
    }

    fn seed_thermostat(conn: &Connection, id: &str) {
        let info = AssetTypeInfo {
            asset_type: "Thermostat".into(),
            attributes: vec![descriptor("temp", ValueBaseType::Number)],
        };
        store::upsert_asset_type(conn, &info).unwrap();
        let asset = Asset::new_with_type(
            id.to_string(),
            "master".into(),
            "Thermostat".into(),
            "Living room".into(),
            &info,
        );
        store::upsert_asset(conn, &asset).unwrap();
    }

    #[test]
    fn project_asset_shape_includes_asset_and_attribute_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let id = "asset-proj-1";
        seed_thermostat(&conn, id);
        // Write a value so the attribute carries a typed current value.
        let ev = crate::iot::model::AttributeEvent {
            asset_id: id.into(),
            attribute_name: "temp".into(),
            value: AttributeValue(json!(22.0)),
            timestamp: 100,
            old_value: None,
            old_value_timestamp: 0,
        };
        store::process_attribute_event(&conn, &ev, 1_000).unwrap();

        let rows = project_asset(&conn, id).unwrap();
        assert_eq!(rows.len(), 2, "one asset row + one attribute row");

        // Asset row shape.
        let asset_row = rows
            .iter()
            .find(|r| r.collection == COLLECTION_ASSETS)
            .expect("asset row present");
        assert_eq!(asset_row.record_id, id);
        let p = &asset_row.payload;
        assert_eq!(p["realm"], json!("master"));
        assert_eq!(p["asset_type"], json!("Thermostat"));
        assert_eq!(p["name"], json!("Living room"));
        assert_eq!(p["sort_key"], json!("Living room"), "sort_key = name");
        assert!(
            p["data"]["id"] == json!(id),
            "canonical data carries asset id"
        );
        assert!(p.get("index_text").is_some());
        // attribute_summary carries the current value.
        assert_eq!(p["attribute_summary"]["temp"], json!(22.0));
        // The builder leaves _deleted for injection (not present on a live row).
        assert!(p.get("_deleted").is_none());

        // Attribute row shape.
        let attr_row = rows
            .iter()
            .find(|r| r.collection == COLLECTION_ATTRIBUTES)
            .expect("attribute row present");
        assert_eq!(attr_row.record_id, format!("{id}:temp"));
        let ap = &attr_row.payload;
        assert_eq!(ap["asset_id"], json!(id));
        assert_eq!(ap["attribute_name"], json!("temp"));
        assert_eq!(ap["realm"], json!("master"));
        assert_eq!(ap["value_type"], json!("Number"));
        assert_eq!(ap["status_key"], json!("Number"), "status_key = value_type");
        assert_eq!(ap["timestamp_ms"], json!(100));
        // Canonical data is the full Attribute incl. value.
        assert_eq!(ap["data"]["value"], json!(22.0));
    }

    #[test]
    fn project_attribute_shape_matches_collection_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let id = "asset-proj-attr";
        seed_thermostat(&conn, id);
        let ev = crate::iot::model::AttributeEvent {
            asset_id: id.into(),
            attribute_name: "temp".into(),
            value: AttributeValue(json!(18.5)),
            timestamp: 200,
            old_value: None,
            old_value_timestamp: 0,
        };
        store::process_attribute_event(&conn, &ev, 1_000).unwrap();

        let rows = project_attribute(&conn, id, "temp").unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.collection, COLLECTION_ATTRIBUTES);
        assert_eq!(row.record_id, format!("{id}:temp"));
        // Required columns from schema.js: id(injected), realm, asset_id,
        // attribute_name, data, updated_at_ms(injected).
        let p = &row.payload;
        for required in ["realm", "asset_id", "attribute_name", "data"] {
            assert!(
                p.get(required).is_some(),
                "missing required column {required}"
            );
        }
        assert_eq!(p["data"]["value"], json!(18.5));

        // Absent attribute → no rows.
        assert!(project_attribute(&conn, id, "nope").unwrap().is_empty());
        // Absent asset → no rows.
        assert!(project_attribute(&conn, "no-asset", "temp")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn project_alarm_shape_matches_collection_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = alarms::open(tmp.path()).unwrap();
        let alarm = alarms::create(
            &conn,
            alarms::NewAlarm {
                realm: "master".into(),
                title: "High CPU".into(),
                content: Some("cpu > 95%".into()),
                severity: alarms::Severity::High,
                assignee_id: None,
                source: alarms::Source::Agent,
                source_id: "agent-1".into(),
            },
            vec![],
        )
        .unwrap();
        // Acknowledge so status_key reflects a lifecycle transition.
        alarms::acknowledge(&conn, &alarm.id).unwrap();

        let rows = project_alarm(&conn, &alarm.id).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.collection, COLLECTION_ALARMS);
        assert_eq!(row.record_id, alarm.id);
        let p = &row.payload;
        // Required columns: realm, title, severity, status, data.
        assert_eq!(p["realm"], json!("master"));
        assert_eq!(p["title"], json!("High CPU"));
        assert_eq!(p["severity"], json!("HIGH"));
        assert_eq!(p["status"], json!("ACKNOWLEDGED"));
        assert_eq!(
            p["status_key"],
            json!("ACKNOWLEDGED"),
            "status_key = lifecycle status"
        );
        assert_eq!(p["source"], json!("AGENT"));
        // sort_key is the inverted (newest-first) created-ms key, 19 chars wide.
        let sk = p["sort_key"].as_str().unwrap();
        assert_eq!(sk.len(), 19, "zero-padded inverted ms key");
        // Canonical data carries the full alarm.
        assert_eq!(p["data"]["id"], json!(alarm.id));

        // Missing alarm → no rows (idempotent resync robustness).
        assert!(project_alarm(&conn, "no-such-alarm").unwrap().is_empty());
    }

    #[test]
    fn project_datapoint_window_is_bounded_and_carries_shape() {
        let data = json!([
            { "x": 0, "y": 1.0 },
            { "x": 10, "y": 2.0 },
            { "x": 20, "y": 3.0 }
        ]);
        let row = project_datapoint_window(
            "asset-x:temp:0:1000:lttb",
            "master",
            "asset-x",
            "temp",
            0,
            1000,
            "lttb",
            3,
            true,
            data,
        );
        assert_eq!(row.collection, COLLECTION_DATAPOINTS);
        assert_eq!(row.record_id, "asset-x:temp:0:1000:lttb");
        let p = &row.payload;
        assert_eq!(p["asset_id"], json!("asset-x"));
        assert_eq!(p["attribute_name"], json!("temp"));
        assert_eq!(p["shape"], json!("lttb"));
        assert_eq!(p["status_key"], json!("lttb"));
        assert_eq!(p["from_ms"], json!(0));
        assert_eq!(p["to_ms"], json!(1000));
        assert_eq!(p["point_count"], json!(3));
        assert_eq!(p["truncated"], json!(true));
        assert!(p["data"].is_array(), "bounded point array under data");
    }

    #[test]
    fn project_asset_deleted_emits_tombstones() {
        let rows = project_asset_deleted("asset-del", &["temp".into(), "hum".into()]);
        assert_eq!(rows.len(), 3, "asset + 2 attribute tombstones");
        for row in &rows {
            assert_eq!(row.payload["_deleted"], json!(true));
            assert_eq!(row.payload["data"], json!({}));
        }
        assert_eq!(rows[0].collection, COLLECTION_ASSETS);
        assert_eq!(rows[0].record_id, "asset-del");
        assert_eq!(rows[1].record_id, "asset-del:temp");
        assert_eq!(rows[2].record_id, "asset-del:hum");
    }

    #[test]
    fn project_agent_status_defaults_to_unconfigured() {
        let tmp = tempfile::tempdir().unwrap();
        // Open the core db (no stub tables created yet) — reader tolerates absence.
        let conn = store::open_iot_store(tmp.path()).unwrap();
        let rows = project_agent_status(&conn, "agent-1").unwrap();
        assert_eq!(rows.len(), 1);
        let p = &rows[0].payload;
        assert_eq!(rows[0].collection, COLLECTION_AGENT_STATUS);
        assert_eq!(p["agent_id"], json!("agent-1"));
        assert_eq!(
            p["link_state"],
            json!("unconfigured"),
            "Phase-2 default link state"
        );
        assert_eq!(p["status_key"], json!("unconfigured"));
    }

    #[test]
    fn project_all_is_idempotent_and_covers_seeded_rows() {
        let tmp = tempfile::tempdir().unwrap();
        // alarms::open ensures the iot_alarms schema on the same core db; the
        // asset/attribute/datapoint schema is ensured by open_iot_store. Both
        // target runtime/ctox.sqlite3, so a single connection sees all tables.
        alarms::open(tmp.path()).unwrap();
        let conn = store::open_iot_store(tmp.path()).unwrap();
        seed_thermostat(&conn, "asset-all-1");
        // An alarm in the same core db.
        let _alarm = alarms::create(
            &conn,
            alarms::NewAlarm {
                realm: "master".into(),
                title: "t".into(),
                content: None,
                severity: alarms::Severity::Low,
                assignee_id: None,
                source: alarms::Source::Manual,
                source_id: "src".into(),
            },
            vec![],
        )
        .unwrap();

        let first = project_all(&conn).unwrap();
        // asset_type(1) + asset(1) + attribute(1) + alarm(1) = 4 rows minimum.
        assert!(
            first.len() >= 4,
            "expected at least 4 projection rows, got {}",
            first.len()
        );
        let collections: std::collections::BTreeSet<&str> =
            first.iter().map(|r| r.collection).collect();
        assert!(collections.contains(COLLECTION_ASSET_TYPES));
        assert!(collections.contains(COLLECTION_ASSETS));
        assert!(collections.contains(COLLECTION_ATTRIBUTES));
        assert!(collections.contains(COLLECTION_ALARMS));

        // Idempotent: re-running yields the same set of (collection, record_id).
        let second = project_all(&conn).unwrap();
        let key = |r: &ProjectionRow| (r.collection, r.record_id.clone());
        let mut a: Vec<_> = first.iter().map(key).collect();
        let mut b: Vec<_> = second.iter().map(key).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "project_all is idempotent on (collection, record_id)");
    }
}
