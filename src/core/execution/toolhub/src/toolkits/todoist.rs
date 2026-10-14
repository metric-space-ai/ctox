//! Todoist toolkit (REST API v1). Action set modelled on Activepieces
//! `piece-todoist` (MIT).
//!
//! Auth: the piece declares `PieceAuth.OAuth2` with authorize
//! `https://todoist.com/oauth/authorize`, token `https://todoist.com/oauth/access_token`
//! and the single scope `data:read_write`; every request in the piece is
//! `AuthenticationType.BEARER_TOKEN`, so the hub's `Authorization: Bearer <token>`
//! header injection is exactly what the piece does.
//!
//! All endpoints come verbatim from the piece's three clients and the actions that
//! call `httpClient` themselves (`lib/common/client/rest-client.ts`,
//! `lib/common/client/labels-client.ts`,
//! `lib/common/client/projects-sections-client.ts` plus the `lib/actions/*.ts`
//! files). The base URL `https://api.todoist.com/api/v1` is account independent,
//! so there is no per-account host input.
//!
//! The clients follow `next_cursor` pages until the cursor is null. This adapter
//! builds a single page; every list tool therefore takes an optional `cursor`
//! input that is passed through as the documented `cursor` query parameter.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "todoist";

/// The only scope `todoistAuth` in `src/index.ts` requests.
const SCOPE: &str = "data:read_write";

