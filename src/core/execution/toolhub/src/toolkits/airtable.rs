//! Airtable toolkit (Web API v0: `https://api.airtable.com/v0`).
//!
//! Action set modelled on Activepieces `piece-airtable` (MIT). Every endpoint is
//! taken verbatim from the piece: the record endpoints from
//! `src/lib/actions/*-ai.ts` (which build their own `httpClient.sendRequest` URL)
//! and the legacy actions from the shared helpers in `src/lib/common/index.ts`
//! (`createRecord`, `updateRecord`, `findRecord`, `fetchTableList`, …).
//!
//! Auth: an Airtable Personal Access Token. The piece sends it as
//! `AuthenticationType.BEARER_TOKEN` on every request, so the toolkit declares
//! `AuthScheme::Bearer` and the hub injects `Authorization: Bearer <pat>`.
//! The scopes are the ones the piece's `src/lib/auth.ts` asks the user to grant.
//!
//! Airtable accepts a base id, table id *or table name*, record id, field id and
//! workspace id as path parameters. Ids go through `path_segment`; the table
//! segment also accepts a table name (spaces and all), which the piece passes
//! through `encodeURIComponent` — so it is validated and then percent-encoded.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "airtable";

/// Airtable PAT scopes (as listed in the piece's `src/lib/auth.ts`).
const RECORDS_R: &str = "data.records:read";
const RECORDS_W: &str = "data.records:write";
const COMMENTS_R: &str = "data.recordComments:read";
const COMMENTS_W: &str = "data.recordComments:write";
const SCHEMA_R: &str = "schema.bases:read";
const SCHEMA_W: &str = "schema.bases:write";

// ---------------------------------------------------------------- schema fragments

fn base_prop() -> Value {
    s("Base-ID (appXXXXXXXXXXXXXX), auflösen mit AIRTABLE_LIST_BASES")
}

fn table_prop() -> Value {
    s("Table-ID (tblXXXXXXXXXXXXXX) oder exakter Tabellenname")
}

fn record_prop() -> Value {
    s("Record-ID (recXXXXXXXXXXXXXX)")
}

fn json_obj(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn json_arr(desc: &str) -> Value {
    json!({"type": "array", "description": desc})
}

/// Optional integer without a default: the piece only sends the query parameter
/// when the property is filled in.
fn num_opt(desc: &str, min: u64, max: u64) -> Value {
    json!({"type": "integer", "description": desc, "minimum": min, "maximum": max})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

// ---------------------------------------------------------------- arg helpers

/// Percent-encode a path segment the way the piece's `encodeURIComponent` does.
fn encode_uri_component(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!'
            | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Table segment: Airtable takes the table id *or* an exact table name here, so
/// unlike `path_segment` a name with spaces is legal. Path separators and
/// percent signs are rejected (they would change what the URL points at) and the
/// rest is percent-encoded.
fn table_segment(args: &Value, key: &str) -> Result<String> {
    let v = req_str(args, key)?.trim();
    if v.is_empty()
        || v.len() > 200
        || v.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | '?' | '#' | '%')
        })
    {
        bail!("invalid argument `{key}`");
    }
    Ok(encode_uri_component(v))
}

/// `/v0/{base}/{table}` — the record collection of one table.
fn opt_u64_present(args: &Value, key: &str, min: u64, max: u64) -> Option<String> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v.clamp(min, max).to_string())
}

/// `fields` as a plain object; the piece's record writes pass it straight into
/// the request body.
fn fields_obj<'a>(args: &'a Value, key: &str) -> Result<&'a Map<String, Value>> {
    match args.get(key) {
        Some(Value::Object(m)) if !m.is_empty() => Ok(m),
        Some(Value::Object(_)) => Err(anyhow::anyhow!("`{key}` cannot be empty")),
        Some(_) => Err(anyhow::anyhow!("`{key}` must be a JSON object")),
        None => Err(anyhow::anyhow!("missing argument `{key}`")),
    }
}

/// `tables` / `fields` / `records` list configs: a non-empty JSON array.
fn array_arg<'a>(args: &'a Value, key: &str, max: usize) -> Result<&'a Vec<Value>> {
    match args.get(key) {
        Some(Value::Array(a)) if !a.is_empty() && a.len() <= max => Ok(a),
        Some(Value::Array(a)) if a.len() > max => {
            Err(anyhow::anyhow!("`{key}` accepts at most {max} entries, got {}", a.len()))
        }
        Some(_) => Err(anyhow::anyhow!("`{key}` must be a non-empty JSON array")),
        None => Err(anyhow::anyhow!("missing argument `{key}`")),
    }
}

/// Airtable formula escaping as the piece does it: `\` first, then `"`.
fn escape_formula_value(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Airtable ids are alphanumeric (`app…`, `tbl…`, `rec…`, `fld…`), which is what
/// `path_segment` allows; this is the same check for values inside a list, where
/// there is no `args` object to hand.
fn is_id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 200
        && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// `?a=b&c=d` from owned pairs, so computed keys (`sort[0][field]`, `records[0]`)
/// can be encoded without leaking.
fn query_owned(pairs: &[(String, Option<String>)]) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    let mut any = false;
    for (k, v) in pairs {
        if let Some(v) = v {
            ser.append_pair(k, v);
            any = true;
        }
    }
    if any {
        format!("?{}", ser.finish())
    } else {
        String::new()
    }
}

/// Field name inside `{…}` — braces cannot be escaped, so they are rejected.
fn formula_field(raw: &str) -> Result<&str> {
    if raw.contains('{') || raw.contains('}') {
        bail!("`search_field` must not contain \"{{\" or \"}}\" — Airtable cannot escape braces inside a field reference.");
    }
    if raw.trim().is_empty() {
        bail!("`search_field` cannot be empty");
    }
    Ok(raw)
}

