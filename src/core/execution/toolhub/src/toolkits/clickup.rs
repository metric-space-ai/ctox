//! ClickUp toolkit (REST API v2).
//! Action set modelled on Activepieces `piece-clickup` (MIT).
//!
//! Auth: ClickUp OAuth2 app exactly as the piece declares it — authorize
//! `https://app.clickup.com/api`, token `https://api.clickup.com/api/v2/oauth/token`,
//! no scopes. Every request carries the access token as `Authorization: Bearer <token>`,
//! which is what `callClickUpApi`'s `AuthenticationType.BEARER_TOKEN` produces.
//! ClickUp OAuth2 apps have no granular scopes, so the `scopes` of every tool carry
//! the permission level the call needs (`read` / `write`), like the Linear port.
//!
//! All endpoints below are the `callClickUpApi` calls of the piece, i.e.
//! `https://api.clickup.com/api/v2/<path>`; the chat actions of the piece
//! (`callClickUpApi3`, `/api/v3/`) are not ported.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "clickup";

const READ: &str = "read";
const WRITE: &str = "write";

// ---------------------------------------------------------------- schema fragments

/// `flag` is a boolean property with a default (Activepieces `Property.Checkbox`).
fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// ISO 8601 date-time (`Property.DateTime`).
fn datetime(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "format": "date-time"})
}

/// Assignee / user id list; ClickUp user ids are numbers.
fn num_list(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": desc})
}

/// Custom fields keyed by field id (`Property.DynamicProperties` in the piece).
fn custom_fields() -> Value {
    json!({"type": "object",
        "description": "Benutzerdefinierte Felder als {\"<Feld-ID>\": Wert}; \
                        Feld-IDs aus CLICKUP_GET_LIST_CUSTOM_FIELDS.",
        "additionalProperties": true})
}

// ---------------------------------------------------------------- arg helpers

fn flag_value(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Number list; a single number is accepted as one element (multi-select props).
fn num_vec(args: &Value, key: &str) -> Vec<i64> {
    match args.get(key) {
        Some(Value::Number(n)) => n.as_i64().into_iter().collect(),
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_i64()).collect(),
        _ => Vec::new(),
    }
}

fn opt_i64(args: &Value, key: &str) -> Result<Option<i64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| anyhow!("`{key}` must be a number")),
    }
}

/// ISO 8601 -> epoch milliseconds (`new Date(v).getTime()` / `dayjs(v).valueOf()`).
/// `iso_millis` counts from the civil year-0 base (its `millis_to_iso` undoes
/// that), so shift by the days from year 0 to 1970-01-01 to get true epoch ms.
fn millis(args: &Value, key: &str) -> Result<Option<i64>> {
    const YEAR0_TO_EPOCH_MS: i64 = 719_468 * 86_400_000;
    match opt_str(args, key) {
        Some(v) => Ok(Some(
            crate::toolkits::google_calendar::iso_millis(v)? - YEAR0_TO_EPOCH_MS,
        )),
        None => Ok(None),
    }
}

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_num(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if let Some(v) = opt_i64(args, key)? {
        m.insert(field.into(), json!(v));
    }
    Ok(())
}

/// `encodeURIComponent(tag_name)` from the piece: every byte outside the RFC 3986
/// unreserved set is percent-encoded, so `/`, `?` and `#` cannot escape the path.
fn tag_segment(args: &Value, key: &str) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| anyhow!("missing argument `{key}`"))?;
    if v.len() > 200
        || v.bytes()
            .any(|b| b < 0x20 || b == 0x7f || matches!(b, b'/' | b'?' | b'#' | b'%'))
    {
        bail!("invalid argument `{key}`");
    }
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(out)
}

