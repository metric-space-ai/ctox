//! Zendesk Support toolkit (REST API v2). Action set modelled on the Activepieces
//! piece `zendesk` (MIT), files under `src/lib/`.
//!
//! Auth (replicated from `src/lib/auth.ts` + `src/lib/common/client.ts`):
//! custom "API Token" connection uses HTTP Basic with `email/token` as the user
//! name and the API token as the password; the OAuth2 connection uses
//! `https://{subdomain}.zendesk.com/oauth/authorizations/new` plus
//! `https://{subdomain}.zendesk.com/oauth/tokens` with scopes `read` + `write`
//! and PKCE (S256). The `{subdomain}` in the OAuth URLs is the piece's own
//! template; the hub does not substitute placeholders, so it is passed through
//! verbatim.
//!
//! Base URL: the piece derives `https://{subdomain}.zendesk.com/api/v2`
//! (`getZendeskBaseUrl`) — it depends on the account, so it is injected with
//! [`Zendesk::with_subdomain`] / [`Zendesk::with_api_base`]. The `Default`
//! instance only carries a documented placeholder; `builtin()` has no per-account
//! wiring, so any caller that knows the subdomain must register its own adapter.
//!
//! Every request arm below carries the `// AP:` comment with the Activepieces
//! file the method, path and body fields were taken from. Dynamic dropdowns of
//! the piece become plain id strings plus the `ZENDESK_LIST_*` tools that feed
//! them (`src/lib/common/props.ts`), so no lookup call is invented.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "zendesk";

/// OAuth2 scopes of the piece's OAuth connection.
const READ: &str = "read";
const WRITE: &str = "write";

/// Placeholder account used by [`Zendesk::default`] — not a real account.
const DEFAULT_SUBDOMAIN: &str = "example";

// ---------------------------------------------------------------- schema helpers

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// Zendesk record id. The piece's dropdowns emit the id as a string
/// (`ticket.id.toString()`), so the port keeps strings and sends them as JSON
/// numbers like the vendor API documents.
fn id_prop(desc: &str) -> Value {
    s(desc)
}

fn id_list_prop(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

/// `per_page` as sent by the piece's dropdown sources (`?per_page=100`).
fn per_page() -> Value {
    int("How many records to return in this page (Zendesk max 100). Default 100.", 1, 100, 100)
}

fn object_list_prop(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "object"}, "description": desc})
}

/// Ticket/user/organization field values keyed by field key.
fn custom_fields_prop() -> Value {
    json!({"type": "object", "description":
        "Zendesk-Feldwerte als JSON-Objekt, Schlüssel = Feld-Key (siehe ZENDESK_LIST_*_FIELDS). / \
         Custom field values as a JSON object keyed by field key (see ZENDESK_LIST_*_FIELDS)."})
}

// ---------------------------------------------------------------- id helpers

/// Zendesk ids are numeric (`id()` in `src/lib/common/api.ts`).
fn numeric(args: &Value, key: &str) -> Result<u64> {
    let raw = path_segment(args, key)?;
    match raw.parse::<u64>() {
        Ok(v) if raw.bytes().all(|b| b.is_ascii_digit()) => Ok(v),
        _ => bail!("`{key}` must be a numeric Zendesk ID, got \"{raw}\"."),
    }
}

fn opt_numeric(args: &Value, key: &str) -> Result<Option<u64>> {
    match args.get(key).and_then(Value::as_str) {
        Some(v) if !v.is_empty() => numeric(args, key).map(Some),
        _ => Ok(None),
    }
}

fn id_list(args: &Value, key: &str) -> Result<Vec<u64>> {
    str_vec(args, key)
        .iter()
        .map(|v| match v.parse::<u64>() {
            Ok(n) if v.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
            _ => bail!("`{key}` must contain numeric Zendesk IDs, got \"{v}\"."),
        })
        .collect()
}

fn opt_id_list(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if !str_vec(args, key).is_empty() {
        m.insert(field.into(), json!(id_list(args, key)?));
    }
    Ok(())
}

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_flag(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = args.get(key).and_then(Value::as_bool) {
        m.insert(field.into(), json!(v));
    }
}

fn put_id(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if let Some(v) = opt_numeric(args, key)? {
        m.insert(field.into(), json!(v));
    }
    Ok(())
}

fn put_list(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    let tags = str_vec(args, key);
    if !tags.is_empty() {
        m.insert(field.into(), json!(tags));
    }
}

/// `custom_fields` as `[{id, value}]` (`ticket.custom_fields`, create-ticket.ts).
fn put_custom_fields(m: &mut Map<String, Value>, args: &Value, field: &str) -> Result<()> {
    let Some(Value::Array(list)) = args.get("custom_fields") else {
        return Ok(());
    };
    let mut out: Vec<Value> = Vec::new();
    for entry in list {
        let Some(o) = entry.as_object() else {
            bail!("each `custom_fields` entry must be an object with `id` and `value`");
        };
        let Some(id) = o.get("id").and_then(Value::as_u64) else {
            bail!("each `custom_fields` entry needs a numeric `id`");
        };
        if o.get("value").is_none() {
            bail!("each `custom_fields` entry needs a `value`");
        }
        out.push(json!({"id": id, "value": o["value"]}));
    }
    if !out.is_empty() {
        m.insert(field.into(), Value::Array(out));
    }
    Ok(())
}

/// `user_fields` / `organization_fields` as `{<field key>: value}`.
fn put_field_map(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(Value::Object(map)) = args.get(key) {
        let clean: Map<String, Value> = map
            .iter()
            .filter(|(_, v)| !v.is_null() && *v != &json!(""))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !clean.is_empty() {
            m.insert(field.into(), Value::Object(clean));
        }
    }
}

// ---------------------------------------------------------------- response helpers