/// `sort[i][field]` / `sort[i][direction]` pairs from the JSON `sort` property.
fn sort_pairs(args: &Value) -> Vec<(String, Option<String>)> {
    let items: Vec<Value> = match args.get("sort") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Object(o)) => vec![Value::Object(o.clone())],
        _ => Vec::new(),
    };
    items
        .iter()
        .filter_map(|e| {
            let field = e.get("field")?.as_str()?;
            Some((
                field.to_string(),
                e.get("direction")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty())
                    .map(String::from),
            ))
        })
        .collect()
}

// ---------------------------------------------------------------- mappers

fn record(r: &Value) -> Value {
    json!({
        "id": r["id"],
        "created_time": r["createdTime"],
        "fields": r["fields"],
    })
}

fn records(v: &Value) -> Vec<Value> {
    v["records"].as_array().map(|a| a.iter().map(record).collect()).unwrap_or_default()
}

fn field(f: &Value) -> Value {
    json!({
        "id": f["id"],
        "name": f["name"],
        "type": f["type"],
        "description": f["description"],
    })
}

fn table(t: &Value) -> Value {
    let fields = t["fields"]
        .as_array()
        .map(|a| a.iter().map(field).collect::<Vec<_>>())
        .unwrap_or_default();
    let views: Vec<Value> = t["views"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| json!({"id": v["id"], "name": v["name"], "type": v["type"]}))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": t["id"],
        "name": t["name"],
        "description": t["description"],
        "primary_field_id": t["primaryFieldId"],
        "fields": fields,
        "views": views,
    })
}

fn base(b: &Value) -> Value {
    json!({
        "id": b["id"],
        "name": b["name"],
        "permission_level": b["permissionLevel"],
        "workspace_id": b["workspaceId"],
    })
}

fn comment(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "text": c["text"],
        "created_time": c["createdTime"],
        "last_updated_time": c["lastUpdatedTime"],
        "parent_comment_id": c["parentCommentId"],
        "author": c["author"],
    })
}

fn comments(v: &Value) -> Vec<Value> {
    v["comments"].as_array().map(|a| a.iter().map(comment).collect()).unwrap_or_default()
}

/// Airtable error bodies are `{"error": {"type": …, "message": …}}` (or a bare
/// string); turn them into an `anyhow` error instead of returning them as data.
fn body(response: &HttpResponse) -> Result<Value> {
    let v = response.json()?;
    match v.get("error") {
        None | Some(Value::Null) => Ok(v),
        Some(Value::String(m)) => bail!("Airtable API error: {m}"),
        Some(e) => {
            let kind = e["type"].as_str().unwrap_or("ERROR");
            let msg = e["message"].as_str().unwrap_or("unknown");
            bail!("Airtable API error ({kind}): {msg}")
        }
    }
}

// ---------------------------------------------------------------- adapter

