//! Google Sheets toolkit (Sheets API v4 plus the Drive `files` endpoints the
//! piece uses for spreadsheet search/creation). Action set modelled on
//! Activepieces `piece-google-sheets` (MIT).
//!
//! Auth: the piece's OAuth2 connection (`accounts.google.com/o/oauth2/auth`,
//! `oauth2.googleapis.com/token`, scope list spreadsheets + drive.readonly +
//! drive + email). `default_scopes` carries exactly that list; the per-tool
//! `scopes` mark what each call exercises.
//!
//! The piece resolves a numeric worksheet id to its tab title with an extra
//! `spreadsheets.get` round-trip (`lib/common/common.ts findSheetName`). This
//! adapter is a pure one-request seam, so the tools that hit the `values`
//! endpoints take the tab title directly (`worksheet_title`, resolve via
//! GOOGLESHEETS_GET_SPREADSHEET) and then send the same request the piece sends
//! after that lookup. Actions whose `run()` is a fixed sequence of two or more
//! different calls (`sheets_add_row_at_top`, `sheets_add_column`,
//! `sheets_find_or_create_*`, the store-backed `sheets_get_next_rows`) and the
//! `docs.google.com` CSV/TSV download (`sheets_export_worksheet`) are not
//! ported for that reason.
use std::sync::Mutex;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "googlesheets";

const SHEETS: &str = "https://www.googleapis.com/auth/spreadsheets";
const DRIVE: &str = "https://www.googleapis.com/auth/drive";
const SPREADSHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
const DIMENSIONS: [&str; 2] = ["ROWS", "COLUMNS"];
const RENDER_OPTIONS: [&str; 3] = ["FORMATTED_VALUE", "UNFORMATTED_VALUE", "FORMULA"];
const INPUT_OPTIONS: [&str; 2] = ["USER_ENTERED", "RAW"];

// ---------------------------------------------------------------- schema helpers

fn b(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}
/// Boolean flag without a default (the TS checkbox ships none).
fn b_opt(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}
fn choice(desc: &str, values: &[&str], default: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": values, "default": default})
}
/// Free-form JSON prop (the piece's Property.Json): arrays/objects, so no type.
fn json_prop(desc: &str) -> Value {
    json!({"description": desc})
}
fn row_number(desc: &str) -> Value {
    json!({"type": "integer", "description": desc, "minimum": 1})
}
fn index_number(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn spreadsheet_id() -> (&'static str, Value) {
    (
        "spreadsheet_id",
        s("Spreadsheet-ID (Drive-Datei-ID); Namen mit GOOGLESHEETS_SEARCH_SPREADSHEETS auflösen / The spreadsheet ID (the Drive file id); resolve a name with GOOGLESHEETS_SEARCH_SPREADSHEETS"),
    )
}
/// Numeric worksheet (tab) id, as the structural actions use it.
fn sheet_id() -> (&'static str, Value) {
    (
        "sheet_id",
        index_number("Numerische Worksheet-Tab-ID; per GOOGLESHEETS_GET_SPREADSHEET auflösen, der erste Tab ist meist 0 / Numeric worksheet (tab) id; resolve via GOOGLESHEETS_GET_SPREADSHEET, the first tab is usually 0"),
    )
}
/// Tab title, as the `values` endpoints need it after the piece's id->title lookup.
fn worksheet_title() -> (&'static str, Value) {
    (
        "worksheet_title",
        s("Exakter Tab-Titel (Groß-/Kleinschreibung zählt); per GOOGLESHEETS_GET_SPREADSHEET oder GOOGLESHEETS_FIND_WORKSHEET auflösen / Exact tab title (case-sensitive); resolve via GOOGLESHEETS_GET_SPREADSHEET or GOOGLESHEETS_FIND_WORKSHEET"),
    )
}
fn first_row_headers() -> (&'static str, Value) {
    (
        "first_row_headers",
        b("Wenn true ist „values“ ein Objekt nach Spaltenbuchstabe, z. B. {\"A\":\"x\"}; sonst ein positionsbezogenes Array / When true, \"values\" is an object keyed by column letter like {\"A\":\"x\"}; otherwise a positional array", false),
    )
}
fn as_string() -> (&'static str, Value) {
    (
        "as_string",
        b("Werte als Text speichern (RAW) statt Formeln/Daten zu parsen (USER_ENTERED) / store values verbatim (RAW) instead of parsing formulas and dates (USER_ENTERED)", false),
    )
}
fn value_input_option() -> (&'static str, Value) {
    (
        "value_input_option",
        choice("USER_ENTERED parst wie die Sheets-UI, RAW speichert wörtlich / USER_ENTERED parses like the Sheets UI, RAW stores verbatim", &INPUT_OPTIONS, "USER_ENTERED"),
    )
}

fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn flag_or(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}
fn num(args: &Value, key: &str) -> Result<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing argument `{key}`"))
}
fn opt_num(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}
/// Whitelist for enum-ish string args that end up in requests.
fn allow<'a>(key: &str, v: &'a str, allowed: &[&str]) -> Result<&'a str> {
    if allowed.contains(&v) {
        Ok(v)
    } else {
        bail!("invalid argument `{key}`")
    }
}

// ---------------------------------------------------------------- piece helpers
// Ports of `lib/common/common.ts` / `lib/triggers/helpers.ts` helpers.

/// `encodeURIComponent`: exactly the escaping the piece's httpClient and the
/// googleapis SDK apply to the `range` path segment.
fn enc(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for byte in v.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// `columnToLabel`: 0-based index -> A, B, ..., Z, AA, ...
fn column_to_label(mut index: i64) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut label = String::new();
    while index >= 0 {
        label.insert(0, alphabet[(index % 26) as usize] as char);
        index = index / 26 - 1;
    }
    label
}

/// `labelToColumn`: A, B, ..., AA -> 0-based index.
fn label_to_column(label: &str) -> i64 {
    let upper = label.to_ascii_uppercase();
    let len = upper.len();
    let mut column: i64 = 0;
    for (i, byte) in upper.bytes().enumerate() {
        let pos = match byte {
            b'A'..=b'Z' => (byte - b'A') as i64,
            _ => 0,
        };
        column += (pos + 1) * 26i64.pow((len - i - 1) as u32);
    }
    column - 1
}

/// `objectToArray`: `{"B":"y"}` -> `[null, "y"]`, holes stay null.
fn object_to_array(obj: &Value) -> Vec<Value> {
    let Some(map) = obj.as_object() else {
        return Vec::new();
    };
    let mut placed: Vec<(i64, &Value)> = Vec::new();
    let mut max_index: i64 = -1;
    for (key, value) in map {
        let index = label_to_column(key);
        max_index = max_index.max(index);
        placed.push((index, value));
    }
    let mut arr = vec![Value::Null; (max_index + 1).max(0) as usize];
    for (index, value) in placed {
        if index >= 0 {
            arr[index as usize] = value.clone();
        }
    }
    arr
}

/// `stringifyArray`: strings verbatim, everything else JSON-encoded.
fn stringify_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `quoteSheetTitle` (sheets_update_values / sheets_append_values).
fn quote_sheet_title(title: &str) -> String {
    format!("'{}'", title.replace('\'', "''"))
}

/// Row number out of an `updatedRange` like `Sheet1!A5:D5`.
fn extract_row_number(updated_range: &Value) -> Option<u64> {
    let range = updated_range.as_str()?;
    let part = range.split('!').nth(1)?;
    let head = part.split(':').next()?;
    head.chars().next()?;
    head[1..].parse().ok()
}

/// `hexToRgb`: `#FFD966` -> Sheets Color channels as floats in [0, 1].
fn hex_to_rgb(hex: Option<&str>) -> Option<Value> {
    let hex = hex?;
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let full: String = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_string()
    };
    let bytes = full.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| {
        u8::from_str_radix(&full[i..i + 2], 16).unwrap_or(0) as f64 / 255.0
    };
    Some(json!({"red": channel(0), "green": channel(2), "blue": channel(4)}))
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        // JS `Array.toString` joins elements with ","; objects are kept as JSON.
        Value::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    cell_text(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
}

