//! Jira Cloud toolkit (Jira REST API v3). Action set modelled on Activepieces
//! `jira-cloud` (MIT). Auth is Atlassian basic auth (email + API token) against
//! the instance URL; the hub injects the `Authorization` header.
use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "jira";
const READ_WORK: &str = "read:jira-work";
const READ_USER: &str = "read:jira-user";
const WRITE_WORK: &str = "write:jira-work";

/// Default instance URL; real accounts use `https://<site>.atlassian.net`.
const DEFAULT_INSTANCE: &str = "https://example.atlassian.net";

pub struct Jira {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Jira {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_INSTANCE)
    }
}

/// Plain text as Atlassian Document Format (the only rich-text form the REST v3 accepts).
fn adf_text(text: &str) -> Value {
    json!({
        "version": 1,
        "type": "doc",
        "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": text}]}
        ]
    })
}

/// Comment body: raw ADF JSON when `is_adf`, else plain text wrapped in ADF.
fn comment_body(args: &Value) -> Result<Value> {
    let text = req_str(args, "comment")?;
    if args.get("is_adf").and_then(Value::as_bool).unwrap_or(false) {
        serde_json::from_str(text).map_err(|e| anyhow::anyhow!("`comment` is not valid ADF JSON: {e}"))
    } else {
        Ok(adf_text(text))
    }
}

fn user(u: &Value) -> Value {
    json!({"account_id": u["accountId"], "display_name": u["displayName"],
        "email_address": u["emailAddress"], "active": u["active"], "time_zone": u["timeZone"]})
}

/// Standard `/search` page envelope renamed to snake_case.
fn page(v: &Value, items: Value) -> Value {
    json!({"items": items, "start_at": v["startAt"], "max_results": v["maxResults"],
        "total": v["total"], "is_last": v["isLast"]})
}

fn ok() -> Value {
    json!({"success": true})
}