pub struct Airtable {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Airtable {
    fn default() -> Self {
        Self::with_api_base("https://api.airtable.com/v0")
    }
}

impl Airtable {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Airtable".into(),
            description: LocalizedText::new(
                "Bases, Tabellen und Datensätze in Airtable lesen, suchen und bearbeiten.",
                "Read, search and change bases, tables and records in Airtable.",
            ),
            auth_schemes: vec![AuthScheme::Bearer],
            tools: vec![
                t(
                    "AIRTABLE_GET_CURRENT_USER",
                    ("Aktuellen Token prüfen", "Get current user"),
                    (
                        "Liest Identität und Scopes des verbundenen Personal Access Tokens.",
                        "Reads the identity and scopes of the connected personal access token.",
                    ),
                    &[],
                    &[],
                    &[],
                    false,
                ),
                t(
                    "AIRTABLE_LIST_BASES",
                    ("Bases auflisten", "List bases"),
                    (
                        "Listet alle Bases, auf die der Token zugreifen darf.",
                        "Lists every base the token can access.",
                    ),
                    &[],
                    &[],
                    &[SCHEMA_R],
                    false,
                ),
                t(
                    "AIRTABLE_GET_BASE",
                    ("Base lesen", "Get base"),
                    (
                        "Liest Name, Berechtigungsstufe und Workspace einer Base.",
                        "Reads one base's name, permission level and workspace.",
                    ),
                    &[("base_id", base_prop())],
                    &["base_id"],
                    &[SCHEMA_R],
                    false,
                ),
                t(
                    "AIRTABLE_GET_BASE_SCHEMA",
                    ("Base-Schema lesen", "Get base schema"),
                    (
                        "Listet alle Tabellen einer Base mit Feldern und Views.",
                        "Lists every table of a base with its fields and views.",
                    ),
                    &[("base_id", base_prop())],
                    &["base_id"],
                    &[SCHEMA_R],
                    false,
                ),
                t(
                    "AIRTABLE_LIST_RECORDS",
                    ("Datensätze auflisten", "List records"),
                    (
                        "Listet Datensätze einer Tabelle, optional mit Formel, View, Sortierung und Feldwahl.",
                        "Lists records of a table, optionally with formula, view, sorting and field selection.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("filter_by_formula", s("Airtable-Formel, z. B. {Status}=\"Done\"")),
                        ("view", s("View-ID oder -Name; leer = ganze Tabelle")),
                        ("max_records", num_opt("Höchstzahl zurückgegebener Datensätze", 1, 100000)),
                        ("page_size", num_opt("Datensätze pro Seite (1–100)", 1, 100)),
                        ("sort", json_arr("JSON-Array, z. B. [{\"field\":\"Name\",\"direction\":\"asc\"}]")),
                        ("fields", str_list("Feldnamen, die zurückgegeben werden sollen")),
                        ("offset", s("Offset eines vorherigen Aufrufs zum Weiterblättern")),
                    ],
                    &["base_id", "table_id_or_name"],
                    &[RECORDS_R],
                    false,
                ),
                t(
                    "AIRTABLE_SEARCH_RECORDS",
                    ("Datensätze suchen", "Search records"),
                    (
                        "Findet Datensätze, in denen ein Feld einen Wert enthält oder exakt gleich ist.",
                        "Finds records where a field contains or exactly equals a value.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("search_field", s("Exakter Feldname")),
                        ("search_value", s("Gesuchter Wert")),
                        ("match_mode", choice("contains (Standard) oder exact", &["contains", "exact"])),
                        ("view", s("View-ID oder -Name; leer = ganze Tabelle")),
                        ("max_records", num_opt("Höchstzahl passender Datensätze", 1, 100000)),
                    ],
                    &["base_id", "table_id_or_name", "search_field", "search_value"],
                    &[RECORDS_R],
                    false,
                ),
                t(
                    "AIRTABLE_FIND_RECORD",
                    ("Datensatz finden", "Find record"),
                    (
                        "Wie AIRTABLE_SEARCH_RECORDS, aber fest auf \"enthält\" und bis zu 1000 Treffer.",
                        "Like AIRTABLE_SEARCH_RECORDS, but fixed to \"contains\" and up to 1000 hits.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("search_field", s("Exakter Feldname")),
                        ("search_value", s("Gesuchter Wert; Groß-/Kleinschreibung beachtet")),
                        ("view", s("View-ID oder -Name; leer = ganze Tabelle")),
                    ],
                    &["base_id", "table_id_or_name", "search_field", "search_value"],
                    &[RECORDS_R],
                    false,
                ),
                t(
                    "AIRTABLE_GET_RECORD",
                    ("Datensatz lesen", "Get record"),
                    ("Liest einen einzelnen Datensatz per ID.", "Reads one record by ID."),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                    ],
                    &["base_id", "table_id_or_name", "record_id"],
                    &[RECORDS_R],
                    false,
                ),
                t(
                    "AIRTABLE_GET_RECORD_BY_ID",
                    ("Datensatz per ID lesen", "Get record by ID"),
                    ("Liest einen einzelnen Datensatz per ID (Alias).", "Reads one record by ID (alias)."),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                    ],
                    &["base_id", "table_id_or_name", "record_id"],
                    &[RECORDS_R],
                    false,
                ),
                t(
                    "AIRTABLE_LIST_RECORD_COMMENTS",
                    ("Kommentare auflisten", "List record comments"),
                    (
                        "Liest den Kommentar-Thread eines Datensatzes.",
                        "Reads a record's comment thread.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                        ("page_size", num_opt("Kommentare pro Seite (max. 100)", 1, 100)),
                        ("offset", s("Offset eines vorherigen Aufrufs zum Weiterblättern")),
                    ],
                    &["base_id", "table_id_or_name", "record_id"],
                    &[COMMENTS_R],
                    false,
                ),
                t(
                    "AIRTABLE_CREATE_RECORD",
                    ("Datensatz anlegen", "Create record"),
                    (
                        "Legt einen Datensatz an (typecast aktiv); braucht --confirm.",
                        "Creates a record (typecast on); requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("fields", json_obj("JSON-Objekt Feldname -> Wert, z. B. {\"Name\":\"Acme\"}")),
                    ],
                    &["base_id", "table_id_or_name", "fields"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_UPDATE_RECORD",
                    ("Datensatz ändern", "Update record"),
                    (
                        "Ändert nur die übergebenen Felder eines Datensatzes; braucht --confirm.",
                        "Changes only the fields you pass on a record; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                        ("fields", json_obj("JSON-Objekt Feldname -> neuer Wert; null leert das Feld")),
                        ("typecast", json!({"type": "boolean", "description": "Airtable wandelt Strings in Feldtypen um und legt Select-Optionen an.", "default": false})),
                    ],
                    &["base_id", "table_id_or_name", "record_id", "fields"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_CLEAR_RECORD",
                    ("Datensatz leeren", "Clear record"),
                    (
                        "Leert jedes Feld, das du leer übergibst (leerer String -> null); braucht --confirm.",
                        "Empties every field you leave blank (empty string -> null); requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                        ("fields", json_obj("JSON-Objekt Feldname -> Wert; \"\" oder null leert das Feld")),
                    ],
                    &["base_id", "table_id_or_name", "record_id", "fields"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_DELETE_RECORD",
                    ("Datensatz löschen", "Delete record"),
                    (
                        "Löscht einen Datensatz; braucht --confirm.",
                        "Deletes one record; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                    ],
                    &["base_id", "table_id_or_name", "record_id"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_DELETE_RECORDS_BATCH",
                    ("Datensätze löschen (Stapel)", "Delete records batch"),
                    (
                        "Löscht bis zu 10 Datensätze in einem Aufruf; braucht --confirm.",
                        "Deletes up to 10 records in one call; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_ids", str_list("Höchstens 10 Record-IDs")),
                    ],
                    &["base_id", "table_id_or_name", "record_ids"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_UPSERT_RECORDS",
                    ("Datensätze Upsert (Stapel)", "Upsert records"),
                    (
                        "Legt an oder ändert bis zu 10 Datensätze in einem Aufruf; braucht --confirm.",
                        "Creates or updates up to 10 records in one call; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("records", json_arr("JSON-Array, z. B. [{\"id\":\"recX\",\"fields\":{…}}] (max. 10)")),
                        ("fields_to_merge_on", str_list("Felder als Merge-Schlüssel, z. B. [\"Email\"]")),
                        ("typecast", json!({"type": "boolean", "description": "Airtable wandelt Strings in Feldtypen um und legt Select-Optionen an.", "default": true})),
                    ],
                    &["base_id", "table_id_or_name", "records"],
                    &[RECORDS_W],
                    true,
                ),
                t(
                    "AIRTABLE_ADD_COMMENT_TO_RECORD",
                    ("Kommentar schreiben", "Add comment to record"),
                    (
                        "Postet einen Kommentar auf einen Datensatz; braucht --confirm.",
                        "Posts a comment on a record; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("record_id", record_prop()),
                        ("text", s("Kommentartext; Nutzer mit @[userId] oder @[userEmail] erwähnen")),
                        ("parent_comment_id", s("ID des Elternkommentars für eine Antwort")),
                    ],
                    &["base_id", "table_id_or_name", "record_id", "text"],
                    &[COMMENTS_W],
                    true,
                ),
                t(
                    "AIRTABLE_CREATE_BASE",
                    ("Base anlegen", "Create base"),
                    (
                        "Legt eine Base mit Tabellenstruktur in einem Workspace an; braucht --confirm.",
                        "Creates a base with a table structure in a workspace; requires --confirm.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (wspXXXXXXXXXXXXXX)")),
                        ("name", s("Name der neuen Base")),
                        ("tables", json_arr("JSON-Array der Tabellen; das erste Feld wird das Primärfeld")),
                    ],
                    &["workspace_id", "name", "tables"],
                    &[SCHEMA_W],
                    true,
                ),
                t(
                    "AIRTABLE_CREATE_TABLE",
                    ("Tabelle anlegen", "Create table"),
                    (
                        "Legt eine Tabelle mit Feldern in einer Base an; braucht --confirm.",
                        "Creates a table with fields in a base; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("name", s("Name der Tabelle")),
                        ("description", s("Beschreibung unter dem Tabellennamen")),
                        ("fields", json_arr("JSON-Array der Felder; das erste wird das Primärfeld")),
                    ],
                    &["base_id", "name", "fields"],
                    &[SCHEMA_W],
                    true,
                ),
                t(
                    "AIRTABLE_UPDATE_TABLE",
                    ("Tabelle ändern", "Update table"),
                    (
                        "Benennt eine Tabelle um oder ändert ihre Beschreibung; braucht --confirm.",
                        "Renames a table or changes its description; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("name", s("Neuer Name")),
                        ("description", s("Neue Beschreibung")),
                    ],
                    &["base_id", "table_id_or_name"],
                    &[SCHEMA_W],
                    true,
                ),
                t(
                    "AIRTABLE_CREATE_FIELD",
                    ("Feld anlegen", "Create field"),
                    (
                        "Fügt einer Tabelle ein Feld (Spalte) hinzu; braucht --confirm.",
                        "Adds a field (column) to a table; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id_or_name", table_prop()),
                        ("name", s("Name des Feldes")),
                        ("type", s("Airtable-Feldtyp, z. B. singleLineText, number, date, singleSelect")),
                        ("description", s("Beschreibung des Feldes")),
                        ("options", json_obj("Typspezifische Optionen, z. B. {\"choices\":[{\"name\":\"Todo\"}]}")),
                    ],
                    &["base_id", "table_id_or_name", "name", "type"],
                    &[SCHEMA_W],
                    true,
                ),
                t(
                    "AIRTABLE_UPDATE_FIELD",
                    ("Feld ändern", "Update field"),
                    (
                        "Benennt ein Feld um oder ändert seine Beschreibung; braucht --confirm.",
                        "Renames a field or changes its description; requires --confirm.",
                    ),
                    &[
                        ("base_id", base_prop()),
                        ("table_id", s("Table-ID (tblXXXXXXXXXXXXXX); dieser Endpunkt braucht die ID, nicht den Namen")),
                        ("field_id", s("Feld-ID (fldXXXXXXXXXXXXXX)")),
                        ("name", s("Neuer Name")),
                        ("description", s("Neue Beschreibung")),
                    ],
                    &["base_id", "table_id", "field_id"],
                    &[SCHEMA_W],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api_base)
    }

    /// `/v0/{base}/{table}` — the record collection of one table.
    fn records_url(&self, args: &Value) -> Result<String> {
        let base = path_segment(args, "base_id")?;
        let table = table_segment(args, "table_id_or_name")?;
        Ok(self.url(&format!("/{base}/{table}")))
    }

    /// `/v0/{base}/{table}/{record}`
    fn record_url(&self, args: &Value) -> Result<String> {
        let base = path_segment(args, "base_id")?;
        let table = table_segment(args, "table_id_or_name")?;
        let record = path_segment(args, "record_id")?;
        Ok(self.url(&format!("/{base}/{table}/{record}")))
    }

    /// `/v0/meta/bases/{base}/tables/{table}` — the metadata endpoints that carry
    /// the table in the path (`update-table`, the field actions).
    fn table_meta_url(&self, base: &str, table: &str) -> String {
        self.url(&format!("/meta/bases/{base}/tables/{table}"))
    }

    /// Query parameters for `AIRTABLE_LIST_RECORDS` — all encoded, all optional.
    fn list_query(&self, args: &Value) -> String {
        let mut pairs: Vec<(String, Option<String>)> = vec![
            ("filterByFormula".into(), opt_str(args, "filter_by_formula").map(String::from)),
            ("view".into(), opt_str(args, "view").map(String::from)),
            ("maxRecords".into(), opt_u64_present(args, "max_records", 1, 100000)),
            ("pageSize".into(), opt_u64_present(args, "page_size", 1, 100)),
            ("offset".into(), opt_str(args, "offset").map(String::from)),
        ];
        for (i, (field, direction)) in sort_pairs(args).into_iter().enumerate() {
            pairs.push((format!("sort[{i}][field]"), Some(field)));
            if let Some(d) = direction {
                pairs.push((format!("sort[{i}][direction]"), Some(d)));
            }
        }
        for (i, f) in str_vec(args, "fields").into_iter().enumerate() {
            pairs.push((format!("fields[{i}]"), Some(f)));
        }
        query_owned(&pairs)
    }
}

