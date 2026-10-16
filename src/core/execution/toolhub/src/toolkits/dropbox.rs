//! Dropbox toolkit (Dropbox API v2). Action set modelled on Activepieces
//! `piece-dropbox` (MIT).
//!
//! Auth: Dropbox OAuth2 (`https://www.dropbox.com/oauth2/authorize`,
//! `https://api.dropboxapi.com/oauth2/token`, scopes `files.metadata.*` and
//! `files.content.*`). The piece sends `token_access_type=offline` on the
//! authorize URL so the connection carries a refresh token. A static app access
//! token from the Dropbox App Console (plain bearer credential) is supported
//! as the second scheme.
//!
//! Dropbox splits its API over two hosts: JSON RPC under
//! `https://api.dropboxapi.com/2/...` and byte endpoints under
//! `https://content.dropboxapi.com/2/...`; both take POST only. There are no
//! path or query parameters: paths, ids and revisions travel inside the JSON
//! body (or the `Dropbox-API-Arg` header of the content host), so arguments are
//! validated as body values, not path segments.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::ToolkitAdapter;
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "dropbox";

const METADATA_R: &str = "files.metadata.read";
const METADATA_W: &str = "files.metadata.write";
const CONTENT_R: &str = "files.content.read";
const CONTENT_W: &str = "files.content.write";

/// Dropbox batch size limits (from the piece's validation messages).
const MAX_THUMBNAILS: usize = 25;
const MAX_BATCH_ENTRIES: usize = 1000;
const MAX_BATCH_FOLDERS: usize = 10000;

// ---------------------------------------------------------------- schema fragments

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// `Property.Array` of `{from_path, to_path}` relocation entries.
fn relocation_list(max: usize) -> Value {
    json!({"type": "array",
        "description": format!("Einträge mit from_path und to_path, höchstens {max}. / Entries with from_path and to_path, at most {max}."),
        "items": {"type": "object", "properties": {
            "from_path": {"type": "string", "description": "Quellpfad oder id:..."},
            "to_path": {"type": "string", "description": "Zielpfad"}
        }, "required": ["from_path", "to_path"], "additionalProperties": false}})
}

// ---------------------------------------------------------------- argument helpers

/// Path, `id:…` or `rev:…` reference as a JSON body value (non-empty, no
/// control characters: those would break the `Dropbox-API-Arg` header).
fn entry_ref(args: &Value, key: &str) -> Result<String> {
    let v = req_str(args, key)?.trim();
    if v.chars().any(char::is_control) {
        bail!("`{key}` must not contain control characters");
    }
    Ok(v.to_string())
}

/// Folder path; unlike `entry_ref` the empty string is allowed (Dropbox root).
fn folder(args: &Value, key: &str) -> Result<String> {
    match args.get(key).and_then(Value::as_str) {
        Some(v) if !v.chars().any(char::is_control) => Ok(v.trim().to_string()),
        _ => bail!("missing argument `{key}`"),
    }
}

