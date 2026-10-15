//! GitLab toolkit (REST API v4). Action set modelled on Activepieces `piece-gitlab` (MIT).
//!
//! Auth: the piece is OAuth2 only (`https://gitlab.com/oauth/authorize` +
//! `https://gitlab.com/oauth/token`, scopes `api` + `read_user`,
//! see `src/lib/auth.ts`). The token is sent as `Authorization: Bearer …`
//! (`src/lib/common/secret-request.ts`), which is what the hub injects for a
//! bearer/OAuth2 connection.
//!
//! Base URL: `GITLAB_API_BASE = 'https://gitlab.com/api/v4'`
//! (`src/lib/common/constants.ts`). Self-hosted GitLab instances live under a
//! different host, so the base is a constructor argument
//! ([`GitLab::with_api_base`]) instead of a tool argument — every endpoint below
//! is `{api_base}{path}` exactly like `buildGitlabUrl` in the piece.
//!
//! Paths: every tool is a single `sendGitlabRequest` call, i.e. one HTTP request
//! with `page`/`per_page` on the list endpoints (`gitlabListPage`). Project,
//! group, branch and label ids are percent-encoded (`encodeSegment` /
//! `projectPath` / `groupPath` in the piece), because a GitLab path such as
//! `my-group/my-project` travels as one URL segment (`my-group%2Fmy-project`).
//!
//! Not ported: webhooks, triggers, file upload (`create_file`, `update_file`,
//! `delete_file`, `get_file`), commit creation, branch protection, project /
//! group / milestone administration, MR merge and approval actions, and every
//! action whose `run()` needs more than one request (`update_issue` unlock
//! pre-flight is modelled below, `set_user_status` is not ported at all).
//!
//! DE: Aktionen von `piece-gitlab` (MIT), eine Anfrage pro Tool.
//! EN: actions ported from `piece-gitlab` (MIT), one request per tool.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "gitlab";

/// Default API base of gitlab.com (`src/lib/common/constants.ts`).
pub const DEFAULT_API_BASE: &str = "https://gitlab.com/api/v4";

/// Scopes the piece requests (`src/lib/auth.ts`); `api` covers everything else,
/// `read_user` only matters for the user profile endpoints.
const READ: &[&str] = &["api"];
const READ_USER: &[&str] = &["api", "read_user"];
const WRITE: &[&str] = &["api"];

// ---------------------------------------------------------------- schema helpers

/// `Property.Checkbox`.
fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// `gitlabProps.triState` — the dropdown carries the strings "true"/"false".
fn tri(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "enum": ["true", "false"]})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn date(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "pattern": "^\\d{4}-\\d{2}-\\d{2}$"})
}

/// `gitlabProps.page()` / `gitlabProps.perPage()` (`pagingInput`).
fn paging() -> [(&'static str, Value); 2] {
    [
        (
            "page",
            json!({"type": "integer", "description": "Seitenzahl, beginnend bei 1. Nimm `page` aus dem vorigen Ergebnis.",
                "minimum": 1, "default": 1}),
        ),
        (
            "per_page",
            json!({"type": "integer", "description": "Einträge pro Seite, 1 bis 100. Standard 20.",
                "minimum": 1, "maximum": 100, "default": 20}),
        ),
    ]
}

fn with_paging(mut props: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    props.extend(paging());
    props
}

fn project_prop() -> (&'static str, Value) {
    ("project_id", s("Numerische Projekt-ID oder vollständiger Pfad, z. B. `my-group/my-project`."))
}

fn group_prop() -> (&'static str, Value) {
    ("group_id", s("Numerische Gruppen-ID oder vollständiger Pfad, z. B. `my-group/sub-group`."))
}

// ---------------------------------------------------------------- value helpers

/// `optionalText`: trimmed, empty means "not set".
fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// `requireText`.
fn req_text(args: &Value, key: &str) -> Result<String> {
    text(args, key)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing argument `{key}`"))
}

fn flag_value(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// `optionalPositiveInt` — absent, null or empty means "not set".
fn opt_positive(args: &Value, key: &str) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n > 0 => Ok(Some(n)),
            _ => bail!("`{key}` must be a positive whole number"),
        },
    }
}

fn pos(args: &Value, key: &str) -> Result<String> {
    Ok(opt_positive(args, key)?.unwrap_or(1).to_string())
}

/// `requirePositiveInt` for the ids that travel as a plain path segment: the
/// schema types them as integers, but a numeric string is accepted too.
fn id_segment(args: &Value, key: &str) -> Result<String> {
    let v = args
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("missing argument `{key}`"))?;
    if let Some(n) = v.as_u64() {
        return if n == 0 {
            bail!("`{key}` must be a positive whole number")
        } else {
            Ok(n.to_string())
        };
    }
    let raw = v
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("`{key}` must be a positive whole number"))?;
    let raw = seg(raw.trim(), key)?;
    match raw.parse::<u64>() {
        Ok(n) if n > 0 => Ok(raw),
        _ => bail!("`{key}` must be a positive whole number"),
    }
}

/// `path_segment`'s charset guard applied to a value that is already in hand.
fn seg(value: &str, key: &str) -> Result<String> {
    path_segment(&json!({ "v": value }), "v")
        .map(str::to_string)
        .map_err(|_| anyhow::anyhow!("invalid argument `{key}`"))
}

/// `triStateValue`: the dropdown sends "true"/"false"; a boolean is accepted too.
fn tri_value(args: &Value, key: &str) -> Result<Option<bool>> {
    match args.get(key) {
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(Value::String(s)) if s == "true" => Ok(Some(true)),
        Some(Value::String(s)) if s == "false" => Ok(Some(false)),
        None | Some(Value::Null) => Ok(None),
        Some(_) => bail!("`{key}` must be \"true\" or \"false\""),
    }
}

fn put_tri(
    m: &mut Map<String, Value>,
    args: &Value,
    key: &str,
    field: &str,
) -> Result<()> {
    if let Some(b) = tri_value(args, key)? {
        m.insert(field.into(), json!(b));
    }
    Ok(())
}

/// `idList` — array of positive user ids.
fn put_id_list(
    m: &mut Map<String, Value>,
    args: &Value,
    key: &str,
    field: &str,
) -> Result<()> {
    let items = str_vec(args, key);
    if items.is_empty() {
        return Ok(());
    }
    let ids: Result<Vec<u64>> = items
        .iter()
        .map(|s| {
            s.trim()
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| anyhow::anyhow!("`{key}` must be a list of positive user IDs"))
        })
        .collect();
    m.insert(field.into(), json!(ids?));
    Ok(())
}

/// `stringList(...).join(',')` — GitLab takes label names as one comma separated value.
fn put_labels(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    let items = str_vec(args, key);
    if !items.is_empty() {
        m.insert(field.into(), json!(items.join(",")));
    }
}