/// `mapRowsToColumnLabels`: number and letter-key each returned row, padded to
/// the widest row the way the piece pads to the header count.
fn labeled_rows(v: &Value, first_row: u64) -> Vec<Value> {
    let empty = Vec::new();
    let rows = v["values"].as_array().unwrap_or(&empty);
    let width = rows
        .iter()
        .map(|r| r.as_array().map(Vec::len).unwrap_or(0))
        .max()
        .unwrap_or(0);
    rows.iter()
        .enumerate()
        .map(|(i, raw)| {
            let cells = raw.as_array().cloned().unwrap_or_default();
            let mut values = Map::new();
            for j in 0..width {
                let cell = cells.get(j).cloned().unwrap_or(Value::Null);
                values.insert(column_to_label(j as i64), json!(cell_text(&cell)));
            }
            json!({"row": first_row + i as u64, "values": values})
        })
        .collect()
}

/// `buildDeletionRanges` (sheets_delete_multiple_rows): 0-based half-open
/// [start, end) pairs, list mode deduplicated and deleted top-down.
fn deletion_ranges(args: &Value) -> Result<Vec<(u64, u64)>> {
    let mode = allow("mode", req_str(args, "mode")?, &["range", "list"])?;
    if mode == "range" {
        let start = num(args, "starting_row")?;
        if start < 1 {
            bail!("`starting_row` must be 1 or greater");
        }
        let end = opt_num(args, "ending_row").filter(|e| *e >= start).unwrap_or(start);
        return Ok(vec![(start - 1, end)]);
    }
    let raw = req_str(args, "row_numbers")?;
    let mut parsed: Vec<u64> = raw
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<u64>().ok())
        .filter(|n| *n >= 1)
        .collect();
    parsed.sort_unstable_by(|a, b| b.cmp(a));
    parsed.dedup();
    if parsed.is_empty() {
        bail!("no valid row numbers were parsed from `row_numbers`");
    }
    Ok(parsed.iter().map(|n| (n - 1, *n)).collect())
}

/// The `valueInputOption` a write uses (`as_string` selects RAW).
fn input_option(args: &Value) -> Result<&'static str> {
    if flag(args, "as_string") {
        return Ok("RAW");
    }
    Ok("USER_ENTERED")
}

// ---------------------------------------------------------------- adapter

pub struct GoogleSheets {
    spec: ToolkitSpec,
    /// Sheets API host (default `https://sheets.googleapis.com`).
    sheets_base: String,
    /// Drive API host the piece hardcodes for files.list/files.create
    /// (default `https://www.googleapis.com`).
    drive_base: String,
    /// Args of the in-flight call. The piece filters found rows/worksheets
    /// client-side after the request, so `parse_response` needs the same args
    /// the `build_request` directly before it received (hub.rs:540 -> hub.rs:588).
    in_flight: Mutex<Option<(String, Value)>>,
}

impl Default for GoogleSheets {
    fn default() -> Self {
        Self::with_api_bases(
            "https://sheets.googleapis.com",
            "https://www.googleapis.com",
        )
    }
}

impl GoogleSheets {
    /// Test constructor: points BOTH the Sheets and the Drive base at one host.
    /// Production defaults are `Default` (sheets.googleapis.com and
    /// www.googleapis.com, the two hosts the piece uses).
    pub fn with_api_base(api_base: &str) -> Self {
        Self::with_api_bases(api_base, api_base)
    }