impl ToolkitAdapter for Airtable {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/get-current-user.ts
            // url: https://api.airtable.com/v0/meta/whoami
            "AIRTABLE_GET_CURRENT_USER" => HttpRequest::new("GET", self.url("/meta/whoami")),

            // AP: src/lib/actions/list-bases.ts -> airtableCommon.fetchAllBases
            // url: https://api.airtable.com/v0/meta/bases
            "AIRTABLE_LIST_BASES" => HttpRequest::new("GET", self.url("/meta/bases")),

            // AP: src/lib/common/index.ts fetchBase (used by the base/workspace lookups)
            // url: https://api.airtable.com/v0/meta/bases/${baseId}
            "AIRTABLE_GET_BASE" => HttpRequest::new(
                "GET",
                self.url(&format!("/meta/bases/{}", path_segment(args, "base_id")?)),
            ),

            // AP: src/lib/actions/get-base-schema.ts -> airtableCommon.fetchTableList
            // url: https://api.airtable.com/v0/meta/bases/${baseId}/tables
            "AIRTABLE_GET_BASE_SCHEMA" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/meta/bases/{}/tables",
                    path_segment(args, "base_id")?
                )),
            ),

            // AP: src/lib/actions/list-records.ts
            // url: https://api.airtable.com/v0/${base_id}/${encodeURIComponent(table_id_or_name)}
            "AIRTABLE_LIST_RECORDS" => {
                HttpRequest::new("GET", format!("{}{}", self.records_url(args)?, self.list_query(args)))
            }

            // AP: src/lib/actions/search-records.ts
            // url: https://api.airtable.com/v0/${base_id}/${encodeURIComponent(table_id_or_name)}
            // query: filterByFormula = FIND("value",{field}) or {field}&""="value"
            "AIRTABLE_SEARCH_RECORDS" => {
                let field = formula_field(req_str(args, "search_field")?.trim())?;
                let value = escape_formula_value(req_str(args, "search_value")?);
                let formula = match opt_str(args, "match_mode") {
                    None | Some("contains") => format!("FIND(\"{value}\",{{{field}}})"),
                    Some("exact") => format!("{{{field}}}&\"\"=\"{value}\""),
                    Some(other) => bail!("`match_mode` must be contains or exact, got `{other}`"),
                };
                let pairs: Vec<(&str, Option<String>)> = vec![
                    ("filterByFormula", Some(formula)),
                    ("view", opt_str(args, "view").map(String::from)),
                    ("maxRecords", opt_u64_present(args, "max_records", 1, 100000)),
                ];
                HttpRequest::new("GET", format!("{}{}", self.records_url(args)?, query(&pairs)))
            }

            // AP: src/lib/actions/find-record.ts -> airtableCommon.findRecord
            // url: https://api.airtable.com/v0/${baseId}/${tableId}
            // query: filterByFormula = FIND("value",{field}), pageSize=100
            "AIRTABLE_FIND_RECORD" => {
                let field = formula_field(req_str(args, "search_field")?.trim())?;
                let value = escape_formula_value(req_str(args, "search_value")?);
                let pairs: Vec<(&str, Option<String>)> = vec![
                    (
                        "filterByFormula",
                        Some(format!("FIND(\"{value}\",{{{field}}})")),
                    ),
                    ("pageSize", Some("100".into())),
                    ("view", opt_str(args, "view").map(String::from)),
                ];
                HttpRequest::new("GET", format!("{}{}", self.records_url(args)?, query(&pairs)))
            }

            // AP: src/lib/actions/get-record-ai.ts -> airtableCommon.getRecordById
            // url: https://api.airtable.com/v0/${baseId}/${tableId}/${recordId}
            "AIRTABLE_GET_RECORD" | "AIRTABLE_GET_RECORD_BY_ID" => {
                HttpRequest::new("GET", self.record_url(args)?)
            }

            // AP: src/lib/actions/list-record-comments.ts
            // url: https://api.airtable.com/v0/${base_id}/${encodeURIComponent(table_id_or_name)}/${record_id}/comments
            "AIRTABLE_LIST_RECORD_COMMENTS" => {
                let pairs: Vec<(&str, Option<String>)> = vec![
                    ("pageSize", opt_u64_present(args, "page_size", 1, 100)),
                    ("offset", opt_str(args, "offset").map(String::from)),
                ];
                HttpRequest::new("GET", format!("{}/comments{}", self.record_url(args)?, query(&pairs)))
            }

            // AP: src/lib/actions/create-record-ai.ts -> airtableCommon.createRecord
            // url: https://api.airtable.com/v0/${baseId}/${tableId}, body: {fields, typecast: true}
            "AIRTABLE_CREATE_RECORD" => HttpRequest::new("POST", self.records_url(args)?).json(&json!({
                "fields": Value::Object(fields_obj(args, "fields")?.clone()),
                "typecast": true,
            })),

            // AP: src/lib/actions/update-record-ai.ts -> airtableCommon.updateRecord
            // url: https://api.airtable.com/v0/${baseId}/${tableId}/${recordId}, body: {fields, typecast?}
            "AIRTABLE_UPDATE_RECORD" => {
                let mut body = Map::new();
                body.insert(
                    "fields".into(),
                    Value::Object(fields_obj(args, "fields")?.clone()),
                );
                if args.get("typecast").and_then(Value::as_bool).unwrap_or(false) {
                    body.insert("typecast".into(), json!(true));
                }
                HttpRequest::new("PATCH", self.record_url(args)?).json(&Value::Object(body))
            }

            // AP: src/lib/actions/clean-record.ts -> createNewFields(..., true) + updateRecord(typecast: true)
            // url: https://api.airtable.com/v0/${baseId}/${tableId}/${recordId}, body: {fields, typecast: true}
            "AIRTABLE_CLEAR_RECORD" => {
                HttpRequest::new("PATCH", self.record_url(args)?).json(&json!({
                    "fields": Value::Object(cleared_fields(args)?),
                    "typecast": true,
                }))
            }

            // AP: src/lib/actions/delete-record-ai.ts -> airtableCommon.deleteRecord
            // url: https://api.airtable.com/v0/${baseId}/${tableId}/${recordId}
            "AIRTABLE_DELETE_RECORD" => HttpRequest::new("DELETE", self.record_url(args)?),

            // AP: src/lib/actions/delete-records-batch.ts
            // url: https://api.airtable.com/v0/${base_id}/${encodeURIComponent(table_id_or_name)}
            // query: records[0]=recX&records[1]=recY
            "AIRTABLE_DELETE_RECORDS_BATCH" => {
                let ids = str_vec(args, "record_ids");
                if ids.is_empty() {
                    bail!("`record_ids` must be a non-empty list");
                }
                if ids.len() > 10 {
                    bail!("Airtable deletes at most 10 records per call; received {}. Split into batches of 10.", ids.len());
                }
                for id in &ids {
                    if !is_id(id) {
                        bail!("invalid record id `{id}`");
                    }
                }
                let pairs: Vec<(String, Option<String>)> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (format!("records[{i}]"), Some(id.clone())))
                    .collect();
                HttpRequest::new(
                    "DELETE",
                    format!("{}{}", self.records_url(args)?, query_owned(&pairs)),
                )
            }

            // AP: src/lib/actions/upsert-records.ts
            // url: https://api.airtable.com/v0/${base_id}/${encodeURIComponent(table_id_or_name)}
            // body: {records, typecast, performUpsert?}
            "AIRTABLE_UPSERT_RECORDS" => {
                let records = array_arg(args, "records", 10)?.clone();
                let mut body = Map::new();
                body.insert("records".into(), Value::Array(records));
                body.insert(
                    "typecast".into(),
                    json!(args.get("typecast").and_then(Value::as_bool).unwrap_or(true)),
                );
                let merge = str_vec(args, "fields_to_merge_on");
                if !merge.is_empty() {
                    body.insert(
                        "performUpsert".into(),
                        json!({"fieldsToMergeOn": merge}),
                    );
                }
                HttpRequest::new("PATCH", self.records_url(args)?).json(&Value::Object(body))
            }

            // AP: src/lib/actions/add-comment-to-record-ai.ts -> airtableCommon.addCommentToRecord
            // url: https://api.airtable.com/v0/${baseId}/${tableId}/${recordId}/comments
            // body: {text, parentCommentId?}
            "AIRTABLE_ADD_COMMENT_TO_RECORD" => {
                let mut body = Map::new();
                body.insert("text".into(), json!(req_str(args, "text")?));
                if let Some(p) = opt_str(args, "parent_comment_id") {
                    body.insert("parentCommentId".into(), json!(p));
                }
                HttpRequest::new(
                    "POST",
                    format!("{}/comments", self.record_url(args)?),
                )
                .json(&Value::Object(body))
            }

            // AP: src/lib/actions/create-base-ai.ts -> airtableCommon.createBase
            // url: https://api.airtable.com/v0/meta/bases, body: {name, workspaceId, tables}
            "AIRTABLE_CREATE_BASE" => {
                let tables = array_arg(args, "tables", 100)?.clone();
                HttpRequest::new("POST", self.url("/meta/bases")).json(&json!({
                    "name": req_str(args, "name")?.trim(),
                    "workspaceId": path_segment(args, "workspace_id")?,
                    "tables": tables,
                }))
            }

            // AP: src/lib/actions/create-table-ai.ts -> airtableCommon.createTable
            // url: https://api.airtable.com/v0/meta/bases/${baseId}/tables
            // body: {name, fields, description?}
            "AIRTABLE_CREATE_TABLE" => {
                let base = path_segment(args, "base_id")?;
                let mut body = Map::new();
                body.insert("name".into(), json!(req_str(args, "name")?.trim()));
                body.insert(
                    "fields".into(),
                    Value::Array(array_arg(args, "fields", 100)?.clone()),
                );
                if let Some(d) = opt_str(args, "description") {
                    body.insert("description".into(), json!(d));
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/meta/bases/{base}/tables")),
                )
                .json(&Value::Object(body))
            }

            // AP: src/lib/actions/update-table.ts
            // url: https://api.airtable.com/v0/meta/bases/${base_id}/tables/${encodeURIComponent(table_id_or_name)}
            // body: {name?, description?}
            "AIRTABLE_UPDATE_TABLE" => {
                let base = path_segment(args, "base_id")?;
                let url = self.table_meta_url(base, &table_segment(args, "table_id_or_name")?);
                HttpRequest::new("PATCH", url).json(&named_body(args)?)
            }

            // AP: src/lib/actions/create-field.ts
            // url: https://api.airtable.com/v0/meta/bases/${base_id}/tables/${encodeURIComponent(table_id_or_name)}/fields
            // body: {name, type, description?, options?}
            "AIRTABLE_CREATE_FIELD" => {
                let base = path_segment(args, "base_id")?;
                let url = format!(
                    "{}/fields",
                    self.table_meta_url(base, &table_segment(args, "table_id_or_name")?)
                );
                let mut body = Map::new();
                body.insert("name".into(), json!(req_str(args, "name")?.trim()));
                body.insert("type".into(), json!(req_str(args, "type")?.trim()));
                if let Some(d) = opt_str(args, "description") {
                    body.insert("description".into(), json!(d));
                }
                if let Some(o) = args.get("options") {
                    body.insert("options".into(), o.clone());
                }
                HttpRequest::new("POST", url).json(&Value::Object(body))
            }

            // AP: src/lib/actions/update-field.ts
            // url: https://api.airtable.com/v0/meta/bases/${base_id}/tables/${table_id}/fields/${field_id}
            // body: {name?, description?}
            "AIRTABLE_UPDATE_FIELD" => {
                let base = path_segment(args, "base_id")?;
                let table = path_segment(args, "table_id")?;
                let field_id = path_segment(args, "field_id")?;
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!("/meta/bases/{base}/tables/{table}/fields/{field_id}")),
                )
                .json(&named_body(args)?)
            }

            other => bail!("unknown Airtable tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = body(response)?;
        let page = |records: Vec<Value>| -> Value {
            json!({"records": records.clone(), "count": records.len()})
        };
        Ok(match tool {
            "AIRTABLE_GET_CURRENT_USER" => json!({
                "id": v["id"],
                "email": v["email"],
                "scopes": v["scopes"],
            }),
            "AIRTABLE_LIST_BASES" => {
                let bases: Vec<Value> = v["bases"]
                    .as_array()
                    .map(|a| a.iter().map(base).collect())
                    .unwrap_or_default();
                json!({"bases": bases.clone(), "count": bases.len(), "offset": v["offset"]})
            }
            "AIRTABLE_GET_BASE" => base(&v),
            "AIRTABLE_GET_BASE_SCHEMA" => {
                let tables: Vec<Value> = v["tables"]
                    .as_array()
                    .map(|a| a.iter().map(table).collect())
                    .unwrap_or_default();
                json!({"tables": tables.clone(), "count": tables.len()})
            }
            "AIRTABLE_LIST_RECORDS" => {
                let items = records(&v);
                json!({"records": items.clone(), "count": items.len(), "offset": v["offset"]})
            }
            "AIRTABLE_SEARCH_RECORDS" | "AIRTABLE_FIND_RECORD" => page(records(&v)),
            "AIRTABLE_GET_RECORD" | "AIRTABLE_GET_RECORD_BY_ID"
            | "AIRTABLE_CREATE_RECORD" | "AIRTABLE_UPDATE_RECORD" | "AIRTABLE_CLEAR_RECORD" => {
                record(&v)
            }
            "AIRTABLE_DELETE_RECORD" => json!({"id": v["id"], "deleted": v["deleted"]}),
            "AIRTABLE_DELETE_RECORDS_BATCH" => {
                let items: Vec<Value> = v["records"]
                    .as_array()
                    .map(|a| a.iter().map(|r| json!({"id": r["id"], "deleted": r["deleted"]})).collect())
                    .unwrap_or_default();
                json!({"records": items.clone(), "count": items.len()})
            }
            "AIRTABLE_UPSERT_RECORDS" => {
                let items = records(&v);
                json!({
                    "records": items.clone(),
                    "count": items.len(),
                    "created_records": v["createdRecords"],
                    "updated_records": v["updatedRecords"],
                })
            }
            "AIRTABLE_ADD_COMMENT_TO_RECORD" => comment(&v),
            "AIRTABLE_LIST_RECORD_COMMENTS" => {
                let items = comments(&v);
                json!({"comments": items.clone(), "count": items.len(), "offset": v["offset"]})
            }
            "AIRTABLE_CREATE_BASE" => json!({
                "id": v["id"],
                "name": v["name"],
                "permission_level": v["permissionLevel"],
                "tables": v["tables"].as_array().map(|a| a.iter().map(table).collect::<Vec<_>>()),
            }),
            "AIRTABLE_CREATE_TABLE" | "AIRTABLE_UPDATE_TABLE" => table(&v),
            "AIRTABLE_CREATE_FIELD" | "AIRTABLE_UPDATE_FIELD" => field(&v),
            other => bail!("unknown Airtable tool `{other}`"),
        })
    }
}