impl Jira {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title: (&str, &str), desc: (&str, &str), props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let page_props: Props<'_> = &[
            ("start_at", int("Offset / start at", 0, 100_000, 0)),
            ("max_results", int("Max results per page", 1, 1000, 50)),
        ];
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Jira Cloud".into(),
            description: LocalizedText::new(
                "Jira-Cloud-Issues durchsuchen, lesen, anlegen, kommentieren und übergehen.",
                "Search, read, create, comment on and transition Jira Cloud issues.",
            ),
            auth_schemes: vec![AuthScheme::Basic],
            tools: vec![
                // --- read / search ---
                t(
                    "JIRA_GET_ISSUE",
                    ("Issue lesen", "Get issue"),
                    (
                        "Liest ein Issue anhand ID oder Key (z.B. `PROJ-1`), optional mit Expand.",
                        "Reads an issue by ID or key (e.g. `PROJ-1`), optionally expanded.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key, z.B. `PROJ-1` / issue ID or key")),
                        ("expand", str_list("renderedFields, names, schema, transitions, editmeta, changelog")),
                    ],
                    &["issue_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_SEARCH_ISSUES",
                    ("Issues suchen", "Search issues"),
                    (
                        "Sucht Issues per JQL (POST /search/jql); über `next_page_token` blättern.",
                        "Searches issues via JQL (POST /search/jql); page via `next_page_token`.",
                    ),
                    &[
                        ("jql", s("z.B. `project = PROJ AND status = 'In Progress'`")),
                        ("fields", str_list("Feld-IDs (`*all`, `*navigable`, `-description`) / field IDs")),
                        ("max_results", int("1-5000, max. 100 pro Seite / per page", 1, 5000, 50)),
                        ("next_page_token", s("Token der letzten Antwort / token from last response")),
                    ],
                    &["jql"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_SANITIZE_JQL",
                    ("JQL bereinigen", "Sanitize JQL"),
                    (
                        "Lässt JQL serverseitig validieren/verbessern.",
                        "Has Jira validate and correct a JQL query server-side.",
                    ),
                    &[("jql", s("zu prüfende JQL-Abfrage / query to check"))],
                    &["jql"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_ISSUE_COMMENTS",
                    ("Kommentare auflisten", "List issue comments"),
                    (
                        "Listet die Kommentare eines Issues (mit gerendertem HTML).",
                        "Lists an issue's comments (with rendered HTML).",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("order_by", s("`-created` (Standard, neueste zuerst) oder `+created`")),
                        ("limit", int("Maximale Anzahl / maximum", 1, 100, 10)),
                    ],
                    &["issue_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_GET_COMMENT",
                    ("Kommentar lesen", "Get comment"),
                    ("Liest einen einzelnen Kommentar.", "Reads a single comment by ID."),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("comment_id", s("Kommentar-ID / comment ID (siehe list_issue_comments)")),
                    ],
                    &["issue_id", "comment_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_FIND_USERS",
                    ("Benutzer finden", "Find users"),
                    (
                        "Sucht Benutzer nach Anzeigename oder E-Mail und liefert Account-IDs.",
                        "Finds users by name or email and returns their account IDs.",
                    ),
                    &[
                        ("query", s("Text gegen Name/E-Mail, z.B. `jane` oder `jane@example.com`")),
                        ("include_inactive", json!({"type": "boolean", "description": "Inaktive einbeziehen / include inactive", "default": false})),
                        ("start_at", int("Offset", 0, 100_000, 0)),
                        ("max_results", int("1-1000", 1, 1000, 50)),
                    ],
                    &["query"],
                    &[READ_USER],
                    false,
                ),
                t(
                    "JIRA_LIST_PROJECTS",
                    ("Projekte auflisten", "List projects"),
                    (
                        "Listet sichtbare Projekte (ID, Key, Name, Typ).",
                        "Lists visible projects (ID, key, name, type).",
                    ),
                    &[
                        ("query", s("Filter auf Name/Key / filter on name or key")),
                        ("type_key", s("`software`, `business`, `service_desk`, `product_discovery`")),
                        ("start_at", int("Offset", 0, 100_000, 0)),
                        ("max_results", int("1-100", 1, 100, 50)),
                    ],
                    &[],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_ISSUE_TYPES",
                    ("Issue-Typen auflisten", "List issue types"),
                    (
                        "Listet die Issue-Typen eines Projekts (für Create Issue).",
                        "Lists a project's issue types (for Create Issue).",
                    ),
                    &[("project_id", s("Numerische Projekt-ID / numeric project ID"))],
                    &["project_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_STATUSES",
                    ("Status auflisten", "List statuses"),
                    (
                        "Listet alle Status aktiver Workflows (für gültige JQL-Statusnamen); Filterung erfolgt clientseitig.",
                        "Lists all statuses used in active workflows (for valid JQL status names); filter client-side.",
                    ),
                    &[],
                    &[],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_PRIORITIES",
                    ("Prioritäten auflisten", "List priorities"),
                    (
                        "Listet Prioritäten (für Create/Update Issue).",
                        "Lists priorities (for Create/Update Issue).",
                    ),
                    &[
                        ("project_id", s("Optional: nur Prioritäten dieses Projekts / project filter")),
                        ("start_at", int("Offset", 0, 100_000, 0)),
                        ("max_results", int("1-1000", 1, 1000, 50)),
                    ],
                    &[],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_ISSUE_RESOLUTIONS",
                    ("Auflösungen auflisten", "List issue resolutions"),
                    (
                        "Listet Auflösungen (z.B. Done, Won't Do).",
                        "Lists resolutions (e.g. Done, Won't Do).",
                    ),
                    page_props,
                    &[],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_ISSUE_LINK_TYPES",
                    ("Link-Typen auflisten", "List issue link types"),
                    (
                        "Listet Issue-Link-Typen (für Link Issues).",
                        "Lists issue link types (for Link Issues).",
                    ),
                    &[],
                    &[],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_GET_ISSUE_TRANSITIONS",
                    ("Übergänge lesen", "Get issue transitions"),
                    (
                        "Listet mögliche Workflow-Übergänge eines Issues (IDs für Transition Issue).",
                        "Lists the workflow transitions available on an issue (IDs for Transition Issue).",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("include_fields", json!({"type": "boolean", "description": "Übergangs-Screen-Felder mitliefern / include transition screen fields", "default": false})),
                    ],
                    &["issue_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_GET_ISSUE_WATCHERS",
                    ("Beobachter lesen", "Get issue watchers"),
                    ("Listet die Beobachter eines Issues.", "Lists an issue's watchers."),
                    &[("issue_id", s("Issue-ID oder Key"))],
                    &["issue_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_LIST_WORKLOGS",
                    ("Arbeitsprotokolle auflisten", "List worklogs"),
                    (
                        "Listet die Arbeitsprotokolle eines Issues (Älteste zuerst).",
                        "Lists an issue's worklog entries (oldest first).",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("started_after_ms", s("Nur Worklogs ab diesem Zeitpunkt (Epoch-Millisekunden)")),
                        ("started_before_ms", s("Nur Worklogs bis diesem Zeitpunkt (Epoch-Millisekunden)")),
                        ("start_at", int("Offset", 0, 100_000, 0)),
                        ("max_results", int("1-5000", 1, 5000, 5000)),
                    ],
                    &["issue_id"],
                    &[READ_WORK],
                    false,
                ),
                t(
                    "JIRA_GET_CURRENT_USER",
                    ("Aktuellen Benutzer lesen", "Get current user"),
                    (
                        "Liefert den Benutzer der Verbindung (Account-ID für Assign/Watcher).",
                        "Returns the authenticated user (account ID for assign/watcher calls).",
                    ),
                    &[],
                    &[],
                    &[READ_USER],
                    false,
                ),
                // --- write ---
                t(
                    "JIRA_CREATE_ISSUE",
                    ("Issue anlegen", "Create issue"),
                    (
                        "Legt ein Issue in einem Projekt an; braucht --confirm.",
                        "Creates an issue in a project; requires --confirm.",
                    ),
                    &[
                        ("project_id", s("Numerische Projekt-ID / numeric project ID")),
                        ("issue_type_id", s("Numerische Issue-Typ-ID / numeric issue type ID")),
                        ("summary", s("Titel / title")),
                        ("description", s("Beschreibung als Klartext (wird als ADF gesendet)")),
                        ("assignee_id", s("Account-ID des Bearbeiters / assignee account ID")),
                        ("priority_id", s("Prioritäts-ID / priority ID")),
                        ("parent_key", s("Übergeordneter Issue-Key (Subtasks/Epics)")),
                        ("fields", json!({"type": "object", "description": "Weitere Felder (Feld-Key → Wert, z.B. ADF-Dokument) / extra fields by field key"})),
                    ],
                    &["project_id", "issue_type_id", "summary"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_UPDATE_ISSUE",
                    ("Issue aktualisieren", "Update issue"),
                    (
                        "Ändert Felder eines Issues; braucht --confirm.",
                        "Updates fields of an existing issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("summary", s("Neuer Titel / new title")),
                        ("description", s("Neue Beschreibung als Klartext (ADF)")),
                        ("issue_type_id", s("Issue-Typ-ID / issue type ID")),
                        ("assignee_id", s("Account-ID des Bearbeiters")),
                        ("priority_id", s("Prioritäts-ID")),
                        ("parent_key", s("Übergeordneter Issue-Key")),
                        ("fields", json!({"type": "object", "description": "Weitere Felder (Feld-Key → Wert) / extra fields by field key"})),
                    ],
                    &["issue_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_TRANSITION_ISSUE",
                    ("Issue übergehen", "Transition issue"),
                    (
                        "Führt einen Workflow-Übergang aus (z.B. In Progress → Done); braucht --confirm.",
                        "Executes a workflow transition (e.g. In Progress → Done); requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("transition_id", s("Übergangs-ID, siehe get_issue_transitions / see get_issue_transitions")),
                    ],
                    &["issue_id", "transition_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_ASSIGN_ISSUE",
                    ("Issue zuweisen", "Assign issue"),
                    (
                        "Setzt den Bearbeiter eines Issues; braucht --confirm.",
                        "Sets an issue's assignee; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("account_id", s("Account-ID des Bearbeiters / assignee account ID")),
                    ],
                    &["issue_id", "account_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_ADD_ISSUE_COMMENT",
                    ("Kommentar hinzufügen", "Add issue comment"),
                    (
                        "Fügt einem Issue einen Kommentar hinzu; braucht --confirm.",
                        "Adds a comment to an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("comment", s("Text oder ADF-JSON, wenn `is_adf` / text, or ADF JSON when `is_adf`")),
                        ("is_adf", json!({"type": "boolean", "description": "Kommentar ist ADF-JSON / comment is raw ADF JSON", "default": false})),
                    ],
                    &["issue_id", "comment"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_UPDATE_ISSUE_COMMENT",
                    ("Kommentar bearbeiten", "Update issue comment"),
                    (
                        "Ersetzt den Text eines Kommentars; braucht --confirm.",
                        "Replaces an existing comment's body; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("comment_id", s("Kommentar-ID")),
                        ("comment", s("Neuer Text oder ADF-JSON / new text, or ADF JSON when `is_adf`")),
                        ("is_adf", json!({"type": "boolean", "description": "Kommentar ist ADF-JSON", "default": false})),
                    ],
                    &["issue_id", "comment_id", "comment"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_DELETE_ISSUE_COMMENT",
                    ("Kommentar löschen", "Delete issue comment"),
                    (
                        "Löscht einen Kommentar endgültig; braucht --confirm.",
                        "Permanently deletes a comment; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("comment_id", s("Kommentar-ID")),
                    ],
                    &["issue_id", "comment_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_LINK_ISSUES",
                    ("Issues verknüpfen", "Link issues"),
                    (
                        "Erzeugt einen Link zwischen zwei Issues; braucht --confirm.",
                        "Creates a link between two issues; requires --confirm.",
                    ),
                    &[
                        ("first_issue_id", s("Ausgehendes Issue (outward) / outward issue")),
                        ("link_type_id", s("Link-Typ-ID, siehe list_issue_link_types")),
                        ("second_issue_id", s("Eingehendes Issue (inward) / inward issue")),
                    ],
                    &["first_issue_id", "link_type_id", "second_issue_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_ADD_ISSUE_WATCHER",
                    ("Beobachter hinzufügen", "Add issue watcher"),
                    (
                        "Trägt einen Benutzer als Beobachter ein; braucht --confirm.",
                        "Subscribes a user as watcher; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("user_id", s("Account-ID des Beobachters / watcher account ID")),
                    ],
                    &["issue_id", "user_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_REMOVE_ISSUE_WATCHER",
                    ("Beobachter entfernen", "Remove issue watcher"),
                    (
                        "Entfernt einen Beobachter; braucht --confirm.",
                        "Removes a watcher from an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("account_id", s("Account-ID des Beobachters")),
                    ],
                    &["issue_id", "account_id"],
                    &[WRITE_WORK],
                    true,
                ),
                t(
                    "JIRA_DELETE_ISSUE",
                    ("Issue löschen", "Delete issue"),
                    (
                        "Löscht ein Issue endgültig; braucht --confirm.",
                        "Permanently deletes an issue; requires --confirm.",
                    ),
                    &[
                        ("issue_id", s("Issue-ID oder Key")),
                        ("delete_subtasks", json!({"type": "boolean", "description": "Subtasks mitlöschen (Pflicht bei vorhandenen Subtasks)", "default": false})),
                    ],
                    &["issue_id"],
                    &[WRITE_WORK],
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
        format!("{}/rest/api/3{path}", self.api_base)
    }

    /// `?a=b&c=d` for Jira query parameters (percent-encoded, order stable).
    fn qs(&self, pairs: &[(&str, Option<String>)]) -> String {
        query(pairs)
    }

    fn issue_fields(args: &Value) -> Result<Value> {
        let mut fields = json!({});
        let f = fields.as_object_mut().expect("object");
        if let Some(v) = opt_str(args, "summary") {
            f.insert("summary".into(), json!(v));
        }
        if let Some(v) = opt_str(args, "description") {
            f.insert("description".into(), adf_text(v));
        }
        if let Some(v) = opt_str(args, "issue_type_id") {
            f.insert("issuetype".into(), json!({"id": v}));
        }
        if let Some(v) = opt_str(args, "assignee_id") {
            f.insert("assignee".into(), json!({"id": v}));
        }
        if let Some(v) = opt_str(args, "priority_id") {
            f.insert("priority".into(), json!({"id": v}));
        }
        if let Some(v) = opt_str(args, "parent_key") {
            f.insert("parent".into(), json!({"key": v}));
        }
        if let Some(extra) = args.get("fields").filter(|v| v.is_object()).and_then(Value::as_object) {
            for (k, v) in extra {
                f.insert(k.clone(), v.clone());
            }
        }
        if f.is_empty() {
            bail!("no fields to update");
        }
        Ok(fields)
    }
}

impl ToolkitAdapter for Jira {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // --- read / search ---
            "JIRA_GET_ISSUE" => {
                let expand = str_vec(args, "expand");
                let q = self.qs(&[("expand", (!expand.is_empty()).then(|| expand.join(",")))]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/issue/{}", path_segment(args, "issue_id")?)) + &q,
                )
            }
            "JIRA_SEARCH_ISSUES" => {
                let mut fields = str_vec(args, "fields");
                fields.retain(|f| !f.trim().is_empty());
                if fields.is_empty() {
                    fields.push("*navigable".into());
                }
                let max = opt_u64(args, "max_results", 50, 1, 5000).min(100);
                let mut body = json!({
                    "jql": req_str(args, "jql")?,
                    "maxResults": max,
                    "fields": fields,
                });
                if let Some(tok) = opt_str(args, "next_page_token") {
                    body["nextPageToken"] = json!(tok);
                }
                HttpRequest::new("POST", self.url("/search/jql")).json(&body)
            }
            "JIRA_SANITIZE_JQL" => HttpRequest::new("POST", self.url("/jql/sanitize"))
                .json(&json!({"queries": [{"query": req_str(args, "jql")?}]})),
            "JIRA_LIST_ISSUE_COMMENTS" => {
                let q = self.qs(&[
                    ("orderBy", Some(opt_str(args, "order_by").unwrap_or("-created").to_string())),
                    ("maxResults", Some(opt_u64(args, "limit", 10, 1, 100).to_string())),
                    ("expand", Some("renderedBody".into())),
                ]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/issue/{}/comment", path_segment(args, "issue_id")?)) + &q,
                )
            }
            "JIRA_GET_COMMENT" => HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/issue/{}/comment/{}?expand=renderedBody",
                    path_segment(args, "issue_id")?,
                    path_segment(args, "comment_id")?
                )),
            ),
            "JIRA_FIND_USERS" => {
                let q = self.qs(&[
                    ("query", Some(req_str(args, "query")?.to_string())),
                    ("startAt", Some(opt_u64(args, "start_at", 0, 0, 100_000).to_string())),
                    ("maxResults", Some(opt_u64(args, "max_results", 50, 1, 1000).to_string())),
                ]);
                HttpRequest::new("GET", self.url("/user/search") + &q)
            }
            "JIRA_LIST_PROJECTS" => {
                let q = self.qs(&[
                    ("query", opt_str(args, "query").map(String::from)),
                    ("typeKey", opt_str(args, "type_key").map(String::from)),
                    ("startAt", Some(opt_u64(args, "start_at", 0, 0, 100_000).to_string())),
                    ("maxResults", Some(opt_u64(args, "max_results", 50, 1, 100).to_string())),
                    ("expand", Some("description,lead".into())),
                ]);
                HttpRequest::new("GET", self.url("/project/search") + &q)
            }
            "JIRA_LIST_ISSUE_TYPES" => {
                let q = self.qs(&[("projectId", Some(req_str(args, "project_id")?.to_string()))]);
                HttpRequest::new("GET", self.url("/issuetype/project") + &q)
            }
            "JIRA_LIST_STATUSES" => HttpRequest::new("GET", self.url("/status")),
            "JIRA_LIST_PRIORITIES" => {
                let q = self.qs(&[
                    ("projectId", opt_str(args, "project_id").map(String::from)),
                    ("startAt", Some(opt_u64(args, "start_at", 0, 0, 100_000).to_string())),
                    ("maxResults", Some(opt_u64(args, "max_results", 50, 1, 1000).to_string())),
                ]);
                HttpRequest::new("GET", self.url("/priority/search") + &q)
            }
            "JIRA_LIST_ISSUE_RESOLUTIONS" => {
                let q = self.qs(&[
                    ("startAt", Some(opt_u64(args, "start_at", 0, 0, 100_000).to_string())),
                    ("maxResults", Some(opt_u64(args, "max_results", 50, 1, 1000).to_string())),
                ]);
                HttpRequest::new("GET", self.url("/resolution/search") + &q)
            }
            "JIRA_LIST_ISSUE_LINK_TYPES" => HttpRequest::new("GET", self.url("/issueLinkType")),
            "JIRA_GET_ISSUE_TRANSITIONS" => {
                let q = self.qs(&[(
                    "expand",
                    args.get("include_fields")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        .then(|| "transitions.fields".to_string()),
                )]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/issue/{}/transitions", path_segment(args, "issue_id")?)) + &q,
                )
            }
            "JIRA_GET_ISSUE_WATCHERS" => HttpRequest::new(
                "GET",
                self.url(&format!("/issue/{}/watchers", path_segment(args, "issue_id")?)),
            ),
            "JIRA_LIST_WORKLOGS" => {
                let q = self.qs(&[
                    ("startAt", Some(opt_u64(args, "start_at", 0, 0, 100_000).to_string())),
                    ("maxResults", Some(opt_u64(args, "max_results", 5000, 1, 5000).to_string())),
                    ("startedAfter", opt_str(args, "started_after_ms").map(String::from)),
                    ("startedBefore", opt_str(args, "started_before_ms").map(String::from)),
                ]);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/issue/{}/worklog", path_segment(args, "issue_id")?)) + &q,
                )
            }
            "JIRA_GET_CURRENT_USER" => {
                let q = self.qs(&[("expand", Some("groups".into()))]);
                HttpRequest::new("GET", self.url("/myself") + &q)
            }
            // --- write ---
            "JIRA_CREATE_ISSUE" => {
                let mut body = json!({"fields": {
                    "project": {"id": req_str(args, "project_id")?},
                    "issuetype": {"id": req_str(args, "issue_type_id")?},
                    "summary": req_str(args, "summary")?,
                }});
                if let Some(extra) = Self::issue_fields(args)?.as_object() {
                    for (k, v) in extra {
                        body["fields"][k] = v.clone();
                    }
                }
                HttpRequest::new("POST", self.url("/issue")).json(&body)
            }
            "JIRA_UPDATE_ISSUE" => {
                let fields = Self::issue_fields(args)?;
                HttpRequest::new(
                    "PUT",
                    self.url(&format!("/issue/{}", path_segment(args, "issue_id")?))
                        + &self.qs(&[("returnIssue", Some("true".into()))]),
                )
                .json(&json!({"fields": fields}))
            }
            "JIRA_TRANSITION_ISSUE" => HttpRequest::new(
                "POST",
                self.url(&format!("/issue/{}/transitions", path_segment(args, "issue_id")?)),
            )
            .json(&json!({"transition": {"id": req_str(args, "transition_id")?}})),
            "JIRA_ASSIGN_ISSUE" => HttpRequest::new(
                "PUT",
                self.url(&format!("/issue/{}/assignee", path_segment(args, "issue_id")?)),
            )
            .json(&json!({"accountId": req_str(args, "account_id")?})),
            "JIRA_ADD_ISSUE_COMMENT" => HttpRequest::new(
                "POST",
                self.url(&format!("/issue/{}/comment", path_segment(args, "issue_id")?)),
            )
            .json(&json!({"body": comment_body(args)?})),
            "JIRA_UPDATE_ISSUE_COMMENT" => HttpRequest::new(
                "PUT",
                self.url(&format!(
                    "/issue/{}/comment/{}",
                    path_segment(args, "issue_id")?,
                    path_segment(args, "comment_id")?
                )),
            )
            .json(&json!({"body": comment_body(args)?})),
            "JIRA_DELETE_ISSUE_COMMENT" => HttpRequest::new(
                "DELETE",
                self.url(&format!(
                    "/issue/{}/comment/{}",
                    path_segment(args, "issue_id")?,
                    path_segment(args, "comment_id")?
                )),
            ),
            "JIRA_LINK_ISSUES" => HttpRequest::new("POST", self.url("/issueLink")).json(&json!({
                "type": {"id": req_str(args, "link_type_id")?},
                "inwardIssue": {"id": req_str(args, "second_issue_id")?},
                "outwardIssue": {"id": req_str(args, "first_issue_id")?},
            })),
            "JIRA_ADD_ISSUE_WATCHER" => HttpRequest::new(
                "POST",
                self.url(&format!("/issue/{}/watchers", path_segment(args, "issue_id")?)),
            )
            .json(&json!(req_str(args, "user_id")?)),
            "JIRA_REMOVE_ISSUE_WATCHER" => {
                let q = self.qs(&[("accountId", Some(req_str(args, "account_id")?.to_string()))]);
                HttpRequest::new(
                    "DELETE",
                    self.url(&format!("/issue/{}/watchers", path_segment(args, "issue_id")?)) + &q,
                )
            }
            "JIRA_DELETE_ISSUE" => {
                let sub = args.get("delete_subtasks").and_then(Value::as_bool).unwrap_or(false);
                let q = self.qs(&[("deleteSubtasks", Some(sub.to_string()))]);
                HttpRequest::new(
                    "DELETE",
                    self.url(&format!("/issue/{}", path_segment(args, "issue_id")?)) + &q,
                )
            }
            other => bail!("unknown Jira tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "JIRA_GET_ISSUE" => json!({"id": v["id"], "key": v["key"], "fields": v["fields"],
                "names": v["names"], "transitions": v["transitions"], "changelog": v["changelog"],
                "rendered_fields": v["renderedFields"]}),
            "JIRA_SEARCH_ISSUES" => json!({"issues": v["issues"], "next_page_token": v["nextPageToken"]}),
            "JIRA_SANITIZE_JQL" => json!({"sanitized_query": v["queries"][0]["sanitizedQuery"]}),
            "JIRA_LIST_ISSUE_COMMENTS" => json!({"comments": v["comments"], "start_at": v["startAt"],
                "total": v["total"], "max_results": v["maxResults"]}),
            "JIRA_GET_COMMENT" => json!({"id": v["id"], "body": v["body"],
                "rendered_body": v["renderedBody"], "created": v["created"], "updated": v["updated"],
                "author_account_id": v["author"]["accountId"], "author_name": v["author"]["displayName"]}),
            "JIRA_FIND_USERS" => {
                let users: Vec<Value> = v
                    .as_array()
                    .map(|a| a.iter().map(user).collect())
                    .unwrap_or_default();
                json!({"found": !users.is_empty(), "users": users})
            }
            "JIRA_LIST_PROJECTS" => {
                let items: Vec<Value> = v["values"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|p| json!({"id": p["id"], "key": p["key"], "name": p["name"],
                                "project_type_key": p["projectTypeKey"], "description": p["description"]}))
                            .collect()
                    })
                    .unwrap_or_default();
                page(&v, json!(items))
            }
            "JIRA_LIST_ISSUE_TYPES" => {
                let items: Vec<Value> = v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| json!({"id": t["id"], "name": t["name"],
                                "description": t["description"], "subtask": t["subtask"]}))
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"items": items})
            }
            "JIRA_LIST_STATUSES" => json!(v),
            "JIRA_LIST_PRIORITIES" | "JIRA_LIST_ISSUE_RESOLUTIONS" => page(&v, json!(v["values"])),
            "JIRA_LIST_ISSUE_LINK_TYPES" => json!({"items": v["issueLinkTypes"]}),
            "JIRA_GET_ISSUE_TRANSITIONS" => {
                let items: Vec<Value> = v["transitions"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| json!({"id": t["id"], "name": t["name"], "to_status": t["to"]["name"]}))
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"items": items})
            }
            "JIRA_GET_ISSUE_WATCHERS" => json!({"watchers": v["watchers"].as_array().map(|a| {
                    a.iter().map(user).collect::<Vec<_>>() }),
                "watch_count": v["watchCount"], "is_watching": v["isWatching"]}),
            "JIRA_LIST_WORKLOGS" => page(&v, json!(v["worklogs"])),
            "JIRA_GET_CURRENT_USER" => user(&v),
            "JIRA_CREATE_ISSUE" => json!({"id": v["id"], "key": v["key"]}),
            "JIRA_UPDATE_ISSUE" => json!({"id": v["id"], "key": v["key"], "summary": v["fields"]["summary"]}),
            "JIRA_TRANSITION_ISSUE" | "JIRA_ASSIGN_ISSUE" | "JIRA_DELETE_ISSUE_COMMENT"
            | "JIRA_LINK_ISSUES" | "JIRA_ADD_ISSUE_WATCHER" | "JIRA_REMOVE_ISSUE_WATCHER"
            | "JIRA_DELETE_ISSUE" => ok(),
            "JIRA_ADD_ISSUE_COMMENT" | "JIRA_UPDATE_ISSUE_COMMENT" => {
                json!({"comment_id": v["id"], "created": v["created"], "updated": v["updated"]})
            }
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adf_and_paths() {
        let j = Jira::with_api_base("https://ex.atlassian.net");
        let r = j
            .build_request("JIRA_ADD_ISSUE_COMMENT", &json!({"issue_id": "PROJ-1", "comment": "hi"}))
            .unwrap();
        assert_eq!(r.url, "https://ex.atlassian.net/rest/api/3/issue/PROJ-1/comment");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["body"]["content"][0]["content"][0]["text"], "hi");
        // path traversal is rejected, keys with dashes are fine
        assert!(j.build_request("JIRA_GET_ISSUE", &json!({"issue_id": "../admin"})).is_err());
        assert!(j.build_request("JIRA_GET_ISSUE", &json!({"issue_id": "PROJ-1"})).is_ok());
        // ADF flag must carry a JSON document
        assert!(j
            .build_request(
                "JIRA_ADD_ISSUE_COMMENT",
                &json!({"issue_id": "P-1", "comment": "not json", "is_adf": true})
            )
            .is_err());
    }
}