/// `{id: value}` map -> the `[{id, value}, …]` array the ClickUp API wants.
fn custom_field_array(args: &Value) -> Option<Value> {
    match args.get("custom_fields") {
        Some(Value::Object(cf)) if !cf.is_empty() => Some(Value::Array(
            cf.iter().map(|(id, value)| json!({"id": id, "value": value})).collect(),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------- adapter

pub struct ClickUp {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for ClickUp {
    fn default() -> Self {
        Self::with_api_base("https://api.clickup.com")
    }
}

#[allow(clippy::too_many_lines)]
impl ClickUp {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "ClickUp".into(),
            description: LocalizedText::new(
                "ClickUp: Spaces, Folder, Listen, Aufgaben, Kommentare und Zeiteinträge lesen und ändern.",
                "ClickUp: read and change spaces, folders, lists, tasks, comments and time entries.",
            ),
            auth_schemes: vec![AuthScheme::OAuth2(OAuth2Scheme {
                authorize_url: "https://app.clickup.com/api".into(),
                token_url: "https://api.clickup.com/api/v2/oauth/token".into(),
                default_scopes: vec![],
                pkce: false,
                scope_separator: " ".into(),
                refreshable: true,
                extra_authorize_params: vec![],
            })],
            tools: vec![
                t(
                    "CLICKUP_LIST_WORKSPACES",
                    ("Workspaces auflisten", "List workspaces"),
                    (
                        "Listet die Workspaces (Teams) des Kontos mit ihren Mitgliedern.",
                        "Lists the workspaces (teams) of the account with their members.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_SPACES",
                    ("Spaces auflisten", "List spaces"),
                    (
                        "Listet die Spaces eines Workspaces.",
                        "Lists the spaces of a workspace.",
                    ),
                    &[("workspace_id", s("Workspace-ID (aus CLICKUP_LIST_WORKSPACES)"))],
                    &["workspace_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_SPACE",
                    ("Space lesen", "Get space"),
                    ("Liest einen Space per ID.", "Reads one space by ID."),
                    &[("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_FOLDERS",
                    ("Folder auflisten", "List folders"),
                    (
                        "Listet die Folder eines Spaces.",
                        "Lists the folders of a space.",
                    ),
                    &[("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_FOLDER_LISTS",
                    ("Listen im Folder auflisten", "List folder lists"),
                    (
                        "Listet die Listen innerhalb eines Folders.",
                        "Lists the lists inside a folder.",
                    ),
                    &[("folder_id", s("Folder-ID (aus CLICKUP_GET_FOLDERS)"))],
                    &["folder_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_FOLDERLESS_LISTS",
                    ("Folderlose Listen auflisten", "List folderless lists"),
                    (
                        "Listet die Listen direkt im Space, außerhalb jedes Folders.",
                        "Lists the lists directly in a space, outside any folder.",
                    ),
                    &[("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_LIST",
                    ("Liste lesen", "Get list"),
                    ("Liest eine Liste per ID.", "Reads one list by ID."),
                    &[("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)"))],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_LIST_CUSTOM_FIELDS",
                    ("Benutzerdefinierte Felder lesen", "Get list custom fields"),
                    (
                        "Listet die benutzerdefinierten Felder einer Liste.",
                        "Lists the custom fields available on a list.",
                    ),
                    &[("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)"))],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_TASK",
                    ("Aufgabe lesen", "Get task"),
                    (
                        "Liest eine Aufgabe per ID, optional mit Subtasks.",
                        "Reads one task by ID, optionally with its subtasks.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("include_subtasks", flag("Subtasks einschließen", false)),
                    ],
                    &["task_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_LIST_LIST_TASKS",
                    ("Aufgaben einer Liste auflisten", "List list tasks"),
                    (
                        "Listet die Aufgaben einer bestimmten Liste, seitenweise.",
                        "Lists the tasks of one specific list, page by page.",
                    ),
                    &[
                        ("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)")),
                        ("page", int("Seite, beginnt bei 0", 0, 100, 0)),
                        ("reverse", flag("Reihenfolge umkehren", false)),
                        ("include_closed", flag("Abgeschlossene Aufgaben einschließen", false)),
                        ("subtasks", flag("Subtasks einschließen", false)),
                        ("order_by", choice("Sortierung", &["id", "created", "updated", "due_date"])),
                    ],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_LIST_WORKSPACE_TASKS",
                    ("Aufgaben im Workspace suchen", "List workspace tasks"),
                    (
                        "Sucht Aufgaben im Workspace nach Ort, Personen und Tags.",
                        "Searches tasks across a workspace by location, people and tags.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (aus CLICKUP_LIST_WORKSPACES)")),
                        ("space_ids", str_list("Nur Aufgaben dieser Spaces")),
                        ("folder_ids", str_list("Nur Aufgaben dieser Folder")),
                        ("list_ids", str_list("Nur Aufgaben dieser Listen")),
                        ("assignees", num_list("Nur Aufgaben dieser Nutzer-IDs")),
                        ("tags", str_list("Nur Aufgaben mit diesen Tags")),
                        ("page", int("Seite, beginnt bei 0 (bis 100 Aufgaben pro Seite)", 0, 100, 0)),
                        ("reverse", flag("Reihenfolge umkehren", false)),
                        ("include_closed", flag("Abgeschlossene Aufgaben einschließen", false)),
                        ("order_by", choice("Sortierung", &["id", "created", "updated", "due_date"])),
                    ],
                    &["workspace_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_TASK_COMMENTS",
                    ("Aufgabenkommentare lesen", "Get task comments"),
                    (
                        "Listet die Kommentare einer Aufgabe.",
                        "Lists the comments on a task.",
                    ),
                    &[("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL"))],
                    &["task_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_TASK_MEMBERS",
                    ("Aufgabenmitglieder lesen", "Get task members"),
                    (
                        "Listet die Nutzer mit Zugriff auf eine Aufgabe.",
                        "Lists the users who have access to a task.",
                    ),
                    &[("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL"))],
                    &["task_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_GET_SPACE_TAGS",
                    ("Space-Tags auflisten", "List space tags"),
                    (
                        "Listet die Tags, die in einem Space definiert sind.",
                        "Lists the tags defined in a space.",
                    ),
                    &[("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)"))],
                    &["space_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_LIST_TIME_ENTRIES",
                    ("Zeiteinträge auflisten", "List time entries"),
                    (
                        "Listet getrackte Zeit eines Workspaces, nach Zeitraum, Ort und Person.",
                        "Lists tracked time in a workspace, by date range, place and person.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (aus CLICKUP_LIST_WORKSPACES)")),
                        ("start_date", datetime("Einträge ab diesem Zeitpunkt; leer: letzte 30 Tage")),
                        ("end_date", datetime("Einträge bis zu diesem Zeitpunkt; leer: jetzt")),
                        ("task_id", s("Nur Einträge dieser Aufgabe")),
                        ("list_id", s("Nur Einträge dieser Liste")),
                        ("folder_id", s("Nur Einträge dieses Folders")),
                        ("space_id", s("Nur Einträge dieses Spaces")),
                        ("assignee", num_list("Nur Einträge dieser Nutzer-IDs")),
                        ("include_task_tags", flag("Tags der Aufgaben mitliefern", false)),
                        ("include_location_names", flag("List-, Folder- und Space-Namen mitliefern", false)),
                    ],
                    &["workspace_id"],
                    &[READ],
                    false,
                ),
                t(
                    "CLICKUP_CREATE_TASK",
                    ("Aufgabe anlegen", "Create task"),
                    (
                        "Legt eine neue Aufgabe in einer Liste an; braucht --confirm.",
                        "Creates a new task in a list; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)")),
                        ("name", s("Titel der Aufgabe")),
                        ("description", s("Beschreibung (Markdown bei use_markdown)")),
                        ("use_markdown", flag("Beschreibung als Markdown senden", false)),
                        ("status_id", s("Statusname der Liste, z. B. \"to do\" (aus CLICKUP_GET_LIST)")),
                        ("priority_id", int("1 dringend, 2 hoch, 3 normal, 4 niedrig", 1, 4, 3)),
                        ("assignee_id", num_list("Nutzer-IDs der Assignees")),
                        ("due_date", datetime("Fälligkeitszeitpunkt, ISO 8601")),
                        ("due_date_time", flag("Fälligkeit mit Uhrzeit", false)),
                        ("start_date", datetime("Startzeitpunkt, ISO 8601")),
                        ("start_date_time", flag("Start mit Uhrzeit", false)),
                        ("time_estimate", int("Zeitschätzung in Millisekunden", 0, 4_000_000_000, 0)),
                        ("check_required_custom_fields", flag("Pflichtfelder wieder prüfen", false)),
                        ("custom_fields", custom_fields()),
                    ],
                    &["list_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_UPDATE_TASK",
                    ("Aufgabe ändern", "Update task"),
                    (
                        "Ändert nur die übergebenen Felder einer Aufgabe; braucht --confirm.",
                        "Changes only the fields you pass on a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("name", s("Neuer Titel")),
                        ("description", s("Neue Beschreibung")),
                        ("status_id", s("Neuer Statusname")),
                        ("priority_id", int("1 dringend, 2 hoch, 3 normal, 4 niedrig", 1, 4, 3)),
                        ("add_assignee", num_list("Nutzer-IDs, die hinzukommen")),
                        ("rem_assignee", num_list("Nutzer-IDs, die entfernt werden")),
                    ],
                    &["task_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_SET_TASK_STATUS",
                    ("Aufgabenstatus setzen", "Set task status"),
                    (
                        "Setzt den Status einer Aufgabe; braucht --confirm.",
                        "Sets the status of a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("status_id", s("Statusname der Liste, z. B. \"in progress\" (aus CLICKUP_GET_LIST)")),
                    ],
                    &["task_id", "status_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_DELETE_TASK",
                    ("Aufgabe löschen", "Delete task"),
                    (
                        "Löscht eine Aufgabe unwiderruflich; braucht --confirm.",
                        "Deletes a task permanently; requires --confirm.",
                    ),
                    &[("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL"))],
                    &["task_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_CREATE_TASK_COMMENT",
                    ("Aufgabenkommentar schreiben", "Create task comment"),
                    (
                        "Schreibt einen Kommentar auf eine Aufgabe; braucht --confirm.",
                        "Posts a comment on a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("comment", s("Kommentartext")),
                        ("assignee", int("Nutzer-ID, dem der Kommentar zugewiesen wird", 0, 9_000_000_000, 0)),
                    ],
                    &["task_id", "comment"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_UPDATE_COMMENT",
                    ("Kommentar ändern", "Update comment"),
                    (
                        "Ändert Text, Assignee oder Erledigt-Status eines Kommentars; braucht --confirm.",
                        "Changes the text, assignee or resolved state of a comment; requires --confirm.",
                    ),
                    &[
                        ("comment_id", s("Kommentar-ID (aus CLICKUP_GET_TASK_COMMENTS)")),
                        ("comment_text", s("Neuer Kommentartext")),
                        ("assignee", int("Nutzer-ID, dem der Kommentar zugewiesen wird", 0, 9_000_000_000, 0)),
                        ("resolved", flag("Kommentar als erledigt markieren", false)),
                    ],
                    &["comment_id", "comment_text"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_DELETE_COMMENT",
                    ("Kommentar löschen", "Delete comment"),
                    (
                        "Löscht einen Kommentar; braucht --confirm.",
                        "Deletes a comment; requires --confirm.",
                    ),
                    &[("comment_id", s("Kommentar-ID (aus CLICKUP_GET_TASK_COMMENTS)"))],
                    &["comment_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_CREATE_FOLDER",
                    ("Folder anlegen", "Create folder"),
                    (
                        "Legt einen Folder in einem Space an; braucht --confirm.",
                        "Creates a folder in a space; requires --confirm.",
                    ),
                    &[
                        ("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)")),
                        ("name", s("Foldername")),
                    ],
                    &["space_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_CREATE_FOLDERLESS_LIST",
                    ("Listen ohne Folder anlegen", "Create folderless list"),
                    (
                        "Legt eine Liste direkt im Space an; braucht --confirm.",
                        "Creates a list directly in a space; requires --confirm.",
                    ),
                    &[
                        ("space_id", s("Space-ID (aus CLICKUP_GET_SPACES)")),
                        ("name", s("Listenname")),
                    ],
                    &["space_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_UPDATE_LIST",
                    ("Liste ändern", "Update list"),
                    (
                        "Ändert Name oder Beschreibung einer Liste; braucht --confirm.",
                        "Changes the name or description of a list; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)")),
                        ("name", s("Neuer Listenname")),
                        ("content", s("Neue Beschreibung")),
                    ],
                    &["list_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_DELETE_LIST",
                    ("Liste löschen", "Delete list"),
                    (
                        "Löscht eine Liste mit allen Aufgaben; braucht --confirm.",
                        "Deletes a list with all of its tasks; requires --confirm.",
                    ),
                    &[("list_id", s("Listen-ID (z. B. aus CLICKUP_GET_FOLDERS)"))],
                    &["list_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_CREATE_SPACE",
                    ("Space anlegen", "Create space"),
                    (
                        "Legt einen Space im Workspace an; braucht --confirm.",
                        "Creates a space in a workspace; requires --confirm.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (aus CLICKUP_LIST_WORKSPACES)")),
                        ("name", s("Spacename")),
                        ("multiple_assignees", flag("Mehrere Assignees pro Aufgabe erlauben", true)),
                    ],
                    &["workspace_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_ADD_TAG_TO_TASK",
                    ("Tag an Aufgabe hängen", "Add tag to task"),
                    (
                        "Hängt einen Space-Tag an eine Aufgabe; braucht --confirm.",
                        "Adds a space tag to a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("tag_name", s("Tagname (aus CLICKUP_GET_SPACE_TAGS)")),
                    ],
                    &["task_id", "tag_name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_REMOVE_TAG_FROM_TASK",
                    ("Tag von Aufgabe entfernen", "Remove tag from task"),
                    (
                        "Entfernt einen Tag von einer Aufgabe; braucht --confirm.",
                        "Removes a tag from a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID, der Code am Ende der Aufgaben-URL")),
                        ("tag_name", s("Tagname (aus CLICKUP_GET_SPACE_TAGS)")),
                    ],
                    &["task_id", "tag_name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "CLICKUP_CREATE_TIME_ENTRY",
                    ("Zeiteintrag anlegen", "Create time entry"),
                    (
                        "Erfasst einen erledigten Zeiteintrag; braucht --confirm.",
                        "Records a completed time entry; requires --confirm.",
                    ),
                    &[
                        ("workspace_id", s("Workspace-ID (aus CLICKUP_LIST_WORKSPACES)")),
                        ("start", datetime("Start des Zeitblocks, ISO 8601")),
                        ("duration", int("Länge in Millisekunden, z. B. 3600000 für eine Stunde", 1, 100_000_000_000, 3_600_000)),
                        ("description", s("Notiz zum Zeiteintrag")),
                        ("task_id", s("Aufgaben-ID, auf die gebucht wird")),
                        ("billable", flag("Als abrechenbar markieren", false)),
                    ],
                    &["workspace_id", "start", "duration"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    /// `callClickUpApi` prefix of the piece: `https://api.clickup.com/api/v2/`.
    fn v2(&self, path: &str) -> String {
        format!("{}/api/v2/{}", self.api_base, path)
    }

    /// Percent-encoded `?a=b&c=d` from optional pairs; `None` entries are dropped
    /// (the piece builds its query strings with `qs.stringify`, which skips them).
    fn query(pairs: &[(&str, Option<String>)]) -> String {
        query(pairs)
    }

    /// Request for `tool`; every arm mirrors one `callClickUpApi` call of the piece.
    fn build(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        match tool {
            // AP: src/lib/actions/spaces/get-spaces.ts
            "CLICKUP_GET_SPACES" => {
                let ws = path_segment(args, "workspace_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("team/{ws}/space"))))
            }
            // AP: src/lib/actions/spaces/get-space.ts
            "CLICKUP_GET_SPACE" => {
                let id = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("space/{id}"))))
            }
            // AP: src/lib/actions/folders/ai-get-folders.ts
            "CLICKUP_GET_FOLDERS" => {
                let id = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("space/{id}/folder"))))
            }
            // AP: src/lib/actions/lists/ai-get-folder-lists.ts
            "CLICKUP_GET_FOLDER_LISTS" => {
                let id = path_segment(args, "folder_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("folder/{id}/list"))))
            }
            // AP: src/lib/actions/lists/ai-get-folderless-lists.ts
            "CLICKUP_GET_FOLDERLESS_LISTS" => {
                let id = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("space/{id}/list"))))
            }
            // AP: src/lib/actions/lists/get-list.ts
            "CLICKUP_GET_LIST" => {
                let id = path_segment(args, "list_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("list/{id}"))))
            }
            // AP: src/lib/actions/custom-fields/get-accessible-custom-fields.ts
            // (helper `listAccessibleCustomFields` in src/lib/common/index.ts)
            "CLICKUP_GET_LIST_CUSTOM_FIELDS" => {
                let id = path_segment(args, "list_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("list/{id}/field"))))
            }
            // AP: src/lib/actions/tasks/get-task.ts
            "CLICKUP_GET_TASK" => {
                let id = path_segment(args, "task_id")?;
                let qs = if flag_value(args, "include_subtasks", false) {
                    "?include_subtasks=true"
                } else {
                    ""
                };
                Ok(HttpRequest::new("GET", self.v2(&format!("task/{id}{qs}"))))
            }
            // AP: src/lib/actions/tasks/clickup-list-list-tasks.ts
            "CLICKUP_LIST_LIST_TASKS" => {
                let id = path_segment(args, "list_id")?;
                let order_by = opt_str(args, "order_by");
                if let Some(o) = order_by {
                    if !["id", "created", "updated", "due_date"].contains(&o) {
                        bail!("`order_by` must be one of id, created, updated, due_date");
                    }
                }
                let url = format!(
                    "list/{id}/task{}",
                    Self::query(&[
                        ("page", Some(opt_u64(args, "page", 0, 0, 10_000).to_string())),
                        ("reverse", Some(flag_value(args, "reverse", false).to_string())),
                        ("include_closed", Some(flag_value(args, "include_closed", false).to_string())),
                        ("subtasks", Some(flag_value(args, "subtasks", false).to_string())),
                        ("order_by", order_by.map(String::from)),
                    ])
                );
                Ok(HttpRequest::new("GET", self.v2(&url)))
            }
            // AP: src/lib/actions/tasks/clickup-list-tasks.ts
            // (the piece sends the location filters as `list_ids`, `project_ids`
            // and `space_ids`, repeated per value)
            "CLICKUP_LIST_WORKSPACE_TASKS" => {
                let ws = path_segment(args, "workspace_id")?;
                let order_by = opt_str(args, "order_by");
                if let Some(o) = order_by {
                    if !["id", "created", "updated", "due_date"].contains(&o) {
                        bail!("`order_by` must be one of id, created, updated, due_date");
                    }
                }
                let mut pairs = vec![
                    ("page", Some(opt_u64(args, "page", 0, 0, 10_000).to_string())),
                    ("reverse", Some(flag_value(args, "reverse", false).to_string())),
                    ("include_closed", Some(flag_value(args, "include_closed", false).to_string())),
                    ("order_by", order_by.map(String::from)),
                ];
                for (key, field) in [
                    ("list_ids", "list_ids"),
                    ("folder_ids", "project_ids"),
                    ("space_ids", "space_ids"),
                ] {
                    for v in str_vec(args, key) {
                        pairs.push((field, Some(v)));
                    }
                }
                for a in num_vec(args, "assignees") {
                    pairs.push(("assignees", Some(a.to_string())));
                }
                for t in str_vec(args, "tags") {
                    pairs.push(("tags", Some(t)));
                }
                Ok(HttpRequest::new(
                    "GET",
                    self.v2(&format!("team/{ws}/task{}", Self::query(&pairs))),
                ))
            }
            // AP: src/lib/actions/comments/clickup-get-task-comments.ts
            "CLICKUP_GET_TASK_COMMENTS" => {
                let id = path_segment(args, "task_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("task/{id}/comment"))))
            }
            // AP: src/lib/actions/tasks/clickup-get-task-members.ts
            "CLICKUP_GET_TASK_MEMBERS" => {
                let id = path_segment(args, "task_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("task/{id}/member"))))
            }
            // AP: src/lib/actions/tags/get-space-tags.ts
            // (helper `listTags` in src/lib/common/index.ts)
            "CLICKUP_GET_SPACE_TAGS" => {
                let id = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("GET", self.v2(&format!("space/{id}/tag"))))
            }
            // AP: src/lib/actions/members/list-workspace-members.ts
            "CLICKUP_LIST_WORKSPACES" => Ok(HttpRequest::new("GET", self.v2("team"))),
            // AP: src/lib/actions/time-tracking/list-time-entries.ts
            "CLICKUP_LIST_TIME_ENTRIES" => {
                let ws = path_segment(args, "workspace_id")?;
                let mut pairs = vec![
                    (
                        "assignee",
                        Some(num_vec(args, "assignee").iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",")),
                    ),
                    ("include_task_tags", Some(flag_value(args, "include_task_tags", false).to_string())),
                    ("include_location_names", Some(flag_value(args, "include_location_names", false).to_string())),
                    ("start_date", millis(args, "start_date")?.map(|ms| ms.to_string())),
                    ("end_date", millis(args, "end_date")?.map(|ms| ms.to_string())),
                ];
                // The piece narrows by exactly one location, in this priority order.
                for (key, field) in [
                    ("task_id", "task_id"),
                    ("list_id", "list_id"),
                    ("folder_id", "project_id"),
                    ("space_id", "space_id"),
                ] {
                    if let Some(v) = opt_str(args, key) {
                        pairs.push((field, Some(v.to_string())));
                        break;
                    }
                }
                Ok(HttpRequest::new(
                    "GET",
                    self.v2(&format!("team/{ws}/time_entries{}", Self::query(&pairs))),
                ))
            }
            // AP: src/lib/actions/tasks/clickup-create-task.ts
            "CLICKUP_CREATE_TASK" => {
                let list = path_segment(args, "list_id")?;
                let mut body = Map::new();
                body.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut body, args, "status_id", "status");
                put_num(&mut body, args, "priority_id", "priority")?;
                let assignees = num_vec(args, "assignee_id");
                if !assignees.is_empty() {
                    body.insert("assignees".into(), json!(assignees));
                }
                if let Some(d) = opt_str(args, "description") {
                    // markdown goes to `markdown_content`, plain text to `description`
                    let field = if flag_value(args, "use_markdown", false) {
                        "markdown_content"
                    } else {
                        "description"
                    };
                    body.insert(field.into(), json!(d));
                }
                if let Some(ms) = millis(args, "due_date")? {
                    body.insert("due_date".into(), json!(ms));
                    body.insert("due_date_time".into(), json!(flag_value(args, "due_date_time", false)));
                }
                if let Some(ms) = millis(args, "start_date")? {
                    body.insert("start_date".into(), json!(ms));
                    body.insert("start_date_time".into(), json!(flag_value(args, "start_date_time", false)));
                }
                put_num(&mut body, args, "time_estimate", "time_estimate")?;
                if flag_value(args, "check_required_custom_fields", false) {
                    body.insert("check_required_custom_fields".into(), json!(true));
                }
                if let Some(fields) = custom_field_array(args) {
                    body.insert("custom_fields".into(), fields);
                }
                Ok(HttpRequest::new("POST", self.v2(&format!("list/{list}/task"))).json(&Value::Object(body)))
            }
            // AP: src/lib/actions/tasks/update-task.ts
            "CLICKUP_UPDATE_TASK" => {
                let id = path_segment(args, "task_id")?;
                let mut body = Map::new();
                put_str(&mut body, args, "name", "name");
                put_str(&mut body, args, "description", "description");
                put_str(&mut body, args, "status_id", "status");
                put_num(&mut body, args, "priority_id", "priority")?;
                let add = num_vec(args, "add_assignee");
                let rem = num_vec(args, "rem_assignee");
                if !add.is_empty() || !rem.is_empty() {
                    body.insert("assignees".into(), json!({"add": add, "rem": rem}));
                }
                if body.is_empty() {
                    bail!("nothing to update: pass at least one field to change");
                }
                Ok(HttpRequest::new("PUT", self.v2(&format!("task/{id}"))).json(&Value::Object(body)))
            }
            // AP: src/lib/actions/tasks/clickup-set-task-status.ts
            "CLICKUP_SET_TASK_STATUS" => {
                let id = path_segment(args, "task_id")?;
                let status = req_str(args, "status_id")?;
                Ok(HttpRequest::new("PUT", self.v2(&format!("task/{id}")))
                    .json(&json!({"status": status})))
            }
            // AP: src/lib/actions/tasks/delete-task.ts
            "CLICKUP_DELETE_TASK" => {
                let id = path_segment(args, "task_id")?;
                Ok(HttpRequest::new("DELETE", self.v2(&format!("task/{id}"))))
            }
            // AP: src/lib/actions/comments/create-task-comment.ts
            // (the piece resolves a missing assignee with a second `GET /api/v2/user`
            // round trip; the single-request hub path leaves `assignee` out, so
            // ClickUp attributes the comment to the authenticated user)
            "CLICKUP_CREATE_TASK_COMMENT" => {
                let id = path_segment(args, "task_id")?;
                let mut body = Map::new();
                body.insert("comment_text".into(), json!(req_str(args, "comment")?));
                put_num(&mut body, args, "assignee", "assignee")?;
                body.insert("notify_all".into(), json!(true));
                Ok(HttpRequest::new("POST", self.v2(&format!("task/{id}/comment")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/comments/clickup-update-comment.ts
            "CLICKUP_UPDATE_COMMENT" => {
                let id = path_segment(args, "comment_id")?;
                let mut body = Map::new();
                body.insert("comment_text".into(), json!(req_str(args, "comment_text")?));
                put_num(&mut body, args, "assignee", "assignee")?;
                if args.get("resolved").and_then(Value::as_bool).is_some() {
                    body.insert("resolved".into(), json!(args["resolved"]));
                }
                Ok(HttpRequest::new("PUT", self.v2(&format!("comment/{id}")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/comments/clickup-delete-comment.ts
            "CLICKUP_DELETE_COMMENT" => {
                let id = path_segment(args, "comment_id")?;
                Ok(HttpRequest::new("DELETE", self.v2(&format!("comment/{id}"))))
            }
            // AP: src/lib/actions/folders/ai-create-folder.ts
            "CLICKUP_CREATE_FOLDER" => {
                let space = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("POST", self.v2(&format!("space/{space}/folder")))
                    .json(&json!({"name": req_str(args, "name")?.trim()})))
            }
            // AP: src/lib/actions/lists/ai-create-folderless-list.ts
            "CLICKUP_CREATE_FOLDERLESS_LIST" => {
                let space = path_segment(args, "space_id")?;
                Ok(HttpRequest::new("POST", self.v2(&format!("space/{space}/list")))
                    .json(&json!({"name": req_str(args, "name")?.trim()})))
            }
            // AP: src/lib/actions/lists/ai-update-list.ts
            "CLICKUP_UPDATE_LIST" => {
                let id = path_segment(args, "list_id")?;
                let mut body = Map::new();
                put_str(&mut body, args, "name", "name");
                put_str(&mut body, args, "content", "content");
                if body.is_empty() {
                    bail!("nothing to update: pass `name` and/or `content`");
                }
                Ok(HttpRequest::new("PUT", self.v2(&format!("list/{id}"))).json(&Value::Object(body)))
            }
            // AP: src/lib/actions/lists/ai-delete-list.ts
            "CLICKUP_DELETE_LIST" => {
                let id = path_segment(args, "list_id")?;
                Ok(HttpRequest::new("DELETE", self.v2(&format!("list/{id}"))))
            }
            // AP: src/lib/actions/spaces/ai-create-space.ts
            "CLICKUP_CREATE_SPACE" => {
                let ws = path_segment(args, "workspace_id")?;
                Ok(HttpRequest::new("POST", self.v2(&format!("team/{ws}/space")))
                    .json(&json!({
                        "name": req_str(args, "name")?.trim(),
                        "multiple_assignees": flag_value(args, "multiple_assignees", true),
                    })))
            }
            // AP: src/lib/actions/tasks/clickup-add-tag-to-task.ts
            "CLICKUP_ADD_TAG_TO_TASK" => {
                let id = path_segment(args, "task_id")?;
                let tag = tag_segment(args, "tag_name")?;
                Ok(HttpRequest::new(
                    "POST",
                    self.v2(&format!("task/{id}/tag/{tag}")),
                ))
            }
            // AP: src/lib/actions/tasks/clickup-remove-tag-from-task.ts
            "CLICKUP_REMOVE_TAG_FROM_TASK" => {
                let id = path_segment(args, "task_id")?;
                let tag = tag_segment(args, "tag_name")?;
                Ok(HttpRequest::new(
                    "DELETE",
                    self.v2(&format!("task/{id}/tag/{tag}")),
                ))
            }
            // AP: src/lib/actions/time-tracking/create-time-entry.ts
            "CLICKUP_CREATE_TIME_ENTRY" => {
                let ws = path_segment(args, "workspace_id")?;
                let mut body = Map::new();
                body.insert(
                    "start".into(),
                    json!(millis(args, "start")?.ok_or_else(|| anyhow!("missing argument `start`"))?),
                );
                body.insert(
                    "duration".into(),
                    json!(opt_i64(args, "duration")?.ok_or_else(|| anyhow!("missing argument `duration`"))?),
                );
                body.insert("billable".into(), json!(flag_value(args, "billable", false)));
                put_str(&mut body, args, "description", "description");
                // `tid` in the piece: the task the entry is attached to.
                put_str(&mut body, args, "task_id", "tid");
                Ok(HttpRequest::new("POST", self.v2(&format!("team/{ws}/time_entries")))
                    .json(&Value::Object(body)))
            }
            other => bail!("unknown ClickUp tool `{other}`"),
        }
    }
}

/// `{"items": [...], "count": n}` from the array under `key`.
fn items(v: &Value, key: &str) -> Value {
    let arr = v[key].as_array().cloned().unwrap_or_default();
    json!({"items": arr, "count": arr.len()})
}

impl ToolkitAdapter for ClickUp {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.build(tool, args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "CLICKUP_LIST_WORKSPACES" => items(&v, "teams"),
            "CLICKUP_GET_SPACES" => items(&v, "spaces"),
            "CLICKUP_GET_FOLDERS" => items(&v, "folders"),
            "CLICKUP_GET_FOLDER_LISTS" | "CLICKUP_GET_FOLDERLESS_LISTS" => items(&v, "lists"),
            "CLICKUP_GET_LIST_CUSTOM_FIELDS" => items(&v, "fields"),
            "CLICKUP_GET_SPACE_TAGS" => items(&v, "tags"),
            "CLICKUP_GET_TASK_COMMENTS" => items(&v, "comments"),
            "CLICKUP_LIST_LIST_TASKS" => {
                let mut out = items(&v, "tasks");
                out["last_page"] = v["last_page"].clone();
                out
            }
            "CLICKUP_LIST_WORKSPACE_TASKS" => items(&v, "tasks"),
            "CLICKUP_LIST_TIME_ENTRIES" => items(&v, "data"),
            // `/task/{id}/member` returns a bare array of `{user: …}` objects.
            "CLICKUP_GET_TASK_MEMBERS" => {
                let arr = v.as_array().cloned().unwrap_or_default();
                json!({"members": arr, "count": arr.len()})
            }
            // Everything else passes the provider payload straight through
            // (single objects and the empty `{}` of the delete calls).
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_encoding() {
        let c = ClickUp::with_api_base("https://clickup.test");
        let r = c
            .build_request("CLICKUP_GET_TASK", &json!({"task_id": "86b0c01d3", "include_subtasks": true}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://clickup.test/api/v2/task/86b0c01d3?include_subtasks=true");

        // Tags are free text in the piece and percent-encoded like `encodeURIComponent`.
        let r = c
            .build_request("CLICKUP_ADD_TAG_TO_TASK", &json!({"task_id": "86b0c01d3", "tag_name": "in progress"}))
            .unwrap();
        assert_eq!(r.url, "https://clickup.test/api/v2/task/86b0c01d3/tag/in%20progress");
        assert!(c
            .build_request("CLICKUP_ADD_TAG_TO_TASK", &json!({"task_id": "86b0c01d3", "tag_name": "../secret"}))
            .is_err());
        assert!(c.build_request("CLICKUP_GET_TASK", &json!({"task_id": "../x"})).is_err());
        assert!(c.build_request("CLICKUP_ADD_TAG_TO_TASK", &json!({"task_id": "86b0c01d3"})).is_err());
    }

    #[test]
    fn bodies_and_millis() {
        let c = ClickUp::default();
        let r = c
            .build_request(
                "CLICKUP_CREATE_TASK",
                &json!({
                    "list_id": "9011001", "name": "Port the ClickUp piece",
                    "description": "# Goal", "use_markdown": true,
                    "status_id": "to do", "priority_id": 3, "assignee_id": 186,
                    "due_date": "2026-10-20T18:00:00Z", "due_date_time": true,
                    "custom_fields": {"0a3c2c11-5d6f-4d9e-bd53-d1b4c6f4b96d": "high"},
                }),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.clickup.com/api/v2/list/9011001/task");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["name"], "Port the ClickUp piece");
        assert_eq!(body["markdown_content"], "# Goal");
        assert!(body.get("description").is_none());
        assert_eq!(body["status"], "to do");
        assert_eq!(body["assignees"], json!([186]));
        assert_eq!(body["due_date"], json!(1_792_519_200_000_i64));
        assert_eq!(body["due_date_time"], json!(true));
        assert_eq!(
            body["custom_fields"],
            json!([{"id": "0a3c2c11-5d6f-4d9e-bd53-d1b4c6f4b96d", "value": "high"}])
        );

        // An empty update is rejected instead of silently writing nothing.
        assert!(c.build_request("CLICKUP_UPDATE_TASK", &json!({"task_id": "86b0c01d3"})).is_err());

        let r = c
            .build_request(
                "CLICKUP_LIST_TIME_ENTRIES",
                &json!({"workspace_id": "901", "start_date": "2026-10-01T00:00:00Z",
                        "assignee": [186, 42], "task_id": "86b0c01d3"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.clickup.com/api/v2/team/901/time_entries?assignee=186%2C42&include_task_tags=false&include_location_names=false&start_date=1790812800000&task_id=86b0c01d3"
        );

        let collection = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"spaces": [{"id": "1"}]})).unwrap(),
            session_id: None,
        };
        let out = c.parse_response("CLICKUP_GET_SPACES", &collection).unwrap();
        assert_eq!(out["count"], json!(1));
        let members = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!([{"user": {"id": 186}}])).unwrap(),
            session_id: None,
        };
        assert_eq!(c.parse_response("CLICKUP_GET_TASK_MEMBERS", &members).unwrap()["count"], json!(1));
    }
}