    /// `sheets_base` serves the Sheets API v4 calls (`googleSheetsCommon.baseUrl`),
    /// `drive_base` the Drive `files` endpoints the piece hardcodes for
    /// spreadsheet search (`sheets_search_spreadsheets`) and creation.
    pub fn with_api_bases(sheets_base: &str, drive_base: &str) -> Self {
        let t = |slug: &str,
                 title: (&str, &str),
                 desc: (&str, &str),
                 props: &[(&str, Value)],
                 req: &[&str],
                 scopes: &[&str],
                 m: bool| { tool(SLUG, slug, title, desc, props, req, scopes, m) };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Google Sheets".into(),
            description: LocalizedText::new(
                "Tabellen, Worksheets und Zeilen in Google Sheets lesen, durchsuchen und bearbeiten.",
                "Read, search and change spreadsheets, worksheets and rows in Google Sheets.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://accounts.google.com/o/oauth2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    default_scopes: vec![
                        "https://www.googleapis.com/auth/spreadsheets".into(),
                        "https://www.googleapis.com/auth/drive.readonly".into(),
                        "https://www.googleapis.com/auth/drive".into(),
                        "email".into(),
                    ],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![
                        ("access_type".into(), "offline".into()),
                        ("prompt".into(), "consent".into()),
                    ],
                }),
                AuthScheme::Bearer,
            ],
            tools: vec![
                // ---------------- read / search ----------------
                t(
                    "GOOGLESHEETS_GET_SPREADSHEET",
                    ("Tabellen-Info lesen", "Get spreadsheet info"),
                    (
                        "Liest Titel, Locale und alle Worksheets (Tabs) einer Tabelle.",
                        "Reads a spreadsheet's title, locale and its worksheets (tabs).",
                    ),
                    &[
                        spreadsheet_id(),
                        ("include_grid_data", b("Zell-Daten mitliefern (schwergewichtige Antwort) / also return cell data (heavy response)", false)),
                    ],
                    &["spreadsheet_id"],
                    &[SHEETS],
                    false,
                ),
                t(
                    "GOOGLESHEETS_FIND_WORKSHEET",
                    ("Worksheets suchen", "Find worksheets"),
                    (
                        "Findet Worksheets einer Tabelle über den Tab-Titel (exakt oder enthält).",
                        "Finds worksheets of a spreadsheet by tab title (exact or contains).",
                    ),
                    &[
                        spreadsheet_id(),
                        ("title", s("Zu suchender Tab-Titel / tab title to search for")),
                        ("exact_match", b("Nur exakte Treffer / only exact matches", false)),
                    ],
                    &["spreadsheet_id", "title"],
                    &[SHEETS],
                    false,
                ),
                t(
                    "GOOGLESHEETS_SEARCH_SPREADSHEETS",
                    ("Tabellen suchen", "Find spreadsheets"),
                    (
                        "Durchsucht Drive nach Tabellen über den Dateinamen.",
                        "Searches Drive for spreadsheets by file name.",
                    ),
                    &[
                        ("spreadsheet_name", s("Zu suchender Tabellenname / spreadsheet name to search for")),
                        ("exact_match", b("Nur exakte Treffer / only exact matches", false)),
                        ("page_token", s("Next page token")),
                    ],
                    &["spreadsheet_name"],
                    &[SHEETS, DRIVE],
                    false,
                ),
                t(
                    "GOOGLESHEETS_GET_VALUES",
                    ("Zellbereich lesen", "Read cell range"),
                    (
                        "Liest Zellwerte per A1-Notation (z. B. A1:D10) als 2D-Array.",
                        "Reads cell values in A1 notation (e.g. A1:D10) as a 2D array.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("range", s("A1-Bereich; leer = ganzes Worksheet / A1 range; empty = whole worksheet")),
                        ("major_dimension", choice("Zeilen- oder spaltenweise gruppieren / group by rows or columns", &DIMENSIONS, "ROWS")),
                        ("value_render_option", choice("Werte formatiert, roh oder als Formel / formatted, raw or as formula", &RENDER_OPTIONS, "FORMATTED_VALUE")),
                    ],
                    &["spreadsheet_id", "worksheet_title", "major_dimension", "value_render_option"],
                    &[SHEETS],
                    false,
                ),
                t(
                    "GOOGLESHEETS_GET_ALL_ROWS",
                    ("Alle Zeilen lesen", "Get all rows"),
                    (
                        "Liest alle Zeilen eines Worksheets; Schlüssel sind Spaltenbuchstaben.",
                        "Reads every row of a worksheet; keys are column letters.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("first_row_headers", b("Zeile 1 als Kopfzeile behandeln (Daten ab Zeile 2) / treat row 1 as headers (data from row 2)", true)),
                    ],
                    &["spreadsheet_id", "worksheet_title"],
                    &[SHEETS],
                    false,
                ),
                t(
                    "GOOGLESHEETS_GET_ROW",
                    ("Zeile nach Nummer lesen", "Get row by number"),
                    (
                        "Liest eine einzelne Zeile nach ihrer Zeilennummer.",
                        "Reads a single row by its row number.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("row_number", row_number("Zu lesende Zeilennummer, ab 1 / row number to read, from 1")),
                    ],
                    &["spreadsheet_id", "worksheet_title", "row_number"],
                    &[SHEETS],
                    false,
                ),
                t(
                    "GOOGLESHEETS_FIND_ROWS",
                    ("Zeilen suchen", "Find rows"),
                    (
                        "Findet Zeilen über einen Spaltenwert; leerer Suchwert liefert Zeilen der Reihe nach.",
                        "Finds rows by a column value; an empty search value returns rows in order.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("column_name", s("Spaltenbuchstabe, z. B. \"A\" (Standard) / column letter, e.g. \"A\" (default)")),
                        ("search_value", s("Suchwert; leer = alle Zeilen / value to look for; empty = all rows")),
                        ("match_case", b("Nur exakte Treffer / only exact matches", false)),
                        ("starting_row", row_number("Ab dieser Zeile suchen / start searching at this row")),
                        ("number_of_rows", row_number("Wie viele Treffer (Standard 1) / how many matches (default 1)")),
                    ],
                    &["spreadsheet_id", "worksheet_title"],
                    &[SHEETS],
                    false,
                ),
                // ---------------- write ----------------
                t(
                    "GOOGLESHEETS_CREATE_SPREADSHEET",
                    ("Tabelle anlegen", "Create spreadsheet"),
                    (
                        "Legt eine neue, leere Google Sheets-Tabelle an; braucht --confirm.",
                        "Creates a new, empty Google Sheets spreadsheet; requires --confirm.",
                    ),
                    &[
                        ("spreadsheet_title", s("Titel der neuen Tabelle / title of the new spreadsheet")),
                        ("parent_folder_id", s("Optionale Drive-Ordner-ID; leer = My Drive / optional Drive folder id; empty = My Drive")),
                    ],
                    &["spreadsheet_title"],
                    &[SHEETS, DRIVE],
                    true,
                ),
                t(
                    "GOOGLESHEETS_ADD_WORKSHEET",
                    ("Worksheet anlegen", "Add worksheet"),
                    (
                        "Legt einen neuen, leeren Tab an; braucht --confirm.",
                        "Adds a new, empty worksheet tab; requires --confirm.",
                    ),
                    &[spreadsheet_id(), ("title", s("Titel des neuen Tabs / title of the new tab"))],
                    &["spreadsheet_id", "title"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_DELETE_WORKSHEET",
                    ("Worksheet löschen", "Delete worksheet"),
                    (
                        "Löscht einen Tab samt Daten endgültig; braucht --confirm.",
                        "Permanently deletes a worksheet tab with its data; requires --confirm.",
                    ),
                    &[spreadsheet_id(), sheet_id()],
                    &["spreadsheet_id", "sheet_id"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_RENAME_WORKSHEET",
                    ("Worksheet umbenennen", "Rename worksheet"),
                    (
                        "Benennt einen Tab um; braucht --confirm.",
                        "Renames a worksheet tab; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("new_name", s("Neuer Tab-Name / new tab name")),
                    ],
                    &["spreadsheet_id", "sheet_id", "new_name"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_COPY_WORKSHEET",
                    ("Worksheet kopieren", "Copy worksheet"),
                    (
                        "Kopiert einen Tab in eine andere Tabelle; braucht --confirm.",
                        "Copies a worksheet tab into another spreadsheet; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("destination_spreadsheet_id", s("Ziel-Tabellen-ID / destination spreadsheet ID")),
                    ],
                    &["spreadsheet_id", "sheet_id", "destination_spreadsheet_id"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_APPEND_VALUES",
                    ("Werte anhängen", "Append values"),
                    (
                        "Hängt 2D-Wertezeilen unter die letzte Datenzeile; braucht --confirm.",
                        "Appends 2D value rows below the last row with data; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("range", s("A1-Bereich, der die Tabelle lokalisiert, z. B. A1 oder A:D / A1 range locating the table, e.g. A1 or A:D")),
                        ("values", json_prop("2D-Array von Zeilen, z. B. [[\"Alice\",30]] / 2D array of rows, e.g. [[\"Alice\",30]]")),
                        value_input_option(),
                    ],
                    &["spreadsheet_id", "worksheet_title", "range", "values", "value_input_option"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_UPDATE_VALUES",
                    ("Zellbereich schreiben", "Write cell range"),
                    (
                        "Überschreibt einen A1-Bereich mit einem 2D-Array; braucht --confirm.",
                        "Overwrites an A1 range with a 2D array of values; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("range", s("A1-Zielbereich; Form muss zu „values“ passen / target A1 range; shape must match \"values\"")),
                        ("values", json_prop("2D-Array von Werten, z. B. [[\"Name\",\"Alter\"],[\"Alice\",30]] / 2D array of values, e.g. [[\"Name\",\"Age\"],[\"Alice\",30]]")),
                        value_input_option(),
                        ("major_dimension", choice("Jedes innere Array ist eine Zeile (ROWS) oder Spalte (COLUMNS) / each inner array is a row (ROWS) or column (COLUMNS)", &DIMENSIONS, "ROWS")),
                    ],
                    &["spreadsheet_id", "worksheet_title", "range", "values", "value_input_option"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_ADD_ROW",
                    ("Zeile anhängen", "Add row"),
                    (
                        "Hängt eine Zeile unten an eine Tabelle an; braucht --confirm.",
                        "Appends one row to the bottom of a worksheet; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        first_row_headers(),
                        ("values", json_prop("Zeilenwerte: Objekt nach Spaltenbuchstabe oder positionsbezogenes Array / row values: object keyed by column letter or positional array")),
                        as_string(),
                    ],
                    &["spreadsheet_id", "worksheet_title", "values"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_ADD_MULTIPLE_ROWS",
                    ("Mehrere Zeilen anhängen", "Add multiple rows"),
                    (
                        "Hängt viele Zeilen auf einmal an (Objekte nach Spaltenbuchstabe); braucht --confirm.",
                        "Appends many rows at once (objects keyed by column letter); requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("values", json_prop("Array von Objekten nach Spaltenbuchstabe, z. B. [{\"A\":\"a\",\"B\":\"1\"}] / array of objects keyed by column letter, e.g. [{\"A\":\"a\",\"B\":\"1\"}]")),
                        as_string(),
                    ],
                    &["spreadsheet_id", "worksheet_title", "values"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_UPDATE_ROW",
                    ("Zeile überschreiben", "Update row"),
                    (
                        "Überschreibt die Zellen einer bekannten Zeilennummer; leere Werte überspringen; braucht --confirm.",
                        "Overwrites the cells of a known row number; empty values skip cells; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("row_id", row_number("Zu überschreibende Zeile / row to overwrite")),
                        first_row_headers(),
                        ("values", json_prop("Zeilenwerte; \"\"/null lässt die Zelle unverändert / row values; \"\"/null leaves the cell untouched")),
                    ],
                    &["spreadsheet_id", "worksheet_title", "row_id", "values"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_UPDATE_MULTIPLE_ROWS",
                    ("Mehrere Zeilen überschreiben", "Update multiple rows"),
                    (
                        "Überschreibt mehrere Zeilen in einem Batch; braucht --confirm.",
                        "Overwrites several rows in one batch call; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("values", json_prop("Array von Objekten mit numerischer \"rowId\" plus Zellen nach Spaltenbuchstabe / array of objects with a numeric \"rowId\" plus cells keyed by column letter")),
                        as_string(),
                    ],
                    &["spreadsheet_id", "worksheet_title", "values"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_CLEAR_VALUES",
                    ("Zellbereich leeren", "Clear cell range"),
                    (
                        "Leert Zeileninhalte, ohne die Zeilen zu entfernen; braucht --confirm.",
                        "Clears row contents without removing the rows; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        worksheet_title(),
                        ("starting_row", row_number("Erste zu leerende Zeile / first row to clear")),
                        ("ending_row", row_number("Letzte zu leerende Zeile; leer = nur starting_row / last row to clear; empty = only starting_row")),
                    ],
                    &["spreadsheet_id", "worksheet_title", "starting_row"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_DELETE_ROW",
                    ("Zeile löschen", "Delete row"),
                    (
                        "Löscht eine Zeile und verschiebt den Rest nach oben; braucht --confirm.",
                        "Deletes one row and shifts the rest up; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("row_number", row_number("Zu löschende Zeilennummer (1-basiert) / row number to delete (1-based)")),
                    ],
                    &["spreadsheet_id", "sheet_id", "row_number"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_DELETE_MULTIPLE_ROWS",
                    ("Mehrere Zeilen löschen", "Delete multiple rows"),
                    (
                        "Löscht einen Zeilenbereich oder eine Liste von Zeilennummern; braucht --confirm.",
                        "Deletes a row range or a list of row numbers; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("mode", choice("Bereich oder Liste / contiguous range or list", &["range", "list"], "range")),
                        ("starting_row", row_number("Erste Zeile (Modus \"range\") / first row (mode \"range\")")),
                        ("ending_row", row_number("Letzte Zeile (Modus \"range\") / last row (mode \"range\")")),
                        ("row_numbers", s("Kommagetrennte Zeilennummern (Modus \"list\"), z. B. \"3, 5, 8\" / comma-separated row numbers (mode \"list\"), e.g. \"3, 5, 8\"")),
                    ],
                    &["spreadsheet_id", "sheet_id", "mode"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_DELETE_DIMENSION",
                    ("Zeilen/Spalten löschen", "Delete rows/columns"),
                    (
                        "Löscht einen Bereich von Zeilen oder Spalten (0-basiert); braucht --confirm.",
                        "Deletes a range of rows or columns (0-based); requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("dimension", choice("Zeilen oder Spalten / rows or columns", &DIMENSIONS, "ROWS")),
                        ("start_index", index_number("Startindex, 0-basiert inklusiv / start index, 0-based inclusive")),
                        ("end_index", index_number("Endindex, 0-basiert exklusiv / end index, 0-based exclusive")),
                    ],
                    &["spreadsheet_id", "sheet_id", "dimension", "start_index", "end_index"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_INSERT_DIMENSION",
                    ("Zeilen/Spalten einfügen", "Insert rows/columns"),
                    (
                        "Fügt leere Zeilen/Spalten an einer Position ein und verschiebt Daten; braucht --confirm.",
                        "Inserts blank rows/columns at a position, shifting data; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("dimension", choice("Zeilen oder Spalten / rows or columns", &DIMENSIONS, "ROWS")),
                        ("start_index", index_number("Startindex, 0-basiert inklusiv / start index, 0-based inclusive")),
                        ("end_index", index_number("Endindex, 0-basiert exklusiv / end index, 0-based exclusive")),
                        ("inherit_from_before", b("Formatierung von davor erben / inherit formatting from before", false)),
                    ],
                    &["spreadsheet_id", "sheet_id", "dimension", "start_index", "end_index"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_APPEND_DIMENSION",
                    ("Zeilen/Spalten anhängen", "Append rows/columns"),
                    (
                        "Vergrößert das Gitter am Ende um leere Zeilen/Spalten; braucht --confirm.",
                        "Grows the grid by blank rows/columns at the end; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("dimension", choice("Zeilen oder Spalten / rows or columns", &DIMENSIONS, "ROWS")),
                        ("length", row_number("Anzahl anzuhängender Zeilen/Spalten / number of rows/columns to append")),
                    ],
                    &["spreadsheet_id", "sheet_id", "dimension", "length"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_AUTO_RESIZE_DIMENSIONS",
                    ("Zeilen/Spalten auto-fit", "Auto-resize rows/columns"),
                    (
                        "Passt Spaltenbreiten/Zeilenhöhen an den Inhalt an; braucht --confirm.",
                        "Auto-fits column widths/row heights to their content; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("dimension", choice("Spalten oder Zeilen / columns or rows", &DIMENSIONS, "COLUMNS")),
                        ("start_index", index_number("Startindex, 0-basiert inklusiv / start index, 0-based inclusive")),
                        ("end_index", index_number("Endindex, 0-basiert exklusiv / end index, 0-based exclusive")),
                    ],
                    &["spreadsheet_id", "sheet_id", "dimension", "start_index", "end_index"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_UPDATE_DIMENSION_PROPERTIES",
                    ("Zeilen/Spalten formatieren", "Resize / hide rows or columns"),
                    (
                        "Setzt Pixelgröße oder versteckt Zeilen/Spalten; braucht --confirm.",
                        "Sets a pixel size or hides rows/columns; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("dimension", choice("Spalten oder Zeilen / columns or rows", &DIMENSIONS, "COLUMNS")),
                        ("start_index", index_number("Startindex, 0-basiert inklusiv / start index, 0-based inclusive")),
                        ("end_index", index_number("Endindex, 0-basiert exklusiv / end index, 0-based exclusive")),
                        ("pixel_size", index_number("Feste Pixelgröße; leer = nur Sichtbarkeit / fixed pixel size; empty = only visibility")),
                        ("hidden", b_opt("Zeilen/Spalten verbergen / hide the rows/columns")),
                    ],
                    &["spreadsheet_id", "sheet_id", "dimension", "start_index", "end_index"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_UPDATE_SHEET_PROPERTIES",
                    ("Worksheet-Eigenschaften", "Update worksheet properties"),
                    (
                        "Setzt fixierte Zeilen/Spalten, Tab-Farbe oder Sichtbarkeit; braucht --confirm.",
                        "Sets frozen rows/columns, tab color or visibility; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("frozen_row_count", index_number("Anzahl fixierter Zeilen (0 = aufheben) / frozen row count (0 = unfreeze)")),
                        ("frozen_column_count", index_number("Anzahl fixierter Spalten (0 = aufheben) / frozen column count (0 = unfreeze)")),
                        ("tab_color", s("HEX-Farbe, z. B. #FF0000 / HEX color, e.g. #FF0000")),
                        ("hidden", b_opt("Tab verbergen / hide the tab")),
                    ],
                    &["spreadsheet_id", "sheet_id"],
                    &[SHEETS],
                    true,
                ),
                t(
                    "GOOGLESHEETS_FORMAT_CELLS",
                    ("Zellen formatieren", "Format cells"),
                    (
                        "Färbt Zeilen und setzt Fettdruck/Kursiv/Durchgestrichen; braucht --confirm.",
                        "Applies background/text color and bold/italic/strikethrough to rows; requires --confirm.",
                    ),
                    &[
                        spreadsheet_id(),
                        sheet_id(),
                        ("starting_row", row_number("Erste zu formatierende Zeile (1-basiert) / first row to format (1-based)")),
                        ("ending_row", row_number("Letzte Zeile; leer = nur starting_row / last row; empty = only starting_row")),
                        ("bg_color", s("HEX-Hintergrundfarbe, z. B. #FFD966 / HEX background color, e.g. #FFD966")),
                        ("text_color", s("HEX-Textfarbe, z. B. #FFD966 / HEX text color, e.g. #FFD966")),
                        ("bold", b_opt("Fettdruck / bold text")),
                        ("italic", b_opt("Kursiv / italic text")),
                        ("strikethrough", b_opt("Durchgestrichen / strikethrough text")),
                    ],
                    &["spreadsheet_id", "sheet_id", "starting_row"],
                    &[SHEETS],
                    true,
                ),
            ],
        };
        Self {
            spec,
            sheets_base: sheets_base.trim_end_matches('/').to_string(),
            drive_base: drive_base.trim_end_matches('/').to_string(),
            in_flight: Mutex::new(None),
        }
    }

    /// `{base}/v4/spreadsheets{path}` — the piece's `googleSheetsCommon.baseUrl`.
    fn sheets(&self, path: &str) -> String {
        format!("{}/v4/spreadsheets{path}", self.sheets_base)
    }

    /// `{base}/drive/v3/files{query}` — the Drive endpoint the piece hardcodes
    /// for spreadsheet search (`sheets_search_spreadsheets`) and creation.
    fn drive_files(&self, query: &str) -> String {
        format!("{}/drive/v3/files{query}", self.drive_base)
    }

    fn remember_args(&self, tool: &str, args: &Value) {
        let mut slot = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some((tool.to_string(), args.clone()));
    }

    /// Args stored by the `build_request` of the same tool call, if any.
    fn take_args(&self, tool: &str) -> Value {
        let slot = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match slot.as_ref() {
            Some((t, args)) if t == tool => args.clone(),
            _ => Value::Null,
        }
    }
}

impl ToolkitAdapter for GoogleSheets {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.remember_args(tool, args);
        Ok(match tool {
            // AP: src/lib/actions/sheets-get-spreadsheet.ts (`spreadsheets.get`
            //     -> GET /v4/spreadsheets/{spreadsheetId}?includeGridData=…)
            "GOOGLESHEETS_GET_SPREADSHEET" => HttpRequest::new(
                "GET",
                self.sheets(&format!(
                    "/{}{}",
                    path_segment(args, "spreadsheet_id")?,
                    query(&[(
                        "includeGridData",
                        Some(flag_or(args, "include_grid_data", false).to_string())
                    )])
                )),
            ),
            // AP: src/lib/actions/sheets-find-worksheet.ts (`spreadsheets.get`)
            "GOOGLESHEETS_FIND_WORKSHEET" => HttpRequest::new(
                "GET",
                self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
            ),
            // AP: src/lib/actions/sheets-search-spreadsheets.ts
            //     (GET https://www.googleapis.com/drive/v3/files)
            "GOOGLESHEETS_SEARCH_SPREADSHEETS" => {
                let name = req_str(args, "spreadsheet_name")?;
                let name_query = if flag(args, "exact_match") {
                    format!("name = '{name}'")
                } else {
                    format!("name contains '{name}'")
                };
                let q = format!("mimeType='{SPREADSHEET_MIME}' and trashed=false and {name_query}");
                let qstr = query(&[
                    ("q", Some(q)),
                    ("includeItemsFromAllDrives", Some("false".into())),
                    ("supportsAllDrives", Some("true".into())),
                    ("corpora", Some("user".into())),
                    (
                        "fields",
                        Some("files(id,name,webViewLink,createdTime,modifiedTime),nextPageToken".into()),
                    ),
                    ("pageToken", opt_str(args, "page_token").map(String::from)),
                ]);
                HttpRequest::new("GET", self.drive_files(&qstr))
            }
            // AP: src/lib/actions/sheets-get-values.ts
            //     (`spreadsheets.values.get` -> GET /v4/spreadsheets/{id}/values/{range})
            "GOOGLESHEETS_GET_VALUES" => {
                let title = req_str(args, "worksheet_title")?;
                let a1 = match opt_str(args, "range") {
                    Some(r) if !r.trim().is_empty() => format!("{title}!{r}"),
                    _ => title.to_string(),
                };
                let dimension = allow(
                    "major_dimension",
                    req_str(args, "major_dimension")?,
                    &DIMENSIONS,
                )?;
                let render = allow(
                    "value_render_option",
                    req_str(args, "value_render_option")?,
                    &RENDER_OPTIONS,
                )?;
                let qstr = query(&[
                    ("majorDimension", Some(dimension.into())),
                    ("valueRenderOption", Some(render.into())),
                ]);
                HttpRequest::new(
                    "GET",
                    format!(
                        "{}/values/{}{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&a1)
                    ),
                )
            }
            // AP: src/lib/actions/sheets-get-all-rows.ts via
            //     src/lib/common/common.ts `getGoogleSheetRows`
            //     (GET /v4/spreadsheets/{id}/values/{sheetName!A2:ZZZ})
            "GOOGLESHEETS_GET_ALL_ROWS" => {
                let title = req_str(args, "worksheet_title")?;
                let range = if flag_or(args, "first_row_headers", true) {
                    format!("{title}!A2:ZZZ")
                } else {
                    title.to_string()
                };
                self.values_get(args, &range)?
            }
            // AP: src/lib/actions/sheets-get-row.ts via `getGoogleSheetRows`
            //     (GET /v4/spreadsheets/{id}/values/{sheetName!A{n}:ZZZ{n}})
            "GOOGLESHEETS_GET_ROW" => {
                let title = req_str(args, "worksheet_title")?;
                let n = num(args, "row_number")?;
                if n < 1 {
                    bail!("`row_number` must be 1 or greater");
                }
                self.values_get(args, &format!("{title}!A{n}:ZZZ{n}"))?
            }
            // AP: src/lib/actions/sheets-find-rows.ts via `getGoogleSheetRows`
            //     (GET /v4/spreadsheets/{id}/values/{sheetName!A{start}:ZZZ})
            "GOOGLESHEETS_FIND_ROWS" => {
                let title = req_str(args, "worksheet_title")?;
                let start = opt_num(args, "starting_row").unwrap_or(1).max(1);
                self.values_get(args, &format!("{title}!A{start}:ZZZ"))?
            }
            // AP: src/lib/actions/sheets-create-spreadsheet.ts
            //     (`drive.files.create` -> POST https://www.googleapis.com/drive/v3/files)
            "GOOGLESHEETS_CREATE_SPREADSHEET" => {
                let mut body = json!({
                    "name": req_str(args, "spreadsheet_title")?,
                    "mimeType": SPREADSHEET_MIME,
                });
                if let Some(folder) = opt_str(args, "parent_folder_id") {
                    body["parents"] = json!([folder]);
                }
                HttpRequest::new("POST", self.drive_files("?supportsAllDrives=true")).json(&body)
            }
            // AP: src/lib/actions/sheets-add-worksheet.ts (batchUpdate `addSheet`;
            //     the piece's optional header seeding is a second values.append
            //     call — use GOOGLESHEETS_APPEND_VALUES afterwards)
            "GOOGLESHEETS_ADD_WORKSHEET" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"addSheet": {"properties": {"title": req_str(args, "title")?}}}]})),
            // AP: src/lib/actions/sheets-delete-worksheet.ts (batchUpdate `deleteSheet`)
            "GOOGLESHEETS_DELETE_WORKSHEET" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"deleteSheet": {"sheetId": num(args, "sheet_id")?}}]})),
            // AP: src/lib/actions/sheets-rename-worksheet.ts
            //     (batchUpdate `updateSheetProperties`, fields: 'title')
            "GOOGLESHEETS_RENAME_WORKSHEET" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"updateSheetProperties": {
                "properties": {"sheetId": num(args, "sheet_id")?, "title": req_str(args, "new_name")?},
                "fields": "title",
            }}]})),
            // AP: src/lib/actions/sheets-copy-worksheet.ts
            //     (`spreadsheets.sheets.copyTo` -> POST .../sheets/{sheetId}:copyTo)
            "GOOGLESHEETS_COPY_WORKSHEET" => HttpRequest::new(
                "POST",
                self.sheets(&format!(
                    "/{}/sheets/{}:copyTo",
                    path_segment(args, "spreadsheet_id")?,
                    num(args, "sheet_id")?
                )),
            )
            .json(&json!({"destinationSpreadsheetId": req_str(args, "destination_spreadsheet_id")?})),
            // AP: src/lib/actions/sheets-append-values.ts
            //     (`spreadsheets.values.append` with insertDataOption=INSERT_ROWS)
            "GOOGLESHEETS_APPEND_VALUES" => {
                let a1 = format!(
                    "{}!{}",
                    quote_sheet_title(req_str(args, "worksheet_title")?),
                    req_str(args, "range")?
                );
                let qstr = query(&[
                    ("valueInputOption", Some(allow("value_input_option", req_str(args, "value_input_option")?, &INPUT_OPTIONS)?.into())),
                    ("insertDataOption", Some("INSERT_ROWS".into())),
                ]);
                HttpRequest::new(
                    "POST",
                    format!(
                        "{}/values/{}:append{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&a1)
                    ),
                )
                .json(&json!({
                    "majorDimension": "ROWS",
                    "values": values_grid(args)?,
                }))
            }
            // AP: src/lib/actions/sheets-update-values.ts
            //     (`spreadsheets.values.update` -> PUT .../values/{range})
            "GOOGLESHEETS_UPDATE_VALUES" => {
                let a1 = format!(
                    "{}!{}",
                    quote_sheet_title(req_str(args, "worksheet_title")?),
                    req_str(args, "range")?
                );
                let qstr = query(&[(
                    "valueInputOption",
                    Some(allow("value_input_option", req_str(args, "value_input_option")?, &INPUT_OPTIONS)?.into()),
                )]);
                HttpRequest::new(
                    "PUT",
                    format!(
                        "{}/values/{}{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&a1)
                    ),
                )
                .json(&json!({
                    "majorDimension": opt_str(args, "major_dimension")
                        .map(|d| allow("major_dimension", d, &DIMENSIONS))
                        .transpose()?
                        .unwrap_or("ROWS"),
                    "values": values_grid(args)?,
                }))
            }
            // AP: src/lib/actions/sheets-add-row.ts (`appendGoogleSheetValues`:
            //     POST .../values/{sheetName!A:A}:append, majorDimension COLUMNS)
            "GOOGLESHEETS_ADD_ROW" => {
                let title = req_str(args, "worksheet_title")?;
                let raw = args.get("values").unwrap_or(&Value::Null);
                let cells: Vec<Value> = if flag(args, "first_row_headers") {
                    object_to_array(raw)
                        .into_iter()
                        .map(|v| if v.is_null() { json!("") } else { v })
                        .collect()
                } else {
                    raw.as_array().cloned().filter(|a| !a.is_empty()).ok_or_else(|| {
                        anyhow::anyhow!("`values` must be a non-empty array (or set first_row_headers and pass an object keyed by column letter)")
                    })?
                };
                let qstr = query(&[("valueInputOption", Some(input_option(args)?.into()))]);
                HttpRequest::new(
                    "POST",
                    format!(
                        "{}/values/{}:append{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&format!("{title}!A:A"))
                    ),
                )
                .json(&json!({
                    "majorDimension": "COLUMNS",
                    "range": format!("{title}!A:A"),
                    "values": cells.iter().map(|v| json!({"values": stringify_value(v)})).collect::<Vec<_>>(),
                }))
            }
            // AP: src/lib/actions/sheets-add-multiple-rows.ts (`normalInsert`:
            //     values.append of rows keyed by column letter; the csv/json
            //     formats additionally read/patch the header row and are not ported)
            "GOOGLESHEETS_ADD_MULTIPLE_ROWS" => {
                let title = req_str(args, "worksheet_title")?;
                let rows = args
                    .get("values")
                    .and_then(Value::as_array)
                    .filter(|a| !a.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("`values` must be a non-empty array of objects keyed by column letter"))?;
                let grid: Vec<Value> = rows.iter().map(|row| json!(object_to_array(row))).collect();
                let qstr = query(&[("valueInputOption", Some(input_option(args)?.into()))]);
                HttpRequest::new(
                    "POST",
                    format!(
                        "{}/values/{}:append{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&format!("{title}!A:A"))
                    ),
                )
                .json(&json!({"values": grid, "majorDimension": "ROWS"}))
            }
            // AP: src/lib/actions/sheets-update-row.ts
            //     (`spreadsheets.values.update` of {sheetName}!A{row}:ZZZ{row})
            "GOOGLESHEETS_UPDATE_ROW" => {
                let title = req_str(args, "worksheet_title")?;
                let row = num(args, "row_id")?;
                let raw = args.get("values").unwrap_or(&Value::Null);
                let cells: Vec<Value> = if flag(args, "first_row_headers") {
                    object_to_array(raw)
                } else {
                    raw.as_array().cloned().ok_or_else(|| {
                        anyhow::anyhow!("`values` must be an array (or set first_row_headers and pass an object keyed by column letter)")
                    })?
                };
                if cells.is_empty() {
                    bail!("`values` is empty");
                }
                // Empty string / null skips (does not clear) the cell.
                let formatted: Vec<Value> = cells
                    .into_iter()
                    .map(|v| match v {
                        Value::Null => Value::Null,
                        Value::String(s) if s.is_empty() => Value::Null,
                        Value::String(s) => json!(s),
                        other => json!(serde_json::to_string_pretty(&other).unwrap_or_default()),
                    })
                    .collect();
                let qstr = query(&[("valueInputOption", Some("USER_ENTERED".into()))]);
                HttpRequest::new(
                    "PUT",
                    format!(
                        "{}/values/{}{qstr}",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&format!("{title}!A{row}:ZZZ{row}"))
                    ),
                )
                .json(&json!({"majorDimension": "ROWS", "values": [formatted]}))
            }
            // AP: src/lib/actions/sheets-update-multiple-rows.ts
            //     (`spreadsheets.values.batchUpdate` -> POST .../values:batchUpdate)
            "GOOGLESHEETS_UPDATE_MULTIPLE_ROWS" => {
                let title = req_str(args, "worksheet_title")?;
                let rows = args
                    .get("values")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("`values` must be an array of row objects"))?;
                let mut data: Vec<Value> = Vec::new();
                for row in rows {
                    let Some(row_id) = row.get("rowId").and_then(Value::as_u64) else {
                        continue; // rows without a rowId are skipped
                    };
                    let mut cells = row.clone();
                    if let Some(map) = cells.as_object_mut() {
                        map.remove("rowId");
                    }
                    let formatted: Vec<Value> = object_to_array(&cells)
                        .into_iter()
                        .map(|v| match v {
                            Value::Null => Value::Null,
                            Value::String(s) if s.is_empty() => Value::Null,
                            Value::String(s) => json!(s),
                            other => json!(serde_json::to_string_pretty(&other).unwrap_or_default()),
                        })
                        .collect();
                    if formatted.is_empty() {
                        continue;
                    }
                    data.push(json!({
                        "range": format!("{title}!A{row_id}:ZZZ{row_id}"),
                        "majorDimension": "ROWS",
                        "values": [formatted],
                    }));
                }
                if data.is_empty() {
                    bail!("no row in `values` carried a numeric `rowId`");
                }
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!(
                        "/{}/values:batchUpdate",
                        path_segment(args, "spreadsheet_id")?
                    )),
                )
                .json(&json!({"valueInputOption": input_option(args)?, "data": data}))
            }
            // AP: src/lib/actions/sheets-clear-values.ts
            //     (`spreadsheets.values.clear` of {sheetName}!A{start}:ZZZ{end})
            "GOOGLESHEETS_CLEAR_VALUES" => {
                let title = req_str(args, "worksheet_title")?;
                let start = num(args, "starting_row")?;
                if start < 1 {
                    bail!("`starting_row` must be 1 or greater");
                }
                let end = opt_num(args, "ending_row").filter(|e| *e >= start).unwrap_or(start);
                HttpRequest::new(
                    "POST",
                    format!(
                        "{}/values/{}:clear",
                        self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                        enc(&format!("{title}!A{start}:ZZZ{end}"))
                    ),
                )
            }
            // AP: src/lib/actions/sheets-delete-row.ts via
            //     src/lib/common/common.ts `deleteRow` (batchUpdate `deleteDimension`)
            "GOOGLESHEETS_DELETE_ROW" => {
                let row = num(args, "row_number")?;
                if row < 1 {
                    bail!("`row_number` must be 1 or greater");
                }
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
                )
                .json(&json!({"requests": [{"deleteDimension": {"range": {
                    "sheetId": num(args, "sheet_id")?,
                    "dimension": "ROWS",
                    "startIndex": row - 1,
                    "endIndex": row,
                }}}]}))
            }
            // AP: src/lib/actions/sheets-delete-multiple-rows.ts (batchUpdate
            //     `deleteDimension`, one request per range, list mode top-down)
            "GOOGLESHEETS_DELETE_MULTIPLE_ROWS" => {
                let ranges = deletion_ranges(args)?;
                let sheet_id = num(args, "sheet_id")?;
                let requests: Vec<Value> = ranges
                    .iter()
                    .map(|(start, end)| {
                        json!({"deleteDimension": {"range": {
                            "sheetId": sheet_id,
                            "dimension": "ROWS",
                            "startIndex": start,
                            "endIndex": end,
                        }}})
                    })
                    .collect();
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
                )
                .json(&json!({"requests": requests}))
            }
            // AP: src/lib/actions/sheets-delete-dimension.ts (batchUpdate `deleteDimension`)
            "GOOGLESHEETS_DELETE_DIMENSION" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"deleteDimension": {"range": {
                "sheetId": num(args, "sheet_id")?,
                "dimension": allow("dimension", req_str(args, "dimension")?, &DIMENSIONS)?,
                "startIndex": num(args, "start_index")?,
                "endIndex": num(args, "end_index")?,
            }}}]})),
            // AP: src/lib/actions/sheets-insert-dimension.ts (batchUpdate `insertDimension`)
            "GOOGLESHEETS_INSERT_DIMENSION" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"insertDimension": {
                "range": {
                    "sheetId": num(args, "sheet_id")?,
                    "dimension": allow("dimension", req_str(args, "dimension")?, &DIMENSIONS)?,
                    "startIndex": num(args, "start_index")?,
                    "endIndex": num(args, "end_index")?,
                },
                "inheritFromBefore": flag(args, "inherit_from_before"),
            }}]})),
            // AP: src/lib/actions/sheets-append-dimension.ts (batchUpdate `appendDimension`)
            "GOOGLESHEETS_APPEND_DIMENSION" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"appendDimension": {
                "sheetId": num(args, "sheet_id")?,
                "dimension": allow("dimension", req_str(args, "dimension")?, &DIMENSIONS)?,
                "length": num(args, "length")?,
            }}]})),
            // AP: src/lib/actions/sheets-auto-resize-dimensions.ts
            //     (batchUpdate `autoResizeDimensions`)
            "GOOGLESHEETS_AUTO_RESIZE_DIMENSIONS" => HttpRequest::new(
                "POST",
                self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
            )
            .json(&json!({"requests": [{"autoResizeDimensions": {"dimensions": {
                "sheetId": num(args, "sheet_id")?,
                "dimension": allow("dimension", req_str(args, "dimension")?, &DIMENSIONS)?,
                "startIndex": num(args, "start_index")?,
                "endIndex": num(args, "end_index")?,
            }}}]})),
            // AP: src/lib/actions/sheets-update-dimension-properties.ts
            //     (batchUpdate `updateDimensionProperties`)
            "GOOGLESHEETS_UPDATE_DIMENSION_PROPERTIES" => {
                let mut properties = Map::new();
                let mut fields: Vec<&str> = Vec::new();
                if let Some(size) = opt_num(args, "pixel_size") {
                    properties.insert("pixelSize".into(), json!(size));
                    fields.push("pixelSize");
                }
                if args.get("hidden").and_then(Value::as_bool).is_some() {
                    properties.insert("hiddenByUser".into(), json!(flag(args, "hidden")));
                    fields.push("hiddenByUser");
                }
                if fields.is_empty() {
                    bail!("provide a `pixel_size` and/or a `hidden` state to update");
                }
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
                )
                .json(&json!({"requests": [{"updateDimensionProperties": {
                    "range": {
                        "sheetId": num(args, "sheet_id")?,
                        "dimension": allow("dimension", req_str(args, "dimension")?, &DIMENSIONS)?,
                        "startIndex": num(args, "start_index")?,
                        "endIndex": num(args, "end_index")?,
                    },
                    "properties": properties,
                    "fields": fields.join(","),
                }}]}))
            }
            // AP: src/lib/actions/sheets-update-sheet-properties.ts
            //     (batchUpdate `updateSheetProperties`)
            "GOOGLESHEETS_UPDATE_SHEET_PROPERTIES" => {
                let mut properties = Map::new();
                properties.insert("sheetId".into(), json!(num(args, "sheet_id")?));
                let mut fields: Vec<String> = Vec::new();
                let mut grid = Map::new();
                if let Some(count) = opt_num(args, "frozen_row_count") {
                    grid.insert("frozenRowCount".into(), json!(count));
                    fields.push("gridProperties.frozenRowCount".into());
                }
                if let Some(count) = opt_num(args, "frozen_column_count") {
                    grid.insert("frozenColumnCount".into(), json!(count));
                    fields.push("gridProperties.frozenColumnCount".into());
                }
                if !grid.is_empty() {
                    properties.insert("gridProperties".into(), Value::Object(grid));
                }
                if let Some(color) = opt_str(args, "tab_color") {
                    match hex_to_rgb(Some(color)) {
                        Some(rgb) => {
                            properties.insert("tabColor".into(), rgb);
                            fields.push("tabColor".into());
                        }
                        None => bail!("`tab_color` must be a HEX color like #FF0000"),
                    }
                }
                if args.get("hidden").and_then(Value::as_bool).is_some() {
                    properties.insert("hidden".into(), json!(flag(args, "hidden")));
                    fields.push("hidden".into());
                }
                if fields.is_empty() {
                    bail!("provide at least one property to update (frozen rows/columns, tab color, or hidden)");
                }
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
                )
                .json(&json!({"requests": [{"updateSheetProperties": {
                    "properties": properties,
                    "fields": fields.join(","),
                }}]}))
            }
            // AP: src/lib/actions/sheets-format-cells.ts (batchUpdate `repeatCell`)
            "GOOGLESHEETS_FORMAT_CELLS" => {
                let start = num(args, "starting_row")?;
                if start < 1 {
                    bail!("`starting_row` must be 1 or greater");
                }
                let end = opt_num(args, "ending_row").unwrap_or(start);
                let mut text_format = Map::new();
                if let Some(v) = args.get("bold").and_then(Value::as_bool) {
                    text_format.insert("bold".into(), json!(v));
                }
                if let Some(v) = args.get("italic").and_then(Value::as_bool) {
                    text_format.insert("italic".into(), json!(v));
                }
                if let Some(v) = args.get("strikethrough").and_then(Value::as_bool) {
                    text_format.insert("strikethrough".into(), json!(v));
                }
                if let Some(rgb) = hex_to_rgb(opt_str(args, "text_color")) {
                    text_format.insert("foregroundColor".into(), rgb);
                }
                let mut user_entered_format = Map::new();
                if let Some(rgb) = hex_to_rgb(opt_str(args, "bg_color")) {
                    user_entered_format.insert("backgroundColor".into(), rgb);
                }
                user_entered_format.insert("textFormat".into(), Value::Object(text_format));
                HttpRequest::new(
                    "POST",
                    self.sheets(&format!("/{}:batchUpdate", path_segment(args, "spreadsheet_id")?)),
                )
                .json(&json!({"requests": [{"repeatCell": {
                    "range": {
                        "sheetId": num(args, "sheet_id")?,
                        "startRowIndex": start - 1,
                        "endRowIndex": end,
                    },
                    "cell": {"userEnteredFormat": user_entered_format},
                    "fields": "userEnteredFormat(backgroundColor,textFormat)",
                }}]}))
            }
            other => bail!("unknown Google Sheets tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json().unwrap_or(Value::Null);
        let args = self.take_args(tool);
        Ok(match tool {
            "GOOGLESHEETS_GET_SPREADSHEET" => {
                let sheets = v["sheets"].as_array().cloned().unwrap_or_default();
                let worksheets: Vec<Value> = sheets
                    .iter()
                    .map(|sheet| {
                        json!({
                            "sheet_id": sheet["properties"]["sheetId"],
                            "title": sheet["properties"]["title"],
                            "index": sheet["properties"]["index"],
                            "sheet_type": sheet["properties"]["sheetType"],
                            "row_count": sheet["properties"]["gridProperties"]["rowCount"],
                            "column_count": sheet["properties"]["gridProperties"]["columnCount"],
                        })
                    })
                    .collect();
                json!({
                    "spreadsheet_id": v["spreadsheetId"],
                    "title": v["properties"]["title"],
                    "locale": v["properties"]["locale"],
                    "time_zone": v["properties"]["timeZone"],
                    "url": v["spreadsheetUrl"],
                    "worksheets": worksheets,
                    "worksheet_count": worksheets.len(),
                })
            }
            "GOOGLESHEETS_FIND_WORKSHEET" => {
                let title = args["title"].as_str().unwrap_or_default();
                let exact = flag(&args, "exact_match");
                let matched: Vec<Value> = v["sheets"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|sheet| {
                        let sheet_title = sheet["properties"]["title"].as_str().unwrap_or_default();
                        if exact {
                            sheet_title == title
                        } else {
                            sheet_title.contains(title)
                        }
                    })
                    .collect();
                json!({"found": !matched.is_empty(), "worksheets": matched})
            }
            "GOOGLESHEETS_SEARCH_SPREADSHEETS" => {
                let files = v["files"].as_array().cloned().unwrap_or_default();
                json!({"found": !files.is_empty(), "spreadsheets": files})
            }
            "GOOGLESHEETS_GET_VALUES" => json!({
                "range": v["range"],
                "major_dimension": v["majorDimension"],
                "values": v["values"].as_array().cloned().unwrap_or_default(),
            }),
            "GOOGLESHEETS_GET_ALL_ROWS" => {
                let first = if flag_or(&args, "first_row_headers", true) { 2 } else { 1 };
                let rows = labeled_rows(&v, first);
                json!({"rows": rows, "count": rows.len()})
            }
            "GOOGLESHEETS_GET_ROW" => {
                // `isRowOutOfGridError`: Google rejects past-the-grid rows with
                // an "exceeds grid limits" message -> {found: false}.
                if response.status >= 400 {
                    let message = v["error"]["message"].as_str().unwrap_or_default();
                    if message.to_ascii_lowercase().contains("exceeds grid limits") {
                        return Ok(json!({"found": false, "row": null}));
                    }
                }
                let n = opt_num(&args, "row_number").unwrap_or(1);
                match labeled_rows(&v, n).into_iter().next() {
                    Some(row) => json!({"found": true, "row": row["row"], "values": row["values"]}),
                    None => json!({"found": false, "row": null}),
                }
            }
            "GOOGLESHEETS_FIND_ROWS" => {
                let start = opt_num(&args, "starting_row").unwrap_or(1).max(1);
                let rows = labeled_rows(&v, start);
                let column = args["column_name"]
                    .as_str()
                    .filter(|c| !c.trim().is_empty())
                    .unwrap_or("A");
                let column_number = label_to_column(column).max(0) as usize;
                let search = args["search_value"].as_str().unwrap_or_default();
                let exact = flag(&args, "match_case");
                let wanted = opt_num(&args, "number_of_rows").unwrap_or(1).max(1);
                let mut matched: Vec<Value> = Vec::new();
                for row in rows {
                    if matched.len() as u64 >= wanted {
                        break;
                    }
                    if search.is_empty() {
                        matched.push(row);
                        continue;
                    }
                    let Some(values) = row["values"].as_object() else {
                        continue;
                    };
                    if values.len() <= column_number {
                        continue;
                    }
                    let Some(entry) = values
                        .get(&column_to_label(column_number as i64))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    if exact {
                        if entry == search {
                            matched.push(row);
                        }
                    } else if entry.to_lowercase().contains(&search.to_lowercase()) {
                        matched.push(row);
                    }
                }
                Value::Array(matched)
            }
            "GOOGLESHEETS_CREATE_SPREADSHEET" => json!({"id": v["id"]}),
            "GOOGLESHEETS_ADD_WORKSHEET" => {
                let mut out = batch_ok(&v);
                out["sheet_id"] = v["replies"][0]["addSheet"]["properties"]["sheetId"].clone();
                out
            }
            "GOOGLESHEETS_COPY_WORKSHEET" => v, // SheetProperties of the new copy
            "GOOGLESHEETS_APPEND_VALUES" => updates_of(&v["updates"]),
            "GOOGLESHEETS_UPDATE_VALUES" => updates_of(&v),
            "GOOGLESHEETS_ADD_ROW" => json!({
                "spreadsheet_id": v["spreadsheetId"],
                "updates": v["updates"],
                "row": extract_row_number(&v["updates"]["updatedRange"]),
            }),
            "GOOGLESHEETS_ADD_MULTIPLE_ROWS" => v,
            "GOOGLESHEETS_UPDATE_ROW" => json!({
                "updates": v,
                "row": extract_row_number(&v["updatedRange"]),
            }),
            "GOOGLESHEETS_UPDATE_MULTIPLE_ROWS" => v,
            "GOOGLESHEETS_CLEAR_VALUES" => {
                let start = opt_num(&args, "starting_row").unwrap_or(1);
                let end = opt_num(&args, "ending_row").filter(|e| *e >= start).unwrap_or(start);
                json!({
                    "success": true,
                    "starting_row": start,
                    "ending_row": end,
                    "cleared_range": v["clearedRange"],
                })
            }
            "GOOGLESHEETS_DELETE_ROW" | "GOOGLESHEETS_DELETE_WORKSHEET"
            | "GOOGLESHEETS_RENAME_WORKSHEET" | "GOOGLESHEETS_DELETE_DIMENSION"
            | "GOOGLESHEETS_INSERT_DIMENSION" | "GOOGLESHEETS_APPEND_DIMENSION"
            | "GOOGLESHEETS_AUTO_RESIZE_DIMENSIONS"
            | "GOOGLESHEETS_UPDATE_DIMENSION_PROPERTIES"
            | "GOOGLESHEETS_UPDATE_SHEET_PROPERTIES" | "GOOGLESHEETS_FORMAT_CELLS" => batch_ok(&v),
            "GOOGLESHEETS_DELETE_MULTIPLE_ROWS" => {
                let mut out = batch_ok(&v);
                out["deleted_ranges"] = json!(deletion_ranges(&args)?
                    .into_iter()
                    .map(|(start, end)| json!({"start_row": start + 1, "end_row": end}))
                    .collect::<Vec<_>>());
                out
            }
            other => bail!("unknown Google Sheets tool `{other}`"),
        })
    }
}