/// `clean-record.ts`: with `allowEmpty` every blank field is written explicitly —
/// empty strings and missing values become `null`, empty arrays stay empty.
fn cleared_fields(args: &Value) -> Result<Map<String, Value>> {
    let src = fields_obj(args, "fields")?;
    let mut out = Map::new();
    for (k, v) in src {
        let value = match v {
            Value::Null => Value::Null,
            Value::String(s) if s.is_empty() => Value::Null,
            Value::Array(a) if a.is_empty() => json!([]),
            other => other.clone(),
        };
        out.insert(k.clone(), value);
    }
    Ok(out)
}

/// `update-table.ts` / `update-field.ts`: `{name?, description?}`, at least one of both.
fn named_body(args: &Value) -> Result<Value> {
    let name = opt_str(args, "name").map(|v| json!(v));
    let description = opt_str(args, "description").map(|v| json!(v));
    if name.is_none() && description.is_none() {
        bail!("supply at least one of new name or new description");
    }
    let mut body = Map::new();
    if let Some(n) = name {
        body.insert("name".into(), n);
    }
    if let Some(d) = description {
        body.insert("description".into(), d);
    }
    Ok(Value::Object(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a() -> Airtable {
        Airtable::with_api_base("https://api.airtable.test/v0")
    }

    #[test]
    fn builds_the_real_endpoints() {
        let a = a();
        let r = a
            .build_request("AIRTABLE_GET_RECORD", &json!({
                "base_id": "appAcme", "table_id_or_name": "Tasks", "record_id": "rec1"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://api.airtable.test/v0/appAcme/Tasks/rec1");

        let r = a
            .build_request("AIRTABLE_LIST_RECORDS", &json!({
                "base_id": "appAcme", "table_id_or_name": "Tasks",
                "filter_by_formula": "{Status}=\"Done\"", "page_size": 50,
                "sort": [{"field": "Name", "direction": "asc"}], "fields": ["Name"]}))
            .unwrap();
        assert!(r.url.starts_with("https://api.airtable.test/v0/appAcme/Tasks?"));
        assert!(r.url.contains("filterByFormula=%7BStatus%7D%3D%22Done%22"), "{}", r.url);
        assert!(r.url.contains("pageSize=50"));
        assert!(r.url.contains("sort%5B0%5D%5Bfield%5D=Name"));
        assert!(r.url.contains("fields%5B0%5D=Name"));
    }

    #[test]
    fn rejects_unsafe_and_incomplete_args() {
        let a = a();
        assert!(a
            .build_request("AIRTABLE_LIST_RECORDS", &json!({"base_id": "appAcme/../x", "table_id_or_name": "T"}))
            .is_err());
        assert!(a
            .build_request("AIRTABLE_LIST_RECORDS", &json!({"base_id": "appAcme", "table_id_or_name": "Ta/ble"}))
            .is_err());
        assert!(a
            .build_request("AIRTABLE_SEARCH_RECORDS", &json!({
                "base_id": "appAcme", "table_id_or_name": "T",
                "search_field": "{Name}", "search_value": "x"}))
            .is_err());
        assert!(a
            .build_request("AIRTABLE_SEARCH_RECORDS", &json!({
                "base_id": "appAcme", "table_id_or_name": "T",
                "search_field": "Name", "search_value": "x", "match_mode": "fuzzy"}))
            .is_err());
        assert!(a
            .build_request("AIRTABLE_DELETE_RECORDS_BATCH", &json!({
                "base_id": "appAcme", "table_id_or_name": "T",
                "record_ids": (0..11).map(|i| format!("rec{i}")).collect::<Vec<_>>()}))
            .is_err());
        assert!(a
            .build_request("AIRTABLE_UPDATE_TABLE", &json!({"base_id": "appAcme", "table_id_or_name": "T"}))
            .is_err());
        assert!(a.build_request("AIRTABLE_NOPE", &json!({})).is_err());
    }

    #[test]
    fn escapes_formula_values_and_clears_fields() {
        let a = a();
        let r = a
            .build_request("AIRTABLE_SEARCH_RECORDS", &json!({
                "base_id": "appAcme", "table_id_or_name": "Tasks",
                "search_field": "Name", "search_value": "a\"b\\c", "match_mode": "exact"}))
            .unwrap();
        assert!(r.url.contains("filterByFormula="), "{}", r.url);
        assert!(!r.url.contains('"'), "unescaped quote in {}", r.url);

        let r = a
            .build_request("AIRTABLE_CLEAR_RECORD", &json!({
                "base_id": "appAcme", "table_id_or_name": "Tasks", "record_id": "rec1",
                "fields": {"Name": "", "Tags": [], "Done": false}}))
            .unwrap();
        assert_eq!(r.method, "PATCH");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["typecast"], json!(true));
        assert_eq!(body["fields"]["Name"], Value::Null);
        assert_eq!(body["fields"]["Tags"], json!([]));
        assert_eq!(body["fields"]["Done"], json!(false));
    }

    #[test]
    fn maps_responses_and_provider_errors() {
        let a = a();
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({
                "records": [{"id": "rec1", "createdTime": "2026-01-02T03:04:05.000Z",
                             "fields": {"Name": "Acme"}}],
                "offset": "itrNext/1"}))
            .unwrap(),
            session_id: None,
        };
        let out = a
            .parse_response("AIRTABLE_LIST_RECORDS", &resp)
            .unwrap();
        assert_eq!(out["count"], json!(1));
        assert_eq!(out["records"][0]["created_time"], json!("2026-01-02T03:04:05.000Z"));
        assert_eq!(out["records"][0]["fields"]["Name"], json!("Acme"));
        assert_eq!(out["offset"], json!("itrNext/1"));

        let resp = HttpResponse {
            status: 404,
            body: serde_json::to_vec(&json!({
                "error": {"type": "NOT_FOUND", "message": "Could not find table 'Nope'"}}))
            .unwrap(),
            session_id: None,
        };
        let err = a
            .parse_response("AIRTABLE_LIST_RECORDS", &resp)
            .unwrap_err();
        assert!(err.to_string().contains("Could not find table 'Nope'"), "{err}");
    }
}