fn bool_or(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Dropdown with a default, restricted to the Activepieces options.
fn choice_or<'a>(args: &'a Value, key: &str, allowed: &[&str], default: &'a str) -> Result<&'a str> {
    match opt_str(args, key) {
        Some(v) if allowed.contains(&v) => Ok(v),
        Some(_) => bail!("`{key}` must be one of {}", allowed.join(", ")),
        None => Ok(default),
    }
}

/// Comma-separated prop -> trimmed, non-empty list (empty when unset).
fn comma_list(args: &Value, key: &str) -> Vec<String> {
    match opt_str(args, key) {
        None => Vec::new(),
        Some(v) => v
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
    }
}

/// List prop that is an array of strings or a JSON-encoded array string
/// (`parseStringArray` in the piece).
fn string_vec(args: &Value, key: &str) -> Result<Vec<String>> {
    let items = match args.get(key) {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) if !s.trim().is_empty() => serde_json::from_str(s.trim())
            .map_err(|_| anyhow::anyhow!("`{key}` must be a list"))?,
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        let Some(s) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            bail!("Every entry in `{key}` must be a non-empty text value.");
        };
        if s.chars().any(char::is_control) {
            bail!("`{key}` must not contain control characters");
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// `parseRelocationEntries`: `{from_path, to_path}` pairs.
fn relocation_entries(args: &Value, key: &str) -> Result<Vec<(String, String)>> {
    let items = match args.get(key) {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) if !s.trim().is_empty() => serde_json::from_str(s.trim())
            .map_err(|_| anyhow::anyhow!("`{key}` must be a list"))?,
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        let (Some(from), Some(to)) = (
            item["from_path"].as_str().map(str::trim).filter(|v| !v.is_empty()),
            item["to_path"].as_str().map(str::trim).filter(|v| !v.is_empty()),
        ) else {
            bail!("Every entry in `{key}` must have a From Path and a To Path.");
        };
        out.push((from.to_string(), to.to_string()));
    }
    Ok(out)
}

// ---------------------------------------------------------------- body helpers

/// `?async_job_id=…` body of the `check_*` polling endpoints.
fn poll(url: String, args: &Value) -> Result<HttpRequest> {
    Ok(HttpRequest::new("POST", url).json(&json!({
        "async_job_id": req_str(args, "async_job_id")?.trim(),
    })))
}

fn relocate(url: String, args: &Value) -> Result<HttpRequest> {
    Ok(HttpRequest::new("POST", url).json(&json!({
        "from_path": entry_ref(args, "from_path")?,
        "to_path": entry_ref(args, "to_path")?,
        "autorename": bool_or(args, "autorename", false),
    })))
}

fn batch_relocate(url: String, args: &Value, verb: &str) -> Result<HttpRequest> {
    let entries = relocation_entries(args, "entries")?;
    if entries.is_empty() {
        bail!("Provide at least one entry to {verb}.");
    }
    if entries.len() > MAX_BATCH_ENTRIES {
        bail!("Dropbox accepts at most {MAX_BATCH_ENTRIES} entries per {verb} batch.");
    }
    let list: Vec<Value> = entries
        .iter()
        .map(|(from, to)| json!({"from_path": from, "to_path": to}))
        .collect();
    Ok(HttpRequest::new("POST", url).json(&json!({
        "entries": list,
        "autorename": bool_or(args, "autorename", false),
    })))
}

fn batch_delete(url: String, args: &Value) -> Result<HttpRequest> {
    let paths = string_vec(args, "paths")?;
    if paths.is_empty() {
        bail!("Provide at least one path to delete.");
    }
    if paths.len() > MAX_BATCH_ENTRIES {
        bail!("Dropbox accepts at most {MAX_BATCH_ENTRIES} entries per delete batch.");
    }
    let entries: Vec<Value> = paths.iter().map(|p| json!({"path": p})).collect();
    Ok(HttpRequest::new("POST", url).json(&json!({"entries": entries})))
}

/// Single-entry lock request shared by lock/unlock/get lock.
fn lock_request(url: String, path: String) -> HttpRequest {
    HttpRequest::new("POST", url).json(&json!({"entries": [{"path": path}]}))
}

// ---------------------------------------------------------------- adapter

pub struct Dropbox {
    spec: ToolkitSpec,
    api_base: String,
    content_base: String,
}

impl Default for Dropbox {
    fn default() -> Self {
        Self::with_api_base("https://api.dropboxapi.com")
    }
}

impl Dropbox {
    /// `api_base` is the host root; the `/2` prefix of both Dropbox hosts is
    /// appended per call. An `api.` first DNS label is mirrored onto
    /// `content.` so tests can point one URL at a mock host; any other base is
    /// used for both hosts unchanged (single reverse-proxy setups).
    pub fn with_api_base(api_base: &str) -> Self {
        Self::with_hosts(api_base, &content_host_of(api_base))
    }

    /// Explicit pair of Dropbox hosts (JSON RPC host, byte endpoint host).
    pub fn with_hosts(api_base: &str, content_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let path_prop = || s("Pfad, z. B. /Berichte/plan.docx; akzeptiert auch id:… / Path such as /reports/plan.docx; also accepts id:…");
        let path_or_rev = || s("Pfad; akzeptiert auch id:… oder rev:… / Path; also accepts id:… or rev:…");
        let job_prop = || s("Job-ID aus dem Start des Batch-Laufs / job id returned when the batch started");
        let format_prop = || choice("Bildformat / image format", &["jpeg", "png", "webp"]);
        let size_prop = || choice("Größe / size", &[
            "w32h32", "w64h64", "w128h128", "w256h256", "w480h320", "w640h480", "w960h640",
            "w1024h768", "w2048h1536",
        ]);
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Dropbox".into(),
            description: LocalizedText::new(
                "Dateien und Ordner in Dropbox suchen, lesen, verschieben und löschen.",
                "Search, read, move and delete files and folders in Dropbox.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://www.dropbox.com/oauth2/authorize".into(),
                    token_url: "https://api.dropboxapi.com/oauth2/token".into(),
                    default_scopes: vec![
                        METADATA_W.into(),
                        METADATA_R.into(),
                        CONTENT_W.into(),
                        CONTENT_R.into(),
                    ],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![("token_access_type".into(), "offline".into())],
                }),
                // Static access token from the Dropbox App Console.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "DROPBOX_SEARCH_FILES",
                    ("Dropbox durchsuchen", "Search files"),
                    (
                        "Durchsucht Dateien und Ordner nach Namen (und Inhalt), mit Filtern für Pfad, Status, Endungen und Kategorien.",
                        "Searches files and folders by name (and content), with filters for path, status, extensions and categories.",
                    ),
                    &[
                        ("query", s("Suchtext, mindestens 3 Zeichen")),
                        ("path", s("Ordner, in dem gesucht wird; leer = ganz Dropbox")),
                        ("max_results", int("Maximale Treffer (1-1000)", 1, 1000, 100)),
                        ("file_status", choice("Nur aktive oder gelöschte Einträge", &["active", "deleted"])),
                        ("filename_only", flag("Nur Dateinamen durchsuchen", false)),
                        ("file_extensions", s("Endungen ohne Punkt, kommagetrennt, z. B. pdf,docx")),
                        ("file_categories", s("Kategorien, kommagetrennt, z. B. image,document")),
                        ("account_id", s("Ergebnisse auf dieses Konto einschränken")),
                    ],
                    &["query"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_LIST_FOLDER",
                    ("Ordner auflisten", "List folder"),
                    (
                        "Listet die Einträge eines Ordners, optional rekursiv.",
                        "Lists the entries of a folder, optionally recursive.",
                    ),
                    &[
                        ("path", s("Ordnerpfad; leerer String listet das Wurzelverzeichnis")),
                        ("recursive", flag("Unterordner einbeziehen", false)),
                        ("limit", int("Maximale Einträge pro Seite (1-2000)", 1, 2000, 2000)),
                    ],
                    &["path"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_FILE_METADATA",
                    ("Datei-Metadaten lesen", "Get file metadata"),
                    (
                        "Liest Name, ID, Pfad, Größe und Revision eines Eintrags.",
                        "Reads name, id, path, size and revision of one entry.",
                    ),
                    &[
                        ("path", path_or_rev()),
                        ("include_deleted", flag("Metadaten gelöschter Einträge zurückgeben", false)),
                        ("include_media_info", flag("Foto-/Video-Metadaten einschließen", false)),
                        ("include_has_explicit_shared_members", flag("Flag für geteilte Mitglieder einschließen", false)),
                    ],
                    &["path"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_LIST_FILE_REVISIONS",
                    ("Revisionen auflisten", "List file revisions"),
                    (
                        "Listet gespeicherte Revisionen einer Datei, neueste zuerst.",
                        "Lists the stored revisions of a file, newest first.",
                    ),
                    &[
                        ("path", path_prop()),
                        ("mode", choice("path folgt dem Pfad, id folgt der Datei über Umbenennungen hinweg", &["path", "id"])),
                        ("limit", int("Maximale Revisionen (1-100)", 1, 100, 10)),
                    ],
                    &["path"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_TEMPORARY_LINK",
                    ("Temporären Link holen", "Get temporary link"),
                    (
                        "Liefert eine kurzlebige Direkt-Download-URL für eine Datei.",
                        "Returns a short-lived direct download URL for a file.",
                    ),
                    &[("path", path_prop())],
                    &["path"],
                    &[CONTENT_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_FILE_LOCK",
                    ("Dateisperre lesen", "Get file lock"),
                    (
                        "Zeigt, ob eine Datei gesperrt ist, von wem und seit wann.",
                        "Reports whether a file is locked, by whom and since when.",
                    ),
                    &[("path", path_prop())],
                    &["path"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_THUMBNAILS_BATCH",
                    ("Vorschaubilder sammeln", "Get thumbnails batch"),
                    (
                        "Holt Vorschaubilder für bis zu 25 Bilddateien in einem Aufruf.",
                        "Fetches thumbnails for up to 25 image files in one call.",
                    ),
                    &[
                        ("paths", str_list("Bilddateien (Pfad, id:… oder rev:…), höchstens 25")),
                        ("format", format_prop()),
                        ("size", size_prop()),
                    ],
                    &["paths"],
                    &[CONTENT_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_COPY_BATCH_STATUS",
                    ("Kopier-Batch-Status lesen", "Get copy batch status"),
                    (
                        "Liest den Status eines Kopier-Batch-Laufs; jedes Ergebnis einzeln prüfen.",
                        "Reads the status of a copy batch job; check each entry individually.",
                    ),
                    &[("async_job_id", job_prop())],
                    &["async_job_id"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_MOVE_BATCH_STATUS",
                    ("Verschiebe-Batch-Status lesen", "Get move batch status"),
                    (
                        "Liest den Status eines Verschiebe-Batch-Laufs; jedes Ergebnis einzeln prüfen.",
                        "Reads the status of a move batch job; check each entry individually.",
                    ),
                    &[("async_job_id", job_prop())],
                    &["async_job_id"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_DELETE_BATCH_STATUS",
                    ("Lösch-Batch-Status lesen", "Get delete batch status"),
                    (
                        "Liest den Status eines Lösch-Batch-Laufs; jedes Ergebnis einzeln prüfen.",
                        "Reads the status of a delete batch job; check each entry individually.",
                    ),
                    &[("async_job_id", job_prop())],
                    &["async_job_id"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_CREATE_FOLDER_BATCH_STATUS",
                    ("Ordner-Batch-Status lesen", "Get create folder batch status"),
                    (
                        "Liest den Status eines Ordner-Batch-Laufs; jedes Ergebnis einzeln prüfen.",
                        "Reads the status of a create folder batch job; check each entry individually.",
                    ),
                    &[("async_job_id", job_prop())],
                    &["async_job_id"],
                    &[METADATA_R],
                    false,
                ),
                t(
                    "DROPBOX_GET_SAVE_URL_STATUS",
                    ("URL-Import-Status lesen", "Get save URL status"),
                    (
                        "Liest den Status eines laufenden URL-Imports (in_progress, complete, failed).",
                        "Reads the status of a running URL import (in_progress, complete, failed).",
                    ),
                    &[("async_job_id", job_prop())],
                    &["async_job_id"],
                    &[CONTENT_W],
                    false,
                ),
                t(
                    "DROPBOX_CREATE_FOLDER",
                    ("Ordner anlegen", "Create folder"),
                    (
                        "Legt einen neuen leeren Ordner an; braucht --confirm.",
                        "Creates a new empty folder; requires --confirm.",
                    ),
                    &[
                        ("path", s("Pfad des neuen Ordners, z. B. /Hausaufgaben/math")),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_CREATE_FOLDER_BATCH",
                    ("Ordner-Stapel anlegen", "Create folder batch"),
                    (
                        "Legt bis zu 10000 Ordner in einem Auftrag an; braucht --confirm.",
                        "Creates up to 10000 folders in one job; requires --confirm.",
                    ),
                    &[
                        ("paths", str_list("Ordnerpfade (nur Pfade, keine id:…), höchstens 10000")),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["paths"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_COPY_ENTRY",
                    ("Eintrag kopieren", "Copy entry"),
                    (
                        "Kopiert eine Datei oder einen Ordner an einen neuen Pfad; braucht --confirm.",
                        "Copies a file or folder to a new path; requires --confirm.",
                    ),
                    &[
                        ("from_path", path_prop()),
                        ("to_path", s("Zielpfad, z. B. /Archiv/plan.docx")),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["from_path", "to_path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_COPY_BATCH",
                    ("Stapel kopieren", "Copy batch"),
                    (
                        "Kopiert bis zu 1000 Einträge in einem Auftrag; braucht --confirm.",
                        "Copies up to 1000 entries in one job; requires --confirm.",
                    ),
                    &[
                        ("entries", relocation_list(MAX_BATCH_ENTRIES)),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["entries"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_MOVE_ENTRY",
                    ("Eintrag verschieben", "Move entry"),
                    (
                        "Verschiebt oder benennt eine Datei bzw. einen Ordner um; braucht --confirm.",
                        "Moves or renames a file or folder; requires --confirm.",
                    ),
                    &[
                        ("from_path", path_prop()),
                        ("to_path", s("Zielpfad, z. B. /Archiv/plan-2026.docx")),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["from_path", "to_path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_MOVE_BATCH",
                    ("Stapel verschieben", "Move batch"),
                    (
                        "Verschiebt bis zu 1000 Einträge in einem Auftrag; braucht --confirm.",
                        "Moves up to 1000 entries in one job; requires --confirm.",
                    ),
                    &[
                        ("entries", relocation_list(MAX_BATCH_ENTRIES)),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                    ],
                    &["entries"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_DELETE_ENTRY",
                    ("Eintrag löschen", "Delete entry"),
                    (
                        "Löscht eine Datei oder einen Ordner (in den Papierkorb); braucht --confirm.",
                        "Deletes a file or folder (to trash); requires --confirm.",
                    ),
                    &[("path", path_prop())],
                    &["path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_DELETE_BATCH",
                    ("Stapel löschen", "Delete batch"),
                    (
                        "Löscht bis zu 1000 Einträge in einem Auftrag; braucht --confirm.",
                        "Deletes up to 1000 entries in one job; requires --confirm.",
                    ),
                    &[("paths", str_list("Pfade oder id:…, höchstens 1000"))],
                    &["paths"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_RESTORE_FILE_REVISION",
                    ("Revision wiederherstellen", "Restore file revision"),
                    (
                        "Stellt eine Datei auf eine frühere Revision zurück; braucht --confirm.",
                        "Restores a file to a previous revision; requires --confirm.",
                    ),
                    &[
                        ("path", s("Dateipfad (keine id:… oder rev:… Werte)")),
                        ("rev", s("Revision aus DROPBOX_LIST_FILE_REVISIONS")),
                    ],
                    &["path", "rev"],
                    &[CONTENT_W],
                    true,
                ),
                t(
                    "DROPBOX_CREATE_COPY_REFERENCE",
                    ("Kopier-Referenz erzeugen", "Create copy reference"),
                    (
                        "Erzeugt eine Kopier-Referenz für einen quellkonten-übergreifenden Kopiervorgang; braucht --confirm.",
                        "Mints a copy reference for a cross-account copy; requires --confirm.",
                    ),
                    &[("path", path_or_rev())],
                    &["path"],
                    &[METADATA_R],
                    true,
                ),
                t(
                    "DROPBOX_SAVE_COPY_REFERENCE",
                    ("Kopier-Referenz speichern", "Save copy reference"),
                    (
                        "Speichert eine Kopier-Referenz aus anderem Konto in diesem Konto; braucht --confirm.",
                        "Saves a copy reference from another account into this one; requires --confirm.",
                    ),
                    &[
                        ("copy_reference", s("Referenz aus DROPBOX_CREATE_COPY_REFERENCE")),
                        ("path", s("Zielpfad (nur Pfade, keine id:… Werte)")),
                    ],
                    &["copy_reference", "path"],
                    &[CONTENT_W],
                    true,
                ),
                t(
                    "DROPBOX_LOCK_FILE",
                    ("Datei sperren", "Lock file"),
                    (
                        "Sperrt eine Datei gegen Bearbeitung durch andere; braucht --confirm.",
                        "Locks a file against edits by other members; requires --confirm.",
                    ),
                    &[("path", path_prop())],
                    &["path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_UNLOCK_FILE",
                    ("Datei entsperren", "Unlock file"),
                    (
                        "Gibt eine Dateisperre wieder frei; braucht --confirm.",
                        "Releases a lock on a file; requires --confirm.",
                    ),
                    &[("path", path_prop())],
                    &["path"],
                    &[METADATA_W],
                    true,
                ),
                t(
                    "DROPBOX_SAVE_URL",
                    ("URL importieren", "Save URL"),
                    (
                        "Lädt Dropbox eine öffentliche URL serverseitig als Datei laden; braucht --confirm.",
                        "Has Dropbox fetch a public URL server-side as a file; requires --confirm.",
                    ),
                    &[
                        ("path", s("Zielpfad (nur Pfade, keine id:… Werte)")),
                        ("url", s("Öffentlich erreichbare URL")),
                    ],
                    &["path", "url"],
                    &[CONTENT_W],
                    true,
                ),
                t(
                    "DROPBOX_CREATE_UPLOAD_LINK",
                    ("Upload-Link erzeugen", "Create upload link"),
                    (
                        "Erzeugt eine temporäre URL, die Uploads ohne Anmeldung annimmt; braucht --confirm.",
                        "Creates a temporary URL that accepts uploads without login; requires --confirm.",
                    ),
                    &[
                        ("path", s("Pfad, unter dem die hochgeladene Datei landet; auch id:…")),
                        ("mode", choice("add ergänzt, overwrite ersetzt", &["add", "overwrite"])),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                        ("duration", int("Gültigkeit in Sekunden (60-14400)", 60, 14400, 14400)),
                    ],
                    &["path"],
                    &[CONTENT_W],
                    true,
                ),
                t(
                    "DROPBOX_CREATE_TEXT_FILE",
                    ("Textdatei anlegen", "Create text file"),
                    (
                        "Schreibt Text in eine neue Datei im Dropbox-Konto; braucht --confirm.",
                        "Writes text into a new file in the Dropbox account; requires --confirm.",
                    ),
                    &[
                        ("path", s("Pfad der neuen Datei, z. B. /Notizen/heute.md")),
                        ("text", s("Inhalt der Datei (UTF-8)")),
                        ("autorename", flag("Bei Konflikt automatisch umbenennen", false)),
                        ("mute", flag("Keine Client-Benachrichtigung auslösen", false)),
                        ("strict_conflict", flag("Konflikterkennung strenger prüfen", false)),
                    ],
                    &["path", "text"],
                    &[CONTENT_W],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: format!("{}/2", api_base.trim_end_matches('/')),
            content_base: format!("{}/2", content_base.trim_end_matches('/')),
        }
    }

    /// JSON RPC endpoint under the `api.` host.
    fn api(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    /// Endpoint under the `content.` host.
    fn content(&self, path: &str) -> String {
        format!("{}{}", self.content_base, path)
    }

    fn request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/search.ts
            "DROPBOX_SEARCH_FILES" => {
                let mut options = Map::new();
                options.insert("path".into(), json!(folder(args, "path")?));
                options.insert(
                    "max_results".into(),
                    json!(opt_u64(args, "max_results", 100, 1, 1000)),
                );
                options.insert(
                    "file_status".into(),
                    json!(choice_or(args, "file_status", &["active", "deleted"], "active")?),
                );
                options.insert(
                    "filename_only".into(),
                    json!(bool_or(args, "filename_only", false)),
                );
                // The piece declares an `order_by` prop but its run() never
                // forwards it to the API, so it is not ported.
                let extensions = comma_list(args, "file_extensions");
                if !extensions.is_empty() {
                    options.insert("file_extensions".into(), json!(extensions));
                }
                let categories = comma_list(args, "file_categories");
                if !categories.is_empty() {
                    options.insert("file_categories".into(), json!(categories));
                }
                if let Some(a) = opt_str(args, "account_id") {
                    options.insert("account_id".into(), json!(a.trim()));
                }
                HttpRequest::new("POST", self.api("/files/search_v2")).json(&json!({
                    "query": req_str(args, "query")?.trim(),
                    "options": Value::Object(options),
                }))
            }
            // AP: src/lib/actions/list-a-folder.ts
            "DROPBOX_LIST_FOLDER" => HttpRequest::new("POST", self.api("/files/list_folder"))
                .json(&json!({
                    "path": folder(args, "path")?,
                    "recursive": bool_or(args, "recursive", false),
                    "limit": opt_u64(args, "limit", 2000, 1, 2000),
                })),
            // AP: src/lib/actions/get-file-metadata.ts
            "DROPBOX_GET_FILE_METADATA" => HttpRequest::new("POST", self.api("/files/get_metadata"))
                .json(&json!({
                    "path": entry_ref(args, "path")?,
                    "include_deleted": bool_or(args, "include_deleted", false),
                    "include_media_info": bool_or(args, "include_media_info", false),
                    "include_has_explicit_shared_members":
                        bool_or(args, "include_has_explicit_shared_members", false),
                })),
            // AP: src/lib/actions/list-file-revisions.ts
            "DROPBOX_LIST_FILE_REVISIONS" => {
                let mode = choice_or(args, "mode", &["path", "id"], "path")?;
                let limit = opt_u64(args, "limit", 10, 1, 100);
                HttpRequest::new("POST", self.api("/files/list_revisions")).json(&json!({
                    "path": entry_ref(args, "path")?,
                    "mode": mode,
                    "limit": limit,
                }))
            }
            // AP: src/lib/actions/get-file-link.ts
            "DROPBOX_GET_TEMPORARY_LINK" => {
                HttpRequest::new("POST", self.api("/files/get_temporary_link"))
                    .json(&json!({"path": entry_ref(args, "path")?}))
            }
            // AP: src/lib/actions/get-file-lock.ts
            "DROPBOX_GET_FILE_LOCK" => {
                lock_request(self.api("/files/get_file_lock_batch"), entry_ref(args, "path")?)
            }
            // AP: src/lib/actions/lock-file.ts
            "DROPBOX_LOCK_FILE" => {
                lock_request(self.api("/files/lock_file_batch"), entry_ref(args, "path")?)
            }
            // AP: src/lib/actions/unlock-file.ts
            "DROPBOX_UNLOCK_FILE" => {
                lock_request(self.api("/files/unlock_file_batch"), entry_ref(args, "path")?)
            }
            // AP: src/lib/actions/get-thumbnails-batch.ts
            "DROPBOX_GET_THUMBNAILS_BATCH" => {
                let paths = string_vec(args, "paths")?;
                if paths.is_empty() {
                    bail!("Provide at least one path to thumbnail.");
                }
                if paths.len() > MAX_THUMBNAILS {
                    bail!("Dropbox accepts at most {MAX_THUMBNAILS} paths per thumbnail batch.");
                }
                let format = choice_or(args, "format", &["jpeg", "png", "webp"], "jpeg")?;
                let size = choice_or(
                    args,
                    "size",
                    &[
                        "w32h32", "w64h64", "w128h128", "w256h256", "w480h320", "w640h480",
                        "w960h640", "w1024h768", "w2048h1536",
                    ],
                    "w64h64",
                )?;
                let entries: Vec<Value> = paths
                    .iter()
                    .map(|p| json!({"path": p, "format": format, "size": size}))
                    .collect();
                HttpRequest::new("POST", self.content("/files/get_thumbnail_batch"))
                    .json(&json!({"entries": entries}))
            }
            // AP: src/lib/actions/get-copy-batch-status.ts
            "DROPBOX_GET_COPY_BATCH_STATUS" => {
                poll(self.api("/files/copy_batch/check_v2"), args)?
            }
            // AP: src/lib/actions/get-move-batch-status.ts
            "DROPBOX_GET_MOVE_BATCH_STATUS" => {
                poll(self.api("/files/move_batch/check_v2"), args)?
            }
            // AP: src/lib/actions/get-delete-batch-status.ts
            "DROPBOX_GET_DELETE_BATCH_STATUS" => {
                poll(self.api("/files/delete_batch/check"), args)?
            }
            // AP: src/lib/actions/get-create-folder-batch-status.ts
            "DROPBOX_GET_CREATE_FOLDER_BATCH_STATUS" => {
                poll(self.api("/files/create_folder_batch/check"), args)?
            }
            // AP: src/lib/actions/get-save-url-status.ts
            "DROPBOX_GET_SAVE_URL_STATUS" => {
                poll(self.api("/files/save_url/check_job_status"), args)?
            }
            // AP: src/lib/actions/create-new-folder.ts
            "DROPBOX_CREATE_FOLDER" => HttpRequest::new("POST", self.api("/files/create_folder_v2"))
                .json(&json!({
                    "autorename": bool_or(args, "autorename", false),
                    "path": entry_ref(args, "path")?,
                })),
            // AP: src/lib/actions/start-create-folder-batch.ts
            "DROPBOX_CREATE_FOLDER_BATCH" => {
                let paths = string_vec(args, "paths")?;
                if paths.is_empty() {
                    bail!("Provide at least one folder path to create.");
                }
                if paths.len() > MAX_BATCH_FOLDERS {
                    bail!("Dropbox accepts at most {MAX_BATCH_FOLDERS} folders per create folder batch.");
                }
                HttpRequest::new("POST", self.api("/files/create_folder_batch")).json(&json!({
                    "paths": paths,
                    "autorename": bool_or(args, "autorename", false),
                }))
            }
            // AP: src/lib/actions/copy-entry.ts
            "DROPBOX_COPY_ENTRY" => relocate(self.api("/files/copy_v2"), args)?,
            // AP: src/lib/actions/start-copy-batch.ts
            "DROPBOX_COPY_BATCH" => batch_relocate(self.api("/files/copy_batch_v2"), args, "copy")?,
            // AP: src/lib/actions/move-entry.ts
            "DROPBOX_MOVE_ENTRY" => relocate(self.api("/files/move_v2"), args)?,
            // AP: src/lib/actions/start-move-batch.ts
            "DROPBOX_MOVE_BATCH" => batch_relocate(self.api("/files/move_batch_v2"), args, "move")?,
            // AP: src/lib/actions/delete-entry.ts
            "DROPBOX_DELETE_ENTRY" => HttpRequest::new("POST", self.api("/files/delete_v2"))
                .json(&json!({"path": entry_ref(args, "path")?})),
            // AP: src/lib/actions/start-delete-batch.ts
            "DROPBOX_DELETE_BATCH" => batch_delete(self.api("/files/delete_batch"), args)?,
            // AP: src/lib/actions/restore-file.ts
            "DROPBOX_RESTORE_FILE_REVISION" => HttpRequest::new("POST", self.api("/files/restore"))
                .json(&json!({
                    "path": entry_ref(args, "path")?,
                    "rev": req_str(args, "rev")?.trim(),
                })),
            // AP: src/lib/actions/create-copy-reference.ts
            "DROPBOX_CREATE_COPY_REFERENCE" => {
                HttpRequest::new("POST", self.api("/files/copy_reference/get"))
                    .json(&json!({"path": entry_ref(args, "path")?}))
            }
            // AP: src/lib/actions/save-copy-reference.ts
            "DROPBOX_SAVE_COPY_REFERENCE" => {
                HttpRequest::new("POST", self.api("/files/copy_reference/save")).json(&json!({
                    "copy_reference": req_str(args, "copy_reference")?.trim(),
                    "path": entry_ref(args, "path")?,
                }))
            }
            // AP: src/lib/actions/save-url.ts
            "DROPBOX_SAVE_URL" => HttpRequest::new("POST", self.api("/files/save_url")).json(&json!({
                "path": entry_ref(args, "path")?,
                "url": req_str(args, "url")?.trim(),
            })),
            // AP: src/lib/actions/create-upload-link.ts
            "DROPBOX_CREATE_UPLOAD_LINK" => HttpRequest::new(
                "POST",
                self.api("/files/get_temporary_upload_link"),
            )
            .json(&json!({
                "commit_info": {
                    "path": entry_ref(args, "path")?,
                    "mode": choice_or(args, "mode", &["add", "overwrite"], "add")?,
                    "autorename": bool_or(args, "autorename", false),
                },
                "duration": opt_u64(args, "duration", 14400, 60, 14400),
            })),
            // AP: src/lib/actions/create-new-text-file.ts
            "DROPBOX_CREATE_TEXT_FILE" => {
                let path = entry_ref(args, "path")?;
                header_safe("path", &path)?;
                let arg = json!({
                    "autorename": bool_or(args, "autorename", false),
                    "path": path,
                    "mode": "add",
                    "mute": bool_or(args, "mute", false),
                    "strict_conflict": bool_or(args, "strict_conflict", false),
                });
                let mut request = HttpRequest::new("POST", self.content("/files/upload"))
                    .header("Dropbox-API-Arg", arg.to_string())
                    .header("Content-Type", "application/octet-stream");
                request.body = Some(text_body(args)?);
                request
            }
            other => bail!("unknown Dropbox tool `{other}`"),
        })
    }
}

/// Raw UTF-8 file content of `text`.
fn text_body(args: &Value) -> Result<Vec<u8>> {
    Ok(req_str(args, "text")?.as_bytes().to_vec())
}

/// `https://api.example.test` -> `https://content.example.test`.
fn content_host_of(api_base: &str) -> String {
    match api_base.split_once("://") {
        Some((scheme, host)) if host.starts_with("api.") => {
            format!("{scheme}://content.{}", &host["api.".len()..])
        }
        _ => api_base.to_string(),
    }
}

// ---------------------------------------------------------------- mappers

/// Dropbox entry metadata, reduced to the fields the piece surfaces.
fn flatten_entry(e: &Value) -> Value {
    json!({
        "kind": e[".tag"],
        "name": e["name"],
        "id": e["id"],
        "path_lower": e["path_lower"],
        "path_display": e["path_display"],
        "size": e["size"],
        "rev": e["rev"],
        "client_modified": e["client_modified"],
        "server_modified": e["server_modified"],
        "content_hash": e["content_hash"],
        "is_downloadable": e["is_downloadable"],
    })
}

fn entry_list(entries: &Value) -> Vec<Value> {
    entries
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(flatten_entry)
        .collect()
}

/// `unwrapEntry` from the piece: the first batch result entry, with a failure
/// tag turned into an error.
fn unwrap_entry(v: &Value, action: &str) -> Result<Value> {
    let entry = v["entries"]
        .as_array()
        .and_then(|e| e.first())
        .ok_or_else(|| anyhow::anyhow!("Dropbox returned no result entry for {action}."))?;
    if entry[".tag"] == json!("failure") {
        bail!("Dropbox could not {action}: {}", entry["failure"]);
    }
    Ok(json!({
        "status": entry[".tag"],
        "lock": entry["lock"],
        "failure": entry["failure"],
    }))
}

impl ToolkitAdapter for Dropbox {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.request(tool, args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "DROPBOX_SEARCH_FILES" => {
                let items: Vec<Value> = v["matches"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    // search v2 nests the entry under metadata.metadata.
                    .map(|m| flatten_entry(&m["metadata"]["metadata"]))
                    .collect();
                json!({
                    "items": items,
                    "count": items.len(),
                    "has_more": v["has_more"],
                    "cursor": v["cursor"],
                })
            }
            "DROPBOX_LIST_FOLDER" => {
                let items = entry_list(&v["entries"]);
                json!({
                    "items": items,
                    "count": items.len(),
                    "has_more": v["has_more"],
                    "cursor": v["cursor"],
                })
            }
            "DROPBOX_GET_FILE_METADATA" => flatten_entry(&v),
            "DROPBOX_LIST_FILE_REVISIONS" => {
                let items = entry_list(&v["entries"]);
                json!({
                    "items": items,
                    "count": items.len(),
                    "is_deleted": v["is_deleted"],
                    "has_more": v["has_more"],
                    "cursor": v["cursor"],
                })
            }
            "DROPBOX_GET_TEMPORARY_LINK" => json!({
                "link": v["link"],
                "metadata": flatten_entry(&v["metadata"]),
            }),
            "DROPBOX_GET_FILE_LOCK" | "DROPBOX_LOCK_FILE" | "DROPBOX_UNLOCK_FILE" => {
                let action = match tool {
                    "DROPBOX_GET_FILE_LOCK" => "read the file lock",
                    "DROPBOX_LOCK_FILE" => "lock the file",
                    _ => "unlock the file",
                };
                unwrap_entry(&v, action)?
            }
            "DROPBOX_GET_THUMBNAILS_BATCH" => {
                let items: Vec<Value> = v["entries"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|e| {
                        json!({
                            "status": e[".tag"],
                            "metadata": flatten_entry(&e["metadata"]),
                            "thumbnail_base64": e["thumbnail"],
                            "failure": e["failure"],
                        })
                    })
                    .collect();
                json!({"items": items, "count": items.len()})
            }
            "DROPBOX_CREATE_FOLDER" => flatten_entry(&v["metadata"]),
            "DROPBOX_COPY_ENTRY" | "DROPBOX_MOVE_ENTRY" | "DROPBOX_DELETE_ENTRY" => {
                flatten_entry(&v["metadata"])
            }
            "DROPBOX_RESTORE_FILE_REVISION" | "DROPBOX_CREATE_TEXT_FILE" => flatten_entry(&v),
            "DROPBOX_SAVE_COPY_REFERENCE" => flatten_entry(&v["metadata"]),
            "DROPBOX_CREATE_COPY_REFERENCE" => json!({
                "copy_reference": v["copy_reference"],
                "expires": v["expires"],
                "metadata": flatten_entry(&v["metadata"]),
            }),
            // Launch and polling results are tagged unions (".tag": in_progress
            // | complete | failed | async_job_id); they pass through unchanged,
            // exactly like the piece returns response.body.
            "DROPBOX_COPY_BATCH" | "DROPBOX_MOVE_BATCH" | "DROPBOX_DELETE_BATCH"
            | "DROPBOX_CREATE_FOLDER_BATCH" | "DROPBOX_GET_COPY_BATCH_STATUS"
            | "DROPBOX_GET_MOVE_BATCH_STATUS" | "DROPBOX_GET_DELETE_BATCH_STATUS"
            | "DROPBOX_GET_CREATE_FOLDER_BATCH_STATUS" | "DROPBOX_SAVE_URL"
            | "DROPBOX_GET_SAVE_URL_STATUS" | "DROPBOX_CREATE_UPLOAD_LINK" => v,
            other => bail!("unknown Dropbox tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_host_follows_the_api_host() {
        assert_eq!(
            content_host_of("https://api.dropboxapi.com"),
            "https://content.dropboxapi.com"
        );
        assert_eq!(content_host_of("https://dropbox-gw.corp"), "https://dropbox-gw.corp");
        let d = Dropbox::with_api_base("https://api.dropbox.test");
        assert_eq!(d.api("/files/delete_v2"), "https://api.dropbox.test/2/files/delete_v2");
        assert_eq!(
            d.content("/files/upload"),
            "https://content.dropbox.test/2/files/upload"
        );
    }

    #[test]
    fn argument_validation() {
        let d = Dropbox::default();
        assert!(d.build_request("DROPBOX_SEARCH_FILES", &json!({"query": "x", "file_status": "bogus"})).is_err());
        assert!(d.build_request("DROPBOX_GET_FILE_METADATA", &json!({"path": "id:x\ny"})).is_err());
        assert!(d.build_request("DROPBOX_GET_FILE_METADATA", &json!({})).is_err());
        assert!(d.build_request("DROPBOX_LIST_FOLDER", &json!({"path": ""})).is_ok());
        assert!(d
            .build_request("DROPBOX_GET_THUMBNAILS_BATCH", &json!({"paths": []}))
            .is_err());
        assert!(d
            .build_request(
                "DROPBOX_GET_THUMBNAILS_BATCH",
                &json!({"paths": (0..26).map(|i| format!("/a{i}.jpg")).collect::<Vec<_>>()})
            )
            .is_err());
        assert!(d
            .build_request("DROPBOX_COPY_BATCH", &json!({"entries": [{"from_path": "/a"}]}))
            .is_err());
        assert!(d.build_request("DROPBOX_DELETE_BATCH", &json!({"paths": ""})).is_err());
        // A JSON-encoded list string is accepted, like parseStringArray does.
        assert!(d
            .build_request("DROPBOX_DELETE_BATCH", &json!({"paths": "[\"/a\", \"/b\"]"}))
            .is_ok());
        // The text upload carries the arg in a header: no CR/LF allowed.
        assert!(d
            .build_request("DROPBOX_CREATE_TEXT_FILE", &json!({"path": "/a\r\nX: y", "text": "hi"}))
            .is_err());
        let r = d
            .build_request(
                "DROPBOX_CREATE_TEXT_FILE",
                &json!({"path": "/notes/today.md", "text": "hällo"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://content.dropboxapi.com/2/files/upload");
        assert_eq!(r.headers["dropbox-api-arg"], r#"{"autorename":false,"mode":"add","mute":false,"path":"/notes/today.md","strict_conflict":false}"#);
        assert_eq!(r.headers["content-type"], "application/octet-stream");
        assert_eq!(r.body.as_deref(), Some("hällo".as_bytes()));
    }
}