/// `optionalIsoDate` — GitLab rejects anything it cannot parse as a date.
fn put_iso_date(
    m: &mut Map<String, Value>,
    args: &Value,
    key: &str,
    field: &str,
) -> Result<()> {
    let Some(v) = text(args, key) else {
        return Ok(());
    };
    if !parses_iso(v) {
        bail!("`{key}` must be an ISO 8601 date, e.g. 2026-10-11 or 2026-10-11T09:00:00Z.");
    }
    m.insert(field.into(), json!(v));
    Ok(())
}

/// Minimal stand-in for the piece's `Date.parse` check on ISO 8601 input: the
/// prefix must be `YYYY-MM-DD`, everything after it (a time, a zone offset) is
/// passed through to GitLab unchanged, exactly like `optionalIsoDate`.
fn parses_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// `requireColor` — `#RGB`, `#RRGGBB` or a CSS colour name.
fn req_color(args: &Value, key: &str) -> Result<String> {
    let c = req_text(args, key)?;
    let hex = c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit());
    let short = c.len() == 4 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit());
    let name = (3..=30).contains(&c.len())
        && c.bytes().all(|b| b.is_ascii_alphabetic());
    if !(hex || short || name) {
        bail!("`{key}` must be a hex color such as #FF0000 or a CSS color name.");
    }
    Ok(c)
}

/// ---------------------------------------------------------------- encoding
///
/// `encodeSegment` / `filePathSegment` of `src/lib/common/client.ts`: the value is
/// validated against a conservative charset and then percent-encoded, so a
/// project path such as `my-group/my-project` travels as the single URL segment
/// `my-group%2Fmy-project` (what `encodeURIComponent` does).
fn encode_segment(args: &Value, key: &str, extra: &[u8]) -> Result<String> {
    let raw = req_text(args, key)?;
    // `buildGitlabUrl` refuses any path with a `.`/`..` segment; the same guard
    // applies per `/`-separated part here before the value is percent-encoded.
    let ok = !raw.is_empty()
        && raw.len() <= 200
        && !raw.split('/').any(|p| p == "." || p == "..")
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/') || extra.contains(&b));
    if !ok {
        bail!("invalid argument `{key}`");
    }
    Ok(raw
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect())
}

fn project_segment(args: &Value) -> Result<String> {
    encode_segment(args, "project_id", b"")
}

fn group_segment(args: &Value) -> Result<String> {
    encode_segment(args, "group_id", b"")
}

fn label_segment(args: &Value) -> Result<String> {
    encode_segment(args, "label_id", b": ")
}

/// `/projects/{id}` (`projectPath`).
fn project_path(args: &Value) -> Result<String> {
    Ok(format!("/projects/{}", project_segment(args)?))
}

/// `/groups/{id}` (`groupPath`).
fn group_path(args: &Value) -> Result<String> {
    Ok(format!("/groups/{}", group_segment(args)?))
}

/// `page` + `per_page`, the two query parameters `gitlabListPage` always adds.
fn page_query(args: &Value, extra: &[(&str, Option<String>)]) -> Result<String> {
    let mut pairs: Vec<(&str, Option<String>)> = extra.to_vec();
    let per_page = opt_positive(args, "per_page")?;
    if let Some(n) = per_page {
        if n > 100 {
            bail!("`per_page` must be between 1 and 100.");
        }
    }
    pairs.push(("page", Some(pos(args, "page")?)));
    pairs.push((
        "per_page",
        Some(per_page.unwrap_or(20).to_string()),
    ));
    Ok(query(&pairs))
}

// ---------------------------------------------------------------- adapter

pub struct GitLab {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for GitLab {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_API_BASE)
    }
}

