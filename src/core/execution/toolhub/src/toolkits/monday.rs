//! Monday toolkit (monday.com GraphQL API v2).
//!
//! Modelled on the Activepieces `piece-monday` (MIT). Every action of the piece goes
//! through one POST to the monday GraphQL endpoint with `{query, variables}`:
//! - the classic actions (`src/lib/actions/*.ts`) use `mondayClient.<method>()` from
//!   `src/lib/common/client.ts`, which calls `monday-sdk-js` `client.api(query, …)`;
//! - the `src/lib/actions/ai/**` actions use `mondayClient.query({query, variables})`.
//! `monday-sdk-js` sends that call to `https://api.monday.com/v2` with the personal
//! token verbatim in the `Authorization` header and the pinned `API-Version` header.
//!
//! Auth: `PieceAuth.SecretText` ("API v2 Token", `src/lib/auth.ts`) — a personal
//! monday API token, sent unchanged as `Authorization: <token>` (no `Bearer` prefix).
//! monday tokens are not scope-based, so every tool declares no scopes.
//!
//! The two source families pin different API versions and the port keeps that:
//! `client.setApiVersion('2023-10')` for the classic actions and
//! `MONDAY_AI_API_VERSION = '2026-07'` for the `ai/` actions.
//!
//! Every id travels in a GraphQL *variable*, never in the URL path, so there are no
//! path parameters. Ids coming from tool args are still pushed through the
//! `path_segment` character guard (as the Linear port does) before they are
//! interpolated into a query, and every free-text value is JSON-escaped into the
//! POST body.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "monday";

/// Monday does not scope personal API tokens; read and write rights are account wide.
const SCOPES: &[&str] = &[];

/// `client.setApiVersion('2023-10')` in `src/lib/common/client.ts`.
const API_VERSION_CLASSIC: &str = "2023-10";
/// `MONDAY_AI_API_VERSION` in `src/lib/common/client.ts`.
const API_VERSION_AI: &str = "2026-07";

/// `ITEM_SUMMARY_FIELDS` in `src/lib/actions/ai/items/item-common.ts`.
const ITEM_SUMMARY_FIELDS: &str = "id name state url board { id } group { id }";
/// `ITEM_WITH_VALUES_FIELDS` in the same file.
const ITEM_WITH_VALUES_FIELDS: &str = "id name state url created_at updated_at board { id } group { id title } column_values(ids: $columnIds) { id type text value }";

/// Column value selections shared by the two classic read actions
/// (`mondayGraphQLQueries.getItemColumnValues` / `getBoardItemValues`).
const LEGACY_COLUMN_VALUES: &str = "id type value text ... on ButtonValue{ label } ... on StatusValue{ label } ... on VoteValue{ vote_count } ... on TagsValue{ tags{ name } } ... on BoardRelationValue { linked_item_ids } ... on DependencyValue { linked_item_ids } ... on WeekValue { start_date end_date }";

// ---------------------------------------------------------------- schema fragments

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn choices(desc: &str, options: &[&str]) -> Value {
    json!({"type": "array", "description": desc, "items": {"type": "string", "enum": options}})
}

/// Optional whole number (no schema default, so an unset value stays unset).
fn num_opt(desc: &str, min: u64, max: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max})
}

fn num(desc: &str, min: u64, max: u64, default: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max, "default": default})
}

fn json_obj(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn ids(desc: &str) -> Value {
    str_list(desc)
}

/// Board/workspace/item id props (`mondayAiProps.*` are plain ShortText).
fn board_id(desc: &str) -> Value {
    s(desc)
}

// ---------------------------------------------------------------- id guards

/// `path_segment` charset, applied to optional/array ids as well.
fn check_segment(v: &str, key: &str) -> Result<String> {
    let v = v.trim();
    if v.is_empty()
        || v.len() > 200
        || v == "."
        || v == ".."
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        bail!("invalid argument `{key}`");
    }
    Ok(v.to_string())
}

fn seg(args: &Value, key: &str) -> Result<String> {
    Ok(path_segment(args, key)?.to_string())
}

fn seg_opt(args: &Value, key: &str) -> Result<Option<String>> {
    match non_blank(args, key) {
        Some(v) => Ok(Some(check_segment(v, key)?)),
        None => Ok(None),
    }
}

fn seg_list(args: &Value, key: &str) -> Result<Vec<String>> {
    str_vec(args, key)
        .iter()
        .map(|v| check_segment(v, key))
        .collect()
}

fn non_blank<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn opt_u64_or_err(args: &Value, key: &str) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("`{key}` must be a whole number")),
    }
}

fn put(m: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(v) = value {
        m.insert(key.into(), json!(v));
    }
}

fn opt_str_owned(args: &Value, key: &str) -> Option<String> {
    non_blank(args, key).map(String::from)
}

fn bool_or(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

// ---------------------------------------------------------------- response helpers

/// monday GraphQL envelope → `data`, with `errors` / `error_message` surfaced.
/// Mirrors `extractErrorMessage` + `unwrap` in `src/lib/common/client.ts`.
fn data(response: &HttpResponse) -> Result<Value> {
    let v = response.json()?;
    if let Some(errors) = v["errors"].as_array() {
        if !errors.is_empty() {
            let msg = errors
                .iter()
                .filter_map(|e| e["message"].as_str())
                .collect::<Vec<_>>()
                .join("; ");
            bail!("monday.com API error: {}", if msg.is_empty() { "unknown" } else { &msg });
        }
    }
    if let Some(m) = v.get("error_message") {
        if !m.is_null() {
            bail!("monday.com API error: {}", m.as_str().unwrap_or("unknown"));
        }
    }
    match v.get("data") {
        Some(d) if !d.is_null() => Ok(d.clone()),
        _ => bail!("monday.com API returned no data."),
    }
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn one<'a>(d: &'a Value, key: &str) -> Result<&'a Value> {
    match d.get(key) {
        Some(v) if v.is_object() => Ok(v),
        _ => bail!("monday.com returned no `{key}`."),
    }
}

fn first_board<'a>(d: &'a Value) -> Result<&'a Value> {
    match list(&d["boards"]).first() {
        Some(b) if b.is_object() => Ok(b),
        _ => bail!("monday.com returned no board (not found or not accessible)."),
    }
}

fn join(items: &[Value], key: &str) -> Value {
    let parts: Vec<&str> = items.iter().filter_map(|i| i[key].as_str()).collect();
    if parts.is_empty() {
        Value::Null
    } else {
        json!(parts.join(", "))
    }
}

// ---------------------------------------------------------------- mappers

/// `itemCommon.mapItemSummary`.
fn map_item_summary(i: &Value) -> Value {
    json!({
        "id": i["id"],
        "name": i["name"],
        "state": i["state"],
        "url": i["url"],
        "board_id": i["board"]["id"],
        "group_id": i["group"]["id"],
    })
}

/// `itemCommon.mapItemWithValues`.
fn map_item_with_values(i: &Value) -> Value {
    let mut v = map_item_summary(i);
    v["group_title"] = i["group"]["title"].clone();
    v["created_at"] = i["created_at"].clone();
    v["updated_at"] = i["updated_at"].clone();
    v["column_values"] = Value::Array(
        list(&i["column_values"])
            .iter()
            .map(|cv| {
                json!({"column_id": cv["id"], "type": cv["type"], "text": cv["text"], "value": cv["value"]})
            })
            .collect(),
    );
    v
}

fn map_user(u: &Value) -> Value {
    json!({
        "id": u["id"], "name": u["name"], "email": u["email"], "title": u["title"],
        "kind": u["user_config"]["kind"], "status": u["status"], "url": u["url"],
        "time_zone": u["time_zone_identifier"], "country_code": u["country_code"],
        "location": u["location"], "phone": u["phone"], "created_at": u["created_at"],
    })
}

fn map_workspace(w: &Value) -> Value {
    json!({
        "id": w["id"], "name": w["name"], "kind": w["kind"], "description": w["description"],
        "state": w["state"], "created_at": w["created_at"],
        "is_default_workspace": if w["is_default_workspace"].is_boolean() {
            w["is_default_workspace"].clone()
        } else {
            json!(false)
        },
    })
}

fn map_board(b: &Value) -> Value {
    let owners = list(&b["owners"]);
    json!({
        "id": b["id"], "name": b["name"], "description": b["description"], "state": b["state"],
        "board_kind": b["board_kind"], "url": b["url"], "workspace_id": b["workspace_id"],
        "folder_id": b["board_folder_id"], "items_count": b["items_count"],
        "updated_at": b["updated_at"],
        "owner_ids": join(&owners, "id"), "owner_names": join(&owners, "name"),
    })
}

fn map_update(u: &Value, item_id: Value) -> Value {
    json!({
        "id": u["id"],
        "item_id": item_id,
        "body": u["body"],
        "text_body": u["text_body"],
        "creator_id": u["creator_id"],
        "creator_name": u["creator"]["name"],
        "created_at": u["created_at"],
        "updated_at": u["updated_at"],
    })
}

// ------------------------------------------------- `parseMondayColumnValue` (helper.ts)

/// `JSON.parse` that yields `null` instead of throwing (the piece lets the throw escape).
fn jparse(raw: &Value) -> Option<Value> {
    match raw {
        Value::String(s) => serde_json::from_str::<Value>(s).ok(),
        Value::Null => None,
        other => Some(other.clone()),
    }
}