pub struct Todoist {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Todoist {
    fn default() -> Self {
        Self::with_api_base("https://api.todoist.com/api/v1")
    }
}

// ---------------------------------------------------------------- schema helpers

/// Free-form JSON object prop (e.g. the comment `attachment`).
fn object(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

/// Todoist ids are the 16 character base32 form, so they pass `path_segment`.
fn cursor_prop() -> Value {
    s("`next_cursor` of a previous call, to fetch the next page.")
}

// ---------------------------------------------------------------- body helpers

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_int(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(v) => match v.as_u64() {
            Some(n) => {
                m.insert(field.into(), json!(n));
                Ok(())
            }
            None => bail!("`{key}` must be a whole number"),
        },
    }
}

fn put_bool(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(b) = args.get(key).and_then(Value::as_bool) {
        m.insert(field.into(), json!(b));
    }
}

fn is_iso_date(v: &str) -> bool {
    v.len() == 10
        && v.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

/// RFC3339-ish datetime, what `Date.parse` accepted and `toISOString()` normalised
/// the piece sent as `due_datetime`.
fn is_iso_datetime(v: &str) -> bool {
    let Some((date, rest)) = v.split_once(['T', ' ']) else {
        return false;
    };
    is_iso_date(date) && rest.len() >= 5 && rest.as_bytes()[2] == b':'
}

/// `dueDateParams` from `rest-client.ts`: `YYYY-MM-DD` -> `due_date`, a real
/// timestamp -> `due_datetime`, everything else ("tomorrow 5pm") -> `due_string`.
fn put_due(m: &mut Map<String, Value>, args: &Value) {
    let Some(v) = opt_str(args, "due_date") else {
        return;
    };
    if is_iso_date(v) {
        m.insert("due_date".into(), json!(v));
    } else if is_iso_datetime(v) {
        m.insert("due_datetime".into(), json!(v));
    } else {
        m.insert("due_string".into(), json!(v));
    }
}

// ---------------------------------------------------------------- response helpers

/// `results` page envelope of the v1 clients.
fn page_items(v: &Value) -> Vec<Value> {
    v["results"].as_array().cloned().unwrap_or_default()
}

/// `GET /sections/archived` is legacy-mapped and may answer with a bare array, a
/// `results` envelope or a `sections` key (see `projects-sections-client.ts`).
fn archived_section_items(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        other => other["results"]
            .as_array()
            .or_else(|| other["sections"].as_array())
            .cloned()
            .unwrap_or_default(),
    }
}

/// `{ <key>: items, count }`, the shape every list action of the piece returns.
fn list_of(key: &str, items: Vec<Value>) -> Value {
    let mut m = Map::new();
    m.insert(key.into(), Value::Array(items.clone()));
    m.insert("count".into(), json!(items.len()));
    Value::Object(m)
}

/// Mutations without an entity in the response (close/reopen/unarchive/delete).
fn ok_result(body: &Value) -> Value {
    match body {
        Value::Object(m) if !m.is_empty() => {
            let mut out = m.clone();
            out.insert("success".into(), json!(true));
            Value::Object(out)
        }
        _ => json!({"success": true}),
    }
}

// ---------------------------------------------------------------- adapter

impl Todoist {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Todoist".into(),
            description: LocalizedText::new(
                "Aufgaben, Projekte, Abschnitte, Labels und Kommentare in Todoist lesen und bearbeiten.",
                "Read and change tasks, projects, sections, labels and comments in Todoist.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://todoist.com/oauth/authorize".into(),
                    token_url: "https://todoist.com/oauth/access_token".into(),
                    // Exactly the scope list of `todoistAuth` in `src/index.ts`.
                    default_scopes: vec![SCOPE.into()],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                // Personal API token from Todoist settings: same `Authorization: Bearer <token>` header.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "TODOIST_GET_CURRENT_USER",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    (
                        "Liest das Profil des angemeldeten Todoist-Kontos.",
                        "Reads the profile of the authenticated Todoist account.",
                    ),
                    &[],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_PROJECTS",
                    ("Projekte auflisten", "List projects"),
                    (
                        "Listet alle aktiven Projekte des Kontos.",
                        "Lists every active project of the account.",
                    ),
                    &[("cursor", cursor_prop())],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_GET_PROJECT",
                    ("Projekt lesen", "Get project"),
                    (
                        "Liest ein einzelnes Projekt mit allen Feldern.",
                        "Reads one project with all of its fields.",
                    ),
                    &[("project_id", s("Projekt-ID (aus TODOIST_LIST_PROJECTS)"))],
                    &["project_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_ARCHIVED_PROJECTS",
                    ("Archivierte Projekte auflisten", "List archived projects"),
                    (
                        "Listet die archivierten Projekte des Kontos.",
                        "Lists the archived projects of the account.",
                    ),
                    &[("cursor", cursor_prop())],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_SECTIONS",
                    ("Abschnitte auflisten", "List sections"),
                    (
                        "Listet Abschnitte, optional auf ein Projekt beschränkt.",
                        "Lists sections, optionally scoped to one project.",
                    ),
                    &[
                        ("project_id", s("Nur Abschnitte dieses Projekts; leer = alle")),
                        ("cursor", cursor_prop()),
                    ],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_GET_SECTION",
                    ("Abschnitt lesen", "Get section"),
                    (
                        "Liest einen einzelnen Abschnitt.",
                        "Reads one section.",
                    ),
                    &[("section_id", s("Abschnitts-ID (aus TODOIST_LIST_SECTIONS)"))],
                    &["section_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_ARCHIVED_SECTIONS",
                    ("Archivierte Abschnitte auflisten", "List archived sections"),
                    (
                        "Listet die archivierten Abschnitte eines Projekts.",
                        "Lists the archived sections of one project.",
                    ),
                    &[
                        ("project_id", s("Projekt-ID der archivierten Abschnitte")),
                        ("cursor", cursor_prop()),
                    ],
                    &["project_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_LABELS",
                    ("Labels auflisten", "List labels"),
                    (
                        "Listet die persönlichen Labels des Kontos.",
                        "Lists the personal labels of the account.",
                    ),
                    &[("cursor", cursor_prop())],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_GET_LABEL",
                    ("Label lesen", "Get label"),
                    (
                        "Liest ein einzelnes Label.",
                        "Reads one label.",
                    ),
                    &[("label_id", s("Label-ID (aus TODOIST_LIST_LABELS)"))],
                    &["label_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_SHARED_LABELS",
                    ("Geteilte Labels auflisten", "List shared labels"),
                    (
                        "Listet die Label-Namen, die mit anderen Konten geteilt sind.",
                        "Lists the label names shared with other accounts.",
                    ),
                    &[(
                        "omit_personal",
                        flag("Persönliche Labels aus der Liste entfernen"),
                    )],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_GET_TASK",
                    ("Aufgabe lesen", "Get task"),
                    (
                        "Liest den aktuellen Datensatz einer Aufgabe.",
                        "Reads the current record of one task.",
                    ),
                    &[("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)"))],
                    &["task_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_FILTER_TASKS",
                    ("Aufgaben filtern", "Filter tasks"),
                    (
                        "Sucht aktive Aufgaben mit Todoists Filter-Syntax, z. B. \"today | overdue\".",
                        "Searches active tasks with Todoist filter syntax, e.g. \"today | overdue\".",
                    ),
                    &[
                        (
                            "query",
                            s("Filter-Query, z. B. \"today | overdue\", \"p1 & !#Work\", \"search: meeting\""),
                        ),
                        ("lang", s("IETF-Sprachcode für Datumswörter, z. B. en oder de")),
                        ("limit", int("Aufgaben pro Seite (1-200)", 1, 200, 50)),
                        ("cursor", cursor_prop()),
                    ],
                    &["query"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_COMPLETED_TASKS",
                    (
                        "Erledigte Aufgaben (Erledigt-am) auflisten",
                        "List completed tasks (by completion date)",
                    ),
                    (
                        "Listet Aufgaben, die zwischen `since` und `until` abgehakt wurden.",
                        "Lists tasks checked off between `since` and `until`.",
                    ),
                    &completed_window_props(),
                    &["since", "until"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_COMPLETED_TASKS_BY_DUE_DATE",
                    (
                        "Erledigte Aufgaben (Fällig-am) auflisten",
                        "List completed tasks (by due date)",
                    ),
                    (
                        "Listet erledigte Aufgaben, deren Fälligkeitsdatum im Zeitfenster liegt.",
                        "Lists completed tasks whose due date falls inside the window.",
                    ),
                    &completed_window_props(),
                    &["since", "until"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_ACTIVITY_LOG",
                    ("Aktivitätsprotokoll auflisten", "List activity log"),
                    (
                        "Liest das Ereignisprotokoll, optional nach Objekt, Ereignis und Datum gefiltert.",
                        "Reads the event log, optionally filtered by object, event and date.",
                    ),
                    &[
                        (
                            "object_type",
                            choice("Nur ein Objekttyp", &["project", "item", "note"]),
                        ),
                        ("event_type", s("Nur ein Ereignistyp, z. B. added, updated, completed, deleted")),
                        ("object_id", s("Nur dieses Objekt; gehört zu `object_type`")),
                        ("parent_project_id", s("Nur ein Projekt samt Aufgaben und Kommentaren")),
                        ("parent_item_id", s("Nur eine Aufgabe samt ihrer Kommentare")),
                        ("date_from", s("Nur Ereignisse ab diesem ISO-8601-Zeitpunkt")),
                        ("date_to", s("Nur Ereignisse vor diesem ISO-8601-Zeitpunkt")),
                        ("limit", int("Ereignisse pro Seite (1-100)", 1, 100, 50)),
                        ("cursor", cursor_prop()),
                    ],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_LIST_COMMENTS",
                    ("Kommentare auflisten", "List comments"),
                    (
                        "Listet alle Kommentare einer Aufgabe oder eines Projekts.",
                        "Lists every comment of one task or one project.",
                    ),
                    &[
                        ("task_id", s("Nur Kommentare dieser Aufgabe; leer = Projekt")),
                        ("project_id", s("Nur Kommentare dieses Projekts; leer = Aufgabe")),
                        ("cursor", cursor_prop()),
                    ],
                    &[],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_GET_COMMENT",
                    ("Kommentar lesen", "Get comment"),
                    (
                        "Liest einen einzelnen Kommentar.",
                        "Reads one comment.",
                    ),
                    &[("comment_id", s("Kommentar-ID (aus TODOIST_LIST_COMMENTS)"))],
                    &["comment_id"],
                    &[SCOPE],
                    false,
                ),
                t(
                    "TODOIST_CREATE_TASK",
                    ("Aufgabe anlegen", "Create task"),
                    (
                        "Legt eine Aufgabe in einem Projekt, Abschnitt oder dem Posteingang an; braucht --confirm.",
                        "Creates a task in a project, a section or the inbox; requires --confirm.",
                    ),
                    &[
                        ("content", s("Titel der Aufgabe; Markdown und Links sind erlaubt")),
                        ("project_id", s("Zielprojekt; leer = Posteingang")),
                        ("section_id", s("Zielabschnitt innerhalb des Projekts")),
                        ("description", s("Zusatzdetails unter dem Titel; Markdown ist erlaubt")),
                        ("labels", str_list("Namen persönlicher oder geteilter Labels")),
                        ("priority", int("1 = normal, 4 = dringend (Todoist zeigt 4 als P1)", 1, 4, 1)),
                        ("due_date", s("YYYY-MM-DD, ISO-Zeitstempel oder Wörter wie \"tomorrow 5pm\"")),
                    ],
                    &["content"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_UPDATE_TASK",
                    ("Aufgabe ändern", "Update task"),
                    (
                        "Ändert nur die übergebenen Felder einer Aufgabe; braucht --confirm.",
                        "Changes only the fields you pass on a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)")),
                        ("content", s("Neuer Titel")),
                        ("description", s("Neue Beschreibung; Markdown ist erlaubt")),
                        (
                            "labels",
                            str_list("Ersetzt alle Labels der Aufgabe; leer = unverändert"),
                        ),
                        ("priority", int("1 = normal, 4 = dringend", 1, 4, 1)),
                        ("due_date", s("YYYY-MM-DD, ISO-Zeitstempel oder Wörter wie \"tomorrow 5pm\"")),
                    ],
                    &["task_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_COMPLETE_TASK",
                    ("Aufgabe abschließen", "Complete task"),
                    (
                        "Hakt eine Aufgabe ab; wiederkehrende Aufgaben rücken zum nächsten Termin; braucht --confirm.",
                        "Closes a task; recurring tasks advance to their next date; requires --confirm.",
                    ),
                    &[("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)"))],
                    &["task_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_REOPEN_TASK",
                    ("Aufgabe wieder öffnen", "Reopen task"),
                    (
                        "Macht eine abgehakte Aufgabe wieder aktiv; braucht --confirm.",
                        "Restores a completed task to the active list; requires --confirm.",
                    ),
                    &[("task_id", s("ID der erledigten Aufgabe"))],
                    &["task_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_MOVE_TASK",
                    ("Aufgabe verschieben", "Move task"),
                    (
                        "Verschiebt eine Aufgabe in ein anderes Projekt, einen anderen Abschnitt oder unter eine Elternaufgabe; braucht --confirm.",
                        "Moves a task to another project, section or parent task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)")),
                        ("project_id", s("Zielprojekt")),
                        ("section_id", s("Zielabschnitt")),
                        ("parent_id", s("ID der neuen Elternaufgabe")),
                    ],
                    &["task_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_DELETE_TASK",
                    ("Aufgabe löschen", "Delete task"),
                    (
                        "Löscht eine Aufgabe endgültig; braucht --confirm.",
                        "Permanently deletes a task; requires --confirm.",
                    ),
                    &[("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)"))],
                    &["task_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_QUICK_ADD_TASK",
                    ("Aufgabe im Schnelleingabe anlegen", "Quick add task"),
                    (
                        "Legt eine Aufgabe aus einem natürlichen Satz an, z. B. \"Bericht bis Freitag #Arbeit p2\"; braucht --confirm.",
                        "Creates a task from a natural-language sentence, e.g. \"Submit report by Friday #Work p2\"; requires --confirm.",
                    ),
                    &[
                        ("text", s("Text mit Inline-Syntax: #Projekt, @Label, p1-p4, Datumsangaben")),
                        ("note", s("Optionale Notiz am erstellen Task")),
                        ("reminder", s("Optionale Erinnerung in Worten, z. B. \"tomorrow at 9am\"")),
                        ("auto_reminder", flag("Standarderinnerung ergänzen, wenn eine Uhrzeit gesetzt ist")),
                    ],
                    &["text"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_CREATE_PROJECT",
                    ("Projekt anlegen", "Create project"),
                    (
                        "Legt ein Projekt an; braucht --confirm.",
                        "Creates a project; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des Projekts")),
                        ("color", s("Hex-Farbe, z. B. #b8256f")),
                        ("parent_id", s("ID des übergeordneten Projekts")),
                        ("view_style", choice("Darstellung in Todoist", &["list", "board"])),
                        ("description", s("Beschreibung; Markdown ist erlaubt")),
                        ("is_favorite", flag("Als Favorit markieren")),
                    ],
                    &["name"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_UPDATE_PROJECT",
                    ("Projekt ändern", "Update project"),
                    (
                        "Ändert nur die übergebenen Projekteinstellungen; braucht --confirm.",
                        "Changes only the project settings you pass; requires --confirm.",
                    ),
                    &[
                        ("project_id", s("Projekt-ID (aus TODOIST_LIST_PROJECTS)")),
                        ("name", s("Neuer Name")),
                        ("color", s("Hex-Farbe, z. B. #b8256f")),
                        ("view_style", choice("Darstellung in Todoist", &["list", "board"])),
                        ("description", s("Neue Beschreibung; Markdown ist erlaubt")),
                        ("is_favorite", flag("Als Favorit markieren")),
                    ],
                    &["project_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_UNARCHIVE_PROJECT",
                    ("Archiviertes Projekt wiederherstellen", "Unarchive project"),
                    (
                        "Holt ein archiviertes Projekt zurück; braucht --confirm.",
                        "Restores an archived project; requires --confirm.",
                    ),
                    &[("project_id", s("ID des archivierten Projekts"))],
                    &["project_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_CREATE_SECTION",
                    ("Abschnitt anlegen", "Create section"),
                    (
                        "Legt einen Abschnitt in einem Projekt an; braucht --confirm.",
                        "Creates a section in a project; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des Abschnitts")),
                        ("project_id", s("Projekt-ID, in dem der Abschnitt entsteht")),
                        ("order", int("Position innerhalb des Projekts", 0, 1000, 0)),
                    ],
                    &["name", "project_id"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_UPDATE_SECTION",
                    ("Abschnitt umbenennen", "Update section"),
                    (
                        "Benennt einen Abschnitt um; braucht --confirm.",
                        "Renames a section; requires --confirm.",
                    ),
                    &[
                        ("section_id", s("Abschnitts-ID (aus TODOIST_LIST_SECTIONS)")),
                        ("name", s("Neuer Name")),
                    ],
                    &["section_id", "name"],
                    &[SCOPE],
                    true,
                ),
                t(
                    "TODOIST_CREATE_TASK_COMMENT",
                    ("Aufgabenkommentar schreiben", "Create task comment"),
                    (
                        "Hängt einen Kommentar an eine Aufgabe; braucht --confirm.",
                        "Posts a comment on a task; requires --confirm.",
                    ),
                    &[
                        ("task_id", s("Aufgaben-ID (aus TODOIST_FILTER_TASKS)")),
                        ("content", s("Kommentartext, 1-15000 Zeichen; Markdown ist erlaubt")),
                        (
                            "attachment",
                            object("Optionales Attachment-Objekt, z. B. {\"resource_type\": \"file\", \"file_url\": \"https://…\"}"),
                        ),
                    ],
                    &["task_id", "content"],
                    &[SCOPE],
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
        format!("{}{}", self.api_base, path)
    }

    /// GET with optional, percent-encoded query parameters.
    fn get(&self, path: &str, pairs: Vec<(&'static str, Option<String>)>) -> HttpRequest {
        HttpRequest::new("GET", format!("{}{}", self.url(path), query(&pairs)))
    }

    /// `pickBy(…, isNotUndefined)`: only the fields the caller actually set.
    fn body_of(args: &Value, keys: &[(&str, &str)]) -> Result<Map<String, Value>> {
        let mut m = Map::new();
        for (key, field) in keys {
            put_str(&mut m, args, key, field);
        }
        Ok(m)
    }

    #[allow(clippy::too_many_lines)]
    fn build(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: lib/actions/get-current-user.action.ts
            "TODOIST_GET_CURRENT_USER" => self.get("/user", vec![]),
            // AP: lib/actions/list-projects.action.ts -> lib/common/client/rest-client.ts (`projects.list`)
            "TODOIST_LIST_PROJECTS" => self.get("/projects", vec![("cursor", opt(args, "cursor"))]),
            // AP: lib/actions/get-project.action.ts -> lib/common/client/projects-sections-client.ts (`projects.get`)
            "TODOIST_GET_PROJECT" => self.get(
                &format!("/projects/{}", path_segment(args, "project_id")?),
                vec![],
            ),
            // AP: lib/actions/list-archived-projects.action.ts -> projects-sections-client.ts (`projects.listArchived`)
            "TODOIST_LIST_ARCHIVED_PROJECTS" => {
                self.get("/projects/archived", vec![("cursor", opt(args, "cursor"))])
            }
            // AP: lib/actions/list-sections.action.ts -> rest-client.ts (`sections.list`)
            "TODOIST_LIST_SECTIONS" => self.get(
                "/sections",
                vec![("project_id", opt(args, "project_id")), ("cursor", opt(args, "cursor"))],
            ),
            // AP: lib/actions/get-section.action.ts -> projects-sections-client.ts (`sections.get`)
            "TODOIST_GET_SECTION" => self.get(
                &format!("/sections/{}", path_segment(args, "section_id")?),
                vec![],
            ),
            // AP: lib/actions/list-archived-sections.action.ts -> projects-sections-client.ts (`sections.listArchived`)
            "TODOIST_LIST_ARCHIVED_SECTIONS" => self.get(
                "/sections/archived",
                vec![
                    ("project_id", Some(path_segment(args, "project_id")?.to_string())),
                    ("cursor", opt(args, "cursor")),
                ],
            ),
            // AP: lib/actions/list-labels.action.ts -> lib/common/client/labels-client.ts (`list`)
            "TODOIST_LIST_LABELS" => self.get("/labels", vec![("cursor", opt(args, "cursor"))]),
            // AP: lib/actions/get-label.action.ts -> labels-client.ts (`get`)
            "TODOIST_GET_LABEL" => {
                self.get(&format!("/labels/{}", path_segment(args, "label_id")?), vec![])
            }
            // AP: lib/actions/list-shared-labels.action.ts -> labels-client.ts (`listShared`)
            "TODOIST_LIST_SHARED_LABELS" => self.get(
                "/labels/shared",
                vec![(
                    "omit_personal",
                    args.get("omit_personal")
                        .and_then(Value::as_bool)
                        .map(|b| b.to_string()),
                )],
            ),
            // AP: lib/actions/get-task.action.ts -> rest-client.ts (`tasks.get`)
            "TODOIST_GET_TASK" => {
                self.get(&format!("/tasks/{}", path_segment(args, "task_id")?), vec![])
            }
            // AP: lib/actions/filter-tasks.action.ts -> rest-client.ts (`tasks.filter`)
            "TODOIST_FILTER_TASKS" => self.get(
                "/tasks/filter",
                vec![
                    ("query", Some(req_str(args, "query")?.to_string())),
                    ("lang", opt(args, "lang")),
                    (
                        "limit",
                        args.get("limit")
                            .and_then(Value::as_u64)
                            .map(|n| n.clamp(1, 200).to_string()),
                    ),
                    ("cursor", opt(args, "cursor")),
                ],
            ),
            // AP: lib/actions/list-completed-tasks.action.ts -> rest-client.ts (`tasks.listCompletedByCompletionDate`)
            "TODOIST_LIST_COMPLETED_TASKS" => self.get(
                "/tasks/completed/by_completion_date",
                completed_window_query(args)?,
            ),
            // AP: lib/actions/list-completed-tasks-by-due-date.action.ts -> rest-client.ts (`tasks.listCompletedByDueDate`)
            "TODOIST_LIST_COMPLETED_TASKS_BY_DUE_DATE" => {
                self.get("/tasks/completed/by_due_date", completed_window_query(args)?)
            }
            // AP: lib/actions/list-activity-log.action.ts
            "TODOIST_LIST_ACTIVITY_LOG" => self.get(
                "/activities",
                vec![
                    ("object_type", opt(args, "object_type")),
                    ("event_type", opt(args, "event_type")),
                    ("object_id", opt(args, "object_id")),
                    ("parent_project_id", opt(args, "parent_project_id")),
                    ("parent_item_id", opt(args, "parent_item_id")),
                    ("date_from", opt(args, "date_from")),
                    ("date_to", opt(args, "date_to")),
                    (
                        "limit",
                        args.get("limit")
                            .and_then(Value::as_u64)
                            .map(|n| n.clamp(1, 100).to_string()),
                    ),
                    ("cursor", opt(args, "cursor")),
                ],
            ),
            // AP: lib/actions/list-comments.action.ts
            "TODOIST_LIST_COMMENTS" => {
                let task = opt(args, "task_id");
                let project = opt(args, "project_id");
                match (&task, &project) {
                    (Some(_), Some(_)) => {
                        bail!("provide exactly one of `task_id` or `project_id`, not both")
                    }
                    (None, None) => bail!("provide exactly one of `task_id` or `project_id`"),
                    _ => self.get(
                        "/comments",
                        vec![("task_id", task), ("project_id", project), ("cursor", opt(args, "cursor"))],
                    ),
                }
            }
            // AP: lib/actions/get-comment.action.ts
            "TODOIST_GET_COMMENT" => self.get(
                &format!("/comments/{}", path_segment(args, "comment_id")?),
                vec![],
            ),
            // AP: lib/actions/create-task-action.ts -> rest-client.ts (`tasks.create`)
            "TODOIST_CREATE_TASK" => {
                let mut m = Self::body_of(
                    args,
                    &[
                        ("content", "content"),
                        ("project_id", "project_id"),
                        ("description", "description"),
                        ("section_id", "section_id"),
                    ],
                )?;
                m.insert(
                    "content".into(),
                    json!(req_str(args, "content")?.trim().to_string()),
                );
                let labels = str_vec(args, "labels");
                if !labels.is_empty() {
                    m.insert("labels".into(), json!(labels));
                }
                put_int(&mut m, args, "priority", "priority")?;
                put_due(&mut m, args);
                HttpRequest::new("POST", self.url("/tasks")).json(&Value::Object(m))
            }
            // AP: lib/actions/update-task.action.ts -> rest-client.ts (`tasks.update`)
            "TODOIST_UPDATE_TASK" => {
                let task_id = path_segment(args, "task_id")?;
                let mut m =
                    Self::body_of(args, &[("content", "content"), ("description", "description")])?;
                // `labels?.length === 0 ? undefined : labels`: an empty list never
                // clears the labels of the task.
                let labels = str_vec(args, "labels");
                if !labels.is_empty() {
                    m.insert("labels".into(), json!(labels));
                }
                put_int(&mut m, args, "priority", "priority")?;
                put_due(&mut m, args);
                if m.is_empty() {
                    bail!("nothing to update: pass at least one field to change");
                }
                HttpRequest::new("POST", self.url(&format!("/tasks/{task_id}")))
                    .json(&Value::Object(m))
            }
            // AP: lib/actions/mark-task-completed.action.ts -> rest-client.ts (`tasks.close`)
            "TODOIST_COMPLETE_TASK" => HttpRequest::new(
                "POST",
                self.url(&format!("/tasks/{}/close", path_segment(args, "task_id")?)),
            ),
            // AP: lib/actions/reopen-task.action.ts -> rest-client.ts (`tasks.reopen`)
            "TODOIST_REOPEN_TASK" => HttpRequest::new(
                "POST",
                self.url(&format!("/tasks/{}/reopen", path_segment(args, "task_id")?)),
            ),
            // AP: lib/actions/move-task.action.ts -> rest-client.ts (`tasks.move`)
            "TODOIST_MOVE_TASK" => {
                let task_id = path_segment(args, "task_id")?;
                let m = Self::body_of(
                    args,
                    &[
                        ("project_id", "project_id"),
                        ("section_id", "section_id"),
                        ("parent_id", "parent_id"),
                    ],
                )?;
                if m.is_empty() {
                    bail!("move task requires at least one destination: `project_id`, `section_id` or `parent_id`");
                }
                HttpRequest::new("POST", self.url(&format!("/tasks/{task_id}/move")))
                    .json(&Value::Object(m))
            }
            // AP: lib/actions/delete-task.action.ts -> rest-client.ts (`tasks.delete`)
            "TODOIST_DELETE_TASK" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/tasks/{}", path_segment(args, "task_id")?)),
            ),
            // AP: lib/actions/quick-add-task.action.ts -> rest-client.ts (`tasks.quickAdd`)
            "TODOIST_QUICK_ADD_TASK" => {
                let mut m = Map::new();
                m.insert("text".into(), json!(req_str(args, "text")?.trim().to_string()));
                put_str(&mut m, args, "note", "note");
                put_str(&mut m, args, "reminder", "reminder");
                put_bool(&mut m, args, "auto_reminder", "auto_reminder");
                HttpRequest::new("POST", self.url("/tasks/quick")).json(&Value::Object(m))
            }
            // AP: lib/actions/create-project.action.ts -> projects-sections-client.ts (`projects.create`)
            "TODOIST_CREATE_PROJECT" => {
                let mut m = Self::body_of(
                    args,
                    &[
                        ("color", "color"),
                        ("parent_id", "parent_id"),
                        ("view_style", "view_style"),
                        ("description", "description"),
                    ],
                )?;
                m.insert(
                    "name".into(),
                    json!(req_str(args, "name")?.trim().to_string()),
                );
                put_bool(&mut m, args, "is_favorite", "is_favorite");
                HttpRequest::new("POST", self.url("/projects")).json(&Value::Object(m))
            }
            // AP: lib/actions/update-project.action.ts -> projects-sections-client.ts (`projects.update`)
            "TODOIST_UPDATE_PROJECT" => {
                let project_id = path_segment(args, "project_id")?;
                let mut m = Self::body_of(
                    args,
                    &[
                        ("name", "name"),
                        ("color", "color"),
                        ("view_style", "view_style"),
                        ("description", "description"),
                    ],
                )?;
                put_bool(&mut m, args, "is_favorite", "is_favorite");
                if m.is_empty() {
                    bail!("nothing to update: pass at least one project setting");
                }
                HttpRequest::new("POST", self.url(&format!("/projects/{project_id}")))
                    .json(&Value::Object(m))
            }
            // AP: lib/actions/unarchive-project.action.ts -> projects-sections-client.ts (`projects.unarchive`)
            "TODOIST_UNARCHIVE_PROJECT" => HttpRequest::new(
                "POST",
                self.url(&format!(
                    "/projects/{}/unarchive",
                    path_segment(args, "project_id")?
                )),
            ),
            // AP: lib/actions/create-section.action.ts -> projects-sections-client.ts (`sections.create`)
            "TODOIST_CREATE_SECTION" => {
                let mut m = Map::new();
                m.insert("name".into(), json!(req_str(args, "name")?.trim().to_string()));
                m.insert(
                    "project_id".into(),
                    json!(path_segment(args, "project_id")?),
                );
                put_int(&mut m, args, "order", "order")?;
                HttpRequest::new("POST", self.url("/sections")).json(&Value::Object(m))
            }
            // AP: lib/actions/update-section.action.ts -> projects-sections-client.ts (`sections.update`)
            "TODOIST_UPDATE_SECTION" => HttpRequest::new(
                "POST",
                self.url(&format!("/sections/{}", path_segment(args, "section_id")?)),
            )
            .json(&json!({"name": req_str(args, "name")?.trim()})),
            // AP: lib/actions/create-task-comment.action.ts
            "TODOIST_CREATE_TASK_COMMENT" => {
                let mut m = Map::new();
                m.insert("task_id".into(), json!(path_segment(args, "task_id")?));
                m.insert("content".into(), json!(req_str(args, "content")?));
                if let Some(att) = args.get("attachment").filter(|v| v.is_object()) {
                    m.insert("attachment".into(), att.clone());
                }
                HttpRequest::new("POST", self.url("/comments")).json(&Value::Object(m))
            }
            other => bail!("unknown Todoist tool `{other}`"),
        })
    }
}

/// Optional query value; empty strings are dropped like `pickBy(isNotUndefined)`.
fn opt(args: &Value, key: &str) -> Option<String> {
    opt_str(args, key).map(String::from)
}

/// Shared props of the two completed-task windows.
fn completed_window_props() -> Vec<(&'static str, Value)> {
    vec![
        ("since", s("Start des Fensters, z. B. \"2026-06-01\" oder RFC3339")),
        ("until", s("Ende des Fensters, z. B. \"2026-06-27\" oder RFC3339")),
        ("project_id", s("Nur erledigte Aufgaben dieses Projekts")),
        ("section_id", s("Nur erledigte Aufgaben dieses Abschnitts")),
        ("parent_id", s("Nur erledigte Teilaufgaben dieser Elternaufgabe")),
        ("filter_query", s("Optionaler Todoist-Filter, gleiche Syntax wie TODOIST_FILTER_TASKS")),
        ("limit", int("Aufgaben pro Seite (1-200)", 1, 200, 200)),
        ("cursor", cursor_prop()),
    ]
}

/// Query of `fetchCompletedByDate` in `rest-client.ts` (default limit 200).
fn completed_window_query(
    args: &Value,
) -> Result<Vec<(&'static str, Option<String>)>> {
    Ok(vec![
        ("since", Some(req_str(args, "since")?.trim().to_string())),
        ("until", Some(req_str(args, "until")?.trim().to_string())),
        ("project_id", opt(args, "project_id")),
        ("section_id", opt(args, "section_id")),
        ("parent_id", opt(args, "parent_id")),
        ("filter_query", opt(args, "filter_query")),
        (
            "limit",
            Some(
                args.get("limit")
                    .and_then(Value::as_u64)
                    .map_or(200, |n| n.clamp(1, 200))
                    .to_string(),
            ),
        ),
        ("cursor", opt(args, "cursor")),
    ])
}

impl ToolkitAdapter for Todoist {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.build(tool, args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let body = response.json()?;
        Ok(match tool {
            "TODOIST_LIST_PROJECTS" => list_of("projects", page_items(&body)),
            "TODOIST_LIST_ARCHIVED_PROJECTS" => list_of("projects", page_items(&body)),
            "TODOIST_LIST_SECTIONS" => list_of("sections", page_items(&body)),
            "TODOIST_LIST_ARCHIVED_SECTIONS" => {
                list_of("sections", archived_section_items(&body))
            }
            "TODOIST_LIST_LABELS" => list_of("labels", page_items(&body)),
            "TODOIST_LIST_SHARED_LABELS" => list_of("shared_labels", page_items(&body)),
            "TODOIST_FILTER_TASKS" => list_of("tasks", page_items(&body)),
            "TODOIST_LIST_COMPLETED_TASKS" | "TODOIST_LIST_COMPLETED_TASKS_BY_DUE_DATE" => {
                list_of("tasks", page_items(&body))
            }
            "TODOIST_LIST_ACTIVITY_LOG" => list_of("events", page_items(&body)),
            "TODOIST_LIST_COMMENTS" => list_of("comments", page_items(&body)),
            "TODOIST_COMPLETE_TASK" | "TODOIST_REOPEN_TASK" | "TODOIST_UNARCHIVE_PROJECT" => {
                ok_result(&body)
            }
            "TODOIST_DELETE_TASK" => {
                // The piece treats an already-deleted task (404) as success too.
                json!({"success": true, "deleted": true})
            }
            _other if !self.spec.tools.iter().any(|t| t.slug == _other) => {
                bail!("unknown Todoist tool `{_other}`")
            }
            _ => body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Todoist {
        Todoist::with_api_base("https://todoist.test/api/v1")
    }

    #[test]
    fn endpoints_and_validation() {
        let a = t();
        let r = a
            .build_request("TODOIST_GET_TASK", &json!({"task_id": "6X7rM8997g3RQmvh"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://todoist.test/api/v1/tasks/6X7rM8997g3RQmvh");

        assert!(a.build_request("TODOIST_GET_TASK", &json!({"task_id": "../x"})).is_err());
        assert!(a.build_request("TODOIST_FILTER_TASKS", &json!({})).is_err());
        assert!(a
            .build_request("TODOIST_LIST_COMMENTS", &json!({"task_id": "a", "project_id": "b"}))
            .is_err());
        assert!(a.build_request("TODOIST_LIST_COMMENTS", &json!({})).is_err());
        assert!(a
            .build_request("TODOIST_UPDATE_TASK", &json!({"task_id": "6X7rM8997g3RQmvh"}))
            .is_err());
        assert!(a
            .build_request(
                "TODOIST_MOVE_TASK",
                &json!({"task_id": "6X7rM8997g3RQmvh"})
            )
            .is_err());
    }

    #[test]
    fn due_dates_and_query_encoding() {
        let a = t();
        for (input, field, value) in [
            ("2026-10-15", "due_date", json!("2026-10-15")),
            ("2026-10-15T17:00:00Z", "due_datetime", json!("2026-10-15T17:00:00Z")),
            ("tomorrow 5pm", "due_string", json!("tomorrow 5pm")),
        ] {
            let r = a
                .build_request(
                    "TODOIST_CREATE_TASK",
                    &json!({"content": "Buy milk", "due_date": input}),
                )
                .unwrap();
            let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            assert_eq!(b[field], value);
        }

        let r = a
            .build_request(
                "TODOIST_FILTER_TASKS",
                &json!({"query": "search: meeting notes", "limit": 10}),
            )
            .unwrap();
        assert!(r.url.contains("query=search%3A+meeting+notes"), "{}", r.url);
        assert!(r.url.contains("limit=10"), "{}", r.url);

        // An empty list never clears the labels of a task.
        let r = a
            .build_request(
                "TODOIST_UPDATE_TASK",
                &json!({"task_id": "6X7rM8997g3RQmvh", "content": "Buy oat milk", "labels": []}),
            )
            .unwrap();
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b, json!({"content": "Buy oat milk"}));
        // Only the fields the caller set are sent.
        assert!(a
            .build_request(
                "TODOIST_UPDATE_TASK",
                &json!({"task_id": "6X7rM8997g3RQmvh", "labels": []})
            )
            .is_err());
    }

    #[test]
    fn response_envelopes() {
        use crate::http::HttpResponse;

        let a = t();
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({
                "results": [{"id": "s1", "name": "Todo"}],
                "next_cursor": null
            }))
            .unwrap(),
            session_id: None,
        };
        let out = a.parse_response("TODOIST_LIST_SECTIONS", &resp).unwrap();
        assert_eq!(out["count"], json!(1));
        assert_eq!(out["sections"][0]["name"], json!("Todo"));

        // Legacy `/sections/archived` may answer with a `sections` key.
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"sections": [{"id": "s2"}], "has_more": false})).unwrap(),
            session_id: None,
        };
        let out = a.parse_response("TODOIST_LIST_ARCHIVED_SECTIONS", &resp).unwrap();
        assert_eq!(out["sections"][0]["id"], json!("s2"));

        let out = a.parse_response("TODOIST_DELETE_TASK", &resp).unwrap();
        assert_eq!(out["success"], json!(true));
    }
}