/// GET of the `values` endpoint used by the row-reading actions (no query
/// parameters, exactly like `getGoogleSheetRows`).
impl GoogleSheets {
    fn values_get(&self, args: &Value, range: &str) -> Result<HttpRequest> {
        Ok(HttpRequest::new(
            "GET",
            format!(
                "{}/values/{}",
                self.sheets(&format!("/{}", path_segment(args, "spreadsheet_id")?)),
                enc(range)
            ),
        ))
    }
}

/// `{spreadsheetId, replies}` of a `:batchUpdate` response.
fn batch_ok(v: &Value) -> Value {
    json!({"success": true, "spreadsheet_id": v["spreadsheetId"], "replies": v["replies"].clone()})
}

/// The four `updated*` counters the piece returns from write actions.
fn updates_of(updates: &Value) -> Value {
    json!({
        "updated_range": updates["updatedRange"],
        "updated_rows": updates["updatedRows"],
        "updated_columns": updates["updatedColumns"],
        "updated_cells": updates["updatedCells"],
    })
}

/// `values` must be a 2D array of rows.
fn values_grid(args: &Value) -> Result<Vec<Value>> {
    args.get("values")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("`values` must be a 2D array of rows, e.g. [[\"Alice\",30]]"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_helpers() {
        // encodeURIComponent semantics.
        assert_eq!(enc("Sheet 1!A2:ZZZ"), "Sheet%201!A2%3AZZZ");
        assert_eq!(enc("'My Sheet'!A1:B2"), "'My%20Sheet'!A1%3AB2");
        assert_eq!(enc("Tab/.."), "Tab%2F..");

        assert_eq!(column_to_label(0), "A");
        assert_eq!(column_to_label(25), "Z");
        assert_eq!(column_to_label(26), "AA");
        assert_eq!(label_to_column("A"), 0);
        assert_eq!(label_to_column("AA"), 26);
        assert_eq!(label_to_column("ZZ"), 701);

        assert_eq!(quote_sheet_title("My'Sheet"), "'My''Sheet'");
        assert_eq!(
            extract_row_number(&json!("Sheet1!A5:D5")),
            Some(5)
        );
        assert_eq!(
            hex_to_rgb(Some("#FFD966")),
            Some(json!({"red": 1.0, "green": 217f64 / 255.0, "blue": 102f64 / 255.0}))
        );
        assert_eq!(
            hex_to_rgb(Some("03F")),
            Some(json!({"red": 0.0, "green": 51f64 / 255.0, "blue": 1.0}))
        );
        assert_eq!(hex_to_rgb(Some("nope")), None);

        assert_eq!(
            object_to_array(&json!({"B": "y", "A": "x"})),
            vec![json!("x"), json!("y")]
        );
        assert_eq!(
            object_to_array(&json!({"C": "z"})),
            vec![Value::Null, Value::Null, json!("z")]
        );
        assert_eq!(stringify_value(&json!("a")), "a");
        assert_eq!(stringify_value(&json!(1.5)), "1.5");
    }

    #[test]
    fn default_bases() {
        let d = GoogleSheets::default();
        let req = d
            .build_request("GOOGLESHEETS_SEARCH_SPREADSHEETS", &json!({"spreadsheet_name": "x"}))
            .unwrap();
        assert!(req.url.starts_with("https://www.googleapis.com/drive/v3/files?"), "{}", req.url);
        let req = d
            .build_request("GOOGLESHEETS_GET_SPREADSHEET", &json!({"spreadsheet_id": "SS1"}))
            .unwrap();
        assert!(
            req.url.starts_with("https://sheets.googleapis.com/v4/spreadsheets/SS1?"),
            "{}",
            req.url
        );
    }

    #[test]
    fn request_guards() {
        let s = GoogleSheets::with_api_base("https://sheets.test");
        assert!(s
            .build_request("GOOGLESHEETS_GET_SPREADSHEET", &json!({"spreadsheet_id": "../x"}))
            .is_err());
        assert!(s
            .build_request("GOOGLESHEETS_GET_VALUES", &json!({
                "spreadsheet_id": "SS1", "worksheet_title": "Sheet 1",
                "major_dimension": "ROWS", "value_render_option": "RAINBOW"}))
            .is_err());
        assert!(s
            .build_request("GOOGLESHEETS_DELETE_MULTIPLE_ROWS", &json!({
                "spreadsheet_id": "SS1", "sheet_id": 0, "mode": "list", "row_numbers": "x, y"}))
            .is_err());
        assert!(s
            .build_request("GOOGLESHEETS_UPDATE_DIMENSION_PROPERTIES", &json!({
                "spreadsheet_id": "SS1", "sheet_id": 0, "dimension": "ROWS",
                "start_index": 0, "end_index": 1}))
            .is_err());
        assert!(s
            .build_request("GOOGLESHEETS_UPDATE_SHEET_PROPERTIES", &json!({
                "spreadsheet_id": "SS1", "sheet_id": 0}))
            .is_err());
        assert!(s.build_request("NOPE", &json!({})).is_err());

        // list mode deletes top-down and deduplicates.
        let r = s
            .build_request("GOOGLESHEETS_DELETE_MULTIPLE_ROWS", &json!({
                "spreadsheet_id": "SS1", "sheet_id": 0, "mode": "list", "row_numbers": "5, 3, 5"}))
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["requests"], json!([
            {"deleteDimension": {"range": {"sheetId": 0, "dimension": "ROWS", "startIndex": 4, "endIndex": 5}}},
            {"deleteDimension": {"range": {"sheetId": 0, "dimension": "ROWS", "startIndex": 2, "endIndex": 3}}},
        ]));
    }
}