fn arr_or_empty(raw: &Value, key: &str) -> Vec<Value> {
    raw.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

/// `parseMondayColumnValue` from `src/lib/common/helper.ts`.
fn parse_column_value(cv: &Value) -> Value {
    let value = jparse(&cv["value"]);
    match cv["type"].as_str().unwrap_or("") {
        "button" | "status" => cv["label"].clone(),
        "checkbox" => value
            .as_ref()
            .and_then(|v| v.get("checked"))
            .cloned()
            .unwrap_or(json!(false)),
        "board_relation" => {
            if cv.get("linked_item_ids").is_some() {
                cv["linked_item_ids"].clone()
            } else {
                json!([])
            }
        }
        "dependency" => jparse(&cv["linked_item_ids"]).unwrap_or(json!([])),
        "subtasks" => {
            let ids: Vec<Value> = value
                .as_ref()
                .map(|v| arr_or_empty(v, "linkedPulseIds"))
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p.get("linkedPulseId").cloned())
                .collect();
            json!(ids)
        }
        "color_picker" => value
            .as_ref()
            .and_then(|v| v.pointer("/color/hex"))
            .cloned()
            .unwrap_or(Value::Null),
        "country" => value
            .as_ref()
            .and_then(|v| v.get("countryName"))
            .cloned()
            .unwrap_or(Value::Null),
        "creation_log" => value
            .as_ref()
            .and_then(|v| v.get("created_at"))
            .cloned()
            .unwrap_or(Value::Null),
        "date" => match &value {
            None => Value::Null,
            Some(v) => json!(format!(
                "{} {}",
                v["date"].as_str().unwrap_or(""),
                v["time"].as_str().unwrap_or("")
            )
            .trim()),
        },
        "doc" => value
            .as_ref()
            .and_then(|v| v.get("files"))
            .and_then(Value::as_array)
            .and_then(|f| f.first())
            .and_then(|f| f.get("linkToFile"))
            .cloned()
            .unwrap_or(Value::Null),
        "dropdown" => value
            .as_ref()
            .and_then(|v| v.get("ids"))
            .cloned()
            .unwrap_or(json!([])),
        "email" => value
            .as_ref()
            .and_then(|v| v.get("email"))
            .cloned()
            .unwrap_or(Value::Null),
        "file" => cv["text"].clone(),
        "hour" => match &value {
            None => Value::Null,
            Some(v) => json!(format!("{}:{}", v["hour"].as_i64().unwrap_or(0), v["minute"].as_i64().unwrap_or(0))),
        },
        "item_id" => value
            .as_ref()
            .and_then(|v| v.get("item_id"))
            .cloned()
            .unwrap_or(Value::Null),
        "last_updated" => value
            .as_ref()
            .and_then(|v| v.get("updated_at"))
            .cloned()
            .unwrap_or(Value::Null),
        "link" => value
            .as_ref()
            .and_then(|v| v.get("url"))
            .cloned()
            .unwrap_or(Value::Null),
        "location" => value
            .as_ref()
            .and_then(|v| v.get("address"))
            .cloned()
            .unwrap_or(Value::Null),
        "long_text" => value
            .as_ref()
            .and_then(|v| v.get("text"))
            .cloned()
            .unwrap_or(Value::Null),
        "mirror" => Value::Null,
        "numbers" => cv["value"]
            .as_str()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map_or(Value::Null, |n| json!(n)),
        "people" => {
            let ids: Vec<Value> = value
                .as_ref()
                .map(|v| arr_or_empty(v, "personsAndTeams"))
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p.get("id").cloned())
                .collect();
            json!(ids)
        }
        "phone" => value
            .as_ref()
            .and_then(|v| v.get("phone"))
            .cloned()
            .unwrap_or(Value::Null),
        "rating" => value
            .as_ref()
            .and_then(|v| v.get("rating"))
            .cloned()
            .unwrap_or(Value::Null),
        "tags" => Value::Array(
            list(&cv["tags"])
                .iter()
                .filter_map(|t| t.get("name").cloned())
                .collect(),
        ),
        "text" => value.unwrap_or(Value::Null),
        "timeline" => match &value {
            None => Value::Null,
            Some(v) => json!({"from": v["from"], "to": v["to"]}),
        },
        "time_tracking" => value
            .as_ref()
            .and_then(|v| v.get("duration"))
            .cloned()
            .unwrap_or(Value::Null),
        "vote" => {
            if cv.get("vote_count").is_some() {
                cv["vote_count"].clone()
            } else {
                json!(0)
            }
        }
        "week" => json!({"start_date": cv["start_date"], "end_date": cv["end_date"]}),
        "world_clock" => value
            .as_ref()
            .and_then(|v| v.get("timezone"))
            .cloned()
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// `{id, name, <column id>: parsed}` as `get-board-values.ts` / `get-column-values.ts`
/// build it.
fn item_with_parsed_columns(item: &Value) -> Value {
    let mut out = Map::new();
    out.insert("id".into(), item["id"].clone());
    out.insert("name".into(), item["name"].clone());
    for cv in list(&item["column_values"]) {
        let Some(id) = cv["id"].as_str() else {
            continue;
        };
        out.insert(id.to_string(), parse_column_value(cv));
    }
    Value::Object(out)
}

// ---------------------------------------------------------------- search specs

/// `SEARCH_SPECS` in `src/lib/actions/ai/other/search.ts`.
struct SearchSpec {
    variable_defs: &'static str,
    args: &'static str,
    fields: &'static str,
    supports_boards: bool,
    supports_workspaces: bool,
}

fn search_spec(entity_type: &str) -> Option<SearchSpec> {
    let (variable_defs, args, fields, supports_boards, supports_workspaces) = match entity_type {
        "items" => (
            "$query: String!, $limit: Int, $board_ids: [ID!], $workspace_ids: [ID!]",
            "query: $query, limit: $limit, board_ids: $board_ids, workspace_ids: $workspace_ids",
            "name url board_id workspace_id",
            true,
            true,
        ),
        "boards" => (
            "$query: String!, $limit: Int, $board_ids: [ID!], $workspace_ids: [ID!]",
            "query: $query, limit: $limit, board_ids: $board_ids, workspace_ids: $workspace_ids",
            "name url description workspace_id",
            true,
            true,
        ),
        "docs" => (
            "$query: String!, $limit: Int, $workspace_ids: [ID!]",
            "query: $query, limit: $limit, workspace_ids: $workspace_ids",
            "name workspace_id",
            false,
            true,
        ),
        "workspaces" => (
            "$query: String!, $limit: Int",
            "query: $query, limit: $limit",
            "name description",
            false,
            false,
        ),
        _ => return None,
    };
    Some(SearchSpec {
        variable_defs,
        args,
        fields,
        supports_boards,
        supports_workspaces,
    })
}

// ---------------------------------------------------------------- adapter

pub struct Monday {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Monday {
    fn default() -> Self {
        Self::with_api_base("https://api.monday.com/v2")
    }
}

/// Column types `COLUMN_TYPE_OPTIONS` in `src/lib/common/constants.ts` offers.
const COLUMN_TYPES: &[&str] = &[
    "auto_number", "color_picker", "checkbox", "country", "creation_log", "date", "dropdown",
    "email", "hour", "item_id", "last_updated", "link", "location", "long_text", "numbers",
    "people", "phone", "progress", "rating", "status", "tags", "text", "timeline", "time_tracking",
    "vote", "week", "world_clock",
];

#[allow(clippy::too_many_lines)]
impl Monday {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, SCOPES, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "monday.com".into(),
            description: LocalizedText::new(
                "Boards, Items, Spalten, Gruppen, Updates, Nutzer und Workspaces in monday.com lesen und bearbeiten.",
                "Read and change boards, items, columns, groups, updates, users and workspaces in monday.com.",
            ),
            // monday personal API tokens go into `Authorization` verbatim — no `Bearer`.
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "authorization".into(),
                prefix: String::new(),
            }],
            tools: vec![
                // ------------------------------------------------------- users / account
                t(
                    "MONDAY_GET_ME",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    ("Liest das Konto hinter dem API-Token.", "Reads the account behind the API token."),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "MONDAY_GET_ACCOUNT",
                    ("Konto lesen", "Get account"),
                    ("Liest Konto, Tarif und Produkte.", "Reads the account, its plan and its products."),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "MONDAY_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet Kontonutzer, optional nach E-Mail, Name, Art oder Status.",
                        "Lists account users, optionally by email, name, kind or status.",
                    ),
                    &[
                        ("emails", ids("Nur Nutzer mit genau diesen E-Mail-Adressen")),
                        ("user_ids", ids("Nur diese Nutzer-IDs")),
                        ("name", s("Unscharfe Suche im Nutzernamen")),
                        ("user_kind", choice("Nur Nutzer dieser Art", &["BASIC", "ADMIN", "MEMBER", "GUEST", "VIEW_ONLY"])),
                        ("status", choices("Nur Nutzer in diesen Status", &["ACTIVE", "PENDING", "INACTIVE"])),
                        ("limit", num_opt("Nutzer pro Seite (1-1000).", 1, 1000)),
                        ("page", num_opt("Seitenzahl, 1-basiert.", 1, 100)),
                    ],
                    &[],
                    false,
                ),
                t(
                    "MONDAY_LIST_TEAMS",
                    ("Teams auflisten", "List teams"),
                    (
                        "Listet Teams mit Mitgliedern und Eigentümern.",
                        "Lists teams with their members and owners.",
                    ),
                    &[("team_ids", ids("Nur diese Team-IDs; leer = alle sichtbaren"))],
                    &[],
                    false,
                ),
                // ------------------------------------------------------- workspaces
                t(
                    "MONDAY_LIST_WORKSPACES",
                    ("Workspaces auflisten", "List workspaces"),
                    (
                        "Listet Workspaces, optional nach Art, Status oder Mitgliedschaft.",
                        "Lists workspaces, optionally by kind, state or membership.",
                    ),
                    &[
                        ("workspace_ids", ids("Nur diese Workspace-IDs")),
                        ("kind", choice("Nur offene oder geschlossene Workspaces", &["open", "closed"])),
                        ("state", choice("Status der Workspaces", &["active", "archived", "deleted", "all"])),
                        ("membership_kind", choice("Nur eigene Workspaces oder alle sichtbaren", &["all", "member"])),
                        ("limit", num("Workspaces pro Seite", 1, 100, 50)),
                        ("page", num("Seitenzahl, 1-basiert", 1, 100, 1)),
                    ],
                    &[],
                    false,
                ),
                // ------------------------------------------------------- boards
                t(
                    "MONDAY_LIST_BOARDS",
                    ("Boards auflisten", "List boards"),
                    (
                        "Listet erreichbare Boards mit Workspace, Ordnern und Eigentümern.",
                        "Lists reachable boards with workspace, folders and owners.",
                    ),
                    &[
                        ("workspace_ids", ids("Nur Boards in diesen Workspaces")),
                        ("board_ids", ids("Nur diese Boards")),
                        ("state", choice("Status der Boards", &["active", "archived", "deleted", "all"])),
                        ("board_kind", choice("Sichtbarkeit der Boards", &["public", "private", "share"])),
                        ("limit", num("Boards pro Seite", 1, 100, 50)),
                        ("page", num("Seitenzahl, 1-basiert", 1, 100, 1)),
                    ],
                    &[],
                    false,
                ),
                t(
                    "MONDAY_LIST_BOARD_VIEWS",
                    ("Board-Ansichten auflisten", "List board views"),
                    (
                        "Listet die gespeicherten Ansichten eines Boards.",
                        "Lists the saved views of a board.",
                    ),
                    &[("board_id", board_id("Board-ID, z. B. 1234567890"))],
                    &["board_id"],
                    false,
                ),
                // ------------------------------------------------------- columns
                t(
                    "MONDAY_LIST_COLUMNS",
                    ("Spalten auflisten", "List columns"),
                    (
                        "Listet die Spalten eines Boards samt Typ und Einstellungen.",
                        "Lists a board's columns with type and settings.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("column_ids", str_list("Nur diese Spalten-IDs, z. B. status, date4")),
                    ],
                    &["board_id"],
                    false,
                ),
                t(
                    "MONDAY_GET_COLUMN_TYPE_SCHEMA",
                    ("Spaltentyp-Schema lesen", "Get column type schema"),
                    (
                        "Liest das JSON-Schema eines monday-Spaltentyps.",
                        "Reads the JSON schema of a monday column type.",
                    ),
                    &[("column_type", s("Spaltentyp, z. B. status oder numbers"))],
                    &["column_type"],
                    false,
                ),
                // ------------------------------------------------------- groups
                t(
                    "MONDAY_LIST_GROUPS",
                    ("Gruppen auflisten", "List groups"),
                    (
                        "Listet die Gruppen (Abschnitte) eines Boards.",
                        "Lists the groups (sections) of a board.",
                    ),
                    &[("board_id", board_id("Board-ID"))],
                    &["board_id"],
                    false,
                ),
                // ------------------------------------------------------- items
                t(
                    "MONDAY_LIST_BOARD_ITEMS",
                    ("Board-Items auflisten", "List board items"),
                    (
                        "Listet Items eines Boards, einer Gruppe oder die nächste Cursor-Seite.",
                        "Lists the items of a board, of one group, or the next cursor page.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID; bei `cursor` wird es nicht benutzt")),
                        ("group_id", s("Nur diese Gruppe, z. B. topics")),
                        ("cursor", s("Cursor aus dem vorherigen Aufruf für die nächste Seite")),
                        ("query_params", json_obj("ItemsQuery als JSON, wird 1:1 an monday weitergereicht")),
                        ("limit", num_opt("Items pro Seite (1-500).", 1, 500)),
                        ("column_ids", str_list("Nur diese Spalten-IDs in den Spaltenwerten")),
                    ],
                    &[],
                    false,
                ),
                t(
                    "MONDAY_GET_ITEMS",
                    ("Items lesen", "Get items"),
                    (
                        "Liest bis zu 100 Items direkt über ihre IDs.",
                        "Reads up to 100 items directly by their ids.",
                    ),
                    &[("item_ids", ids("Item-IDs (höchstens 100)"))],
                    &["item_ids"],
                    false,
                ),
                t(
                    "MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES",
                    ("Items nach Spaltenwerten suchen", "Search items by column values"),
                    (
                        "Findet Items, deren Spaltenwerte den Filtern entsprechen.",
                        "Finds items whose column values match the given filters.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("filters", json!({"type": "array", "description": "Filter als Liste von {\"column_id\": \"status\", \"value\": \"Done\"}",
                            "items": {"type": "object"}})),
                        ("cursor", s("Cursor aus dem vorherigen Aufruf für die nächste Seite")),
                        ("limit", num_opt("Items pro Seite (1-500).", 1, 500)),
                        ("column_ids", str_list("Nur diese Spalten-IDs in den Spaltenwerten")),
                    ],
                    &["board_id"],
                    false,
                ),
                t(
                    "MONDAY_LIST_SUBITEMS",
                    ("Subitems auflisten", "List subitems"),
                    (
                        "Listet die Subitems eines Items.",
                        "Lists the subitems of an item.",
                    ),
                    &[
                        ("item_id", s("ID des Eltern-Items")),
                        ("column_ids", str_list("Nur diese Spalten-IDs in den Spaltenwerten")),
                    ],
                    &["item_id"],
                    false,
                ),
                t(
                    "MONDAY_SEARCH",
                    ("Suchen", "Search"),
                    (
                        "Volltextsuche über Items, Boards, Docs oder Workspaces.",
                        "Full-text search across items, boards, docs or workspaces.",
                    ),
                    &[
                        ("entity_type", choice("Wonach gesucht wird", &["items", "boards", "docs", "workspaces"])),
                        ("query", s("Suchbegriff")),
                        ("limit", num_opt("Höchstzahl Treffer (1-20).", 1, 20)),
                        ("board_ids", ids("Nur diese Boards (items, boards)")),
                        ("workspace_ids", ids("Nur diese Workspaces (items, boards, docs)")),
                    ],
                    &["entity_type", "query"],
                    false,
                ),
                // ------------------------------------------------------- updates
                t(
                    "MONDAY_LIST_ITEM_UPDATES",
                    ("Item-Updates auflisten", "List item updates"),
                    (
                        "Listet die Updates (Kommentare) eines Items mit Antworten.",
                        "Lists the updates (comments) of an item including their replies.",
                    ),
                    &[
                        ("item_id", s("Item-ID")),
                        ("limit", num("Updates pro Seite", 1, 100, 25)),
                        ("page", num("Seitenzahl, 1-basiert", 1, 100, 1)),
                    ],
                    &["item_id"],
                    false,
                ),
                t(
                    "MONDAY_LIST_BOARD_UPDATES",
                    ("Board-Updates auflisten", "List board updates"),
                    (
                        "Listet die Updates aller Items eines Boards.",
                        "Lists the updates of every item on a board.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("from_date", s("Nur Updates ab diesem ISO-Zeitpunkt")),
                        ("to_date", s("Nur Updates bis zu diesem ISO-Zeitpunkt")),
                        ("limit", num("Updates pro Seite", 1, 100, 25)),
                        ("page", num("Seitenzahl, 1-basiert", 1, 100, 1)),
                    ],
                    &["board_id"],
                    false,
                ),
                // ------------------------------------------------------- folders
                t(
                    "MONDAY_LIST_FOLDERS",
                    ("Ordner auflisten", "List folders"),
                    (
                        "Listet Board-Ordner mit Unterordnern und enthaltenen Boards.",
                        "Lists board folders with their subfolders and contained boards.",
                    ),
                    &[
                        ("folder_ids", ids("Nur diese Ordner-IDs")),
                        ("workspace_ids", ids("Nur Ordner in diesen Workspaces")),
                        ("limit", num("Ordner pro Seite", 1, 100, 25)),
                        ("page", num("Seitenzahl, 1-basiert", 1, 100, 1)),
                    ],
                    &[],
                    false,
                ),
                // ------------------------------------- classic read actions (2023-10)
                t(
                    "MONDAY_GET_BOARD_VALUES",
                    ("Board-Werte lesen", "Get board values"),
                    (
                        "Liest alle Items eines Boards mit ihren (gefilterten) Spaltenwerten.",
                        "Reads every item of a board with its (filtered) column values.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("column_ids", str_list("Nur diese Spalten-IDs; leer = alle")),
                    ],
                    &["board_id"],
                    false,
                ),
                t(
                    "MONDAY_GET_ITEM_COLUMN_VALUES",
                    ("Item-Spaltenwerte lesen", "Get item column values"),
                    (
                        "Liest die Spaltenwerte eines einzelnen Items.",
                        "Reads the column values of a single item.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("item_id", s("Item-ID")),
                        ("column_ids", str_list("Nur diese Spalten-IDs; leer = alle")),
                    ],
                    &["board_id", "item_id"],
                    false,
                ),
                // ------------------------------------------------------- writes
                t(
                    "MONDAY_CREATE_BOARD_ITEM",
                    ("Item anlegen", "Create board item"),
                    (
                        "Legt ein Item auf einem Board an; braucht --confirm.",
                        "Creates an item on a board; requires --confirm.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("item_name", s("Name des Items")),
                        ("group_id", s("Gruppe, z. B. topics")),
                        ("column_values", json_obj("column_values als JSON-Objekt, z. B. {\"status\": {\"label\": \"Working on it\"}}")),
                        ("create_labels_if_missing", flag("Fehlende Status-/Dropdown-Labels anlegen (braucht Rechte auf die Boardstruktur)", false)),
                    ],
                    &["board_id", "item_name"],
                    true,
                ),
                t(
                    "MONDAY_RENAME_ITEM",
                    ("Item umbenennen", "Rename item"),
                    (
                        "Ändert nur den Namen eines Items; braucht --confirm.",
                        "Changes only the name of an item; requires --confirm.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("item_id", s("Item-ID")),
                        ("name", s("Neuer Name")),
                    ],
                    &["board_id", "item_id", "name"],
                    true,
                ),
                t(
                    "MONDAY_SET_ITEM_COLUMN_VALUES",
                    ("Item-Spaltenwerte setzen", "Set item column values"),
                    (
                        "Setzt mehrere Spaltenwerte eines Items; braucht --confirm.",
                        "Sets several column values on an item; requires --confirm.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("item_id", s("Item-ID")),
                        ("column_values", json_obj("column_values als JSON-Objekt, z. B. {\"status\": {\"label\": \"Done\"}}")),
                        ("create_labels_if_missing", flag("Fehlende Labels anlegen", false)),
                    ],
                    &["board_id", "item_id", "column_values"],
                    true,
                ),
                t(
                    "MONDAY_ARCHIVE_ITEM",
                    ("Item archivieren", "Archive item"),
                    ("Archiviert ein Item; braucht --confirm.", "Archives an item; requires --confirm."),
                    &[("item_id", s("Item-ID"))],
                    &["item_id"],
                    true,
                ),
                t(
                    "MONDAY_CREATE_SUBITEM",
                    ("Subitem anlegen", "Create subitem"),
                    (
                        "Legt ein Subitem unter einem Item an; braucht --confirm.",
                        "Creates a subitem under an item; requires --confirm.",
                    ),
                    &[
                        ("parent_item_id", s("ID des Eltern-Items")),
                        ("item_name", s("Name des Subitems")),
                        ("column_values", json_obj("column_values als JSON-Objekt")),
                        ("create_labels_if_missing", flag("Fehlende Labels anlegen", false)),
                    ],
                    &["parent_item_id", "item_name"],
                    true,
                ),
                t(
                    "MONDAY_POST_ITEM_UPDATE",
                    ("Update schreiben", "Post item update"),
                    (
                        "Postet ein Update auf ein Item oder eine Antwort auf ein Update.",
                        "Posts an update on an item or a reply to an update.",
                    ),
                    &[
                        ("item_id", s("Item-ID; alternativ parent_id verwenden")),
                        ("parent_id", s("Update-ID, auf das geantwortet wird")),
                        ("body", s("Text des Updates")),
                    ],
                    &["body"],
                    true,
                ),
                t(
                    "MONDAY_SEND_NOTIFICATION",
                    ("Benachrichtigung senden", "Send notification"),
                    (
                        "Schickt eine In-App-Benachrichtigung an einen Nutzer.",
                        "Sends an in-app notification to a user.",
                    ),
                    &[
                        ("user_id", s("Nutzer-ID, der benachrichtigt wird")),
                        ("target_id", s("ID des Ziels (Item, Board, Update …)")),
                        ("target_type", choice("Art des Ziels", &["item", "board", "update"])),
                        ("text", s("Text der Benachrichtigung")),
                    ],
                    &["user_id", "target_id", "target_type", "text"],
                    true,
                ),
                // ------------------------------------- classic write actions (2023-10)
                t(
                    "MONDAY_CREATE_COLUMN",
                    ("Spalte anlegen", "Create column"),
                    (
                        "Legt eine Spalte in einem Board an; braucht --confirm.",
                        "Creates a column on a board; requires --confirm.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("column_title", s("Titel der Spalte")),
                        ("column_type", choice("Typ der Spalte", COLUMN_TYPES)),
                    ],
                    &["board_id", "column_title", "column_type"],
                    true,
                ),
                t(
                    "MONDAY_CREATE_GROUP",
                    ("Gruppe anlegen", "Create group"),
                    (
                        "Legt eine Gruppe in einem Board an; braucht --confirm.",
                        "Creates a group on a board; requires --confirm.",
                    ),
                    &[
                        ("board_id", board_id("Board-ID")),
                        ("group_name", s("Name der Gruppe")),
                    ],
                    &["board_id", "group_name"],
                    true,
                ),
                t(
                    "MONDAY_CREATE_UPDATE",
                    ("Update anlegen", "Create update"),
                    (
                        "Postet ein Update auf ein Item; braucht --confirm.",
                        "Posts an update on an item; requires --confirm.",
                    ),
                    &[
                        ("item_id", s("Item-ID")),
                        ("body", s("Text des Updates")),
                    ],
                    &["item_id", "body"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self) -> String {
        self.api_base.clone()
    }

    /// `(API-Version header, GraphQL document)` for `tool`.
    fn document(&self, tool: &str, args: &Value) -> Result<(&'static str, String)> {
        let q = match tool {
            // AP: src/lib/actions/ai/users/get-me.ts
            "MONDAY_GET_ME" => (
                API_VERSION_AI,
                "query { me { id name email title status url time_zone_identifier country_code created_at user_config { kind } photo_url { small } account { id name slug } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/users/get-account.ts
            "MONDAY_GET_ACCOUNT" => (
                API_VERSION_AI,
                "query { account { id name slug tier country_code active_members_count first_day_of_the_week is_during_trial is_trial_expired show_timeline_weekends plan { tier period max_users version } products { id kind tier } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/users/list-users.ts
            "MONDAY_LIST_USERS" => (
                API_VERSION_AI,
                "query ($emails: [String!], $ids: [ID!], $name: String, $user_kind: UserKindFilterInput, $status: [UserStatus!], $limit: Int, $page: Int) { users(emails: $emails, ids: $ids, name: $name, user_kind: $user_kind, status: $status, limit: $limit, page: $page) { id name email title status url time_zone_identifier country_code location phone created_at user_config { kind } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/users/list-teams.ts
            "MONDAY_LIST_TEAMS" => (
                API_VERSION_AI,
                "query ($ids: [ID!]) { teams(ids: $ids) { id name picture_url users { id name email } owners { id name } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/workspaces/list-workspaces.ts
            "MONDAY_LIST_WORKSPACES" => (
                API_VERSION_AI,
                "query ($ids: [ID!], $kind: WorkspaceKind, $state: State, $membership_kind: WorkspaceMembershipKind, $limit: Int, $page: Int) { workspaces(ids: $ids, kind: $kind, state: $state, membership_kind: $membership_kind, limit: $limit, page: $page) { id name kind description state created_at is_default_workspace } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/boards/list-boards.ts
            "MONDAY_LIST_BOARDS" => (
                API_VERSION_AI,
                "query ($ids: [ID!], $workspace_ids: [ID], $state: State, $board_kind: BoardKind, $limit: Int, $page: Int) { boards(ids: $ids, workspace_ids: $workspace_ids, state: $state, board_kind: $board_kind, limit: $limit, page: $page) { id name description state board_kind url workspace_id board_folder_id items_count updated_at owners { id name } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/boards/list-board-views.ts
            "MONDAY_LIST_BOARD_VIEWS" => (
                API_VERSION_AI,
                "query ($ids: [ID!]) { boards(ids: $ids) { views { id name type access_level source_view_id } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/columns/list-columns.ts
            "MONDAY_LIST_COLUMNS" => (
                API_VERSION_AI,
                "query ($boardIds: [ID!], $columnIds: [String]) { boards(ids: $boardIds) { id columns(ids: $columnIds) { id title type description settings archived width } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/columns/get-column-type-schema.ts
            "MONDAY_GET_COLUMN_TYPE_SCHEMA" => (
                API_VERSION_AI,
                "query ($type: ColumnType!) { get_column_type_schema(type: $type) }".to_string(),
            ),
            // AP: src/lib/actions/ai/groups/list-groups.ts
            "MONDAY_LIST_GROUPS" => (
                API_VERSION_AI,
                "query ($boardIds: [ID!]) { boards(ids: $boardIds) { id groups { id title color archived deleted } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/items/list-board-items.ts
            "MONDAY_LIST_BOARD_ITEMS" => {
                if non_blank(args, "cursor").is_some() {
                    (
                        API_VERSION_AI,
                        format!(
                            "query ($cursor: String!, $limit: Int!, $columnIds: [String!]) {{ next_items_page(cursor: $cursor, limit: $limit) {{ cursor items {{ {ITEM_WITH_VALUES_FIELDS} }} }} }}"
                        ),
                    )
                } else if non_blank(args, "group_id").is_some() {
                    (
                        API_VERSION_AI,
                        format!(
                            "query ($boardId: [ID!], $groupId: [String], $limit: Int!, $queryParams: ItemsQuery, $columnIds: [String!]) {{ boards(ids: $boardId) {{ groups(ids: $groupId) {{ items_page(limit: $limit, query_params: $queryParams) {{ cursor items {{ {ITEM_WITH_VALUES_FIELDS} }} }} }} }} }}"
                        ),
                    )
                } else {
                    (
                        API_VERSION_AI,
                        format!(
                            "query ($boardId: [ID!], $limit: Int!, $queryParams: ItemsQuery, $columnIds: [String!]) {{ boards(ids: $boardId) {{ items_page(limit: $limit, query_params: $queryParams) {{ cursor items {{ {ITEM_WITH_VALUES_FIELDS} }} }} }} }}"
                        ),
                    )
                }
            }
            // AP: src/lib/actions/ai/items/get-items.ts
            "MONDAY_GET_ITEMS" => (
                API_VERSION_AI,
                "query ($ids: [ID!]) { items(ids: $ids) { id name state created_at updated_at url board { id name } group { id title } parent_item { id } column_values { id type text value } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/items/search-items-by-column-values.ts
            "MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES" => (
                API_VERSION_AI,
                format!(
                    "query ($boardId: ID!, $limit: Int!, $cursor: String, $columns: [ItemsPageByColumnValuesQuery!], $columnIds: [String!]) {{ items_page_by_column_values(board_id: $boardId, limit: $limit, cursor: $cursor, columns: $columns) {{ cursor items {{ {ITEM_WITH_VALUES_FIELDS} }} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/list-subitems.ts
            "MONDAY_LIST_SUBITEMS" => (
                API_VERSION_AI,
                format!(
                    "query ($ids: [ID!], $columnIds: [String!]) {{ items(ids: $ids) {{ id subitems {{ {ITEM_WITH_VALUES_FIELDS} }} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/other/search.ts
            "MONDAY_SEARCH" => {
                let entity_type = req_str(args, "entity_type")?;
                let spec = search_spec(entity_type)
                    .ok_or_else(|| anyhow::anyhow!("Unsupported entity type: {entity_type}"))?;
                (
                    API_VERSION_AI,
                    format!(
                        "query ({}) {{ search {{ {entity_type}({}) {{ results {{ id indexed_data {{ {} }} }} }} }} }}",
                        spec.variable_defs, spec.args, spec.fields
                    ),
                )
            }
            // AP: src/lib/actions/ai/updates/list-item-updates.ts
            "MONDAY_LIST_ITEM_UPDATES" => (
                API_VERSION_AI,
                "query ($ids: [ID!], $limit: Int, $page: Int) { items(ids: $ids) { id updates(limit: $limit, page: $page) { id body text_body creator_id creator { name } created_at updated_at replies { id body text_body creator_id created_at } } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/updates/list-board-updates.ts
            "MONDAY_LIST_BOARD_UPDATES" => (
                API_VERSION_AI,
                "query ($ids: [ID!], $limit: Int, $page: Int, $fromDate: ISO8601DateTime, $toDate: ISO8601DateTime) { boards(ids: $ids) { id updates(limit: $limit, page: $page, from_date: $fromDate, to_date: $toDate) { id body text_body item_id creator_id creator { name } created_at updated_at } } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/folders/list-folders.ts
            "MONDAY_LIST_FOLDERS" => (
                API_VERSION_AI,
                "query ($ids: [ID!], $workspace_ids: [ID], $limit: Int, $page: Int) { folders(ids: $ids, workspace_ids: $workspace_ids, limit: $limit, page: $page) { id name color created_at owner_id workspace { id name } parent { id name } sub_folders { id name } children { id name } } }"
                    .to_string(),
            ),
            // AP: src/lib/common/queries.ts (mondayGraphQLQueries.getBoardItemValues)
            "MONDAY_GET_BOARD_VALUES" => (
                API_VERSION_CLASSIC,
                format!(
                    "query getItemColumnValues($boardId: ID!,$columnIds: [String!]) {{ boards(ids: [$boardId]) {{ items_page(query_params: {{order_by: {{column_id: \"__last_updated__\",direction: desc}}}}) {{ items{{ id name column_values(ids: $columnIds){{{LEGACY_COLUMN_VALUES}}} }} }} }} }}"
                ),
            ),
            // AP: src/lib/common/queries.ts (mondayGraphQLQueries.getItemColumnValues)
            "MONDAY_GET_ITEM_COLUMN_VALUES" => (
                API_VERSION_CLASSIC,
                format!(
                    "query getItemColumnValues($boardId: ID!,$itemId: ID!,$columnIds: [String!]) {{ boards(ids: [$boardId]) {{ items_page(query_params: {{ids: [$itemId]}}) {{ items{{ id name column_values(ids: $columnIds){{{LEGACY_COLUMN_VALUES}}} }} }} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/create-board-item.ts
            "MONDAY_CREATE_BOARD_ITEM" => (
                API_VERSION_AI,
                format!(
                    "mutation ($boardId: ID!, $itemName: String!, $groupId: String, $columnValues: JSON, $createLabels: Boolean) {{ create_item(board_id: $boardId, item_name: $itemName, group_id: $groupId, column_values: $columnValues, create_labels_if_missing: $createLabels) {{ {ITEM_SUMMARY_FIELDS} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/rename-item.ts
            "MONDAY_RENAME_ITEM" => (
                API_VERSION_AI,
                format!(
                    "mutation ($boardId: ID!, $itemId: ID!, $columnValues: JSON!) {{ change_multiple_column_values(board_id: $boardId, item_id: $itemId, column_values: $columnValues) {{ {ITEM_SUMMARY_FIELDS} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/set-item-column-values.ts
            "MONDAY_SET_ITEM_COLUMN_VALUES" => (
                API_VERSION_AI,
                format!(
                    "mutation ($boardId: ID!, $itemId: ID!, $columnValues: JSON!, $createLabels: Boolean) {{ change_multiple_column_values(board_id: $boardId, item_id: $itemId, column_values: $columnValues, create_labels_if_missing: $createLabels) {{ {ITEM_SUMMARY_FIELDS} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/archive-item.ts
            "MONDAY_ARCHIVE_ITEM" => (
                API_VERSION_AI,
                format!(
                    "mutation ($itemId: ID!) {{ archive_item(item_id: $itemId) {{ {ITEM_SUMMARY_FIELDS} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/items/create-subitem.ts
            "MONDAY_CREATE_SUBITEM" => (
                API_VERSION_AI,
                format!(
                    "mutation ($parentItemId: ID!, $itemName: String!, $columnValues: JSON, $createLabels: Boolean) {{ create_subitem(parent_item_id: $parentItemId, item_name: $itemName, column_values: $columnValues, create_labels_if_missing: $createLabels) {{ {ITEM_SUMMARY_FIELDS} }} }}"
                ),
            ),
            // AP: src/lib/actions/ai/updates/post-item-update.ts
            "MONDAY_POST_ITEM_UPDATE" => (
                API_VERSION_AI,
                "mutation ($itemId: ID, $parentId: ID, $body: String!) { create_update(item_id: $itemId, parent_id: $parentId, body: $body) { id body text_body item_id creator_id created_at } }"
                    .to_string(),
            ),
            // AP: src/lib/actions/ai/other/send-notification.ts
            "MONDAY_SEND_NOTIFICATION" => (
                API_VERSION_AI,
                "mutation ($userId: ID!, $targetId: ID!, $targetType: NotificationTargetType!, $text: String!) { create_notification(user_id: $userId, target_id: $targetId, target_type: $targetType, text: $text) { text } }"
                    .to_string(),
            ),
            // AP: src/lib/common/mutations.ts (mondayGraphQLMutations.createColumn)
            "MONDAY_CREATE_COLUMN" => (
                API_VERSION_CLASSIC,
                "mutation createColumn($boardId: ID!, $columnTitle: String!, $columnType: ColumnType!) { create_column(board_id: $boardId, title: $columnTitle, column_type: $columnType) { id } }"
                    .to_string(),
            ),
            // AP: src/lib/common/mutations.ts (mondayGraphQLMutations.createGroup)
            "MONDAY_CREATE_GROUP" => (
                API_VERSION_CLASSIC,
                "mutation createGroup($boardId: ID!, $groupName: String!) { create_group(board_id: $boardId, group_name: $groupName) { id } }"
                    .to_string(),
            ),
            // AP: src/lib/common/mutations.ts (mondayGraphQLMutations.createUpdate)
            "MONDAY_CREATE_UPDATE" => (
                API_VERSION_CLASSIC,
                "mutation createUpdate($itemId: ID!, $body: String!) { create_update(item_id: $itemId, body: $body) { id } }"
                    .to_string(),
            ),
            other => bail!("unknown monday tool `{other}`"),
        };
        Ok(q)
    }

    /// GraphQL variables for `tool`.
    #[allow(clippy::too_many_lines)]
    fn variables(&self, tool: &str, args: &Value) -> Result<Value> {
        let mut m = Map::new();
        let put_array = |m: &mut Map<String, Value>, key: &str, v: Vec<String>| {
            if !v.is_empty() {
                m.insert(key.into(), json!(v));
            }
        };
        match tool {
            "MONDAY_GET_ME" | "MONDAY_GET_ACCOUNT" => {}
            "MONDAY_LIST_USERS" => {
                put_array(&mut m, "emails", emails(args));
                put_array(&mut m, "ids", seg_list(args, "user_ids")?);
                put(&mut m, "name", opt_str_owned(args, "name"));
                if let Some(k) = opt_str(args, "user_kind") {
                    m.insert("user_kind".into(), json!({"in": [k]}));
                }
                let status = str_vec(args, "status");
                if !status.is_empty() {
                    m.insert("status".into(), json!(status));
                }
                if let Some(l) = opt_u64_or_err(args, "limit")? {
                    m.insert("limit".into(), json!(l.clamp(1, 1000)));
                }
                if let Some(p) = opt_u64_or_err(args, "page")? {
                    m.insert("page".into(), json!(p));
                }
            }
            "MONDAY_LIST_TEAMS" => {
                put_array(&mut m, "ids", seg_list(args, "team_ids")?);
            }
            "MONDAY_LIST_WORKSPACES" => {
                put_array(&mut m, "ids", seg_list(args, "workspace_ids")?);
                put(&mut m, "kind", opt_str_owned(args, "kind"));
                m.insert(
                    "state".into(),
                    json!(opt_str(args, "state").unwrap_or("active")),
                );
                m.insert(
                    "membership_kind".into(),
                    json!(opt_str(args, "membership_kind").unwrap_or("all")),
                );
                m.insert("limit".into(), json!(opt_u64(args, "limit", 50, 1, 100)));
                m.insert("page".into(), json!(opt_u64(args, "page", 1, 1, 100)));
            }
            "MONDAY_LIST_BOARDS" => {
                put_array(&mut m, "ids", seg_list(args, "board_ids")?);
                put_array(&mut m, "workspace_ids", seg_list(args, "workspace_ids")?);
                m.insert(
                    "state".into(),
                    json!(opt_str(args, "state").unwrap_or("active")),
                );
                put(&mut m, "board_kind", opt_str_owned(args, "board_kind"));
                m.insert("limit".into(), json!(opt_u64(args, "limit", 50, 1, 100)));
                m.insert("page".into(), json!(opt_u64(args, "page", 1, 1, 100)));
            }
            "MONDAY_LIST_BOARD_VIEWS" => {
                m.insert("ids".into(), json!([seg(args, "board_id")?]));
            }
            "MONDAY_LIST_COLUMNS" => {
                m.insert("boardIds".into(), json!([seg(args, "board_id")?]));
                put_array(&mut m, "columnIds", str_vec(args, "column_ids"));
            }
            "MONDAY_GET_COLUMN_TYPE_SCHEMA" => {
                m.insert(
                    "type".into(),
                    json!(req_str(args, "column_type")?.trim()),
                );
            }
            "MONDAY_LIST_GROUPS" => {
                m.insert("boardIds".into(), json!([seg(args, "board_id")?]));
            }
            "MONDAY_LIST_BOARD_ITEMS" => {
                let limit = opt_u64(args, "limit", 50, 1, 500);
                let column_ids = str_vec(args, "column_ids");
                if let Some(cursor) = non_blank(args, "cursor") {
                    m.insert("cursor".into(), json!(cursor));
                    m.insert("limit".into(), json!(limit));
                    put_array(&mut m, "columnIds", column_ids);
                } else if let Some(group) = seg_opt(args, "group_id")? {
                    m.insert("boardId".into(), json!([seg(args, "board_id")?]));
                    m.insert("groupId".into(), json!([group]));
                    m.insert("limit".into(), json!(limit));
                    if let Some(qp) = args.get("query_params").filter(|v| v.is_object()) {
                        m.insert("queryParams".into(), qp.clone());
                    }
                    put_array(&mut m, "columnIds", column_ids);
                } else {
                    m.insert("boardId".into(), json!([seg(args, "board_id")?]));
                    m.insert("limit".into(), json!(limit));
                    if let Some(qp) = args.get("query_params").filter(|v| v.is_object()) {
                        m.insert("queryParams".into(), qp.clone());
                    }
                    put_array(&mut m, "columnIds", column_ids);
                }
            }
            "MONDAY_GET_ITEMS" => {
                let ids = seg_list(args, "item_ids")?;
                if ids.is_empty() {
                    bail!("`item_ids` needs at least one item ID");
                }
                if ids.len() > 100 {
                    bail!("monday.com accepts at most 100 item IDs per request.");
                }
                m.insert("ids".into(), json!(ids));
            }
            "MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert(
                    "limit".into(),
                    json!(opt_u64(args, "limit", 50, 1, 500)),
                );
                put_array(&mut m, "columnIds", str_vec(args, "column_ids"));
                if let Some(cursor) = non_blank(args, "cursor") {
                    m.insert("cursor".into(), json!(cursor));
                } else {
                    let columns = columns_filter(args)?;
                    if columns.is_empty() {
                        bail!("Provide at least one filter, or a cursor from a previous page.");
                    }
                    m.insert("columns".into(), Value::Array(columns));
                }
            }
            "MONDAY_LIST_SUBITEMS" => {
                m.insert("ids".into(), json!([seg(args, "item_id")?]));
                put_array(&mut m, "columnIds", str_vec(args, "column_ids"));
            }
            "MONDAY_SEARCH" => {
                let entity_type = req_str(args, "entity_type")?;
                let spec = search_spec(entity_type)
                    .ok_or_else(|| anyhow::anyhow!("Unsupported entity type: {entity_type}"))?;
                m.insert("query".into(), json!(req_str(args, "query")?));
                if let Some(l) = opt_u64_or_err(args, "limit")? {
                    m.insert("limit".into(), json!(l.clamp(1, 20)));
                }
                if spec.supports_boards {
                    put_array(&mut m, "board_ids", seg_list(args, "board_ids")?);
                }
                if spec.supports_workspaces {
                    put_array(&mut m, "workspace_ids", seg_list(args, "workspace_ids")?);
                }
            }
            "MONDAY_LIST_ITEM_UPDATES" => {
                m.insert("ids".into(), json!([seg(args, "item_id")?]));
                m.insert("limit".into(), json!(opt_u64(args, "limit", 25, 1, 100)));
                m.insert("page".into(), json!(opt_u64(args, "page", 1, 1, 100)));
            }
            "MONDAY_LIST_BOARD_UPDATES" => {
                m.insert("ids".into(), json!([seg(args, "board_id")?]));
                m.insert("limit".into(), json!(opt_u64(args, "limit", 25, 1, 100)));
                m.insert("page".into(), json!(opt_u64(args, "page", 1, 1, 100)));
                put(&mut m, "fromDate", opt_str_owned(args, "from_date"));
                put(&mut m, "toDate", opt_str_owned(args, "to_date"));
            }
            "MONDAY_LIST_FOLDERS" => {
                put_array(&mut m, "ids", seg_list(args, "folder_ids")?);
                put_array(&mut m, "workspace_ids", seg_list(args, "workspace_ids")?);
                m.insert("limit".into(), json!(opt_u64(args, "limit", 25, 1, 100)));
                m.insert("page".into(), json!(opt_u64(args, "page", 1, 1, 100)));
            }
            "MONDAY_GET_BOARD_VALUES" | "MONDAY_GET_ITEM_COLUMN_VALUES" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                if tool == "MONDAY_GET_ITEM_COLUMN_VALUES" {
                    m.insert("itemId".into(), json!(seg(args, "item_id")?));
                }
                put_array(&mut m, "columnIds", str_vec(args, "column_ids"));
            }
            "MONDAY_CREATE_BOARD_ITEM" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert("itemName".into(), json!(req_str(args, "item_name")?));
                put(&mut m, "groupId", seg_opt(args, "group_id")?);
                if let Some(cv) = args.get("column_values").filter(|v| v.is_object()) {
                    // `toJsonString` of the JSON object; keys come out sorted, which the
                    // GraphQL JSON scalar does not care about.
                    m.insert(
                        "columnValues".into(),
                        json!(serde_json::to_string(cv).unwrap_or_default()),
                    );
                }
                m.insert(
                    "createLabels".into(),
                    json!(bool_or(args, "create_labels_if_missing", false)),
                );
            }
            "MONDAY_RENAME_ITEM" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert("itemId".into(), json!(seg(args, "item_id")?));
                let name = req_str(args, "name")?;
                m.insert(
                    "columnValues".into(),
                    json!(json!({"name": name}).to_string()),
                );
            }
            "MONDAY_SET_ITEM_COLUMN_VALUES" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert("itemId".into(), json!(seg(args, "item_id")?));
                let cv = args
                    .get("column_values")
                    .filter(|v| v.is_object())
                    .ok_or_else(|| anyhow::anyhow!("missing argument `column_values`"))?;
                m.insert(
                    "columnValues".into(),
                    json!(serde_json::to_string(cv).unwrap_or_default()),
                );
                m.insert(
                    "createLabels".into(),
                    json!(bool_or(args, "create_labels_if_missing", false)),
                );
            }
            "MONDAY_ARCHIVE_ITEM" => {
                m.insert("itemId".into(), json!(seg(args, "item_id")?));
            }
            "MONDAY_CREATE_SUBITEM" => {
                m.insert("parentItemId".into(), json!(seg(args, "parent_item_id")?));
                m.insert("itemName".into(), json!(req_str(args, "item_name")?));
                if let Some(cv) = args.get("column_values").filter(|v| v.is_object()) {
                    // `toJsonString` of the JSON object; keys come out sorted, which the
                    // GraphQL JSON scalar does not care about.
                    m.insert(
                        "columnValues".into(),
                        json!(serde_json::to_string(cv).unwrap_or_default()),
                    );
                }
                m.insert(
                    "createLabels".into(),
                    json!(bool_or(args, "create_labels_if_missing", false)),
                );
            }
            "MONDAY_POST_ITEM_UPDATE" => {
                let item = seg_opt(args, "item_id")?;
                let parent = seg_opt(args, "parent_id")?;
                if item.is_none() && parent.is_none() {
                    bail!("provide an `item_id` to post an update, or a `parent_id` to reply");
                }
                put(&mut m, "itemId", item);
                put(&mut m, "parentId", parent);
                m.insert("body".into(), json!(req_str(args, "body")?));
            }
            "MONDAY_SEND_NOTIFICATION" => {
                m.insert("userId".into(), json!(seg(args, "user_id")?));
                m.insert("targetId".into(), json!(seg(args, "target_id")?));
                let target = req_str(args, "target_type")?;
                // monday's `NotificationTargetType` enum is `Project` (item/board) or `Post` (update).
                let target = match target {
                    "item" | "board" | "Project" => "Project",
                    "update" | "Post" => "Post",
                    _ => bail!("`target_type` must be item, board or update"),
                };
                m.insert("targetType".into(), json!(target));
                m.insert("text".into(), json!(req_str(args, "text")?));
            }
            "MONDAY_CREATE_COLUMN" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert("columnTitle".into(), json!(req_str(args, "column_title")?));
                let kind = req_str(args, "column_type")?;
                if !COLUMN_TYPES.contains(&kind) {
                    bail!("`column_type` must be one of the monday column types");
                }
                m.insert("columnType".into(), json!(kind));
            }
            "MONDAY_CREATE_GROUP" => {
                m.insert("boardId".into(), json!(seg(args, "board_id")?));
                m.insert("groupName".into(), json!(req_str(args, "group_name")?));
            }
            "MONDAY_CREATE_UPDATE" => {
                m.insert("itemId".into(), json!(seg(args, "item_id")?));
                m.insert("body".into(), json!(req_str(args, "body")?));
            }
            other => bail!("unknown monday tool `{other}`"),
        }
        Ok(Value::Object(m))
    }
}

/// Exact email addresses (`$emails: [String!]`); no `path_segment` guard, `@` is legal.
fn emails(args: &Value) -> Vec<String> {
    str_vec(args, "emails")
        .into_iter()
        .filter(|e| e.contains('@') && !e.contains(char::is_whitespace))
        .collect()
}

/// `toColumnsFilter` in `search-items-by-column-values.ts`: group filter rows by column
/// in first-seen order.
fn columns_filter(args: &Value) -> Result<Vec<Value>> {
    let mut order: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for row in args.get("filters").and_then(Value::as_array).into_iter().flatten() {
        let Some(column_id) = non_blank(row, "column_id") else {
            continue;
        };
        let column_id = check_segment(column_id, "filters.column_id")?;
        let Some(value) = row.get("value").filter(|v| !v.is_null()) else {
            bail!("Filter for column \"{column_id}\" has no value. Use an empty string to match empty cells.");
        };
        let value = match value {
            Value::String(s) => s.trim().to_string(),
            other => other.to_string(),
        };
        if !order.contains(&column_id) {
            order.push(column_id.clone());
        }
        pairs.push((column_id, value));
    }
    Ok(order
        .into_iter()
        .map(|column_id| {
            let values: Vec<String> = pairs
                .iter()
                .filter(|(c, _)| *c == column_id)
                .map(|(_, v)| v.clone())
                .collect();
            json!({"column_id": column_id, "column_values": values})
        })
        .collect())
}

impl ToolkitAdapter for Monday {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let (api_version, query) = self.document(tool, args)?;
        let variables = self.variables(tool, args)?;
        Ok(HttpRequest::new("POST", self.url())
            .header("api-version", api_version)
            .header("content-type", "application/json")
            .json(&json!({"query": query, "variables": variables})))
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let d = data(response)?;
        Ok(match tool {
            "MONDAY_GET_ME" => {
                let me = one(&d, "me")?;
                json!({
                    "id": me["id"], "name": me["name"], "email": me["email"], "title": me["title"],
                    "kind": me["user_config"]["kind"], "status": me["status"], "url": me["url"],
                    "time_zone": me["time_zone_identifier"], "country_code": me["country_code"],
                    "photo_url": me["photo_url"]["small"], "created_at": me["created_at"],
                    "account_id": me["account"]["id"], "account_name": me["account"]["name"],
                    "account_slug": me["account"]["slug"],
                })
            }
            "MONDAY_GET_ACCOUNT" => {
                let a = one(&d, "account")?;
                let products: Vec<String> = list(&a["products"])
                    .iter()
                    .map(|p| {
                        [p["kind"].as_str(), p["tier"].as_str()]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(":")
                    })
                    .filter(|s| !s.is_empty())
                    .collect();
                json!({
                    "id": a["id"], "name": a["name"], "slug": a["slug"], "tier": a["tier"],
                    "country_code": a["country_code"], "active_members_count": a["active_members_count"],
                    "first_day_of_the_week": a["first_day_of_the_week"],
                    "is_during_trial": a["is_during_trial"], "is_trial_expired": a["is_trial_expired"],
                    "show_timeline_weekends": a["show_timeline_weekends"],
                    "plan_tier": a["plan"]["tier"], "plan_period": a["plan"]["period"],
                    "plan_max_users": a["plan"]["max_users"], "plan_version": a["plan"]["version"],
                    "products": if products.is_empty() { Value::Null } else { json!(products.join(", ")) },
                })
            }
            "MONDAY_LIST_USERS" => {
                let users: Vec<Value> = list(&d["users"])
                    .iter()
                    .filter(|u| u.is_object())
                    .map(map_user)
                    .collect();
                json!({"users": users.clone(), "count": users.len()})
            }
            "MONDAY_LIST_TEAMS" => {
                let teams: Vec<Value> = list(&d["teams"])
                    .iter()
                    .map(|t| {
                        let members: Vec<Value> = list(&t["users"])
                            .iter()
                            .filter(|u| u.is_object())
                            .cloned()
                            .collect();
                        let owners = list(&t["owners"]);
                        json!({
                            "id": t["id"], "name": t["name"], "picture_url": t["picture_url"],
                            "member_count": members.len(),
                            "member_ids": join(&members, "id"),
                            "member_names": join(&members, "name"),
                            "member_emails": join(&members, "email"),
                            "owner_ids": join(&owners, "id"),
                            "owner_names": join(&owners, "name"),
                        })
                    })
                    .collect();
                json!({"teams": teams.clone(), "count": teams.len()})
            }
            "MONDAY_LIST_WORKSPACES" => {
                let ws: Vec<Value> = list(&d["workspaces"]).iter().map(map_workspace).collect();
                json!({"workspaces": ws.clone(), "count": ws.len()})
            }
            "MONDAY_LIST_BOARDS" => {
                let boards: Vec<Value> = list(&d["boards"]).iter().map(map_board).collect();
                json!({"boards": boards.clone(), "count": boards.len()})
            }
            "MONDAY_LIST_BOARD_VIEWS" => {
                let board = first_board(&d)?;
                let views: Vec<Value> = list(&board["views"])
                    .iter()
                    .map(|v| {
                        // The `views` document does not select the board id, so it is not echoed.
                        json!({"id": v["id"], "name": v["name"], "type": v["type"],
                            "access_level": v["access_level"],
                            "source_view_id": v["source_view_id"]})
                    })
                    .collect();
                json!({"views": views.clone(), "count": views.len()})
            }
            "MONDAY_LIST_COLUMNS" => {
                let board = first_board(&d)?;
                let columns: Vec<Value> = list(&board["columns"])
                    .iter()
                    .map(|c| {
                        json!({"id": c["id"], "title": c["title"], "type": c["type"],
                            "description": c["description"],
                            "settings": if c["settings"].is_null() { Value::Null }
                                        else { json!(c["settings"].to_string()) },
                            "archived": c["archived"], "width": c["width"]})
                    })
                    .collect();
                json!({"board_id": board["id"], "columns": columns.clone(), "count": columns.len()})
            }
            "MONDAY_GET_COLUMN_TYPE_SCHEMA" => {
                let schema = &d["get_column_type_schema"];
                json!({"schema": if schema.is_string() { schema.clone() } else { json!(schema.to_string()) }})
            }
            "MONDAY_LIST_GROUPS" => {
                let board = first_board(&d)?;
                let groups: Vec<Value> = list(&board["groups"])
                    .iter()
                    .map(|g| {
                        json!({"id": g["id"], "title": g["title"], "color": g["color"],
                            "archived": if g["archived"].is_boolean() { g["archived"].clone() } else { json!(false) },
                            "deleted": if g["deleted"].is_boolean() { g["deleted"].clone() } else { json!(false) }})
                    })
                    .collect();
                json!({"board_id": board["id"], "groups": groups.clone(), "count": groups.len()})
            }
            "MONDAY_LIST_BOARD_ITEMS" | "MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES" => {
                // `next_items_page`, `boards.groups.items_page`, `boards.items_page` and
                // `items_page_by_column_values` all wrap the same `{cursor, items}` payload.
                let mut candidates: Vec<&Value> = vec![&d["next_items_page"], &d["items_page_by_column_values"]];
                if let Ok(b) = first_board(&d) {
                    candidates.push(&b["items_page"]);
                    if let Some(g) = b["groups"].as_array().and_then(|g| g.first()) {
                        candidates.push(&g["items_page"]);
                    }
                }
                let page = candidates
                    .into_iter()
                    .find(|p| p.is_object())
                    .ok_or_else(|| anyhow::anyhow!("monday.com returned no items page (board or group not accessible)."))?;
                let items: Vec<Value> = list(&page["items"]).iter().map(map_item_with_values).collect();
                json!({"items": items.clone(), "count": items.len(),
                    "cursor": if page["cursor"].is_null() { Value::Null } else { page["cursor"].clone() }})
            }
            "MONDAY_GET_ITEMS" => {
                let items: Vec<Value> = list(&d["items"])
                    .iter()
                    .map(|i| {
                        json!({
                            "id": i["id"], "name": i["name"], "state": i["state"], "url": i["url"],
                            "board_id": i["board"]["id"], "board_name": i["board"]["name"],
                            "group_id": i["group"]["id"], "group_title": i["group"]["title"],
                            "parent_item_id": i["parent_item"]["id"],
                            "created_at": i["created_at"], "updated_at": i["updated_at"],
                            "column_values": list(&i["column_values"]).iter().map(|cv| json!({
                                "column_id": cv["id"], "type": cv["type"],
                                "text": cv["text"], "value": cv["value"]})).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                json!({"items": items.clone(), "count": items.len()})
            }
            "MONDAY_LIST_SUBITEMS" => {
                let parent = list(&d["items"])
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("monday.com returned no item (not found or not accessible)."))?;
                let subitems: Vec<Value> = list(&parent["subitems"])
                    .iter()
                    .map(|s| {
                        let mut v = map_item_with_values(s);
                        v["parent_item_id"] = parent["id"].clone();
                        v
                    })
                    .collect();
                json!({"subitems": subitems.clone(), "count": subitems.len()})
            }
            "MONDAY_SEARCH" => {
                let results: Vec<Value> = d["search"]
                    .as_object()
                    .and_then(|s| s.values().next())
                    .and_then(|e| e.get("results"))
                    .map_or(Vec::new(), |r| list(r).to_vec())
                    .iter()
                    .map(|r| {
                        let i = &r["indexed_data"];
                        json!({"id": r["id"], "entity_type": d["search"].as_object()
                                .and_then(|s| s.keys().next()).cloned().unwrap_or_default(),
                            "name": i["name"], "url": i["url"], "board_id": i["board_id"],
                            "workspace_id": i["workspace_id"], "description": i["description"]})
                    })
                    .collect();
                json!({"results": results.clone(), "count": results.len()})
            }
            "MONDAY_LIST_ITEM_UPDATES" => {
                let item = list(&d["items"])
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("monday.com returned no item (not found or not accessible)."))?;
                let updates: Vec<Value> = list(&item["updates"])
                    .iter()
                    .map(|u| {
                        let mut v = map_update(u, item["id"].clone());
                        let replies = list(&u["replies"]);
                        v["reply_count"] = json!(replies.len());
                        v["replies"] = Value::Array(
                            replies
                                .iter()
                                .map(|r| json!({"id": r["id"], "body": r["body"],
                                    "text_body": r["text_body"], "creator_id": r["creator_id"],
                                    "created_at": r["created_at"]}))
                                .collect(),
                        );
                        v
                    })
                    .collect();
                json!({"updates": updates.clone(), "count": updates.len()})
            }
            "MONDAY_LIST_BOARD_UPDATES" => {
                let board = first_board(&d)?;
                let updates: Vec<Value> = list(&board["updates"])
                    .iter()
                    .map(|u| map_update(u, u["item_id"].clone()))
                    .collect();
                json!({"updates": updates.clone(), "count": updates.len()})
            }
            "MONDAY_LIST_FOLDERS" => {
                let folders: Vec<Value> = list(&d["folders"])
                    .iter()
                    .map(|f| {
                        let subs = list(&f["sub_folders"]);
                        let boards: Vec<Value> = list(&f["children"])
                            .iter()
                            .filter(|b| !b.is_null())
                            .cloned()
                            .collect();
                        json!({
                            "id": f["id"], "name": f["name"], "color": f["color"],
                            "created_at": f["created_at"], "owner_id": f["owner_id"],
                            "workspace_id": f["workspace"]["id"], "workspace_name": f["workspace"]["name"],
                            "parent_folder_id": f["parent"]["id"], "parent_folder_name": f["parent"]["name"],
                            "sub_folder_ids": join(&subs, "id"),
                            "board_ids": join(&boards, "id"), "board_names": join(&boards, "name"),
                        })
                    })
                    .collect();
                json!({"folders": folders.clone(), "count": folders.len()})
            }
            "MONDAY_GET_BOARD_VALUES" => {
                let page = &first_board(&d)?["items_page"];
                Value::Array(list(&page["items"]).iter().map(item_with_parsed_columns).collect())
            }
            "MONDAY_GET_ITEM_COLUMN_VALUES" => {
                let page = &first_board(&d)?["items_page"];
                let item = list(&page["items"])
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("monday.com returned no item for the given item id."))?;
                item_with_parsed_columns(item)
            }
            "MONDAY_CREATE_BOARD_ITEM" => map_item_summary(one(&d, "create_item")?),
            "MONDAY_RENAME_ITEM" | "MONDAY_SET_ITEM_COLUMN_VALUES" => {
                map_item_summary(one(&d, "change_multiple_column_values")?)
            }
            "MONDAY_ARCHIVE_ITEM" => map_item_summary(one(&d, "archive_item")?),
            "MONDAY_CREATE_SUBITEM" => map_item_summary(one(&d, "create_subitem")?),
            "MONDAY_POST_ITEM_UPDATE" => {
                let u = one(&d, "create_update")?;
                json!({"id": u["id"], "item_id": u["item_id"], "body": u["body"],
                    "text_body": u["text_body"], "creator_id": u["creator_id"],
                    "created_at": u["created_at"]})
            }
            "MONDAY_SEND_NOTIFICATION" => {
                let text = &d["create_notification"]["text"];
                json!({"sent": true, "text": if text.is_string() { text.clone() } else { Value::Null }})
            }
            "MONDAY_CREATE_COLUMN" => json!({"id": d["create_column"]["id"]}),
            "MONDAY_CREATE_GROUP" => json!({"id": d["create_group"]["id"]}),
            "MONDAY_CREATE_UPDATE" => json!({"id": d["create_update"]["id"]}),
            other => bail!("unknown monday tool `{other}`"),
        })
    }
}

// `parse_response` sees only the response, so the `send-notification` result cannot
// echo `user_id`/`target_id`/`target_type` the way the piece does — the mutation
// payload only returns `text`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;

    fn a() -> Monday {
        Monday::with_api_base("https://monday.test/v2")
    }

    fn body(req: &HttpRequest) -> Value {
        serde_json::from_slice(req.body.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn posts_graphql_to_the_pinned_endpoint() {
        let m = a();
        let r = m.build_request("MONDAY_GET_ME", &json!({})).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://monday.test/v2");
        assert_eq!(r.headers.get("api-version").map(String::as_str), Some(API_VERSION_AI));
        let b = body(&r);
        assert!(b["query"].as_str().unwrap().contains("me {"));
        assert_eq!(b["variables"], json!({}));

        let r = m
            .build_request("MONDAY_CREATE_GROUP", &json!({"board_id": "123", "group_name": "Done"}))
            .unwrap();
        assert_eq!(r.headers.get("api-version").map(String::as_str), Some(API_VERSION_CLASSIC));
        let b = body(&r);
        assert_eq!(b["variables"]["boardId"], "123");
        assert!(b["query"].as_str().unwrap().contains("create_group(board_id: $boardId"));
    }

    #[test]
    fn ids_are_guarded_and_inputs_validated() {
        let m = a();
        assert!(m.build_request("MONDAY_LIST_GROUPS", &json!({"board_id": "../etc"})).is_err());
        assert!(m.build_request("MONDAY_GET_ITEMS", &json!({"item_ids": []})).is_err());
        let many: Vec<String> = (0..101).map(|i| i.to_string()).collect();
        assert!(m.build_request("MONDAY_GET_ITEMS", &json!({"item_ids": many})).is_err());
        assert!(m
            .build_request("MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES", &json!({"board_id": "1"}))
            .is_err());
        assert!(m.build_request("MONDAY_POST_ITEM_UPDATE", &json!({"body": "hi"})).is_err());
        assert!(m
            .build_request(
                "MONDAY_CREATE_COLUMN",
                &json!({"board_id": "1", "column_title": "T", "column_type": "nope"})
            )
            .is_err());
        assert!(m.build_request("MONDAY_SEARCH", &json!({"entity_type": "nope", "query": "x"})).is_err());
    }

    #[test]
    fn list_board_items_picks_the_matching_document() {
        let m = a();
        let plain = body(&m.build_request("MONDAY_LIST_BOARD_ITEMS", &json!({"board_id": "9"})).unwrap());
        assert!(plain["query"].as_str().unwrap().contains("boards(ids: $boardId)"));
        assert_eq!(plain["variables"]["limit"], 50);

        let grouped = body(
            &m.build_request("MONDAY_LIST_BOARD_ITEMS", &json!({"board_id": "9", "group_id": "topics"}))
                .unwrap(),
        );
        assert!(grouped["query"].as_str().unwrap().contains("groups(ids: $groupId)"));
        assert_eq!(grouped["variables"]["groupId"], json!(["topics"]));

        let paged = body(
            &m.build_request("MONDAY_LIST_BOARD_ITEMS", &json!({"cursor": "abc", "limit": 10})).unwrap(),
        );
        assert!(paged["query"].as_str().unwrap().contains("next_items_page"));
        assert_eq!(paged["variables"]["cursor"], "abc");
    }

    #[test]
    fn column_values_are_parsed_like_the_piece() {
        let checked = serde_json::from_str::<Value>(r#"{"id":"done","type":"checkbox","value":"{\"checked\":\"true\"}"}"#).unwrap();
        // monday sends `{"checked":"true"}`; the piece hands `value.checked` through as-is.
        assert_eq!(parse_column_value(&checked), json!("true"));
        let label = json!({"id": "status", "type": "status", "label": "Done"});
        assert_eq!(parse_column_value(&label), json!("Done"));
        let vote = json!({"id": "v", "type": "vote", "vote_count": 3});
        assert_eq!(parse_column_value(&vote), json!(3));
        let week = json!({"id": "w", "type": "week", "start_date": "2026-01-01", "end_date": "2026-01-07"});
        assert_eq!(parse_column_value(&week)["start_date"], "2026-01-01");
    }

    #[test]
    fn graphql_errors_surface_as_tool_errors() {
        let payload = serde_json::json!({"errors": [{"message": "Column not found"}]});
        let resp = HttpResponse { status: 200, body: payload.to_string().into_bytes(), session_id: None };
        let err = a().parse_response("MONDAY_LIST_GROUPS", &resp).unwrap_err();
        assert!(err.to_string().contains("Column not found"), "{err}");
    }
}