/// `vendorMessage` + `STATUS_HINTS` of `src/lib/common/api.ts`.
fn check(response: &HttpResponse) -> Result<Value> {
    let v = response.json()?;
    if response.status >= 400 {
        let detail = ["error", "description", "details"]
            .iter()
            .filter_map(|k| v.get(*k))
            .filter(|p| !p.is_null())
            .map(|p| match p.as_str() {
                Some(s) => s.to_string(),
                None => p.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" — ");
        let hint = match response.status {
            401 => "the connection is invalid or expired",
            403 => "the connected user lacks the permission or plan feature for this operation",
            404 => "the record was not found",
            409 => "the record changed since it was read (safe update conflict)",
            422 => "Zendesk rejected the input",
            429 => "rate limit exceeded, retry later",
            _ => "",
        };
        // `['Zendesk API error ${status}', hint, detail].join(': ')` of api.ts.
        let mut parts: Vec<String> = vec![format!("Zendesk API error {}", response.status)];
        for part in [hint, detail.as_str()] {
            if !part.is_empty() {
                parts.push(part.to_string());
            }
        }
        bail!("{}", parts.join(": "));
    }
    Ok(v)
}

/// `<collection>` plus the raw envelope, for list mappers.
fn rows(response: &HttpResponse, key: &str) -> Result<(Vec<Value>, Value)> {
    let v = check(response)?;
    let items = v[key].as_array().cloned().unwrap_or_default();
    Ok((items, v))
}

/// `next_page`/`count` envelope of the cursor-paginated collections.
fn page(items: Vec<Value>, map: fn(&Value) -> Value, envelope: &Value) -> Value {
    let next = envelope["next_page"].clone();
    json!({
        "items": items.iter().map(map).collect::<Vec<_>>(),
        "count": items.len(),
        "total_count": envelope["count"].clone(),
        "has_more": next.is_string(),
        "next_page": next,
    })
}

fn tags_of(v: &Value) -> String {
    v["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn flatten_ticket(t: &Value) -> Value {
    json!({
        "id": t["id"],
        "subject": t["subject"],
        "status": t["status"],
        "type": t["type"],
        "priority": t["priority"],
        "requester_id": t["requester_id"],
        "assignee_id": t["assignee_id"],
        "organization_id": t["organization_id"],
        "group_id": t["group_id"],
        "brand_id": t["brand_id"],
        "problem_id": t["problem_id"],
        "forum_topic_id": t["forum_topic_id"],
        "external_id": t["external_id"],
        "tags": tags_of(t),
        "comment_count": t["comment_count"],
        "created_at": t["created_at"],
        "updated_at": t["updated_at"],
        "due_at": t["due_at"],
        "url": t["url"],
    })
}

fn flatten_user(u: &Value) -> Value {
    json!({
        "id": u["id"],
        "name": u["name"],
        "email": u["email"],
        "role": u["role"],
        "active": u["active"],
        "verified": u["verified"],
        "suspended": u["suspended"],
        "organization_id": u["organization_id"],
        "group_id": u["group_id"],
        "default_group_id": u["default_group_id"],
        "ticket_restriction": u["ticket_restriction"],
        "tags": tags_of(u),
        "custom_role_id": u["custom_role_id"],
        "created_at": u["created_at"],
        "updated_at": u["updated_at"],
    })
}

fn flatten_organization(o: &Value) -> Value {
    let domains = o["domain_names"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    json!({
        "id": o["id"],
        "name": o["name"],
        "details": o["details"],
        "notes": o["notes"],
        "external_id": o["external_id"],
        "group_id": o["group_id"],
        "domain_names": domains,
        "tags": tags_of(o),
        "shared_tickets": o["shared_tickets"],
        "shared_comments": o["shared_comments"],
        "created_at": o["created_at"],
        "updated_at": o["updated_at"],
    })
}

fn flatten_group(g: &Value) -> Value {
    json!({
        "id": g["id"], "name": g["name"], "is_public": g["is_public"],
        "default": g["default"], "description": g["description"],
        "created_at": g["created_at"], "updated_at": g["updated_at"],
    })
}

fn flatten_brand(b: &Value) -> Value {
    json!({
        "id": b["id"], "name": b["name"], "subdomain": b["subdomain"],
        "active": b["active"], "default": b["default"], "url": b["url"],
        "created_at": b["created_at"], "updated_at": b["updated_at"],
    })
}

fn flatten_custom_role(r: &Value) -> Value {
    json!({
        "id": r["id"], "name": r["name"], "description": r["description"],
        "role_type": r["role_type"], "team_member_count": r["team_member_count"],
        "created_at": r["created_at"], "updated_at": r["updated_at"],
    })
}

/// `ticket_fields` / `user_fields` / `organization_fields` share one shape.
fn flatten_field(f: &Value) -> Value {
    let options = f["custom_field_options"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    json!({
        "id": f["id"], "key": f["key"], "title": f["title"], "type": f["type"],
        "active": f["active"], "description": f["description"],
        "custom_field_options": options,
        "regexp_for_validation": f["regexp_for_validation"],
        "required": f["required"],
        "created_at": f["created_at"], "updated_at": f["updated_at"],
    })
}

fn flatten_comment(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "type": c["type"],
        "body": c["body"],
        "html_body": c["html_body"],
        "plain_body": c["plain_body"],
        "public": c["public"],
        "is_public": c["public"],
        "author_id": c["author_id"],
        "attachments": c["attachments"],
        "via": c["via"],
        "created_at": c["created_at"],
    })
}

/// `{ <collection>: [...], count, next_page }` mappers.
fn list_value(items: Vec<Value>, envelope: Value, map: fn(&Value) -> Value) -> Value {
    page(items, map, &envelope)
}

// ---------------------------------------------------------------- adapter

pub struct Zendesk {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Zendesk {
    fn default() -> Self {
        Self::with_subdomain(DEFAULT_SUBDOMAIN)
    }
}

#[allow(clippy::too_many_lines)]
impl Zendesk {
    /// `Zendesk::with_subdomain("acme")` -> `https://acme.zendesk.com/api/v2`,
    /// exactly `getZendeskBaseUrl` of `src/lib/common/client.ts`.
    pub fn with_subdomain(subdomain: &str) -> Self {
        let sub = subdomain.trim().trim_end_matches('/');
        Self::with_api_base(&format!("https://{sub}.zendesk.com/api/v2"))
    }

    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Zendesk".into(),
            description: LocalizedText::new(
                "Tickets, Kommentare, Tags, Benutzer und Organisationen in Zendesk lesen und bearbeiten.",
                "Read and change tickets, comments, tags, users and organizations in Zendesk.",
            ),
            // The piece offers two connections: the API-token custom auth (Basic)
            // and OAuth2 with `read`/`write`; both are declared here.
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://{subdomain}.zendesk.com/oauth/authorizations/new".into(),
                    token_url: "https://{subdomain}.zendesk.com/oauth/tokens".into(),
                    default_scopes: vec![READ.into(), WRITE.into()],
                    pkce: true,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: vec![],
                }),
                AuthScheme::Basic,
            ],
            tools: vec![
                // ---------------------------------------------------------- tickets: list
                t(
                    "ZENDESK_LIST_TICKETS",
                    ("Tickets auflisten", "List tickets"),
                    (
                        "Listet Tickets; füllt die Ticket-Dropdown der Aktionen.",
                        "Lists tickets; feeds the ticket dropdown of the other actions.",
                    ),
                    &[("per_page", per_page())],
                    &[],
                    &[READ],
                    false,
                ),
                // ---------------------------------------------------------- tickets: find
                t(
                    "ZENDESK_FIND_TICKETS",
                    ("Tickets suchen", "Find tickets"),
                    (
                        "Sucht Tickets per ID, Status, Priorität, Typ, Tag, Anfragenden, Bearbeiter oder Freitext.",
                        "Searches tickets by ID, status, priority, type, tag, requester, assignee or free text.",
                    ),
                    &[
                        ("search_type", choice("Wonach wird gesucht", &["id", "status", "priority", "type", "tag", "requester", "assignee", "content", "custom"])),
                        ("ticket_id", s("Ticket-ID, wenn search_type=id")),
                        ("status", choice("Status, wenn search_type=status", &["new", "open", "pending", "hold", "solved", "closed"])),
                        ("priority", choice("Priorität, wenn search_type=priority", &["low", "normal", "high", "urgent"])),
                        ("ticket_type", choice("Typ, wenn search_type=type", &["problem", "incident", "question", "task"])),
                        ("tag", s("Tag, wenn search_type=tag")),
                        ("requester_email", s("E-Mail der anfragenden Person, wenn search_type=requester")),
                        ("assignee_email", s("E-Mail der bearbeitenden Person, wenn search_type=assignee")),
                        ("content", s("Suchtext für Betreff und Inhalt, wenn search_type=content")),
                        ("custom_query", s("Eigene Zendesk-Search-Syntax, wenn search_type=custom")),
                        (
                            "date_filters",
                            object_list_prop(
                                "Datumfilter: Objekte {field, operator, value}; field = created|updated|solved|due_date, operator = :|>|>=|<|<=, value = ISO-Datum",
                            ),
                        ),
                        ("sort_by", choice("Sortierung; relevance wird nicht gesendet", &["relevance", "created_at", "updated_at", "priority", "status", "ticket_type"])),
                        ("sort_order", choice("Sortierrichtung", &["desc", "asc"])),
                    ],
                    &["search_type"],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_FIND_LATEST_COMMENT",
                    ("Letzten Kommentar lesen", "Find latest comment"),
                    (
                        "Liest den neuesten Kommentar eines Tickets (neueste zuerst).",
                        "Reads the most recent comment of a ticket (newest first).",
                    ),
                    &[("ticket_id", id_prop("Ticket-ID (aus ZENDESK_LIST_TICKETS)"))],
                    &["ticket_id"],
                    &[READ],
                    false,
                ),
                // ---------------------------------------------------------- users / orgs / groups: search
                t(
                    "ZENDESK_FIND_USER",
                    ("Benutzer suchen", "Find users"),
                    (
                        "Sucht Benutzer per E-Mail, Name, Rolle, Organisation, Tag, External ID oder eigener Query.",
                        "Searches users by email, name, role, organization, tag, external ID or custom query.",
                    ),
                    &[
                        ("search_type", choice("Wonach wird gesucht", &["email", "name", "role", "organization", "tag", "external_id", "custom"])),
                        ("email", s("E-Mail, wenn search_type=email")),
                        ("name", s("Name, wenn search_type=name")),
                        ("role", choice("Rolle, wenn search_type=role", &["end-user", "agent", "admin"])),
                        ("organization", s("Organisationsname, wenn search_type=organization")),
                        ("tag", s("Tag, wenn search_type=tag")),
                        ("external_id", s("External ID, wenn search_type=external_id")),
                        ("custom_query", s("Eigene Zendesk-Search-Syntax, wenn search_type=custom")),
                        ("sort_by", choice("Sortierung; relevance wird nicht gesendet", &["relevance", "created_at", "updated_at"])),
                        ("sort_order", choice("Sortierrichtung", &["desc", "asc"])),
                    ],
                    &["search_type"],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_FIND_ORGANIZATION",
                    ("Organisationen suchen", "Find organizations"),
                    (
                        "Sucht Organisationen per Name, Domain, External ID, Tag, Details oder eigener Query.",
                        "Searches organizations by name, domain, external ID, tag, details or custom query.",
                    ),
                    &[
                        ("search_type", choice("Wonach wird gesucht", &["name", "domain", "external_id", "tag", "details", "custom"])),
                        ("name", s("Name, wenn search_type=name")),
                        ("domain", s("Domain, wenn search_type=domain")),
                        ("external_id", s("External ID, wenn search_type=external_id")),
                        ("tag", s("Tag, wenn search_type=tag")),
                        ("details", s("Details/Notizen, wenn search_type=details")),
                        ("custom_query", s("Eigene Zendesk-Search-Syntax, wenn search_type=custom")),
                        ("sort_by", choice("Sortierung; relevance wird nicht gesendet", &["relevance", "created_at", "updated_at"])),
                        ("sort_order", choice("Sortierrichtung", &["desc", "asc"])),
                    ],
                    &["search_type"],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_FIND_USER_BY_EMAIL",
                    ("Benutzer nach E-Mail suchen", "Find user by email"),
                    (
                        "Löst eine E-Mail zur Benutzer-ID auf (`users/search.json`).",
                        "Resolves an email address to a user id (`users/search.json`).",
                    ),
                    &[("email", s("Exakte E-Mail-Adresse"))],
                    &["email"],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_AGENTS",
                    ("Agenten auflisten", "List agents"),
                    (
                        "Listet die Agenten (`users.json?role=agent`); die Aktion find-agent filtert diese Liste in Activepieces clientseitig.",
                        "Lists the agents (`users.json?role=agent`); the find-agent action filters this list client-side in Activepieces.",
                    ),
                    &[("per_page", per_page())],
                    &[],
                    &[READ],
                    false,
                ),
                // ---------------------------------------------------------- lookup lists
                t(
                    "ZENDESK_LIST_USERS",
                    ("Benutzer auflisten", "List users"),
                    (
                        "Listet Benutzer; füllt die Benutzer-Dropdown der Aktionen.",
                        "Lists users; feeds the user dropdown of the other actions.",
                    ),
                    &[("per_page", per_page())],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_ORGANIZATIONS",
                    ("Organisationen auflisten", "List organizations"),
                    (
                        "Listet Organisationen; füllt die Organisations-Dropdown der Aktionen.",
                        "Lists organizations; feeds the organization dropdown of the other actions.",
                    ),
                    &[("per_page", per_page())],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_GROUPS",
                    ("Gruppen auflisten", "List groups"),
                    (
                        "Listet Gruppen; füllt die Gruppen-Dropdown der Aktionen.",
                        "Lists groups; feeds the group dropdown of the other actions.",
                    ),
                    &[("per_page", per_page())],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_BRANDS",
                    ("Brands auflisten", "List brands"),
                    (
                        "Listet die Brands der Instanz (Dropdown-Quelle von create-ticket/update-ticket).",
                        "Lists the instance brands (dropdown source of create-ticket/update-ticket).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_CUSTOM_ROLES",
                    ("Custom Roles auflisten", "List custom roles"),
                    (
                        "Listet die Custom Roles der Instanz (Dropdown-Quelle von create-user).",
                        "Lists the instance custom roles (dropdown source of create-user).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_TICKET_FIELDS",
                    ("Ticket-Felder auflisten", "List ticket fields"),
                    (
                        "Listet die Ticket-Felder inkl. Custom-Field-Keys und Optionen.",
                        "Lists ticket fields including custom field keys and options.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_USER_FIELDS",
                    ("Benutzerfelder auflisten", "List user fields"),
                    (
                        "Listet die Benutzerfelder inkl. Custom-Field-Keys und Optionen.",
                        "Lists user fields including custom field keys and options.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "ZENDESK_LIST_ORGANIZATION_FIELDS",
                    ("Organisationsfelder auflisten", "List organization fields"),
                    (
                        "Listet die Organisationsfelder inkl. Custom-Field-Keys und Optionen.",
                        "Lists organization fields including custom field keys and options.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                // ---------------------------------------------------------- tickets: write
                t(
                    "ZENDESK_CREATE_TICKET",
                    ("Ticket anlegen", "Create ticket"),
                    (
                        "Legt ein Ticket mit Eröffnungskommentar an; braucht --confirm.",
                        "Creates a ticket with its opening comment; requires --confirm.",
                    ),
                    &[
                        ("comment_body", s("Eröffnungskommentar als Text (Pflicht, wenn kein HTML)")),
                        ("comment_html_body", s("Eröffnungskommentar als HTML; hat Vorrang vor comment_body")),
                        ("comment_public", flag("Kommentar öffentlich (Standard true)")),
                        ("subject", s("Betreff; leer = erster Kommentar")),
                        ("requester_email", s("E-Mail der anfragenden Person; Zendesk legt sie bei Bedarf an")),
                        ("requester_name", s("Name der anfragenden Person (mit requester_email)")),
                        ("requester_id", id_prop("ID der anfragenden Person (statt E-Mail)")),
                        ("assignee_id", id_prop("ID des Agenten (ZENDESK_FIND_USER_BY_EMAIL)")),
                        ("collaborator_ids", id_list_prop("IDs der Collaborator")),
                        ("follower_ids", id_list_prop("IDs der Follower")),
                        ("priority", choice("Priorität", &["low", "normal", "high", "urgent"])),
                        ("type", choice("Typ", &["problem", "incident", "question", "task"])),
                        ("status", choice("Status", &["new", "open", "pending", "hold", "solved", "closed"])),
                        ("tags", str_list("Tags des Tickets")),
                        ("organization_id", id_prop("Organisations-ID")),
                        ("group_id", id_prop("Gruppen-ID")),
                        ("external_id", s("External ID für Integrationen")),
                        ("due_at", s("Fällig am (ISO 8601)")),
                        ("brand_id", id_prop("Brand-ID")),
                        ("forum_topic_id", s("Forum-Topic-ID")),
                        ("problem_id", id_prop("ID des Problem-Tickets")),
                        ("custom_fields", object_list_prop("Custom-Felder als [{id, value}] (siehe ZENDESK_LIST_TICKET_FIELDS)")),
                    ],
                    &[],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_UPDATE_TICKET",
                    ("Ticket ändern", "Update ticket"),
                    (
                        "Ändert nur die übergebenen Ticket-Felder; tags ersetzt alle Tags; braucht --confirm.",
                        "Changes only the ticket fields you pass; tags replaces all tags; requires --confirm.",
                    ),
                    &[
                        ("ticket_id", id_prop("Ticket-ID")),
                        ("subject", s("Neuer Betreff")),
                        ("comment_body", s("Angehängter Kommentar als Text")),
                        ("comment_html_body", s("Angehängter Kommentar als HTML; hat Vorrang")),
                        ("comment_public", flag("Angehängter Kommentar öffentlich (Standard true)")),
                        ("priority", choice("Priorität", &["low", "normal", "high", "urgent"])),
                        ("type", choice("Typ", &["problem", "incident", "question", "task"])),
                        ("status", choice("Status", &["new", "open", "pending", "hold", "solved", "closed"])),
                        ("tags", str_list("Ersetzt alle Tags des Tickets")),
                        ("assignee_id", id_prop("ID des Agenten")),
                        ("requester_id", id_prop("ID der anfragenden Person")),
                        ("collaborator_ids", id_list_prop("Ersetzt alle Collaborator")),
                        ("follower_ids", id_list_prop("Ersetzt alle Follower")),
                        ("organization_id", id_prop("Organisations-ID")),
                        ("group_id", id_prop("Gruppen-ID")),
                        ("external_id", s("External ID")),
                        ("due_at", s("Fällig am (ISO 8601)")),
                        ("custom_status_id", s("ID eines benutzerdefinierten Status")),
                        ("brand_id", id_prop("Brand-ID")),
                        ("forum_topic_id", s("Forum-Topic-ID")),
                        ("problem_id", id_prop("ID des Problem-Tickets")),
                        ("safe_update", flag("Kollisionen über updated_stamp verhindern")),
                        ("updated_stamp", s("updated_at des Tickets; Pflicht bei safe_update")),
                        ("custom_fields", object_list_prop("Custom-Felder als [{id, value}]")),
                    ],
                    &["ticket_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_ADD_COMMENT_TO_TICKET",
                    ("Kommentar zum Ticket hinzufügen", "Add comment to ticket"),
                    (
                        "Hängt einen öffentlichen oder privaten Kommentar an; braucht --confirm.",
                        "Appends a public or private comment; requires --confirm.",
                    ),
                    &[
                        ("ticket_id", id_prop("Ticket-ID")),
                        ("comment_body", s("Kommentar als Text (Pflicht, wenn kein HTML)")),
                        ("comment_html_body", s("Kommentar als HTML; hat Vorrang vor comment_body")),
                        ("public", flag("Öffentlich (Standard true); false = interner Kommentar")),
                        ("author_id", id_prop("ID des Autors (ZENDESK_FIND_USER_BY_EMAIL); leer = angemeldeter Nutzer")),
                        ("uploads", str_list("Upload-Tokens für Anhänge")),
                        ("via_followup_source_id", s("ID des Ursprungstickets bei Follow-up")),
                    ],
                    &["ticket_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_ADD_TAG_TO_TICKET",
                    ("Tags zum Ticket hinzufügen", "Add tags to ticket"),
                    (
                        "Ergänzt Tags, ohne die vorhandenen zu entfernen; braucht --confirm.",
                        "Adds tags without removing the existing ones; requires --confirm.",
                    ),
                    &[
                        ("ticket_id", id_prop("Ticket-ID")),
                        ("tags", str_list("Tags, die hinzugefügt werden")),
                        ("safe_update", flag("Kollisionen über updated_stamp verhindern")),
                        ("updated_stamp", s("updated_at des Tickets; Pflicht bei safe_update")),
                    ],
                    &["ticket_id", "tags"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_REMOVE_TAG_FROM_TICKET",
                    ("Tags vom Ticket entfernen", "Remove tags from ticket"),
                    (
                        "Entfernt einzelne Tags, der Rest bleibt; braucht --confirm.",
                        "Removes single tags and keeps the rest; requires --confirm.",
                    ),
                    &[
                        ("ticket_id", id_prop("Ticket-ID")),
                        ("tags", str_list("Tags, die entfernt werden")),
                    ],
                    &["ticket_id", "tags"],
                    &[WRITE],
                    true,
                ),
                // ---------------------------------------------------------- users: write
                t(
                    "ZENDESK_CREATE_USER",
                    ("Benutzer anlegen", "Create user"),
                    (
                        "Legt einen Benutzer an (Standard Rolle end-user); braucht --confirm.",
                        "Creates a user (role defaults to end-user); requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des Benutzers")),
                        ("email", s("E-Mail des Benutzers")),
                        ("role", choice("Rolle; leer = end-user", &["end-user", "agent", "admin"])),
                        ("custom_role_id", id_prop("ID einer Custom Role; erzwingt Rolle agent")),
                        ("organization_id", id_prop("Organisations-ID")),
                        ("organization_name", s("Name einer neuen Organisation (statt ID)")),
                        ("phone", s("Telefonnummer")),
                        ("alias", s("Alias für Endnutzer")),
                        ("details", s("Details zum Benutzer")),
                        ("notes", s("Interne Notizen")),
                        ("external_id", s("External ID")),
                        ("time_zone", s("Zeitzone, z. B. Europe/Berlin")),
                        ("locale", s("Locale, z. B. de-DE")),
                        ("signature", s("Signatur für E-Mail-Antworten")),
                        ("ticket_restriction", choice("Ticket-Zugriff", &["organization", "groups", "assigned", "requested"])),
                        ("default_group_id", id_prop("Standard-Gruppen-ID")),
                        ("agent_brand_ids", id_list_prop("Brand-IDs, auf die der Agent zugreifen darf")),
                        ("tags", str_list("Tags des Benutzers")),
                        ("user_fields", custom_fields_prop()),
                        ("identities", object_list_prop("Weitere Identitäten [{type, value}]")),
                        ("verified", flag("E-Mail verifiziert")),
                        ("active", flag("Aktiv")),
                        ("suspended", flag("Gesperrt")),
                        ("shared", flag("Aus einer anderen Instanz geteilt")),
                        ("shared_agent", flag("Shared Agent")),
                        ("moderator", flag("Forum-Moderator")),
                        ("restricted_agent", flag("Eingeschränkter Agent")),
                        ("only_private_comments", flag("Nur private Kommentare")),
                        ("report_csv", flag("Zugriff auf CSV-Berichte")),
                        ("skip_verify_email", flag("Keine Bestätigungs-E-Mail senden")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_UPDATE_USER",
                    ("Benutzer ändern", "Update user"),
                    (
                        "Ändert nur die übergebenen Benutzerfelder; tags ersetzt alle Tags; braucht --confirm.",
                        "Changes only the user fields you pass; tags replaces all tags; requires --confirm.",
                    ),
                    &[
                        ("user_id", id_prop("Benutzer-ID")),
                        ("name", s("Neuer Name")),
                        ("email", s("Neue E-Mail")),
                        ("phone", s("Neue Telefonnummer")),
                        ("tags", str_list("Ersetzt alle Tags des Benutzers")),
                        ("user_fields", custom_fields_prop()),
                    ],
                    &["user_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_DELETE_USER",
                    ("Benutzer löschen", "Delete user"),
                    (
                        "Löscht einen Benutzer endgültig aus der Instanz; braucht --confirm.",
                        "Deletes a user from the instance; requires --confirm.",
                    ),
                    &[
                        ("user_id", id_prop("Benutzer-ID")),
                        ("confirmation", flag("Bestätigung, dass das Löschen nicht rückgängig gemacht werden kann")),
                    ],
                    &["user_id", "confirmation"],
                    &[WRITE],
                    true,
                ),
                // ---------------------------------------------------------- organizations: write
                t(
                    "ZENDESK_CREATE_ORGANIZATION",
                    ("Organisation anlegen", "Create organization"),
                    (
                        "Legt eine Organisation an; der Name muss eindeutig sein; braucht --confirm.",
                        "Creates an organization; the name must be unique; requires --confirm.",
                    ),
                    &[
                        ("name", s("Eindeutiger Name der Organisation")),
                        ("details", s("Details zur Organisation")),
                        ("notes", s("Interne Notizen")),
                        ("external_id", s("External ID")),
                        ("group_id", id_prop("Standard-Gruppen-ID")),
                        ("domain_names", str_list("Domains der Organisation")),
                        ("tags", str_list("Tags der Organisation")),
                        ("organization_fields", custom_fields_prop()),
                        ("shared_tickets", flag("Tickets der Nutzer gegenseitig sichtbar")),
                        ("shared_comments", flag("Kommentare der Nutzer gegenseitig sichtbar")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "ZENDESK_UPDATE_ORGANIZATION",
                    ("Organisation ändern", "Update organization"),
                    (
                        "Ändert nur die übergebenen Organisationsfelder; braucht --confirm.",
                        "Changes only the organization fields you pass; requires --confirm.",
                    ),
                    &[
                        ("organization_id", id_prop("Organisations-ID")),
                        ("name", s("Neuer eindeutiger Name")),
                        ("details", s("Neue Details")),
                        ("notes", s("Neue interne Notizen")),
                        ("external_id", s("External ID")),
                        ("group_id", id_prop("Standard-Gruppen-ID")),
                        ("domain_names", str_list("Ersetzt alle Domains")),
                        ("tags", str_list("Ersetzt alle Tags")),
                        ("organization_fields", custom_fields_prop()),
                        ("shared_tickets", flag("Tickets der Nutzer gegenseitig sichtbar")),
                        ("shared_comments", flag("Kommentare der Nutzer gegenseitig sichtbar")),
                    ],
                    &["organization_id"],
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

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    // -------------------------------------------------------------- request builders

    /// Zendesk search query of `find-tickets` / `find-user` / `find-organization`.
    fn search_query(&self, tool: &str, args: &Value) -> Result<String> {
        let kind = req_str(args, "search_type")?;
        let mut q = match tool {
            "ZENDESK_FIND_TICKETS" => "type:ticket".to_string(),
            "ZENDESK_FIND_USER" => "type:user".to_string(),
            "ZENDESK_FIND_ORGANIZATION" => "type:organization".to_string(),
            other => bail!("unknown Zendesk tool `{other}`"),
        };
        // Both helpers trim like the piece's template literals do.
        let term = match (tool, kind) {
            (_, "custom") => req_str(args, "custom_query")?.trim().to_string(),
            ("ZENDESK_FIND_TICKETS", "id") => format!(" {}", req_str(args, "ticket_id")?.trim()),
            ("ZENDESK_FIND_TICKETS", "content") => format!(" {}", req_str(args, "content")?.trim()),
            ("ZENDESK_FIND_TICKETS", other @ ("status" | "priority" | "tag" | "requester" | "assignee")) => {
                let key = match other {
                    "status" => "status",
                    "priority" => "priority",
                    "tag" => "tag",
                    "requester" => "requester_email",
                    _ => "assignee_email",
                };
                format!(" {}:{}", field_of(kind), req_str(args, key)?.trim())
            }
            ("ZENDESK_FIND_TICKETS", "type") => format!(" ticket_type:{}", req_str(args, "ticket_type")?.trim()),
            ("ZENDESK_FIND_USER", "email") => format!(" email:{}", req_str(args, "email")?.trim()),
            ("ZENDESK_FIND_USER", "role") => format!(" role:{}", req_str(args, "role")?.trim()),
            ("ZENDESK_FIND_USER", "tag") => format!(" tags:{}", req_str(args, "tag")?.trim()),
            ("ZENDESK_FIND_USER", "external_id") => format!(" external_id:{}", req_str(args, "external_id")?.trim()),
            ("ZENDESK_FIND_USER", "name") => format!(" name:\"{}\"", req_str(args, "name")?.trim()),
            ("ZENDESK_FIND_USER", "organization") => {
                format!(" organization:\"{}\"", req_str(args, "organization")?.trim())
            }
            ("ZENDESK_FIND_ORGANIZATION", "domain") => format!(" domain:{}", req_str(args, "domain")?.trim()),
            ("ZENDESK_FIND_ORGANIZATION", "external_id") => {
                format!(" external_id:{}", req_str(args, "external_id")?.trim())
            }
            ("ZENDESK_FIND_ORGANIZATION", "tag") => format!(" tags:{}", req_str(args, "tag")?.trim()),
            ("ZENDESK_FIND_ORGANIZATION", "name") => format!(" name:\"{}\"", req_str(args, "name")?.trim()),
            ("ZENDESK_FIND_ORGANIZATION", "details") => {
                format!(" details:\"{}\"", req_str(args, "details")?.trim())
            }
            (_, unknown) => bail!("`search_type` {unknown:?} is not supported by this tool"),
        };
        if kind == "custom" {
            // find-tickets replaces the whole query, the other two prefix `type:…`.
            q = term;
        } else {
            q.push_str(&term);
        }
        if tool == "ZENDESK_FIND_TICKETS" {
            q.push_str(&date_filters(args)?);
        }
        Ok(q)
    }

    /// `page[size]`-free `sort_by` / `sort_order` pair (`URLSearchParams` of the piece).
    fn sort_params(&self, args: &Value) -> Vec<(&'static str, Option<String>)> {
        let sort_by = opt_str(args, "sort_by").filter(|v| *v != "relevance");
        let sort_order = opt_str(args, "sort_order");
        vec![
            ("sort_by", sort_by.map(String::from)),
            ("sort_order", sort_order.map(String::from)),
        ]
    }

    #[allow(clippy::too_many_lines)]
    fn build(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        match tool {
            // AP: src/lib/common/props.ts (ticketIdDropdown / problemTicketIdDropdown)
            "ZENDESK_LIST_TICKETS" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/tickets.json{}",
                    query(&[("per_page", Some(opt_u64(args, "per_page", 100, 1, 100).to_string()))])
                )),
            )),
            // AP: src/lib/actions/find-tickets.ts (`${base}/search.json?${searchParams}`)
            "ZENDESK_FIND_TICKETS" | "ZENDESK_FIND_USER" | "ZENDESK_FIND_ORGANIZATION" => {
                let mut params = vec![("query", Some(self.search_query(tool, args)?))];
                params.extend(self.sort_params(args));
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!("/search.json{}", query(&params))),
                ))
            }
            // AP: src/lib/actions/find-latest-comment.ts (`/tickets/${ticket_id}/comments?sort_order=desc`)
            "ZENDESK_FIND_LATEST_COMMENT" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/tickets/{}/comments{}",
                    numeric(args, "ticket_id")?,
                    query(&[("sort_order", Some("desc".into()))])
                )),
            )),
            // AP: src/lib/actions/create-ticket.ts (`/users/search.json?query=email:${…}`)
            "ZENDESK_FIND_USER_BY_EMAIL" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/users/search.json{}",
                    query(&[(
                        "query",
                        Some(format!("email:{}", req_str(args, "email")?.trim())),
                    )])
                )),
            )),
            // AP: src/lib/actions/find-agent.ts (`/users.json?role=agent&per_page=100`)
            "ZENDESK_LIST_AGENTS" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/users.json{}",
                    query(&[
                        ("role", Some("agent".into())),
                        ("per_page", Some(opt_u64(args, "per_page", 100, 1, 100).to_string())),
                    ])
                )),
            )),
            // AP: src/lib/actions/find-group.ts (`/groups.json?per_page=100`)
            "ZENDESK_LIST_GROUPS" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/groups.json{}",
                    query(&[("per_page", Some(opt_u64(args, "per_page", 100, 1, 100).to_string()))])
                )),
            )),
            // AP: src/lib/common/props.ts (userIdDropdown: `/users.json?per_page=100`)
            "ZENDESK_LIST_USERS" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/users.json{}",
                    query(&[("per_page", Some(opt_u64(args, "per_page", 100, 1, 100).to_string()))])
                )),
            )),
            // AP: src/lib/common/props.ts (organizationIdDropdown: `/organizations.json?per_page=100`)
            "ZENDESK_LIST_ORGANIZATIONS" => Ok(HttpRequest::new(
                "GET",
                self.url(&format!(
                    "/organizations.json{}",
                    query(&[("per_page", Some(opt_u64(args, "per_page", 100, 1, 100).to_string()))])
                )),
            )),
            // AP: src/lib/common/props.ts (brandIdDropdown: `/brands.json`)
            "ZENDESK_LIST_BRANDS" => Ok(HttpRequest::new("GET", self.url("/brands.json"))),
            // AP: src/lib/common/props.ts (customRoleIdDropdown: `/custom_roles.json`)
            "ZENDESK_LIST_CUSTOM_ROLES" => {
                Ok(HttpRequest::new("GET", self.url("/custom_roles.json")))
            }
            // AP: src/lib/actions/create-ticket.ts (`/ticket_fields.json`)
            "ZENDESK_LIST_TICKET_FIELDS" => {
                Ok(HttpRequest::new("GET", self.url("/ticket_fields.json")))
            }
            // AP: src/lib/actions/create-user.ts (`/user_fields.json`)
            "ZENDESK_LIST_USER_FIELDS" => Ok(HttpRequest::new("GET", self.url("/user_fields.json"))),
            // AP: src/lib/actions/create-organization.ts (`/organization_fields.json`)
            "ZENDESK_LIST_ORGANIZATION_FIELDS" => {
                Ok(HttpRequest::new("GET", self.url("/organization_fields.json")))
            }

            // AP: src/lib/actions/create-ticket.ts (`POST ${base}/tickets.json`, body `{ticket}`)
            "ZENDESK_CREATE_TICKET" => {
                let ticket = self.ticket_body(args, true)?;
                Ok(HttpRequest::new("POST", self.url("/tickets.json"))
                    .json(&json!({"ticket": Value::Object(ticket)})))
            }
            // AP: src/lib/actions/update-ticket.ts (`PUT ${base}/tickets/${ticket_id}.json`, body `{ticket}`)
            "ZENDESK_UPDATE_TICKET" => {
                let ticket = self.ticket_body(args, false)?;
                Ok(HttpRequest::new(
                    "PUT",
                    self.url(&format!("/tickets/{}.json", numeric(args, "ticket_id")?)),
                )
                .json(&json!({"ticket": Value::Object(ticket)})))
            }
            // AP: src/lib/actions/add-comment-to-ticket.ts (`PUT ${base}/tickets/${ticket_id}.json`, body `{ticket:{comment}}`)
            "ZENDESK_ADD_COMMENT_TO_TICKET" => {
                let mut comment = Map::new();
                comment_body(&mut comment, args, "comment_html_body", "comment_body")?;
                // `comment.public = isPublic !== false` — always sent, default true.
                comment.insert(
                    "public".into(),
                    json!(args.get("public").and_then(Value::as_bool).unwrap_or(true)),
                );
                if let Some(v) = opt_numeric(args, "author_id")? {
                    comment.insert("author_id".into(), json!(v));
                }
                let uploads = str_vec(args, "uploads");
                if !uploads.is_empty() {
                    comment.insert("uploads".into(), json!(uploads));
                }
                if let Some(v) = opt_str(args, "via_followup_source_id") {
                    // Property.Number in the piece: sent as a JSON number.
                    let source: u64 = v
                        .trim()
                        .parse()
                        .map_err(|_| anyhow!("`via_followup_source_id` must be a numeric ticket id, got \"{v}\""))?;
                    comment.insert("via".into(), json!({"followup_source_id": source}));
                }
                Ok(HttpRequest::new(
                    "PUT",
                    self.url(&format!("/tickets/{}.json", numeric(args, "ticket_id")?)),
                )
                .json(&json!({"ticket": {"comment": Value::Object(comment)}})))
            }
            // AP: src/lib/actions/add-tag-to-ticket.ts (`PUT ${base}/tickets/${ticket_id}/tags.json`)
            "ZENDESK_ADD_TAG_TO_TICKET" => {
                let tags = tags_arg(args)?;
                let mut body = Map::new();
                body.insert("tags".into(), json!(tags));
                let safe = args.get("safe_update").and_then(Value::as_bool).unwrap_or(false);
                if safe {
                    let stamp = req_str(args, "updated_stamp")?;
                    body.insert("updated_stamp".into(), json!(stamp));
                    // The piece sends the string "true" here, not a JSON boolean.
                    body.insert("safe_update".into(), json!("true"));
                }
                Ok(HttpRequest::new(
                    "PUT",
                    self.url(&format!(
                        "/tickets/{}/tags.json",
                        numeric(args, "ticket_id")?
                    )),
                )
                .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/remove-tag-from-ticket.ts (`DELETE ${base}/tickets/${ticket_id}/tags.json`)
            "ZENDESK_REMOVE_TAG_FROM_TICKET" => {
                let tags = tags_arg(args)?;
                Ok(HttpRequest::new(
                    "DELETE",
                    self.url(&format!(
                        "/tickets/{}/tags.json",
                        numeric(args, "ticket_id")?
                    )),
                )
                .json(&json!({"tags": tags})))
            }

            // AP: src/lib/actions/create-user.ts (`POST ${base}/users.json`, body `{user, skip_verify_email}`)
            "ZENDESK_CREATE_USER" => {
                let mut user = Map::new();
                user.insert("name".into(), json!(req_str(args, "name")?.trim()));
                put_str(&mut user, args, "email", "email");
                let role = opt_str(args, "role").unwrap_or("");
                match (role.is_empty(), opt_numeric(args, "custom_role_id")?) {
                    (false, Some(cr)) => {
                        user.insert("role".into(), json!(role));
                        user.insert("custom_role_id".into(), json!(cr));
                    }
                    (false, None) => {
                        user.insert("role".into(), json!(role));
                    }
                    (true, Some(cr)) => {
                        user.insert("role".into(), json!("agent"));
                        user.insert("custom_role_id".into(), json!(cr));
                    }
                    (true, None) => {}
                }
                if opt_numeric(args, "organization_id")?.is_some() {
                    put_id(&mut user, args, "organization_id", "organization_id")?;
                } else if let Some(name) = opt_str(args, "organization_name") {
                    user.insert("organization".into(), json!({"name": name}));
                }
                for key in [
                    "phone",
                    "alias",
                    "details",
                    "notes",
                    "external_id",
                    "time_zone",
                    "locale",
                    "signature",
                    "ticket_restriction",
                ] {
                    put_str(&mut user, args, key, key);
                }
                put_id(&mut user, args, "default_group_id", "default_group_id")?;
                opt_id_list(&mut user, args, "agent_brand_ids", "agent_brand_ids")?;
                put_list(&mut user, args, "tags", "tags");
                for key in [
                    "verified",
                    "active",
                    "shared",
                    "shared_agent",
                    "moderator",
                    "suspended",
                    "restricted_agent",
                    "only_private_comments",
                    "report_csv",
                ] {
                    put_flag(&mut user, args, key, key);
                }
                put_field_map(&mut user, args, "user_fields", "user_fields");
                if let Some(Value::Array(list)) = args.get("identities") {
                    for entry in list {
                        let Some(o) = entry.as_object() else {
                            bail!("each `identities` entry must be an object");
                        };
                        if o.get("type").is_none() || o.get("value").is_none() {
                            bail!("each `identities` entry needs both `type` and `value`");
                        }
                    }
                    if !list.is_empty() {
                        user.insert("identities".into(), Value::Array(list.clone()));
                    }
                }
                let mut body = Map::new();
                body.insert("user".into(), Value::Object(user));
                if args
                    .get("skip_verify_email")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    body.insert("skip_verify_email".into(), json!(true));
                }
                Ok(HttpRequest::new("POST", self.url("/users.json")).json(&Value::Object(body)))
            }
            // AP: src/lib/actions/update-user.ts (`PUT ${base}/users/${user_id}.json`, body `{user}`)
            "ZENDESK_UPDATE_USER" => {
                let mut user = Map::new();
                put_str(&mut user, args, "name", "name");
                put_str(&mut user, args, "email", "email");
                put_str(&mut user, args, "phone", "phone");
                put_list(&mut user, args, "tags", "tags");
                put_field_map(&mut user, args, "user_fields", "user_fields");
                if user.is_empty() {
                    bail!("no fields provided to update; pass at least one field to modify");
                }
                Ok(HttpRequest::new(
                    "PUT",
                    self.url(&format!("/users/{}.json", numeric(args, "user_id")?)),
                )
                .json(&json!({"user": Value::Object(user)})))
            }
            // AP: src/lib/actions/delete-user.ts (`DELETE ${base}/users/${user_id}.json`)
            "ZENDESK_DELETE_USER" => {
                if !args
                    .get("confirmation")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    bail!("you must confirm that you understand this action cannot be undone");
                }
                Ok(HttpRequest::new(
                    "DELETE",
                    self.url(&format!("/users/{}.json", numeric(args, "user_id")?)),
                ))
            }

            // AP: src/lib/actions/create-organization.ts (`POST ${base}/organizations.json`, body `{organization}`)
            "ZENDESK_CREATE_ORGANIZATION" => {
                let mut org = Map::new();
                org.insert("name".into(), json!(req_str(args, "name")?.trim()));
                self.organization_fields(&mut org, args)?;
                Ok(HttpRequest::new("POST", self.url("/organizations.json"))
                    .json(&json!({"organization": Value::Object(org)})))
            }
            // AP: src/lib/actions/update-organization.ts (`PUT ${base}/organizations/${organization_id}.json`)
            "ZENDESK_UPDATE_ORGANIZATION" => {
                let mut org = Map::new();
                if let Some(v) = opt_str(args, "name") {
                    org.insert("name".into(), json!(v.trim()));
                }
                self.organization_fields(&mut org, args)?;
                if org.is_empty() {
                    bail!("no fields provided to update; pass at least one field to modify");
                }
                Ok(HttpRequest::new(
                    "PUT",
                    self.url(&format!(
                        "/organizations/{}.json",
                        numeric(args, "organization_id")?
                    )),
                )
                .json(&json!({"organization": Value::Object(org)})))
            }
            other => bail!("unknown Zendesk tool `{other}`"),
        }
    }

    /// `organization` body shared by create-organization / update-organization.
    fn organization_fields(&self, org: &mut Map<String, Value>, args: &Value) -> Result<()> {
        for key in ["details", "notes", "external_id"] {
            put_str(org, args, key, key);
        }
        put_id(org, args, "group_id", "group_id")?;
        put_list(org, args, "domain_names", "domain_names");
        put_list(org, args, "tags", "tags");
        put_flag(org, args, "shared_tickets", "shared_tickets");
        put_flag(org, args, "shared_comments", "shared_comments");
        put_field_map(org, args, "organization_fields", "organization_fields");
        Ok(())
    }

    /// `ticket` body shared by create-ticket / update-ticket.
    fn ticket_body(&self, args: &Value, create: bool) -> Result<Map<String, Value>> {
        let mut t = Map::new();

        // comment (opening comment on create, appended comment on update)
        let has_comment = opt_str(args, "comment_html_body").is_some()
            || opt_str(args, "comment_body").is_some();
        if has_comment {
            let mut comment = Map::new();
            comment_body(&mut comment, args, "comment_html_body", "comment_body")?;
            // `if (comment_public !== undefined) comment.public = comment_public`
            if let Some(v) = args.get("comment_public").and_then(Value::as_bool) {
                comment.insert("public".into(), json!(v));
            }
            t.insert("comment".into(), Value::Object(comment));
        } else if create {
            bail!("either `comment_body` or `comment_html_body` is required");
        }

        put_str(&mut t, args, "subject", "subject");

        // requester: `requester: {email, name}` (create-ticket.ts) or `requester_id`
        let by_email = opt_str(args, "requester_email");
        if by_email.is_some() && opt_str(args, "requester_id").is_some() {
            bail!("pass either `requester_email` or `requester_id`, not both");
        }
        if let Some(email) = by_email {
            let name = opt_str(args, "requester_name").unwrap_or(email);
            t.insert("requester".into(), json!({"email": email, "name": name}));
        }
        put_id(&mut t, args, "requester_id", "requester_id")?;
        put_id(&mut t, args, "assignee_id", "assignee_id")?;
        opt_id_list(&mut t, args, "collaborator_ids", "collaborator_ids")?;
        opt_id_list(&mut t, args, "follower_ids", "follower_ids")?;

        for key in ["priority", "type", "status", "external_id", "due_at"] {
            put_str(&mut t, args, key, key);
        }
        put_list(&mut t, args, "tags", "tags");
        put_id(&mut t, args, "organization_id", "organization_id")?;
        put_id(&mut t, args, "group_id", "group_id")?;
        put_id(&mut t, args, "brand_id", "brand_id")?;
        put_str(&mut t, args, "forum_topic_id", "forum_topic_id");
        put_id(&mut t, args, "problem_id", "problem_id")?;
        put_custom_fields(&mut t, args, "custom_fields")?;

        // update-ticket only (`safe_update` + `updated_stamp`)
        let safe = args.get("safe_update").and_then(Value::as_bool).unwrap_or(false);
        if safe {
            let stamp = req_str(args, "updated_stamp")?;
            t.insert("safe_update".into(), json!(true));
            t.insert("updated_stamp".into(), json!(stamp));
        }
        if opt_str(args, "custom_status_id").is_some() {
            put_str(&mut t, args, "custom_status_id", "custom_status_id");
        }

        if !create && t.is_empty() {
            bail!("no fields provided to update; pass at least one field to modify");
        }
        Ok(t)
    }
}

