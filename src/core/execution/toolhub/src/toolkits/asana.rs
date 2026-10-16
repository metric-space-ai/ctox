//! Asana toolkit (REST API v1.0), modelled on Activepieces `piece-asana` (MIT).
//!
//! Auth: the piece uses `PieceAuth.OAuth2` with `authUrl https://app.asana.com/-/oauth_authorize`,
//! `tokenUrl https://app.asana.com/-/oauth_token`, scope `default` and no PKCE. The token is
//! then sent as `Authorization: Bearer <token>` by `httpClient`, which is exactly what the hub
//! injects for `AuthScheme::OAuth2` / `AuthScheme::Bearer`, so both are declared.
//!
//! The API base is account independent (`https://app.asana.com/api/1.0` in
//! `src/lib/common/client.ts` and `src/lib/common/index.ts`), so no per-account host is needed.
//!
//! Every action maps to exactly one request taken from the TypeScript `run()`:
//! `asanaClient.asanaData` / `asanaClient.asanaApi` do
//! `httpClient.sendRequest({ url: \`${BASE_URL}${path}\`, body: { data } })`, so write bodies are
//! wrapped in the Asana `{"data": …}` envelope. `asanaClient.asanaListPage` additionally always
//! sends `limit` (default 50, max 100) and an optional `offset` pagination token.
//!
//! `opt_fields` query values are the `ASANA_FIELDS` constants of `src/lib/common/client.ts`.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "asana";

/// `client.ts`: `const BASE_URL = 'https://app.asana.com/api/1.0'`.
const DEFAULT_API_BASE: &str = "https://app.asana.com/api/1.0";

/// `client.ts`: `DEFAULT_LIMIT` / `MAX_LIMIT`.
const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 100;

// `ASANA_FIELDS` from src/lib/common/client.ts, copied verbatim.
const F_USER: &str = "name,email,workspaces.name";
const F_USER_LIST: &str = "name,email";
const F_WORKSPACE: &str = "name,is_organization,email_domains";
const F_TEAM: &str = "name,description,visibility,organization.name,permalink_url";
const F_TEAM_FULL: &str = "name,description,html_description,visibility,endorsed,organization.name,permalink_url";
const F_PROJECT: &str = "name,archived,color,completed,completed_at,created_at,modified_at,default_view,due_on,start_on,notes,owner.name,privacy_setting,team.name,workspace.name,current_status_update.title,current_status_update.status_type,project_brief,members.name,followers.name,permalink_url";
const F_PROJECT_LIST: &str =
    "name,archived,color,created_at,modified_at,due_on,owner.name,privacy_setting,team.name,permalink_url";
const F_SECTION: &str = "name,created_at,project.name";
const F_TAG: &str = "name,color,notes,created_at,followers.name,workspace.name,permalink_url";
const F_TAG_LIST: &str = "name,color,created_at,permalink_url";
const F_TASK: &str = "name,resource_subtype,notes,completed,completed_at,created_at,modified_at,due_on,due_at,start_on,start_at,assignee.name,assignee.email,parent.name,projects.name,memberships.project.name,memberships.section.name,tags.name,followers.name,workspace.name,num_subtasks,permalink_url";
const F_TASK_LIST: &str = "name,resource_subtype,completed,completed_at,created_at,modified_at,due_on,due_at,start_on,assignee.name,parent.name,permalink_url";
const F_STORY: &str = "created_at,created_by.name,resource_subtype,type,text,html_text,is_pinned,is_edited,is_editable,sticker_name,target.name";
const F_STORY_LIST: &str = "created_at,created_by.name,resource_subtype,type,text,is_pinned,is_edited";

const TASK_SUBTYPES: [&str; 4] = ["default_task", "milestone", "approval", "custom"];
const TASK_SORTS: [&str; 6] = [
    "modified_at",
    "created_at",
    "due_date",
    "completed_at",
    "likes",
    "relevance",
];
const TYPEAHEAD_TYPES: [&str; 9] = [
    "task",
    "project",
    "user",
    "tag",
    "team",
    "portfolio",
    "goal",
    "custom_field",
    "project_template",
];

// ---------------------------------------------------------------- schema helpers