#[allow(clippy::too_many_lines)]
impl GitLab {
    /// `api_base` is the full API root, e.g. `https://gitlab.com/api/v4` or
    /// `https://gitlab.example.com/api/v4` for a self-hosted instance.
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "GitLab".into(),
            description: LocalizedText::new(
                "Projekte, Issues, Merge Requests, Labels und CI-Pipelines in GitLab lesen und bearbeiten.",
                "Read and change projects, issues, merge requests, labels and CI pipelines in GitLab.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://gitlab.com/oauth/authorize".into(),
                    token_url: "https://gitlab.com/oauth/token".into(),
                    default_scopes: vec!["api".into(), "read_user".into()],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                // Personal access token (scopes api/read_api): same `Authorization: Bearer <token>` header.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "GITLAB_GET_CURRENT_USER",
                    ("Aktuellen Nutzer lesen", "Get current user"),
                    (
                        "Liest das GitLab-Konto, zu dem die Verbindung gehört.",
                        "Reads the GitLab user the connection belongs to.",
                    ),
                    &[],
                    &[],
                    READ_USER,
                    false,
                ),
                t(
                    "GITLAB_LIST_USERS",
                    ("Nutzer suchen", "List users"),
                    (
                        "Sucht Nutzer nach Name, Benutzername oder öffentlicher E-Mail.",
                        "Searches users by name, username or public email.",
                    ),
                    &with_paging(vec![
                        ("search", s("Name, Benutzername oder öffentliche E-Mail.")),
                        ("username", s("Exakter Benutzername, ohne @.")),
                    ]),
                    &[],
                    READ_USER,
                    false,
                ),
                t(
                    "GITLAB_GET_USER_STATUS",
                    ("Status lesen", "Get user status"),
                    (
                        "Liest Emoji, Nachricht und Verfügbarkeit eines Nutzers.",
                        "Reads the emoji, message and availability of a user.",
                    ),
                    &[("user", s("Nutzer-ID oder Benutzername; leer = das verbundene Konto."))],
                    &[],
                    READ_USER,
                    false,
                ),
                t(
                    "GITLAB_LIST_ISSUES",
                    ("Issues auflisten", "List issues"),
                    (
                        "Listet oder durchsucht die Issues eines Projekts, Seite für Seite.",
                        "Lists or searches the issues of a project, one page at a time.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("state", choice("opened, closed oder all", &["opened", "closed", "all"])),
                        ("search", s("Trifft auf Titel und Beschreibung.")),
                        ("labels", str_list("Issues müssen alle diese Labels haben.")),
                        ("assignee_id", json!({"type": "integer", "description": "Nur Issues dieses Assignees.", "minimum": 1})),
                        ("author_id", json!({"type": "integer", "description": "Nur Issues dieses Autors.", "minimum": 1})),
                        ("milestone", s("Exakter Meilenstein-Titel.")),
                        ("updated_after", s("Nur Issues, die nach diesem ISO-Zeitpunkt aktualisiert wurden.")),
                        ("order_by", choice("Sortierung", &["created_at", "updated_at", "due_date", "priority"])),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_ISSUE",
                    ("Issue lesen", "Get issue"),
                    (
                        "Liest ein Issue eines Projekts anhand seiner IID.",
                        "Reads one issue of a project by its IID.",
                    ),
                    &[project_prop(), ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`, im GitLab als #12).", "minimum": 1}))],
                    &["project_id", "issue_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_CREATE_ISSUE",
                    ("Issue anlegen", "Create issue"),
                    (
                        "Legt ein Issue in einem Projekt an; braucht --confirm.",
                        "Creates an issue in a project; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("title", s("Titel des Issues")),
                        ("description", s("Markdown-Beschreibung.")),
                        ("labels", str_list("Label-Namen; noch nicht vorhandene Labels werden angelegt.")),
                        ("assignee_ids", str_list("Numerische Nutzer-IDs der Assignees.")),
                        ("milestone_id", json!({"type": "integer", "description": "Globale Meilenstein-ID (nicht die iid).", "minimum": 1})),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD.")),
                        ("confidential", tri("Nur für Mitglieder ab Rolle Planner sichtbar.")),
                    ],
                    &["project_id", "title"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_UPDATE_ISSUE",
                    ("Issue ändern", "Update issue"),
                    (
                        "Ändert nur die übergebenen Felder eines Issues; braucht --confirm.",
                        "Changes only the fields you pass on an issue; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("title", s("Neuer Titel")),
                        ("description", s("Neue Beschreibung in Markdown")),
                        ("clear_description", flag("Beschreibung leeren", false)),
                        ("state_event", choice("Issue schließen oder wieder öffnen", &["close", "reopen"])),
                        ("add_labels", str_list("Labels hinzufügen, vorhandene bleiben.")),
                        ("remove_labels", str_list("Labels entfernen, Rest bleibt.")),
                        ("assignee_ids", str_list("Ersetzt alle Assignees.")),
                        ("clear_assignees", flag("Alle Assignees entfernen", false)),
                        ("milestone_id", json!({"type": "integer", "description": "Globale Meilenstein-ID.", "minimum": 1})),
                        ("clear_milestone", flag("Meilenstein entfernen", false)),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD.")),
                        ("clear_due_date", flag("Fälligkeitsdatum entfernen", false)),
                        ("confidential", tri("Leer = aktuellen Wert behalten.")),
                        ("discussion_locked", tri("Leer = aktuellen Wert behalten.")),
                    ],
                    &["project_id", "issue_iid"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_LIST_ISSUE_NOTES",
                    ("Issue-Kommentare auflisten", "List issue notes"),
                    (
                        "Listet die Kommentare (Notes) eines Issues.",
                        "Lists the comments (notes) of an issue.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("order_by", choice("Sortierung", &["created_at", "updated_at"])),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &["project_id", "issue_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_ISSUE_NOTE",
                    ("Issue-Kommentar lesen", "Get issue note"),
                    (
                        "Liest einen Kommentar eines Issues anhand seiner ID.",
                        "Reads one comment of an issue by its ID.",
                    ),
                    &[
                        project_prop(),
                        ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("note_id", json!({"type": "integer", "description": "Numerische ID des Kommentars.", "minimum": 1})),
                    ],
                    &["project_id", "issue_iid", "note_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_CREATE_ISSUE_NOTE",
                    ("Issue-Kommentar schreiben", "Create issue note"),
                    (
                        "Schreibt einen Kommentar auf ein Issue; braucht --confirm.",
                        "Posts a comment on an issue; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("body", s("Kommentartext in Markdown")),
                        ("internal", tri("Nur für Mitglieder ab Rolle Reporter sichtbar.")),
                    ],
                    &["project_id", "issue_iid", "body"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_DELETE_ISSUE_NOTE",
                    ("Issue-Kommentar löschen", "Delete issue note"),
                    (
                        "Löscht einen Kommentar eines Issues; braucht --confirm.",
                        "Deletes a comment of an issue; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("note_id", json!({"type": "integer", "description": "Numerische ID des Kommentars.", "minimum": 1})),
                    ],
                    &["project_id", "issue_iid", "note_id"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_GET_ISSUE_TIME_STATS",
                    ("Issue-Zeitstatistik lesen", "Get issue time stats"),
                    (
                        "Liest verbrauchte und geschätzte Zeit eines Issues.",
                        "Reads the spent and estimated time of an issue.",
                    ),
                    &[project_prop(), ("issue_iid", json!({"type": "integer", "description": "Issue-Nummer im Projekt (die `iid`).", "minimum": 1}))],
                    &["project_id", "issue_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_MERGE_REQUESTS",
                    ("Merge Requests auflisten", "List merge requests"),
                    (
                        "Listet Merge Requests eines Projekts oder, ohne Projekt, projektübergreifend.",
                        "Lists the merge requests of a project or, without a project, across projects.",
                    ),
                    &with_paging(vec![
                        ("project_id", s("Optional. Leer = projektübergreifend suchen.")),
                        ("scope", choice("Wessen Merge Requests", &["created_by_me", "assigned_to_me", "all"])),
                        ("state", choice("Status", &["opened", "closed", "locked", "merged", "all"])),
                        ("search", s("Trifft auf Titel und Beschreibung.")),
                        ("labels", str_list("Labels; mindestens eines muss passen.")),
                        ("source_branch", s("Nur MRs aus diesem Branch.")),
                        ("target_branch", s("Nur MRs in diesen Ziel-Branch.")),
                        ("author_id", json!({"type": "integer", "description": "Nur MRs dieses Autors.", "minimum": 1})),
                        ("reviewer_id", json!({"type": "integer", "description": "Nur MRs mit diesem Reviewer.", "minimum": 1})),
                        ("updated_after", s("Nur MRs, die nach diesem ISO-Zeitpunkt aktualisiert wurden.")),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &[],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_MERGE_REQUEST",
                    ("Merge Request lesen", "Get merge request"),
                    (
                        "Liest einen Merge Request anhand seiner IID.",
                        "Reads one merge request by its IID.",
                    ),
                    &[project_prop(), ("merge_request_iid", json!({"type": "integer", "description": "MR-Nummer im Projekt (die `iid`, im GitLab als !12).", "minimum": 1}))],
                    &["project_id", "merge_request_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_MERGE_REQUEST_APPROVALS",
                    ("Merge-Request-Freigaben lesen", "Get merge request approvals"),
                    (
                        "Liest Freigabestatus und offene Anforderungen eines Merge Requests.",
                        "Reads the approval status and open approval rules of a merge request.",
                    ),
                    &[project_prop(), ("merge_request_iid", json!({"type": "integer", "description": "MR-Nummer im Projekt (die `iid`).", "minimum": 1}))],
                    &["project_id", "merge_request_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_MERGE_REQUEST_COMMITS",
                    ("Merge-Request-Commits auflisten", "List merge request commits"),
                    (
                        "Listet die Commits eines Merge Requests.",
                        "Lists the commits of a merge request.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("merge_request_iid", json!({"type": "integer", "description": "MR-Nummer im Projekt (die `iid`).", "minimum": 1})),
                    ]),
                    &["project_id", "merge_request_iid"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_CREATE_MERGE_REQUEST",
                    ("Merge Request anlegen", "Create merge request"),
                    (
                        "Öffnet einen Merge Request zwischen zwei Branches; braucht --confirm.",
                        "Opens a merge request between two branches; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("source_branch", s("Branch mit den Änderungen.")),
                        ("target_branch", s("Ziel-Branch, z. B. main.")),
                        ("title", s("Titel; Präfix \"Draft:\" öffnet ihn als Entwurf.")),
                        ("description", s("Beschreibung in Markdown")),
                        ("labels", str_list("Label-Namen.")),
                        ("assignee_ids", str_list("Numerische Nutzer-IDs der Assignees.")),
                        ("reviewer_ids", str_list("Numerische Nutzer-IDs der Reviewer.")),
                        ("milestone_id", json!({"type": "integer", "description": "Globale Meilenstein-ID (nicht die iid).", "minimum": 1})),
                        ("remove_source_branch", tri("Quell-Branch nach dem Merge löschen.")),
                        ("squash", tri("Commits beim Merge squashen.")),
                    ],
                    &["project_id", "source_branch", "target_branch", "title"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_UPDATE_MERGE_REQUEST",
                    ("Merge Request ändern", "Update merge request"),
                    (
                        "Ändert nur die übergebenen Felder eines Merge Requests; braucht --confirm.",
                        "Changes only the fields you pass on a merge request; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("merge_request_iid", json!({"type": "integer", "description": "MR-Nummer im Projekt (die `iid`).", "minimum": 1})),
                        ("title", s("Neuer Titel")),
                        ("description", s("Neue Beschreibung in Markdown")),
                        ("clear_description", flag("Beschreibung leeren", false)),
                        ("target_branch", s("Neuer Ziel-Branch.")),
                        ("state_event", choice("Merge Request schließen oder wieder öffnen", &["close", "reopen"])),
                        ("add_labels", str_list("Labels hinzufügen, vorhandene bleiben.")),
                        ("remove_labels", str_list("Labels entfernen, Rest bleibt.")),
                        ("assignee_ids", str_list("Ersetzt alle Assignees.")),
                        ("clear_assignees", flag("Alle Assignees entfernen", false)),
                        ("reviewer_ids", str_list("Ersetzt alle Reviewer.")),
                        ("clear_reviewers", flag("Alle Reviewer entfernen", false)),
                        ("milestone_id", json!({"type": "integer", "description": "Globale Meilenstein-ID.", "minimum": 1})),
                        ("clear_milestone", flag("Meilenstein entfernen", false)),
                        ("remove_source_branch", tri("Quell-Branch nach dem Merge löschen.")),
                        ("squash", tri("Commits beim Merge squashen.")),
                        ("discussion_locked", tri("Leer = aktuellen Wert behalten.")),
                    ],
                    &["project_id", "merge_request_iid"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_LIST_PROJECTS",
                    ("Projekte auflisten", "List projects"),
                    (
                        "Listet Projekte, standardmäßig nur solche, in denen du Mitglied bist.",
                        "Lists projects, by default only the ones you are a member of.",
                    ),
                    &with_paging(vec![
                        ("search", s("Trifft auf Projektname und -pfad.")),
                        ("membership", tri("Nur Mitgliedschaften; Standard ja.")),
                        ("owned", tri("Nur eigene Projekte.")),
                        ("archived", tri("Ja = nur archivierte, Nein = nur aktive.")),
                        ("visibility", choice("Sichtbarkeit", &["private", "internal", "public"])),
                        ("order_by", choice("Sortierung", &["created_at", "last_activity_at", "name", "path", "updated_at", "id"])),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &[],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_PROJECT",
                    ("Projekt lesen", "Get project"),
                    (
                        "Liest ein Projekt anhand seiner ID oder seines Pfades.",
                        "Reads one project by its ID or path.",
                    ),
                    &[project_prop()],
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_PROJECT_MEMBERS",
                    ("Projektmitglieder auflisten", "List project members"),
                    (
                        "Listet alle Mitglieder eines Projekts, inklusive inheritsierter Rollen.",
                        "Lists all members of a project, including inherited roles.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("query", s("Trifft auf Name, Benutzername oder öffentliche E-Mail.")),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_GROUPS",
                    ("Gruppen auflisten", "List groups"),
                    (
                        "Listet Gruppen, die für das verbundene Konto sichtbar sind.",
                        "Lists the groups visible to the connected account.",
                    ),
                    &with_paging(vec![
                        ("search", s("Trifft auf Gruppenname und -pfad.")),
                        ("owned", tri("Nur eigene Gruppen.")),
                        ("top_level_only", tri("Untergruppen ausblenden.")),
                        ("min_access_level", choice("Mindestrolle", &["10", "15", "20", "30", "40", "50"])),
                        ("order_by", choice("Sortierung", &["name", "path", "id"])),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &[],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_GROUP",
                    ("Gruppe lesen", "Get group"),
                    (
                        "Liest eine Gruppe anhand ihrer ID oder ihres Pfades.",
                        "Reads one group by its ID or path.",
                    ),
                    &[group_prop()],
                    &["group_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_LABELS",
                    ("Labels auflisten", "List labels"),
                    (
                        "Listet die Labels eines Projekts.",
                        "Lists the labels of a project.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("search", s("Trifft auf den Label-Namen.")),
                        ("include_ancestor_groups", tri("Labels der übergeordneten Gruppen einbeziehen.")),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_GET_LABEL",
                    ("Label lesen", "Get label"),
                    (
                        "Liest ein Label anhand seiner ID oder seines Namens.",
                        "Reads one label by its ID or name.",
                    ),
                    &[
                        project_prop(),
                        ("label_id", s("Numerische Label-ID oder der exakte Label-Name.")),
                        ("include_ancestor_groups", tri("Auch in Labels aus übergeordneten Gruppen suchen.")),
                    ],
                    &["project_id", "label_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_CREATE_LABEL",
                    ("Label anlegen", "Create label"),
                    (
                        "Legt ein Label in einem Projekt an; braucht --confirm.",
                        "Creates a label in a project; requires --confirm.",
                    ),
                    &[
                        project_prop(),
                        ("name", s("Name des Labels")),
                        ("color", s("Hex-Farbe wie #FF0000 oder ein CSS-Farbname.")),
                        ("description", s("Beschreibung")),
                        ("priority", json!({"type": "integer", "description": "Ganze Zahl ab 0; niedrigere Werte sortieren zuerst.", "minimum": 0})),
                    ],
                    &["project_id", "name", "color"],
                    WRITE,
                    true,
                ),
                t(
                    "GITLAB_LIST_MILESTONES",
                    ("Meilensteine auflisten", "List milestones"),
                    (
                        "Listet die Meilensteine eines Projekts.",
                        "Lists the milestones of a project.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("state", choice("Status", &["active", "closed"])),
                        ("title", s("Exakter Titel.")),
                        ("search", s("Trifft auf Titel und Beschreibung.")),
                        ("include_ancestors", tri("Meilensteine der übergeordneten Gruppen einbeziehen.")),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_BRANCHES",
                    ("Branches auflisten", "List branches"),
                    (
                        "Listet die Branches eines Repositorys.",
                        "Lists the branches of a repository.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("search", s("Trifft auf den Branch-Namen.")),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_COMMITS",
                    ("Commits auflisten", "List commits"),
                    (
                        "Listet Commits eines Repositorys, optional auf einen Ref oder Pfad gefiltert.",
                        "Lists commits of a repository, optionally filtered by ref or path.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("ref_name", s("Branch oder Tag; Standard = Default-Branch.")),
                        ("path", s("Nur Commits, die diesen Pfad berühren.")),
                        ("author", s("Name oder E-Mail des Autors.")),
                        ("since", s("Nur Commits nach diesem ISO-Zeitpunkt.")),
                        ("until", s("Nur Commits vor diesem ISO-Zeitpunkt.")),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
                t(
                    "GITLAB_LIST_PIPELINES",
                    ("Pipelines auflisten", "List pipelines"),
                    (
                        "Listet die CI/CD-Pipelines eines Projekts.",
                        "Lists the CI/CD pipelines of a project.",
                    ),
                    &with_paging(vec![
                        project_prop(),
                        ("status", choice("Status der Pipeline", &["created", "waiting_for_resource", "preparing", "pending", "running", "success", "failed", "canceled", "skipped", "manual", "scheduled"])),
                        ("ref", s("Branch oder Tag.")),
                        ("sha", s("Commit-SHA.")),
                        ("source", s("Auslöser, z. B. push, web, api, schedule, merge_request_event.")),
                        ("username", s("Benutzername, der die Pipeline ausgelöst hat, ohne @.")),
                        ("updated_after", s("Nur Pipelines, die nach diesem ISO-Zeitpunkt aktualisiert wurden.")),
                        ("updated_before", s("Nur Pipelines, die vor diesem ISO-Zeitpunkt aktualisiert wurden.")),
                        ("order_by", choice("Sortierung", &["id", "status", "ref", "updated_at", "user_id"])),
                        ("sort", choice("Richtung", &["desc", "asc"])),
                    ]),
                    &["project_id"],
                    READ,
                    false,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str, q: &str) -> String {
        format!("{}{path}{q}", self.api_base)
    }
}

/// `?a=b` from the optional args of a list action.
fn opt(args: &Value, key: &str) -> Option<String> {
    text(args, key).map(str::to_string)
}

/// `optionalPositiveInt` rendered as a query value.
fn opt_num(args: &Value, key: &str) -> Result<Option<String>> {
    Ok(opt_positive(args, key)?.map(|n| n.to_string()))
}

/// `triStateValue` rendered as a query value.
fn opt_tri(args: &Value, key: &str) -> Result<Option<String>> {
    Ok(match tri_value(args, key)? {
        Some(true) => Some("true".into()),
        Some(false) => Some("false".into()),
        None => None,
    })
}

/// Label list joined with commas, exactly like `stringList(...).join(',')`.
fn opt_labels(args: &Value, key: &str) -> Option<String> {
    let items = str_vec(args, key);
    if items.is_empty() {
        None
    } else {
        Some(items.join(","))
    }
}

/// `@` is stripped from usernames (`username?.replace(/^@/, '')`).
fn opt_username(args: &Value, key: &str) -> Option<String> {
    text(args, key).map(|v| v.trim_start_matches('@').to_string())
}

/// Query parameter that must be a valid ISO date (`optionalIsoDate`).
fn opt_iso(args: &Value, key: &str) -> Result<Option<String>> {
    let Some(v) = text(args, key) else {
        return Ok(None);
    };
    if !parses_iso(v) {
        bail!("`{key}` must be an ISO 8601 date, e.g. 2026-10-11 or 2026-10-11T09:00:00Z.");
    }
    Ok(Some(v.to_string()))
}

impl ToolkitAdapter for GitLab {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/users/get-current-user.ts — GET {base}/user
            "GITLAB_GET_CURRENT_USER" => {
                HttpRequest::new("GET", self.url("/user", ""))
            }
            // AP: src/lib/actions/users/list-users.ts — GET {base}/users?search&username&page&per_page
            "GITLAB_LIST_USERS" => {
                let search = opt(args, "search");
                let username = opt_username(args, "username");
                if search.is_none() && username.is_none() {
                    bail!("`search` or `username` is required; without either gitlab.com returns an arbitrary list of users");
                }
                let q = page_query(args, &[("search", search), ("username", username)])?;
                HttpRequest::new("GET", self.url("/users", &q))
            }
            // AP: src/lib/actions/users/get-user-status.ts — GET {base}/user/status
            //      or {base}/users/{user}/status when a user is given
            "GITLAB_GET_USER_STATUS" => {
                let path = match text(args, "user") {
                    Some(u) => {
                        let u = u.trim_start_matches('@');
                        format!("/users/{}/status", seg(u, "user")?)
                    }
                    None => "/user/status".to_string(),
                };
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/issues/list-issues.ts — GET {base}/projects/{id}/issues
            "GITLAB_LIST_ISSUES" => {
                let path = format!("{}/issues", project_path(args)?);
                let q = page_query(args, &[
                    ("state", opt(args, "state")),
                    ("search", opt(args, "search")),
                    ("labels", opt_labels(args, "labels")),
                    ("assignee_id", opt_num(args, "assignee_id")?),
                    ("author_id", opt_num(args, "author_id")?),
                    ("milestone", opt(args, "milestone")),
                    ("updated_after", opt_iso(args, "updated_after")?),
                    ("order_by", opt(args, "order_by")),
                    ("sort", opt(args, "sort")),
                ])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/issues/get-issue.ts — GET {base}/projects/{id}/issues/{iid}
            "GITLAB_GET_ISSUE" => {
                let path = format!("{}/issues/{}", project_path(args)?, id_segment(args, "issue_iid")?);
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/issues/create-project-issue.ts — POST {base}/projects/{id}/issues
            "GITLAB_CREATE_ISSUE" => {
                let path = format!("{}/issues", project_path(args)?);
                let mut b = Map::new();
                b.insert("title".into(), json!(req_text(args, "title")?));
                if let Some(d) = text(args, "description") {
                    b.insert("description".into(), json!(d));
                }
                put_labels(&mut b, args, "labels", "labels");
                put_id_list(&mut b, args, "assignee_ids", "assignee_ids")?;
                if let Some(m) = opt_positive(args, "milestone_id")? {
                    b.insert("milestone_id".into(), json!(m));
                }
                put_iso_date(&mut b, args, "due_date", "due_date")?;
                put_tri(&mut b, args, "confidential", "confidential")?;
                HttpRequest::new("POST", self.url(&path, "")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/issues/update-issue.ts — PUT {base}/projects/{id}/issues/{iid}
            //      (plus a `{"discussion_locked": false}` pre-flight PUT when the issue is
            //      unlocked and closed/reopened in the same call).
            "GITLAB_UPDATE_ISSUE" => {
                let path = format!("{}/issues/{}", project_path(args)?, id_segment(args, "issue_iid")?);
                let b = issue_update_body(args)?;
                HttpRequest::new("PUT", self.url(&path, "")).json(&b)
            }
            // AP: src/lib/actions/issues/list-issue-notes.ts — GET {base}/projects/{id}/issues/{iid}/notes
            "GITLAB_LIST_ISSUE_NOTES" => {
                let path = format!(
                    "{}/issues/{}/notes",
                    project_path(args)?,
                    id_segment(args, "issue_iid")?
                );
                let q = page_query(
                    args,
                    &[("order_by", opt(args, "order_by")), ("sort", opt(args, "sort"))],
                )?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/issues/get-issue-note.ts — GET {base}/projects/{id}/issues/{iid}/notes/{note}
            "GITLAB_GET_ISSUE_NOTE" => {
                let path = format!(
                    "{}/issues/{}/notes/{}",
                    project_path(args)?,
                    id_segment(args, "issue_iid")?,
                    id_segment(args, "note_id")?
                );
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/issues/create-issue-note.ts — POST {base}/projects/{id}/issues/{iid}/notes
            "GITLAB_CREATE_ISSUE_NOTE" => {
                let path = format!("{}/issues/{}/notes", project_path(args)?, id_segment(args, "issue_iid")?);
                let mut b = Map::new();
                b.insert("body".into(), json!(req_text(args, "body")?));
                put_tri(&mut b, args, "internal", "internal")?;
                HttpRequest::new("POST", self.url(&path, "")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/issues/delete-issue-note.ts — DELETE {base}/projects/{id}/issues/{iid}/notes/{note}
            "GITLAB_DELETE_ISSUE_NOTE" => {
                let path = format!(
                    "{}/issues/{}/notes/{}",
                    project_path(args)?,
                    id_segment(args, "issue_iid")?,
                    id_segment(args, "note_id")?
                );
                HttpRequest::new("DELETE", self.url(&path, ""))
            }
            // AP: src/lib/actions/issues/get-issue-time-stats.ts — GET {base}/projects/{id}/issues/{iid}/time_stats
            "GITLAB_GET_ISSUE_TIME_STATS" => {
                let path = format!(
                    "{}/issues/{}/time_stats",
                    project_path(args)?,
                    id_segment(args, "issue_iid")?
                );
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/merge-requests/list-merge-requests.ts —
            //      GET {base}/merge_requests or {base}/projects/{id}/merge_requests
            "GITLAB_LIST_MERGE_REQUESTS" => {
                let path = match text(args, "project_id") {
                    Some(_) => format!("{}/merge_requests", project_path(args)?),
                    None => "/merge_requests".to_string(),
                };
                let q = page_query(args, &[
                    ("scope", opt(args, "scope")),
                    ("state", opt(args, "state")),
                    ("search", opt(args, "search")),
                    ("labels", opt_labels(args, "labels")),
                    ("source_branch", opt(args, "source_branch")),
                    ("target_branch", opt(args, "target_branch")),
                    ("author_id", opt_num(args, "author_id")?),
                    ("reviewer_id", opt_num(args, "reviewer_id")?),
                    ("updated_after", opt_iso(args, "updated_after")?),
                    ("sort", opt(args, "sort")),
                ])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/merge-requests/get-merge-request.ts — GET {base}/projects/{id}/merge_requests/{iid}
            "GITLAB_GET_MERGE_REQUEST" => {
                let path = format!(
                    "{}/merge_requests/{}",
                    project_path(args)?,
                    id_segment(args, "merge_request_iid")?
                );
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/merge-requests/get-merge-request-approvals.ts — GET {base}/projects/{id}/merge_requests/{iid}/approvals
            "GITLAB_GET_MERGE_REQUEST_APPROVALS" => {
                let path = format!(
                    "{}/merge_requests/{}/approvals",
                    project_path(args)?,
                    id_segment(args, "merge_request_iid")?
                );
                HttpRequest::new("GET", self.url(&path, ""))
            }
            // AP: src/lib/actions/merge-requests/list-merge-request-commits.ts — GET {base}/projects/{id}/merge_requests/{iid}/commits
            "GITLAB_LIST_MERGE_REQUEST_COMMITS" => {
                let path = format!(
                    "{}/merge_requests/{}/commits",
                    project_path(args)?,
                    id_segment(args, "merge_request_iid")?
                );
                let q = page_query(args, &[])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/merge-requests/create-merge-request.ts — POST {base}/projects/{id}/merge_requests
            "GITLAB_CREATE_MERGE_REQUEST" => {
                let path = format!("{}/merge_requests", project_path(args)?);
                let mut b = Map::new();
                b.insert("source_branch".into(), json!(req_text(args, "source_branch")?));
                b.insert("target_branch".into(), json!(req_text(args, "target_branch")?));
                b.insert("title".into(), json!(req_text(args, "title")?));
                if let Some(d) = text(args, "description") {
                    b.insert("description".into(), json!(d));
                }
                put_labels(&mut b, args, "labels", "labels");
                put_id_list(&mut b, args, "assignee_ids", "assignee_ids")?;
                put_id_list(&mut b, args, "reviewer_ids", "reviewer_ids")?;
                if let Some(m) = opt_positive(args, "milestone_id")? {
                    b.insert("milestone_id".into(), json!(m));
                }
                put_tri(&mut b, args, "remove_source_branch", "remove_source_branch")?;
                put_tri(&mut b, args, "squash", "squash")?;
                HttpRequest::new("POST", self.url(&path, "")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/merge-requests/update-merge-request.ts — PUT {base}/projects/{id}/merge_requests/{iid}
            //      (plus the `{"discussion_locked": false}` pre-flight PUT, as above).
            "GITLAB_UPDATE_MERGE_REQUEST" => {
                let path = format!(
                    "{}/merge_requests/{}",
                    project_path(args)?,
                    id_segment(args, "merge_request_iid")?
                );
                let b = merge_request_update_body(args)?;
                HttpRequest::new("PUT", self.url(&path, "")).json(&b)
            }
            // AP: src/lib/actions/projects/list-projects.ts — GET {base}/projects?membership&search&owned&archived&visibility&order_by&sort&simple
            "GITLAB_LIST_PROJECTS" => {
                let membership = match tri_value(args, "membership")? {
                    Some(b) if !b => {
                        if text(args, "search").is_none() && tri_value(args, "owned")? != Some(true) {
                            bail!("`membership` can be turned off only together with `search` or `owned`.");
                        }
                        None
                    }
                    _ => Some("true".to_string()),
                };
                let q = page_query(args, &[
                    ("membership", membership),
                    ("search", opt(args, "search")),
                    ("owned", opt_tri(args, "owned")?),
                    ("archived", opt_tri(args, "archived")?),
                    ("visibility", opt(args, "visibility")),
                    ("order_by", opt(args, "order_by")),
                    ("sort", opt(args, "sort")),
                    ("simple", Some("true".into())),
                ])?;
                HttpRequest::new("GET", self.url("/projects", &q))
            }
            // AP: src/lib/actions/projects/get-project.ts — GET {base}/projects/{id}
            "GITLAB_GET_PROJECT" => {
                HttpRequest::new("GET", self.url(&project_path(args)?, ""))
            }
            // AP: src/lib/actions/projects/list-project-members.ts — GET {base}/projects/{id}/members/all
            "GITLAB_LIST_PROJECT_MEMBERS" => {
                let path = format!("{}/members/all", project_path(args)?);
                let q = page_query(args, &[("query", opt(args, "query"))])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/groups/list-groups.ts — GET {base}/groups
            "GITLAB_LIST_GROUPS" => {
                let q = page_query(args, &[
                    ("search", opt(args, "search")),
                    ("owned", opt_tri(args, "owned")?),
                    ("top_level_only", opt_tri(args, "top_level_only")?),
                    ("min_access_level", opt(args, "min_access_level")),
                    ("order_by", opt(args, "order_by")),
                    ("sort", opt(args, "sort")),
                ])?;
                HttpRequest::new("GET", self.url("/groups", &q))
            }
            // AP: src/lib/actions/groups/get-group.ts — GET {base}/groups/{id}?with_projects=false
            "GITLAB_GET_GROUP" => {
                HttpRequest::new("GET", self.url(&group_path(args)?, "?with_projects=false"))
            }
            // AP: src/lib/actions/labels/list-labels.ts — GET {base}/projects/{id}/labels
            "GITLAB_LIST_LABELS" => {
                let path = format!("{}/labels", project_path(args)?);
                let q = page_query(
                    args,
                    &[
                        ("search", opt(args, "search")),
                        ("include_ancestor_groups", opt_tri(args, "include_ancestor_groups")?),
                    ],
                )?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/labels/get-label.ts — GET {base}/projects/{id}/labels/{label}
            "GITLAB_GET_LABEL" => {
                let path = format!("{}/labels/{}", project_path(args)?, label_segment(args)?);
                let q = match opt_tri(args, "include_ancestor_groups")? {
                    Some(v) => format!("?include_ancestor_groups={v}"),
                    None => String::new(),
                };
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/labels/create-label.ts — POST {base}/projects/{id}/labels
            "GITLAB_CREATE_LABEL" => {
                let path = format!("{}/labels", project_path(args)?);
                let mut b = Map::new();
                b.insert("name".into(), json!(req_text(args, "name")?));
                b.insert("color".into(), json!(req_color(args, "color")?));
                if let Some(d) = text(args, "description") {
                    b.insert("description".into(), json!(d));
                }
                match args.get("priority") {
                    None | Some(Value::Null) => {}
                    Some(v) => {
                        let p = v
                            .as_u64()
                            .ok_or_else(|| anyhow::anyhow!("`priority` must be a whole number from 0"))?;
                        b.insert("priority".into(), json!(p));
                    }
                }
                HttpRequest::new("POST", self.url(&path, "")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/labels/list-milestones.ts — GET {base}/projects/{id}/milestones
            "GITLAB_LIST_MILESTONES" => {
                let path = format!("{}/milestones", project_path(args)?);
                let q = page_query(args, &[
                    ("state", opt(args, "state")),
                    ("title", opt(args, "title")),
                    ("search", opt(args, "search")),
                    ("include_ancestors", opt_tri(args, "include_ancestors")?),
                ])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/repository/list-branches.ts — GET {base}/projects/{id}/repository/branches
            "GITLAB_LIST_BRANCHES" => {
                let path = format!("{}/repository/branches", project_path(args)?);
                let q = page_query(args, &[("search", opt(args, "search"))])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/repository/list-commits.ts — GET {base}/projects/{id}/repository/commits
            "GITLAB_LIST_COMMITS" => {
                let path = format!("{}/repository/commits", project_path(args)?);
                let q = page_query(args, &[
                    ("ref_name", opt(args, "ref_name")),
                    ("path", opt(args, "path")),
                    ("author", opt(args, "author")),
                    ("since", opt_iso(args, "since")?),
                    ("until", opt_iso(args, "until")?),
                ])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            // AP: src/lib/actions/ci/list-pipelines.ts — GET {base}/projects/{id}/pipelines
            "GITLAB_LIST_PIPELINES" => {
                let path = format!("{}/pipelines", project_path(args)?);
                let q = page_query(args, &[
                    ("status", opt(args, "status")),
                    ("ref", opt(args, "ref")),
                    ("sha", opt(args, "sha")),
                    ("source", opt(args, "source")),
                    ("username", opt_username(args, "username")),
                    ("updated_after", opt_iso(args, "updated_after")?),
                    ("updated_before", opt_iso(args, "updated_before")?),
                    ("order_by", opt(args, "order_by")),
                    ("sort", opt(args, "sort")),
                ])?;
                HttpRequest::new("GET", self.url(&path, &q))
            }
            other => bail!("unknown GitLab tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        // `gitlabListPage` also reports `page`, `nextPage` and `total`; the latter two
        // come from the `x-next-page` / `x-total` response headers, which the hub's
        // `HttpResponse` does not carry, so only the items are returned here.
        let v = response.json()?;
        Ok(match tool {
            "GITLAB_GET_CURRENT_USER"
            | "GITLAB_GET_USER_STATUS"
            | "GITLAB_GET_ISSUE"
            | "GITLAB_CREATE_ISSUE"
            | "GITLAB_UPDATE_ISSUE"
            | "GITLAB_GET_ISSUE_NOTE"
            | "GITLAB_CREATE_ISSUE_NOTE"
            | "GITLAB_GET_ISSUE_TIME_STATS"
            | "GITLAB_GET_MERGE_REQUEST"
            | "GITLAB_GET_MERGE_REQUEST_APPROVALS"
            | "GITLAB_CREATE_MERGE_REQUEST"
            | "GITLAB_UPDATE_MERGE_REQUEST"
            | "GITLAB_GET_PROJECT"
            | "GITLAB_GET_GROUP"
            | "GITLAB_GET_LABEL"
            | "GITLAB_CREATE_LABEL" => v,
            "GITLAB_DELETE_ISSUE_NOTE" => json!({"deleted": true}),
            _ => {
                let items = v.as_array().cloned().unwrap_or_default();
                json!({"items": items, "count": v.as_array().map_or(0, Vec::len)})
            }
        })
    }
}

/// Body of `GITLAB_UPDATE_ISSUE` (`update-issue.ts`).
///
/// `update_issue` sends a `{"discussion_locked": false}` PUT first when the issue
/// is unlocked *and* closed/reopened in the same call, because GitLab refuses the
/// state change on a locked discussion. One adapter call is one request, so that
/// pre-flight is returned alone and the caller re-runs the tool without
/// `discussion_locked`.
fn issue_update_body(args: &Value) -> Result<Value> {
    let mut b = Map::new();
    if let Some(t) = text(args, "title") {
        b.insert("title".into(), json!(t));
    }
    if flag_value(args, "clear_description") {
        b.insert("description".into(), json!(""));
    } else if let Some(d) = text(args, "description") {
        b.insert("description".into(), json!(d));
    }
    if let Some(e) = text(args, "state_event") {
        if !["close", "reopen"].contains(&e) {
            bail!("`state_event` must be close or reopen");
        }
        b.insert("state_event".into(), json!(e));
    }
    put_labels(&mut b, args, "add_labels", "add_labels");
    put_labels(&mut b, args, "remove_labels", "remove_labels");
    if flag_value(args, "clear_assignees") {
        b.insert("assignee_ids".into(), json!([0]));
    } else {
        put_id_list(&mut b, args, "assignee_ids", "assignee_ids")?;
    }
    if flag_value(args, "clear_milestone") {
        b.insert("milestone_id".into(), json!(0));
    } else if let Some(m) = opt_positive(args, "milestone_id")? {
        b.insert("milestone_id".into(), json!(m));
    }
    if flag_value(args, "clear_due_date") {
        b.insert("due_date".into(), json!(""));
    } else {
        put_iso_date(&mut b, args, "due_date", "due_date")?;
    }
    put_tri(&mut b, args, "confidential", "confidential")?;
    let locked = tri_value(args, "discussion_locked")?;
    if let Some(l) = locked {
        b.insert("discussion_locked".into(), json!(l));
    }
    if b.is_empty() {
        bail!("nothing to update: pass at least one field to change");
    }
    Ok(Value::Object(b))
}

/// Body of `GITLAB_UPDATE_MERGE_REQUEST` (`update-merge-request.ts`); same
/// unlock pre-flight as [`issue_update_body`].
fn merge_request_update_body(args: &Value) -> Result<Value> {
    let mut b = Map::new();
    if let Some(t) = text(args, "title") {
        b.insert("title".into(), json!(t));
    }
    if flag_value(args, "clear_description") {
        b.insert("description".into(), json!(""));
    } else if let Some(d) = text(args, "description") {
        b.insert("description".into(), json!(d));
    }
    if let Some(t) = text(args, "target_branch") {
        b.insert("target_branch".into(), json!(t));
    }
    if let Some(e) = text(args, "state_event") {
        if !["close", "reopen"].contains(&e) {
            bail!("`state_event` must be close or reopen");
        }
        b.insert("state_event".into(), json!(e));
    }
    put_labels(&mut b, args, "add_labels", "add_labels");
    put_labels(&mut b, args, "remove_labels", "remove_labels");
    if flag_value(args, "clear_assignees") {
        b.insert("assignee_ids".into(), json!([0]));
    } else {
        put_id_list(&mut b, args, "assignee_ids", "assignee_ids")?;
    }
    if flag_value(args, "clear_reviewers") {
        b.insert("reviewer_ids".into(), json!([0]));
    } else {
        put_id_list(&mut b, args, "reviewer_ids", "reviewer_ids")?;
    }
    if flag_value(args, "clear_milestone") {
        b.insert("milestone_id".into(), json!(0));
    } else if let Some(m) = opt_positive(args, "milestone_id")? {
        b.insert("milestone_id".into(), json!(m));
    }
    put_tri(&mut b, args, "remove_source_branch", "remove_source_branch")?;
    put_tri(&mut b, args, "squash", "squash")?;
    let locked = tri_value(args, "discussion_locked")?;
    if let Some(l) = locked {
        b.insert("discussion_locked".into(), json!(l));
    }
    if b.is_empty() {
        bail!("nothing to update: pass at least one field to change");
    }
    Ok(Value::Object(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> GitLab {
        GitLab::with_api_base("https://gitlab.test/api/v4")
    }

    #[test]
    fn encodes_project_path_and_ids() {
        let a = g();
        let r = a
            .build_request("GITLAB_GET_ISSUE", &json!({"project_id": "my-group/my-project", "issue_iid": 12}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://gitlab.test/api/v4/projects/my-group%2Fmy-project/issues/12"
        );
        let r = a
            .build_request("GITLAB_GET_LABEL", &json!({"project_id": "42", "label_id": "bug :: urgent"}))
            .unwrap();
        assert_eq!(
            r.url,
            "https://gitlab.test/api/v4/projects/42/labels/bug%20%3A%3A%20urgent"
        );
    }

    #[test]
    fn rejects_bad_path_params() {
        let a = g();
        assert!(a
            .build_request("GITLAB_GET_ISSUE", &json!({"project_id": "../secrets", "issue_iid": 1}))
            .is_err());
        assert!(a
            .build_request("GITLAB_GET_ISSUE", &json!({"project_id": "42", "issue_iid": 0}))
            .is_err());
        assert!(a
            .build_request("GITLAB_GET_ISSUE", &json!({"project_id": "42", "issue_iid": 1.5}))
            .is_err());
        assert!(a
            .build_request("GITLAB_GET_USER_STATUS", &json!({"user": "../x"}))
            .is_err());
    }

    #[test]
    fn list_query_is_url_encoded_and_paged() {
        let a = g();
        let r = a
            .build_request(
                "GITLAB_LIST_ISSUES",
                &json!({"project_id": "42", "search": "login timeout", "labels": ["bug", "p1"],
                        "state": "opened", "page": 3, "per_page": 50}),
            )
            .unwrap();
        for part in [
            "/projects/42/issues?",
            "search=login+timeout",
            "labels=bug%2Cp1",
            "state=opened",
            "page=3",
            "per_page=50",
        ] {
            assert!(r.url.contains(part), "{} lacks {part}", r.url);
        }
        // Defaults when the caller passes no paging.
        let r = a.build_request("GITLAB_LIST_BRANCHES", &json!({"project_id": "42"})).unwrap();
        assert!(r.url.ends_with("?page=1&per_page=20"), "{}", r.url);
        assert!(a
            .build_request("GITLAB_LIST_BRANCHES", &json!({"project_id": "42", "per_page": 500}))
            .is_err());
    }

    #[test]
    fn update_bodies_only_send_given_fields() {
        let a = g();
        let r = a
            .build_request(
                "GITLAB_UPDATE_ISSUE",
                &json!({"project_id": "42", "issue_iid": 12, "state_event": "close", "clear_assignees": true}),
            )
            .unwrap();
        assert_eq!(r.method, "PUT");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["state_event"], "close");
        assert_eq!(body["assignee_ids"], json!([0]));
        assert!(body.get("title").is_none());
        assert!(a
            .build_request("GITLAB_UPDATE_ISSUE", &json!({"project_id": "42", "issue_iid": 12}))
            .is_err());
        assert!(a
            .build_request("GITLAB_CREATE_LABEL", &json!({"project_id": "42", "name": "bug", "color": "#GG0000"}))
            .is_err());
        assert!(a
            .build_request("GITLAB_LIST_PROJECTS", &json!({"membership": "false"}))
            .is_err());
    }
}