/// `comment_html_body` wins over `comment_body` (create-ticket.ts / add-comment-to-ticket.ts).
fn comment_body(
    comment: &mut Map<String, Value>,
    args: &Value,
    html_key: &str,
    text_key: &str,
) -> Result<()> {
    if let Some(v) = opt_str(args, html_key) {
        comment.insert("html_body".into(), json!(v));
    } else if let Some(v) = opt_str(args, text_key) {
        comment.insert("body".into(), json!(v));
    } else {
        bail!("either `{html_key}` or `{text_key}` is required");
    }
    Ok(())
}

fn tags_arg(args: &Value) -> Result<Vec<String>> {
    let tags = str_vec(args, "tags");
    if tags.is_empty() {
        bail!("`tags` must contain at least one tag");
    }
    Ok(tags)
}

/// Zendesk search field name for a `find-tickets` search type.
fn field_of(search_type: &str) -> &str {
    match search_type {
        "status" => "status",
        "priority" => "priority",
        "tag" => "tags",
        "requester" => "requester",
        "assignee" => "assignee",
        other => other,
    }
}

/// `dateTimeFilterProp.map(...)` of find-tickets.ts.
fn date_filters(args: &Value) -> Result<String> {
    let Some(Value::Array(list)) = args.get("date_filters") else {
        return Ok(String::new());
    };
    if list.is_empty() {
        return Ok(String::new());
    }
    let mut parts: Vec<String> = Vec::new();
    for entry in list {
        let Some(o) = entry.as_object() else {
            bail!("each `date_filters` entry must be an object with `field`, `operator` and `value`");
        };
        let field = o
            .get("field")
            .and_then(Value::as_str)
            .filter(|v| ["created", "updated", "solved", "due_date"].contains(v))
            .ok_or_else(|| anyhow!("`date_filters[].field` must be created, updated, solved or due_date"))?;
        let operator = o
            .get("operator")
            .and_then(Value::as_str)
            .filter(|v| [":", ">", "<", ">=", "<="].contains(v))
            .ok_or_else(|| anyhow!("`date_filters[].operator` must be :, >, <, >= or <="))?;
        let value = o
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("`date_filters[].value` is required"))?;
        // dayjs(value).format("YYYY-MM-DD")
        let head: String = value.trim().chars().take(10).collect();
        if head.len() != 10
            || !head
                .bytes()
                .enumerate()
                .all(|(i, b)| if matches!(i, 4 | 7) { b == b'-' } else { b.is_ascii_digit() })
        {
            bail!("`date_filters[].value` must be a date in YYYY-MM-DD format, got \"{value}\"");
        }
        parts.push(format!("{field}{operator}{head}"));
    }
    Ok(format!(" {}", parts.join(" ")))
}