/// Optional tri-state boolean (`asanaProps.optionalBoolean`): unset sends nothing at all.
fn tri(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn limit_prop(noun: &str) -> Value {
    json!({"type": "integer",
        "description": format!(
            "Maximum number of {noun} to return in this page, 1 to {MAX_LIMIT}. Defaults to {DEFAULT_LIMIT}."
        ),
        "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT})
}

fn offset_prop() -> Value {
    s("Pagination token for the next page; pass the `next_offset` of the previous call. Tokens expire after a while.")
}

fn page_props(noun: &str) -> Vec<(&'static str, Value)> {
    vec![("limit", limit_prop(noun)), ("offset", offset_prop())]
}

fn with_page(noun: &str, extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = page_props(noun);
    v.extend(extra);
    v
}

fn date(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "pattern": "^\\d{4}-\\d{2}-\\d{2}$"})
}

// ---------------------------------------------------------------- argument helpers

fn non_blank<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// `asanaUtils.pathSegment` = `encodeURIComponent(value.trim())`, used where the piece accepts a
/// reference that is not a bare gid (the user prop also takes `me` and an email address).
fn path_ref(args: &Value, key: &str) -> Result<String> {
    let v = non_blank(args, key).ok_or_else(|| anyhow!("missing argument `{key}`"))?;
    if v.len() > 200 || v == "." || v == ".." || v.chars().any(char::is_control) {
        bail!("invalid argument `{key}`");
    }
    Ok(encode_uri_component(v))
}

fn encode_uri_component(v: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(v.len());
    for b in v.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~'
            | b'*' | b'\'' | b'(' | b')' => out.push(*b as char),
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
    out
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn q(v: Option<&str>) -> Option<String> {
    v.map(String::from)
}

fn qb(v: Option<bool>) -> Option<String> {
    v.map(|b| b.to_string())
}

fn qint(v: u64) -> Option<String> {
    Some(v.to_string())
}

/// `asanaUtils.assertDate` — a real calendar date in `YYYY-MM-DD`.
fn assert_date(v: &str, field: &str) -> Result<String> {
    let t = v.trim();
    let ok = t.len() == 10
        && t.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
        && (1..=12).contains(&t[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&t[8..10].parse::<u32>().unwrap_or(0));
    if !ok {
        bail!("{field} must be a calendar date in YYYY-MM-DD format, got \"{v}\".");
    }
    Ok(t.to_string())
}

/// `asanaUtils.assertDateTime` — ISO 8601 with a time part ending in `Z` or a `±hh:mm` offset.
fn assert_datetime(v: &str, field: &str) -> Result<String> {
    let t = v.trim();
    let b = t.as_bytes();
    let n = t.len();
    let zulu = n > 1 && b[n - 1] == b'Z';
    let offset = n > 6
        && matches!(b[n - 6], b'+' | b'-')
        && b[n - 5].is_ascii_digit()
        && b[n - 4].is_ascii_digit()
        && &t[n - 3..] == ":00";
    let day_ok = n > 10 && b[4] == b'-' && b[7] == b'-';
    if !((zulu || offset) && day_ok && t.contains('T')) {
        bail!("{field} must be an ISO 8601 date-time that ends in Z or a +hh:mm/-hh:mm offset, such as 2026-10-01T15:00:00Z, got \"{v}\".");
    }
    Ok(t.to_string())
}

/// `asanaUtils.hasValue` — defined, non-null and not an empty string.
fn has_value(v: &Value, key: &str) -> bool {
    match v.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = non_blank(args, key) {
        m.insert(field.into(), json!(v));
    }
}

// ---------------------------------------------------------------- adapter

pub struct Asana {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Asana {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_API_BASE)
    }
}

impl Asana {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc: (&str, &str), props: Props<'_>, req: &[&str], m| {
            // Asana's OAuth app declares the single scope `default`; the token carries all
            // permissions, so every tool carries that scope.
            tool(SLUG, slug, title, desc, props, req, &["default"], m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Asana".into(),
            description: LocalizedText::new(
                "Tasks, Projekte, Abschnitte, Tags, Teams und Kommentare in Asana lesen und bearbeiten.",
                "Read and change tasks, projects, sections, tags, teams and comments in Asana.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://app.asana.com/-/oauth_authorize".into(),
                    token_url: "https://app.asana.com/-/oauth_token".into(),
                    default_scopes: vec!["default".into()],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                // A personal access token works the same way (`Authorization: Bearer <token>`).
                AuthScheme::Bearer,
            ],
            tools: vec![
                // --- read / search ---
                t(
                    "ASANA_GET_CURRENT_USER",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    (
                        "Liest den Nutzer hinter der Verbindung (gid, Name, E-Mail) samt Workspaces.",
                        "Reads the user behind this connection (gid, name, email) with their workspaces.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "ASANA_GET_USER",
                    ("Nutzer lesen", "Get user"),
                    (
                        "Liest einen Nutzer per \"me\", E-Mail-Adresse oder User-gid.",
                        "Reads one user by \"me\", email address or user gid.",
                    ),
                    &[("user", s("\"me\", eine E-Mail-Adresse oder eine User-gid"))],
                    &["user"],
                    false,
                ),
                t(
                    "ASANA_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet die Nutzer eines Workspace oder eines Teams.",
                        "Lists the users of a workspace or a team.",
                    ),
                    &with_page(
                        "users",
                        vec![
                            ("workspace", s("Gid des Workspace; leer lassen, wenn `team` gesetzt ist")),
                            ("team", s("Gid des Teams; leer lassen, wenn `workspace` gesetzt ist")),
                        ],
                    ),
                    &[],
                    false,
                ),
                t(
                    "ASANA_GET_WORKSPACE",
                    ("Workspace lesen", "Get workspace"),
                    (
                        "Liest einen Workspace bzw. eine Organisation.",
                        "Reads one workspace or organization.",
                    ),
                    &[("workspace", s("Workspace- bzw. Organisations-gid"))],
                    &["workspace"],
                    false,
                ),
                t(
                    "ASANA_LIST_WORKSPACES",
                    ("Workspaces auflisten", "List workspaces"),
                    (
                        "Listet die Workspaces und Organisationen der Verbindung.",
                        "Lists the workspaces and organizations the connection can see.",
                    ),
                    &page_props("workspaces"),
                    &[],
                    false,
                ),
                t(
                    "ASANA_GET_TEAM",
                    ("Team lesen", "Get team"),
                    (
                        "Liest ein Team; Teams gibt es nur in Organisationen.",
                        "Reads one team; teams exist only inside organizations.",
                    ),
                    &[("team", s("Team-gid"))],
                    &["team"],
                    false,
                ),
                t(
                    "ASANA_LIST_TEAMS",
                    ("Teams auflisten", "List teams"),
                    (
                        "Listet die Teams einer Organisation.",
                        "Lists the teams of one organization.",
                    ),
                    &with_page("teams", vec![("workspace", s("Gid der Organisation"))]),
                    &["workspace"],
                    false,
                ),
                t(
                    "ASANA_GET_PROJECT",
                    ("Projekt lesen", "Get project"),
                    (
                        "Liest ein Projekt mit Status, Team, Mitgliedern und Link.",
                        "Reads one project with status, team, members and link.",
                    ),
                    &[("project", s("Projekt-gid"))],
                    &["project"],
                    false,
                ),
                t(
                    "ASANA_LIST_PROJECTS",
                    ("Projekte auflisten", "List projects"),
                    (
                        "Listet die Projekte eines Workspace, optional nur archivierte oder aktive.",
                        "Lists the projects of a workspace, optionally only archived or only active ones.",
                    ),
                    &with_page(
                        "projects",
                        vec![
                            ("workspace", s("Gid des Workspace bzw. der Organisation")),
                            ("archived", tri("Ja nur archivierte, Nein nur aktive Projekte; leer = beide")),
                        ],
                    ),
                    &["workspace"],
                    false,
                ),
                t(
                    "ASANA_GET_SECTION",
                    ("Abschnitt lesen", "Get section"),
                    (
                        "Liest einen Abschnitt (Listenüberschrift oder Board-Spalte).",
                        "Reads one section (list heading or board column).",
                    ),
                    &[("section", s("Abschnitts-gid"))],
                    &["section"],
                    false,
                ),
                t(
                    "ASANA_LIST_SECTIONS",
                    ("Abschnitte auflisten", "List sections"),
                    (
                        "Listet die Abschnitte eines Projekts in Anzeigereihenfolge.",
                        "Lists the sections of a project in display order.",
                    ),
                    &with_page("sections", vec![("project", s("Projekt-gid"))]),
                    &["project"],
                    false,
                ),
                t(
                    "ASANA_GET_TAG",
                    ("Tag lesen", "Get tag"),
                    (
                        "Liest einen Tag mit Farbe, Followern und Workspace.",
                        "Reads one tag with color, followers and workspace.",
                    ),
                    &[("tag", s("Tag-gid"))],
                    &["tag"],
                    false,
                ),
                t(
                    "ASANA_LIST_TAGS",
                    ("Tags auflisten", "List tags"),
                    (
                        "Listet die Tags eines Workspace.",
                        "Lists the tags of a workspace.",
                    ),
                    &with_page("tags", vec![("workspace", s("Gid des Workspace"))]),
                    &["workspace"],
                    false,
                ),
                t(
                    "ASANA_GET_TASK",
                    ("Task lesen", "Get task"),
                    (
                        "Liest einen Task mit Beschreibung, Terminen, Assignee, Tags und Link.",
                        "Reads one task with description, dates, assignee, tags and link.",
                    ),
                    &[("task", s("Task-gid"))],
                    &["task"],
                    false,
                ),
                t(
                    "ASANA_LIST_PROJECT_TASKS",
                    ("Projekt-Tasks auflisten", "List project tasks"),
                    (
                        "Listet die Tasks eines Projekts in Projektreihenfolge.",
                        "Lists the tasks of one project in project order.",
                    ),
                    &with_page(
                        "tasks",
                        vec![
                            ("project", s("Projekt-gid")),
                            ("completed_since", s("Nur unfertige Tasks plus Tasks, die nach diesem ISO-Zeitpunkt fertig wurden; \"now\" = nur unfertige")),
                        ],
                    ),
                    &["project"],
                    false,
                ),
                t(
                    "ASANA_LIST_SUBTASKS",
                    ("Subtasks auflisten", "List subtasks"),
                    (
                        "Listet die direkten Subtasks eines Tasks.",
                        "Lists the direct subtasks of one task.",
                    ),
                    &with_page("subtasks", vec![("task", s("Gid des Eltern-Tasks"))]),
                    &["task"],
                    false,
                ),
                t(
                    "ASANA_LIST_TASK_TAGS",
                    ("Task-Tags auflisten", "List task tags"),
                    (
                        "Listet die Tags, die aktuell an einem Task hängen.",
                        "Lists the tags currently applied to one task.",
                    ),
                    &with_page("tags", vec![("task", s("Task-gid"))]),
                    &["task"],
                    false,
                ),
                t(
                    "ASANA_GET_STORY",
                    ("Kommentar lesen", "Get story"),
                    (
                        "Liest einen Kommentar bzw. einen Aktivitätseintrag (Story) per gid.",
                        "Reads one comment or activity entry (story) by gid.",
                    ),
                    &[("story", s("Story-gid"))],
                    &["story"],
                    false,
                ),
                t(
                    "ASANA_LIST_TASK_STORIES",
                    ("Task-Kommentare auflisten", "List task stories"),
                    (
                        "Listet die Kommentare und Aktivitäten eines Tasks, älteste zuerst.",
                        "Lists the comments and activity of a task, oldest first.",
                    ),
                    &with_page("stories", vec![("task", s("Task-gid"))]),
                    &["task"],
                    false,
                ),
                t(
                    "ASANA_SEARCH_TASKS",
                    ("Tasks durchsuchen", "Search tasks"),
                    (
                        "Asana Advanced Search über einen Workspace (nur bezahlte Pläne).",
                        "Asana advanced search over one workspace (paid plans only).",
                    ),
                    &[
                        ("workspace", s("Gid des zu durchsuchenden Workspace")),
                        ("text", s("Wörter für Name und Beschreibung; leer = nur filtern")),
                        (
                            "resource_subtype",
                            choice("Nur Tasks dieses Typs; leer = alle", &TASK_SUBTYPES),
                        ),
                        ("assignee_ids", str_list("Users, von denen einer der Assignee sein darf")),
                        ("project_ids", str_list("Projekt-gids; Tasks in einem davon treffen")),
                        ("section_ids", str_list("Abschnitts-gids; Tasks in einem davon treffen")),
                        ("tag_ids", str_list("Tag-gids; Tasks mit einem davon treffen")),
                        ("completed", tri("Ja nur fertige, Nein nur offene Tasks; leer = beide")),
                        ("is_blocked", tri("Ja nur blockierte, Nein nur unblockierte Tasks; leer = beide")),
                        ("is_subtask", tri("Ja nur Subtasks, Nein nur Top-Level-Tasks; leer = beide")),
                        ("due_on_after", date("Nur Tasks mit Fälligkeit nach diesem Datum")),
                        ("due_on_before", date("Nur Tasks mit Fälligkeit vor diesem Datum")),
                        ("modified_at_after", s("Nur Tasks, die nach diesem ISO-Zeitpunkt geändert wurden (Z oder ±hh:mm)")),
                        ("created_at_after", s("Nur Tasks, die nach diesem ISO-Zeitpunkt erstellt wurden (Z oder ±hh:mm)")),
                        ("sort_by", choice("Sortierung; leer = zuletzt geändert zuerst", &TASK_SORTS)),
                        ("sort_ascending", tri("Ja = älteste/kleinste zuerst; leer = absteigend")),
                        ("limit", limit_prop("tasks")),
                    ],
                    &["workspace"],
                    false,
                ),
                t(
                    "ASANA_SEARCH_WORKSPACE_OBJECTS",
                    ("Objekte finden", "Search workspace objects"),
                    (
                        "Typeahead-Suche nach Objektnamen im Workspace (Tasks, Projekte, Nutzer …).",
                        "Typeahead lookup by object name in a workspace (tasks, projects, users, …).",
                    ),
                    &[
                        ("workspace", s("Gid des Workspace")),
                        (
                            "resource_type",
                            choice("Welche Objektart zurückgegeben wird", &TYPEAHEAD_TYPES),
                        ),
                        ("query", s("Text für Objektnamen; leer = zuletzt relevante Objekte")),
                        (
                            "count",
                            json!({"type": "integer",
                                "description": "Anzahl der Treffer, 1 bis 100; leer = Asana-Standard.",
                                "minimum": 1, "maximum": 100}),
                        ),
                    ],
                    &["workspace", "resource_type"],
                    false,
                ),
                // --- write ---
                t(
                    "ASANA_CREATE_TASK",
                    ("Task anlegen", "Create task"),
                    (
                        "Legt einen Task in einem Projekt an; Tags werden als gid übergeben. Braucht --confirm.",
                        "Creates a task in a project; tags are passed as gids. Requires --confirm.",
                    ),
                    &[
                        ("project", s("Projekt-gid, in dem der Task landet")),
                        ("name", s("Titel des Tasks")),
                        ("notes", s("Beschreibung des Tasks")),
                        ("assignee", s("Assignee: \"me\", E-Mail oder User-gid")),
                        ("due_at", s("Fälligkeit als ISO-Zeitpunkt mit Z oder ±hh:mm")),
                        ("tag_gids", str_list("Tag-gids zum Setzen (aus ASANA_LIST_TAGS)")),
                    ],
                    &["project", "name", "notes"],
                    true,
                ),
                t(
                    "ASANA_UPDATE_TASK",
                    ("Task ändern", "Update task"),
                    (
                        "Ändert nur die übergebenen Felder eines Tasks; braucht --confirm.",
                        "Changes only the fields you pass on a task; requires --confirm.",
                    ),
                    &[
                        ("task", s("Task-gid")),
                        ("name", s("Neuer Titel; leer lassen zum Beibehalten")),
                        ("notes", s("Neue Beschreibung; nicht mit `html_notes` kombinieren")),
                        ("html_notes", s("Neue Rich-Text-Beschreibung in <body>…</body>; nicht mit `notes`")),
                        ("assignee", s("Neuer Assignee: \"me\", E-Mail oder User-gid")),
                        ("due_on", date("Neues Ganztags-Fälligkeitsdatum; nicht mit `due_at`")),
                        ("due_at", s("Neuer Fälligkeitszeitpunkt, ISO mit Z oder ±hh:mm; nicht mit `due_on`")),
                        ("start_on", date("Neues Startdatum; Asana verlangt dafür ein Fälligkeitsdatum")),
                        ("completed", tri("Ja = Task abschließen, Nein = wieder öffnen; leer = unverändert")),
                    ],
                    &["task"],
                    true,
                ),
                t(
                    "ASANA_DELETE_TASK",
                    ("Task löschen", "Delete task"),
                    (
                        "Verschiebt einen Task in den Papierkorb (30 Tage wiederherstellbar); braucht --confirm.",
                        "Moves a task to the trash (restorable for 30 days); requires --confirm.",
                    ),
                    &[("task", s("Task-gid"))],
                    &["task"],
                    true,
                ),
                t(
                    "ASANA_CREATE_SUBTASK",
                    ("Subtask anlegen", "Create subtask"),
                    (
                        "Legt einen Subtask unter einem Task an; braucht --confirm.",
                        "Creates a subtask under a task; requires --confirm.",
                    ),
                    &[
                        ("task", s("Gid des Eltern-Tasks")),
                        ("name", s("Titel des Subtasks")),
                        ("notes", s("Beschreibung; nicht mit `html_notes` kombinieren")),
                        ("html_notes", s("Rich-Text-Beschreibung; nicht mit `notes` kombinieren")),
                        ("assignee", s("Assignee: \"me\", E-Mail oder User-gid")),
                        ("due_on", date("Ganztags-Fälligkeit; nicht mit `due_at` kombinieren")),
                        ("due_at", s("Fälligkeitszeitpunkt, ISO mit Z oder ±hh:mm; nicht mit `due_on`")),
                        ("start_on", date("Startdatum; braucht ein Fälligkeitsdatum")),
                        ("followers", str_list("User-gids bzw. \"me\" als Follower")),
                    ],
                    &["task", "name"],
                    true,
                ),
                t(
                    "ASANA_ADD_TASK_COMMENT",
                    ("Kommentar schreiben", "Add task comment"),
                    (
                        "Postet einen Kommentar auf einem Task; braucht --confirm.",
                        "Posts a comment on a task; requires --confirm.",
                    ),
                    &[
                        ("task", s("Task-gid")),
                        ("text", s("Klartext-Kommentar; nicht mit `html_text` kombinieren")),
                        ("html_text", s("Rich-Text-Kommentar in <body>…</body>; nicht mit `text` kombinieren")),
                        ("is_pinned", tri("Ja pinnt den Kommentar an; leer = nicht angeheftet")),
                    ],
                    &["task"],
                    true,
                ),
                t(
                    "ASANA_ADD_TAG_TO_TASK",
                    ("Tag hinzufügen", "Add tag to task"),
                    (
                        "Hängt einen bestehenden Tag an einen Task; braucht --confirm.",
                        "Applies an existing tag to a task; requires --confirm.",
                    ),
                    &[
                        ("task", s("Task-gid")),
                        ("tag", s("Tag-gid (aus ASANA_LIST_TAGS)")),
                    ],
                    &["task", "tag"],
                    true,
                ),
                t(
                    "ASANA_ADD_TASK_TO_PROJECT",
                    ("Task zum Projekt hinzufügen", "Add task to project"),
                    (
                        "Fügt einen bestehenden Task (optional in einen Abschnitt) einem Projekt hinzu; braucht --confirm.",
                        "Adds an existing task (optionally into a section) to a project; requires --confirm.",
                    ),
                    &[
                        ("task", s("Task-gid")),
                        ("project", s("Projekt-gid")),
                        ("section", s("Abschnitts-gid, in den der Task am Ende eingefügt wird")),
                        ("insert_before", s("Task-gid, vor dem eingefügt wird; nicht mit `insert_after`")),
                        ("insert_after", s("Task-gid, nach dem eingefügt wird; nicht mit `insert_before`")),
                    ],
                    &["task", "project"],
                    true,
                ),
                t(
                    "ASANA_MOVE_TASK_TO_SECTION",
                    ("Task in Abschnitt verschieben", "Move task to section"),
                    (
                        "Verschiebt einen Task in einen Abschnitt; braucht --confirm.",
                        "Moves a task into a section; requires --confirm.",
                    ),
                    &[
                        ("section", s("Gid des Zielabschnitts")),
                        ("task", s("Task-gid")),
                        ("insert_before", s("Task-gid, vor dem eingefügt wird; nicht mit `insert_after`")),
                        ("insert_after", s("Task-gid, nach dem eingefügt wird; nicht mit `insert_before`")),
                    ],
                    &["section", "task"],
                    true,
                ),
                t(
                    "ASANA_CREATE_SECTION",
                    ("Abschnitt anlegen", "Create section"),
                    (
                        "Legt einen Abschnitt (Listenüberschrift oder Board-Spalte) in einem Projekt an; braucht --confirm.",
                        "Creates a section (list heading or board column) in a project; requires --confirm.",
                    ),
                    &[
                        ("project", s("Projekt-gid")),
                        ("name", s("Name des Abschnitts, z. B. \"In review\"")),
                        ("insert_before", s("Abschnitts-gid, vor dem eingefügt wird; nicht mit `insert_after`")),
                        ("insert_after", s("Abschnitts-gid, nach dem eingefügt wird; nicht mit `insert_before`")),
                    ],
                    &["project", "name"],
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

    /// `limit`/`offset` tail that `asanaClient.asanaListPage` always appends.
    fn page_tail<'a>(args: &'a Value, pairs: &mut Vec<(&'a str, Option<String>)>) {
        pairs.push(("limit", qint(opt_u64(args, "limit", DEFAULT_LIMIT, 1, MAX_LIMIT))));
        pairs.push(("offset", q(opt_str(args, "offset"))));
    }
}

impl ToolkitAdapter for Asana {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let mut pairs: Vec<(&str, Option<String>)> = Vec::new();
        let path: String;
        let body: Option<Value>;

        match tool {
            // AP: src/lib/actions/ai/get-current-user.ts
            "ASANA_GET_CURRENT_USER" => {
                path = "/users/me".into();
                pairs.push(("opt_fields", q(Some(F_USER))));
                body = None;
            }
            // AP: src/lib/actions/ai/get-user.ts
            "ASANA_GET_USER" => {
                path = format!("/users/{}", path_ref(args, "user")?);
                pairs.push(("opt_fields", q(Some(F_USER))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-users.ts
            "ASANA_LIST_USERS" => {
                if non_blank(args, "workspace").is_none() && non_blank(args, "team").is_none() {
                    bail!("Set Workspace GID or Team GID so Asana knows which users to list.");
                }
                path = "/users".into();
                pairs.push(("workspace", q(non_blank(args, "workspace"))));
                pairs.push(("team", q(non_blank(args, "team"))));
                pairs.push(("opt_fields", q(Some(F_USER_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-workspace.ts
            "ASANA_GET_WORKSPACE" => {
                path = format!("/workspaces/{}", path_segment(args, "workspace")?);
                pairs.push(("opt_fields", q(Some(F_WORKSPACE))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-workspaces.ts
            "ASANA_LIST_WORKSPACES" => {
                path = "/workspaces".into();
                pairs.push(("opt_fields", q(Some(F_WORKSPACE))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-team.ts
            "ASANA_GET_TEAM" => {
                path = format!("/teams/{}", path_segment(args, "team")?);
                pairs.push(("opt_fields", q(Some(F_TEAM_FULL))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-teams.ts
            "ASANA_LIST_TEAMS" => {
                path = format!(
                    "/workspaces/{}/teams",
                    path_segment(args, "workspace")?
                );
                pairs.push(("opt_fields", q(Some(F_TEAM))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-project.ts
            "ASANA_GET_PROJECT" => {
                path = format!("/projects/{}", path_segment(args, "project")?);
                pairs.push(("opt_fields", q(Some(F_PROJECT))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-projects.ts
            "ASANA_LIST_PROJECTS" => {
                path = "/projects".into();
                pairs.push((
                    "workspace",
                    q(Some(req_str(args, "workspace")?.trim())),
                ));
                pairs.push(("archived", qb(opt_bool(args, "archived"))));
                pairs.push(("opt_fields", q(Some(F_PROJECT_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-section.ts
            "ASANA_GET_SECTION" => {
                path = format!("/sections/{}", path_segment(args, "section")?);
                pairs.push(("opt_fields", q(Some(F_SECTION))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-sections.ts
            "ASANA_LIST_SECTIONS" => {
                path = format!(
                    "/projects/{}/sections",
                    path_segment(args, "project")?
                );
                pairs.push(("opt_fields", q(Some(F_SECTION))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-tag.ts
            "ASANA_GET_TAG" => {
                path = format!("/tags/{}", path_segment(args, "tag")?);
                pairs.push(("opt_fields", q(Some(F_TAG))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-tags.ts
            "ASANA_LIST_TAGS" => {
                path = "/tags".into();
                pairs.push((
                    "workspace",
                    q(Some(req_str(args, "workspace")?.trim())),
                ));
                pairs.push(("opt_fields", q(Some(F_TAG_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-task.ts
            "ASANA_GET_TASK" => {
                path = format!("/tasks/{}", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_TASK))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-project-tasks.ts
            "ASANA_LIST_PROJECT_TASKS" => {
                path = format!(
                    "/projects/{}/tasks",
                    path_segment(args, "project")?
                );
                pairs.push(("completed_since", q(non_blank(args, "completed_since"))));
                pairs.push(("opt_fields", q(Some(F_TASK_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/list-subtasks.ts
            "ASANA_LIST_SUBTASKS" => {
                path = format!("/tasks/{}/subtasks", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_TASK_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/list-task-tags.ts
            "ASANA_LIST_TASK_TAGS" => {
                path = format!("/tasks/{}/tags", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_TAG_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/get-story.ts
            "ASANA_GET_STORY" => {
                path = format!("/stories/{}", path_segment(args, "story")?);
                pairs.push(("opt_fields", q(Some(F_STORY))));
                body = None;
            }
            // AP: src/lib/actions/ai/list-task-stories.ts
            "ASANA_LIST_TASK_STORIES" => {
                path = format!("/tasks/{}/stories", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_STORY_LIST))));
                Self::page_tail(args, &mut pairs);
                body = None;
            }
            // AP: src/lib/actions/ai/search-tasks.ts
            "ASANA_SEARCH_TASKS" => {
                path = format!(
                    "/workspaces/{}/tasks/search",
                    path_segment(args, "workspace")?
                );
                let join = |key: &str| -> Option<String> {
                    let v = str_vec(args, key);
                    (!v.is_empty()).then(|| v.join(","))
                };
                pairs.push(("text", q(non_blank(args, "text"))));
                pairs.push((
                    "resource_subtype",
                    q(if TASK_SUBTYPES.contains(&opt_str(args, "resource_subtype").unwrap_or("")) {
                        opt_str(args, "resource_subtype")
                    } else {
                        None
                    }),
                ));
                pairs.push(("assignee.any", join("assignee_ids")));
                pairs.push(("projects.any", join("project_ids")));
                pairs.push(("sections.any", join("section_ids")));
                pairs.push(("tags.any", join("tag_ids")));
                pairs.push(("completed", qb(opt_bool(args, "completed"))));
                pairs.push(("is_blocked", qb(opt_bool(args, "is_blocked"))));
                pairs.push(("is_subtask", qb(opt_bool(args, "is_subtask"))));
                if let Some(v) = non_blank(args, "due_on_after") {
                    pairs.push(("due_on.after", Some(assert_date(v, "Due After")?)));
                }
                if let Some(v) = non_blank(args, "due_on_before") {
                    pairs.push(("due_on.before", Some(assert_date(v, "Due Before")?)));
                }
                if let Some(v) = non_blank(args, "modified_at_after") {
                    pairs.push((
                        "modified_at.after",
                        Some(assert_datetime(v, "Modified After")?),
                    ));
                }
                if let Some(v) = non_blank(args, "created_at_after") {
                    pairs.push((
                        "created_at.after",
                        Some(assert_datetime(v, "Created After")?),
                    ));
                }
                pairs.push((
                    "sort_by",
                    q(if TASK_SORTS.contains(&opt_str(args, "sort_by").unwrap_or("")) {
                        opt_str(args, "sort_by")
                    } else {
                        None
                    }),
                ));
                pairs.push(("sort_ascending", qb(opt_bool(args, "sort_ascending"))));
                pairs.push(("limit", qint(opt_u64(args, "limit", DEFAULT_LIMIT, 1, MAX_LIMIT))));
                pairs.push(("opt_fields", q(Some(F_TASK_LIST))));
                body = None;
            }
            // AP: src/lib/actions/ai/search-workspace-objects.ts
            "ASANA_SEARCH_WORKSPACE_OBJECTS" => {
                path = format!(
                    "/workspaces/{}/typeahead",
                    path_segment(args, "workspace")?
                );
                let rt = req_str(args, "resource_type")?.trim();
                if !TYPEAHEAD_TYPES.contains(&rt) {
                    bail!("`resource_type` must be one of {}", TYPEAHEAD_TYPES.join(", "));
                }
                pairs.push(("resource_type", q(Some(rt))));
                pairs.push(("query", q(non_blank(args, "query"))));
                pairs.push((
                    "count",
                    args.get("count").and_then(Value::as_u64).map(|c| c.to_string()),
                ));
                body = None;
            }
            // AP: src/lib/actions/create-task.ts
            "ASANA_CREATE_TASK" => {
                path = "/tasks".into();
                let mut data = Map::new();
                data.insert("name".into(), json!(req_str(args, "name")?.trim()));
                data.insert(
                    "projects".into(),
                    json!([req_str(args, "project")?.trim()]),
                );
                data.insert("notes".into(), json!(req_str(args, "notes")?));
                put_str(&mut data, args, "assignee", "assignee");
                if let Some(v) = non_blank(args, "due_at") {
                    data.insert("due_at".into(), json!(assert_datetime(v, "Due Date and Time")?));
                }
                let tags = str_vec(args, "tag_gids");
                if !tags.is_empty() {
                    data.insert("tags".into(), json!(tags));
                }
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/update-task.ts
            "ASANA_UPDATE_TASK" => {
                path = format!("/tasks/{}", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_TASK))));
                if has_value(args, "notes") && has_value(args, "html_notes") {
                    bail!("Set either Description or HTML Description, not both; Asana accepts only one of them.");
                }
                if has_value(args, "due_on") && has_value(args, "due_at") {
                    bail!("Set either Due Date or Due Date and Time, not both; Asana accepts only one of them.");
                }
                let mut data = Map::new();
                put_str(&mut data, args, "name", "name");
                put_str(&mut data, args, "notes", "notes");
                put_str(&mut data, args, "html_notes", "html_notes");
                put_str(&mut data, args, "assignee", "assignee");
                if let Some(v) = non_blank(args, "due_on") {
                    data.insert("due_on".into(), json!(assert_date(v, "Due Date")?));
                }
                if let Some(v) = non_blank(args, "due_at") {
                    data.insert(
                        "due_at".into(),
                        json!(assert_datetime(v, "Due Date and Time")?),
                    );
                }
                if let Some(v) = non_blank(args, "start_on") {
                    data.insert(
                        "start_on".into(),
                        json!(assert_date(v, "Start Date")?),
                    );
                }
                if let Some(c) = opt_bool(args, "completed") {
                    data.insert("completed".into(), json!(c));
                }
                if data.is_empty() {
                    bail!("Nothing to update: set at least one of Task Name, Description, HTML Description, Assignee, Due Date, Due Date and Time, Start Date or Completed.");
                }
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/delete-task.ts
            "ASANA_DELETE_TASK" => {
                path = format!("/tasks/{}", path_segment(args, "task")?);
                body = None;
            }
            // AP: src/lib/actions/ai/create-subtask.ts
            "ASANA_CREATE_SUBTASK" => {
                path = format!("/tasks/{}/subtasks", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_TASK))));
                if has_value(args, "notes") && has_value(args, "html_notes") {
                    bail!("Set either Description or HTML Description, not both; Asana accepts only one of them.");
                }
                if has_value(args, "due_on") && has_value(args, "due_at") {
                    bail!("Set either Due Date or Due Date and Time, not both; Asana accepts only one of them.");
                }
                let mut data = Map::new();
                data.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut data, args, "notes", "notes");
                put_str(&mut data, args, "html_notes", "html_notes");
                put_str(&mut data, args, "assignee", "assignee");
                if let Some(v) = non_blank(args, "due_on") {
                    data.insert("due_on".into(), json!(assert_date(v, "Due Date")?));
                }
                if let Some(v) = non_blank(args, "due_at") {
                    data.insert(
                        "due_at".into(),
                        json!(assert_datetime(v, "Due Date and Time")?),
                    );
                }
                if let Some(v) = non_blank(args, "start_on") {
                    data.insert(
                        "start_on".into(),
                        json!(assert_date(v, "Start Date")?),
                    );
                }
                let followers = str_vec(args, "followers");
                if !followers.is_empty() {
                    data.insert("followers".into(), json!(followers));
                }
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/add-task-comment.ts
            "ASANA_ADD_TASK_COMMENT" => {
                path = format!("/tasks/{}/stories", path_segment(args, "task")?);
                pairs.push(("opt_fields", q(Some(F_STORY))));
                if has_value(args, "text") && has_value(args, "html_text") {
                    bail!("Set either Text or HTML Text, not both; Asana accepts only one of them.");
                }
                if !has_value(args, "text") && !has_value(args, "html_text") {
                    bail!("Provide the comment in Text or HTML Text.");
                }
                let mut data = Map::new();
                put_str(&mut data, args, "text", "text");
                put_str(&mut data, args, "html_text", "html_text");
                if let Some(p) = opt_bool(args, "is_pinned") {
                    data.insert("is_pinned".into(), json!(p));
                }
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/add-tag-to-task.ts
            "ASANA_ADD_TAG_TO_TASK" => {
                path = format!("/tasks/{}/addTag", path_segment(args, "task")?);
                let tag = req_str(args, "tag")?.trim();
                body = Some(json!({"data": {"tag": tag}}));
            }
            // AP: src/lib/actions/ai/add-task-to-project.ts
            "ASANA_ADD_TASK_TO_PROJECT" => {
                path = format!(
                    "/tasks/{}/addProject",
                    path_segment(args, "task")?
                );
                if non_blank(args, "insert_before").is_some() && non_blank(args, "insert_after").is_some() {
                    bail!("Set either Insert Before Task or Insert After Task, not both; Asana accepts only one of them.");
                }
                let mut data = Map::new();
                data.insert(
                    "project".into(),
                    json!(req_str(args, "project")?.trim()),
                );
                put_str(&mut data, args, "section", "section");
                put_str(&mut data, args, "insert_before", "insert_before");
                put_str(&mut data, args, "insert_after", "insert_after");
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/move-task-to-section.ts
            "ASANA_MOVE_TASK_TO_SECTION" => {
                path = format!(
                    "/sections/{}/addTask",
                    path_segment(args, "section")?
                );
                if non_blank(args, "insert_before").is_some() && non_blank(args, "insert_after").is_some() {
                    bail!("Set either Insert Before Task or Insert After Task, not both; Asana accepts only one of them.");
                }
                let mut data = Map::new();
                data.insert("task".into(), json!(req_str(args, "task")?.trim()));
                put_str(&mut data, args, "insert_before", "insert_before");
                put_str(&mut data, args, "insert_after", "insert_after");
                body = Some(json!({"data": Value::Object(data)}));
            }
            // AP: src/lib/actions/ai/create-section.ts
            "ASANA_CREATE_SECTION" => {
                path = format!(
                    "/projects/{}/sections",
                    path_segment(args, "project")?
                );
                pairs.push(("opt_fields", q(Some(F_SECTION))));
                if non_blank(args, "insert_before").is_some() && non_blank(args, "insert_after").is_some() {
                    bail!("Set either Insert Before Section or Insert After Section, not both; Asana accepts only one of them.");
                }
                let mut data = Map::new();
                data.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut data, args, "insert_before", "insert_before");
                put_str(&mut data, args, "insert_after", "insert_after");
                body = Some(json!({"data": Value::Object(data)}));
            }
            other => bail!("unknown Asana tool `{other}`"),
        }

        let method = method_of(tool);
        let url = format!("{}{}", self.url(&path), query(&pairs));
        Ok(match body {
            Some(b) => HttpRequest::new(method, url).json(&b),
            None => HttpRequest::new(method, url),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let body = response.json()?;
        check_errors(&body)?;
        Ok(match tool {
            // `asanaClient.asanaData` returns `body.data` as-is.
            "ASANA_GET_CURRENT_USER"
            | "ASANA_GET_USER"
            | "ASANA_GET_WORKSPACE"
            | "ASANA_GET_TEAM"
            | "ASANA_GET_PROJECT"
            | "ASANA_GET_SECTION"
            | "ASANA_GET_TAG"
            | "ASANA_GET_TASK"
            | "ASANA_GET_STORY"
            | "ASANA_CREATE_TASK"
            | "ASANA_UPDATE_TASK"
            | "ASANA_CREATE_SUBTASK"
            | "ASANA_ADD_TASK_COMMENT"
            | "ASANA_CREATE_SECTION" => envelope_data(&body)?,
            // `asanaClient.asanaListPage` returns `{data, next_offset}`.
            "ASANA_LIST_USERS"
            | "ASANA_LIST_WORKSPACES"
            | "ASANA_LIST_TEAMS"
            | "ASANA_LIST_PROJECTS"
            | "ASANA_LIST_SECTIONS"
            | "ASANA_LIST_TAGS"
            | "ASANA_LIST_PROJECT_TASKS"
            | "ASANA_LIST_SUBTASKS"
            | "ASANA_LIST_TASK_TAGS"
            | "ASANA_LIST_TASK_STORIES" => page(&body)?,
            // `{ data }` wrappers around a bare array.
            "ASANA_SEARCH_TASKS" => json!({
                "data": body["data"].as_array().cloned().unwrap_or_default()
            }),
            "ASANA_SEARCH_WORKSPACE_OBJECTS" => json!({"data": body["data"].clone()}),
            // `asanaClient.asanaEmpty` ignores the provider body; the actions answer with a
            // fixed `{success: true, …}` object built from their own props. `parse_response`
            // does not see the arguments, so the provider echo is merged in instead.
            "ASANA_ADD_TAG_TO_TASK"
            | "ASANA_DELETE_TASK"
            | "ASANA_ADD_TASK_TO_PROJECT"
            | "ASANA_MOVE_TASK_TO_SECTION" => {
                let mut out = Map::new();
                out.insert("success".into(), json!(true));
                if let Some(d) = body["data"].as_object() {
                    for (k, v) in d {
                        out.insert(k.clone(), v.clone());
                    }
                }
                Value::Object(out)
            }
            other => bail!("unknown Asana tool `{other}`"),
        })
    }
}

/// HTTP method of `tool`, as used by the `run()` of each TypeScript action.
fn method_of(tool: &str) -> &'static str {
    match tool {
        "ASANA_UPDATE_TASK" => "PUT",
        "ASANA_DELETE_TASK" => "DELETE",
        "ASANA_CREATE_TASK"
        | "ASANA_CREATE_SUBTASK"
        | "ASANA_CREATE_SECTION"
        | "ASANA_ADD_TASK_COMMENT"
        | "ASANA_ADD_TAG_TO_TASK"
        | "ASANA_ADD_TASK_TO_PROJECT"
        | "ASANA_MOVE_TASK_TO_SECTION" => "POST",
        _ => "GET",
    }
}

fn check_errors(body: &Value) -> Result<()> {
    let Some(errs) = body["errors"].as_array() else {
        return Ok(());
    };
    if errs.is_empty() {
        return Ok(());
    }
    let msg = errs
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect::<Vec<_>>()
        .join("; ");
    bail!(
        "Asana API error: {}",
        if msg.is_empty() { "unknown" } else { &msg }
    )
}

fn envelope_data(body: &Value) -> Result<Value> {
    match body.get("data") {
        Some(d) if !d.is_null() => Ok(d.clone()),
        _ => Err(anyhow!("Asana returned an empty response")),
    }
}

/// `asanaClient.asanaListPage`.
fn page(body: &Value) -> Result<Value> {
    Ok(json!({
        "data": body["data"].as_array().cloned().unwrap_or_default(),
        "next_offset": body["next_page"]["offset"].as_str().filter(|s| !s.is_empty()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_validation() {
        let a = Asana::with_api_base("https://asana.test/api/1.0");
        let r = a
            .build_request("ASANA_GET_TASK", &json!({"task": "1204567890123456"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://asana.test/api/1.0/tasks/1204567890123456?opt_fields=name%2Cresource_subtype%2Cnotes%2Ccompleted%2Ccompleted_at%2Ccreated_at%2Cmodified_at%2Cdue_on%2Cdue_at%2Cstart_on%2Cstart_at%2Cassignee.name%2Cassignee.email%2Cparent.name%2Cprojects.name%2Cmemberships.project.name%2Cmemberships.section.name%2Ctags.name%2Cfollowers.name%2Cworkspace.name%2Cnum_subtasks%2Cpermalink_url"
        );

        // path_segment rejects traversal, emails go through the encodeURIComponent helper.
        assert!(a.build_request("ASANA_GET_TASK", &json!({"task": "../x"})).is_err());
        let r = a
            .build_request("ASANA_GET_USER", &json!({"user": "ada@acme.com"}))
            .unwrap();
        assert!(r.url.contains("/users/ada%40acme.com?"), "{}", r.url);

        // List actions always carry the limit default.
        let r = a
            .build_request("ASANA_LIST_TAGS", &json!({"workspace": "1201234567890123"}))
            .unwrap();
        assert!(r.url.contains("limit=50"), "{}", r.url);
        assert!(r.url.contains("workspace=1201234567890123"), "{}", r.url);
        assert!(a.build_request("ASANA_LIST_USERS", &json!({})).is_err());

        // Update needs at least one field and rejects the mutually exclusive pairs.
        assert!(a.build_request("ASANA_UPDATE_TASK", &json!({"task": "1"})).is_err());
        assert!(a
            .build_request(
                "ASANA_UPDATE_TASK",
                &json!({"task": "1", "notes": "a", "html_notes": "b"})
            )
            .is_err());
        assert!(a
            .build_request(
                "ASANA_UPDATE_TASK",
                &json!({"task": "1", "completed": true})
            )
            .is_ok());

        // Write bodies use the Asana `{"data": …}` envelope.
        let r = a
            .build_request(
                "ASANA_CREATE_TASK",
                &json!({"project": "12", "name": "Ship", "notes": "N", "tag_gids": ["7"]}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        let b: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["data"]["projects"], json!(["12"]));
        assert_eq!(b["data"]["tags"], json!(["7"]));
    }

    #[test]
    fn search_and_error_mapping() {
        let a = Asana::with_api_base("https://asana.test/api/1.0");
        let r = a
            .build_request(
                "ASANA_SEARCH_TASKS",
                &json!({"workspace": "12", "text": "login timeout", "assignee_ids": ["me", "9"],
                        "completed": true, "due_on_after": "2026-10-01", "limit": 25}),
            )
            .unwrap();
        assert!(r.url.contains("/workspaces/12/tasks/search?"), "{}", r.url);
        assert!(r.url.contains("text=login+timeout"), "{}", r.url);
        assert!(r.url.contains("assignee.any=me%2C9"), "{}", r.url);
        assert!(r.url.contains("completed=true"), "{}", r.url);
        assert!(r.url.contains("due_on.after=2026-10-01"), "{}", r.url);
        assert!(r.url.contains("limit=25"), "{}", r.url);

        assert!(a
            .build_request("ASANA_SEARCH_TASKS", &json!({"workspace": "12", "due_on_after": "01.10.2026"}))
            .is_err());
        assert!(a
            .build_request("ASANA_SEARCH_TASKS", &json!({"workspace": "12", "created_at_after": "2026-10-01"}))
            .is_err());
        assert!(a
            .build_request(
                "ASANA_SEARCH_WORKSPACE_OBJECTS",
                &json!({"workspace": "12", "resource_type": "nope"})
            )
            .is_err());

        let resp = HttpResponse {
            status: 404,
            body: serde_json::to_vec(&json!({"errors": [{"message": "Not Found"}]})).unwrap(),
            session_id: None,
        };
        let err = a.parse_response("ASANA_GET_TASK", &resp).unwrap_err();
        assert!(err.to_string().contains("Not Found"), "{err}");
    }
}