impl ToolkitAdapter for Zendesk {
    fn connection_fields(&self) -> Vec<ConnectionField> {
        vec![ConnectionField::new(
            "subdomain",
            ("Zendesk-Subdomain", "Zendesk subdomain"),
            (
                "Der Teil vor .zendesk.com, z. B. „acme“ für https://acme.zendesk.com. Als Zugangsdaten „E-Mail/token:API-Token“ eingeben.",
                "The part before .zendesk.com, e.g. \"acme\" for https://acme.zendesk.com. Enter the credential as \"email/token:API token\".",
            ),
            "acme",
            true,
        )]
    }
    fn connection_api_base(&self, params: &std::collections::BTreeMap<String, String>) -> Result<Option<String>> {
        params
            .get("subdomain")
            .map(|v| Ok(format!("https://{}.zendesk.com/api/v2", host_label(v, "zendesk.com", "subdomain")?)))
            .transpose()
    }
    fn default_api_base(&self) -> Option<String> {
        Some(self.api_base.clone())
    }
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        self.build(tool, args)
    }

    #[allow(clippy::too_many_lines)]
    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let list = |key: &str, map: fn(&Value) -> Value| -> Result<Value> {
            let (items, envelope) = rows(response, key)?;
            Ok(list_value(items, envelope, map))
        };
        let one = |key: &str| -> Result<Value> {
            let v = check(response)?;
            if v[key].is_object() {
                Ok(v[key].clone())
            } else {
                Err(anyhow!("Zendesk returned no `{key}`"))
            }
        };
        // `/search.json` mixes result types; each search tool keeps only its own.
        let search = |result_type: &str, map: fn(&Value) -> Value| -> Result<Value> {
            let (results, envelope) = rows(response, "results")?;
            let items: Vec<Value> = results
                .iter()
                .filter(|r| r["result_type"] == json!(result_type))
                .cloned()
                .collect();
            Ok(list_value(items, envelope, map))
        };
        Ok(match tool {
            "ZENDESK_LIST_TICKETS" => list("tickets", flatten_ticket)?,
            "ZENDESK_LIST_USERS" => list("users", flatten_user)?,
            "ZENDESK_LIST_AGENTS" => list("users", flatten_user)?,
            "ZENDESK_LIST_ORGANIZATIONS" => list("organizations", flatten_organization)?,
            "ZENDESK_LIST_GROUPS" => list("groups", flatten_group)?,
            "ZENDESK_LIST_BRANDS" => list("brands", flatten_brand)?,
            "ZENDESK_LIST_CUSTOM_ROLES" => list("custom_roles", flatten_custom_role)?,
            "ZENDESK_LIST_TICKET_FIELDS" => list("ticket_fields", flatten_field)?,
            "ZENDESK_LIST_USER_FIELDS" => list("user_fields", flatten_field)?,
            "ZENDESK_LIST_ORGANIZATION_FIELDS" => list("organization_fields", flatten_field)?,
            "ZENDESK_FIND_TICKETS" => search("ticket", flatten_ticket)?,
            "ZENDESK_FIND_USER" => search("user", flatten_user)?,
            "ZENDESK_FIND_ORGANIZATION" => search("organization", flatten_organization)?,
            // AP: src/lib/actions/find-latest-comment.ts — comments[0] is the newest
            // because the request asks for `sort_order=desc`.
            "ZENDESK_FIND_LATEST_COMMENT" => {
                let (comments, _) = rows(response, "comments")?;
                let latest = comments
                    .first()
                    .ok_or_else(|| anyhow!("no comments found on ticket"))?;
                let public = comments
                    .iter()
                    .find(|c| c["public"] == json!(true))
                    .ok_or_else(|| anyhow!("no public comments found on ticket"))?;
                json!({"success": true,
                    "comment": flatten_comment(latest),
                    "public_comment": flatten_comment(public),
                    "count": comments.len()})
            }
            // AP: src/lib/actions/create-ticket.ts / update-ticket.ts (`resolveUserByEmail`)
            "ZENDESK_FIND_USER_BY_EMAIL" => {
                let (users, _) = rows(response, "users")?;
                json!({
                    "success": true,
                    "count": users.len(),
                    "user_id": users.first().map(|u| u["id"].clone()).unwrap_or(Value::Null),
                    "users": users.iter().map(flatten_user).collect::<Vec<_>>(),
                })
            }
            // AP: src/lib/actions/create-ticket.ts — POST `${base}/tickets.json`
            "ZENDESK_CREATE_TICKET" => json!({
                "success": true,
                "message": "Ticket created successfully",
                "ticket": flatten_ticket(&one("ticket")?),
            }),
            "ZENDESK_UPDATE_TICKET" => json!({
                "success": true,
                "message": "Ticket updated successfully",
                "ticket": flatten_ticket(&one("ticket")?),
            }),
            "ZENDESK_ADD_COMMENT_TO_TICKET" => {
                let ticket = one("ticket")?;
                json!({
                    "success": true,
                    "message": "Comment added successfully",
                    "ticket": flatten_ticket(&ticket),
                    "comment_details": {
                        "is_public": ticket["comment"]["public"].clone(),
                        "has_attachments": ticket["comment"]["uploads"]
                            .as_array().is_some_and(|a| !a.is_empty()),
                        "content_type": if ticket["comment"]["html_body"].is_string() { "html" } else { "text" },
                    },
                })
            }
            // AP: src/lib/actions/add-tag-to-ticket.ts — returns `{tags: […]}`.
            "ZENDESK_ADD_TAG_TO_TICKET" => {
                let v = check(response)?;
                let tags = v["tags"].as_array().cloned().unwrap_or_default();
                json!({"success": true, "tags": tags, "count": tags.len()})
            }
            // AP: src/lib/actions/remove-tag-from-ticket.ts — Zendesk answers with
            // the tags that are left on the ticket.
            "ZENDESK_REMOVE_TAG_FROM_TICKET" => {
                let v = check(response)?;
                let tags = v["tags"].as_array().cloned().unwrap_or_default();
                json!({"success": true, "tags": tags, "count": tags.len()})
            }
            "ZENDESK_CREATE_USER" => {
                let user = one("user")?;
                json!({
                    "success": true,
                    "message": "User created successfully",
                    "user": flatten_user(&user),
                    "user_role": user["role"].clone(),
                })
            }
            "ZENDESK_UPDATE_USER" => json!({
                "success": true,
                "message": "User updated successfully",
                "user": flatten_user(&one("user")?),
            }),
            // AP: src/lib/actions/delete-user.ts — Zendesk answers with the deleted user.
            "ZENDESK_DELETE_USER" => {
                let v = check(response)?;
                json!({"success": true, "id": v["id"].clone(), "user": flatten_user(&v)})
            }
            "ZENDESK_CREATE_ORGANIZATION" => json!({
                "success": true,
                "message": "Organization created successfully",
                "organization": flatten_organization(&one("organization")?),
            }),
            "ZENDESK_UPDATE_ORGANIZATION" => json!({
                "success": true,
                "message": "Organization updated successfully",
                "organization": flatten_organization(&one("organization")?),
            }),
            other => bail!("unknown Zendesk tool `{other}`"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;

    fn adapter() -> Zendesk {
        Zendesk::with_subdomain("acme")
    }

    fn body(r: &HttpRequest) -> Value {
        serde_json::from_slice(r.body.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn base_url_is_the_piece_base_url() {
        let z = adapter();
        let r = z.build_request("ZENDESK_LIST_TICKETS", &json!({})).unwrap();
        assert_eq!(
            r.url,
            "https://acme.zendesk.com/api/v2/tickets.json?per_page=100"
        );
        assert_eq!(r.method, "GET");
    }

    #[test]
    fn search_query_matches_the_piece() {
        let z = adapter();
        let r = z
            .build_request(
                "ZENDESK_FIND_TICKETS",
                &json!({"search_type": "status", "status": "open", "sort_by": "relevance",
                        "sort_order": "desc"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://acme.zendesk.com/api/v2/search.json?query=type%3Aticket+status%3Aopen&sort_order=desc"
        );
        // `relevance` is never sent, custom replaces the whole query.
        let r = z
            .build_request(
                "ZENDESK_FIND_TICKETS",
                &json!({"search_type": "custom", "custom_query": "type:ticket status:open"}),
            )
            .unwrap();
        assert!(r.url.contains("query=type%3Aticket+status%3Aopen"));
        assert!(!r.url.contains("sort_by"));

        let r = z
            .build_request(
                "ZENDESK_FIND_TICKETS",
                &json!({"search_type": "id", "ticket_id": "42",
                        "date_filters": [{"field": "created", "operator": ">=", "value": "2026-01-02T09:00:00Z"}]}),
            )
            .unwrap();
        assert!(r.url.contains("created%3E%3D2026-01-02"), "{}", r.url);
    }

    #[test]
    fn ticket_bodies_follow_the_piece() {
        let z = adapter();
        let r = z
            .build_request(
                "ZENDESK_CREATE_TICKET",
                &json!({"comment_body": "It broke", "subject": "Crash",
                        "requester_email": "a@acme.test", "organization_id": "36001",
                        "custom_fields": [{"id": 36000, "value": "x"}]}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://acme.zendesk.com/api/v2/tickets.json");
        let b = body(&r);
        assert_eq!(b["ticket"]["comment"]["body"], "It broke");
        assert_eq!(b["ticket"]["requester"], json!({"email": "a@acme.test", "name": "a@acme.test"}));
        assert_eq!(b["ticket"]["organization_id"], 36001);
        assert_eq!(b["ticket"]["custom_fields"], json!([{"id": 36000, "value": "x"}]));

        // create-ticket requires a comment; update-ticket requires a change.
        assert!(z
            .build_request("ZENDESK_CREATE_TICKET", &json!({"subject": "x"}))
            .is_err());
        assert!(z
            .build_request("ZENDESK_UPDATE_TICKET", &json!({"ticket_id": "42"}))
            .is_err());

        let r = z
            .build_request(
                "ZENDESK_UPDATE_TICKET",
                &json!({"ticket_id": "42", "status": "solved", "safe_update": true,
                        "updated_stamp": "2026-10-01T09:00:00Z"}),
            )
            .unwrap();
        let b = body(&r);
        assert_eq!(r.url, "https://acme.zendesk.com/api/v2/tickets/42.json");
        assert_eq!(b["ticket"]["safe_update"], true);
        assert_eq!(b["ticket"]["updated_stamp"], "2026-10-01T09:00:00Z");
    }

    #[test]
    fn tag_actions_match_the_piece() {
        let z = adapter();
        let r = z
            .build_request(
                "ZENDESK_ADD_TAG_TO_TICKET",
                &json!({"ticket_id": "42", "tags": ["vip"], "safe_update": true,
                        "updated_stamp": "2026-10-01T09:00:00Z"}),
            )
            .unwrap();
        assert_eq!(r.method, "PUT");
        assert_eq!(
            r.url,
            "https://acme.zendesk.com/api/v2/tickets/42/tags.json"
        );
        assert_eq!(body(&r)["safe_update"], "true");

        let r = z
            .build_request(
                "ZENDESK_REMOVE_TAG_FROM_TICKET",
                &json!({"ticket_id": "42", "tags": ["vip"]}),
            )
            .unwrap();
        assert_eq!(r.method, "DELETE");
        assert_eq!(body(&r), json!({"tags": ["vip"]}));
        assert!(z
            .build_request("ZENDESK_REMOVE_TAG_FROM_TICKET", &json!({"ticket_id": "42"}))
            .is_err());
    }

    #[test]
    fn ids_are_validated() {
        let z = adapter();
        assert!(z
            .build_request("ZENDESK_DELETE_USER", &json!({"user_id": "../x", "confirmation": true}))
            .is_err());
        assert!(z
            .build_request(
                "ZENDESK_DELETE_USER",
                &json!({"user_id": "12", "confirmation": true})
            )
            .is_ok());
        // confirmation is mandatory
        assert!(z
            .build_request("ZENDESK_DELETE_USER", &json!({"user_id": "12"}))
            .is_err());
    }

    #[test]
    fn provider_errors_are_reported() {
        let z = adapter();
        let resp = HttpResponse {
            status: 422,
            body: serde_json::to_vec(&json!({"error": "RecordInvalid",
                "description": "Record validation errors", "details": {"base": "Email has already been taken"}}))
                .unwrap(),
            session_id: None,
        };
        let err = z.parse_response("ZENDESK_CREATE_USER", &resp).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Zendesk API error 422"), "{msg}");
        assert!(msg.contains("Zendesk rejected the input"), "{msg}");
        assert!(msg.contains("Email has already been taken"), "{msg}");
    }

    #[test]
    fn mappers_and_agents_page() {
        let z = adapter();
        let r = z.build_request("ZENDESK_LIST_AGENTS", &json!({})).unwrap();
        assert_eq!(
            r.url,
            "https://acme.zendesk.com/api/v2/users.json?role=agent&per_page=100"
        );
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"users": [
                {"id": 1, "name": "Ada Lovelace", "email": "ada@acme.test", "role": "agent", "active": true},
                {"id": 2, "name": "Grace Hopper", "email": "grace@acme.test", "role": "agent"}
            ]}))
            .unwrap(),
            session_id: None,
        };
        let out = z.parse_response("ZENDESK_LIST_AGENTS", &resp).unwrap();
        assert_eq!(out["count"], 2);
        assert_eq!(out.pointer("/items/0/email"), Some(&json!("ada@acme.test")));
        assert_eq!(out["has_more"], false);

        let org = flatten_organization(&json!({"id": 1, "name": "Acme",
            "domain_names": ["acme.test", "acme.de"], "tags": ["vip", "eu"]}));
        assert_eq!(org["domain_names"], "acme.test, acme.de");
        assert_eq!(org["tags"], "vip, eu");
    }
}