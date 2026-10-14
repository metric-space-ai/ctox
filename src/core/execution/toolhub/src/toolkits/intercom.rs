//! Intercom toolkit (REST API 2.11). Action set modelled on Activepieces
//! `piece-intercom` (MIT), files under `src/lib/`.
//!
//! Auth (replicated from `src/lib/auth.ts` + `src/lib/common/client.ts`):
//! the OAuth2 connection authorizes against `https://app.{region}.com/oauth`
//! and exchanges the code at `https://api.{region}.io/auth/eagle/token` with an
//! **empty** scope list (the piece declares `scope: []`); the alternative
//! "Access Token" connection is a plain access token. Both are sent as
//! `Authorization: Bearer <token>` — exactly what `intercomRequest()` and the
//! custom-auth validator (`GET /me`) do. Every request additionally carries the
//! piece's `Intercom-Version: 2.11` and `Accept: application/json` headers.
//!
//! Base URL: `getIntercomBaseUrl(region)` in `src/lib/common/client.ts` picks
//! `https://api.intercom.io` (US), `https://api.eu.intercom.io` (EU) or
//! `https://api.au.intercom.io` (AU) from the *connection* property `region`
//! (`intercom` / `eu.intercom` / `au.intercom`). The region belongs to the
//! connection, not to an individual call, so it is not a tool argument here:
//! [`Intercom::default`] uses the real US base URL and [`Intercom::with_region`]
//! selects EU/AU; any other base URL can be injected with
//! [`Intercom::with_api_base`].
//!
//! Every request arm below carries an `// AP:` comment naming the Activepieces
//! file the method, path and body/query fields were taken from. Actions that
//! use the `intercom-client` SDK are expressed as the REST endpoints that SDK
//! wraps. One tool call builds exactly one request, so the two-step actions of
//! the piece are not ported:
//!
//! * search-then-write upserts — `create-or-update-user`, `create-or-update-lead`,
//!   `find-or-create-company`, `find-or-create-lead`, `create_tag`
//!   (all: first `POST /<resource>/search` or `GET /tags`, then a write),
//! * ticket actions that first read `GET /ticket_types/{id}` to convert the
//!   typed attribute values — `create-ticket`, `update-ticket`,
//!   `reply_to_ticket`, `convert_conversation_to_ticket`,
//! * `add-note-to-user` (resolves the contact id by e-mail search first; the
//!   single-request `POST /notes` variant is `INTERCOM_ADD_NOTE_TO_CONTACT`),
//! * the untag halves of `add-or-remove-tag-*`: the piece sends the untag call
//!   through the SDK with an `admin_id` the REST reference does not document
//!   as a body field, so no untag request was ported.
//!
//! `create-article`, `create-data-event` and the lookup-only `find-user` /
//! `find-lead` (both `POST /contacts/search` with a `role` filter, covered by
//! `INTERCOM_SEARCH_CONTACTS`) were left out to stay within the action budget,
//! and the AI-flavoured actions under `src/lib/actions/ai/` are not ported by
//! design.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "intercom";

/// `INTERCOM_API_VERSION` from `src/lib/common/client.ts`.
const API_VERSION: &str = "2.11";

/// `DEFAULT_PER_PAGE` / `MAX_PER_PAGE` from `src/lib/common/lookups.ts`.
const DEFAULT_PER_PAGE: u64 = 50;
const MAX_PER_PAGE: u64 = 150;

/// `REGION_BASE_URLS` from `src/lib/common/client.ts`.
fn region_base_url(region: &str) -> Option<&'static str> {
    match region {
        "intercom" => Some("https://api.intercom.io"),
        "eu.intercom" => Some("https://api.eu.intercom.io"),
        "au.intercom" => Some("https://api.au.intercom.io"),
        _ => None,
    }
}

// ---------------------------------------------------------------- schema helpers

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn obj(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

/// Date props become Unix seconds, like `toUnixSeconds` in `lookups.ts`.
fn date_prop(desc: &str) -> Value {
    json!({"type": "string", "format": "date-time", "description": desc})
}

fn per_page_prop() -> Value {
    json!({"type": "integer", "description": "1 to 150. Default 50.", "minimum": 1, "maximum": 150,
        "default": DEFAULT_PER_PAGE})
}

fn page_prop() -> Value {
    json!({"type": "integer", "description": "Page number, starting at 1.", "minimum": 1, "default": 1})
}

fn starting_after_prop() -> Value {
    s("The next_starting_after value from the previous page.")
}

fn id_prop(desc: &str) -> Value {
    s(desc)
}

// ---------------------------------------------------------------- time helpers

/// `YYYY-MM-DD` or a full ISO 8601 timestamp -> Unix seconds, exactly like
/// `toUnixSeconds` (dayjs `value.unix()`) of the piece. A bare date is read as
/// midnight UTC.
fn unix_seconds(key: &str, v: &str) -> Result<i64> {
    let t = v.trim();
    let bad = || anyhow::anyhow!("`{key}` must be a date (YYYY-MM-DD) or ISO 8601 timestamp, got `{v}`");
    let b = t.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| -> Result<i64> {
        t.get(r).ok_or_else(bad)?.parse::<i64>().map_err(|_| bad())
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    let mut secs = (days_from_civil(y, mo as u32, d as u32) - DAYS_TO_UNIX_EPOCH) * 86_400;
    if b.len() == 10 {
        return Ok(secs);
    }
    if !matches!(b[10], b'T' | b't' | b' ') || b.len() < 19 || b[13] != b':' || b[16] != b':' {
        return Err(bad());
    }
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if h > 23 || mi > 59 || sec > 60 {
        return Err(bad());
    }
    secs += h * 3_600 + mi * 60 + sec.min(59);
    let rest = &t[19..];
    let offset = match rest.as_bytes().first() {
        None => 0,
        Some(b'Z') | Some(b'z') => 0,
        Some(sign @ (b'+' | b'-')) => {
            let (oh, om) = rest[1..].split_once(':').ok_or_else(bad)?;
            let (oh, om) = (oh.parse::<i64>().map_err(|_| bad())?, om.parse::<i64>().map_err(|_| bad())?);
            let v = oh * 3_600 + om * 60;
            if *sign == b'-' {
                v
            } else {
                -v
            }
        }
        Some(_) => return Err(bad()),
    };
    Ok(secs + offset)
}

/// Days between 0000-03-01 (the epoch of [`days_from_civil`]) and 1970-01-01.
const DAYS_TO_UNIX_EPOCH: i64 = 719_468;

/// Days since 0000-03-01 (Howard Hinnant's civil calendar algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    era * 146_097 + (365 * yoe + yoe / 4 - yoe / 100 + doy)
}

// ---------------------------------------------------------------- query builders

fn filt(field: &str, operator: &str, value: Value) -> Value {
    json!({"field": field, "operator": operator, "value": value})
}

/// Optional text filter (`stringFilter` / `equalsFilter` of the piece): an
/// empty or absent prop contributes no filter.
fn text_filter(field: &str, operator: &str, args: &Value, key: &str) -> Option<Value> {
    opt_str(args, key).map(|v| filt(field, operator, json!(v.trim())))
}

fn opt_filter(field: &str, operator: &str, args: &Value, key: &str) -> Option<Value> {
    opt_str(args, key).map(|v| filt(field, operator, json!(v)))
}

fn date_filter(field: &str, args: &Value, key: &str) -> Result<Option<Value>> {
    match opt_str(args, key) {
        Some(v) => Ok(Some(filt(field, ">", json!(unix_seconds(key, v)?)))),
        None => Ok(None),
    }
}

fn date_filter_before(field: &str, args: &Value, key: &str) -> Result<Option<Value>> {
    match opt_str(args, key) {
        Some(v) => Ok(Some(filt(field, "<", json!(unix_seconds(key, v)?)))),
        None => Ok(None),
    }
}

/// `buildSearchQuery` from `lookups.ts`: filters AND-ed with the optional
/// advanced query, or the piece's fallback when nothing is given.
fn search_query(filters: Vec<Value>, fallback: Value, args: &Value) -> Value {
    let mut all = filters;
    match args.get("advanced_query") {
        Some(v) if !v.is_null() => all.push(v.clone()),
        _ => {}
    }
    match all.len() {
        0 => fallback,
        1 => all.pop().unwrap(),
        _ => json!({"operator": "AND", "value": all}),
    }
}

/// `pagination` object of `searchRecords` / `searchContacts`.
fn search_pagination(args: &Value) -> Value {
    let mut p = Map::new();
    p.insert(
        "per_page".into(),
        json!(opt_u64(args, "per_page", DEFAULT_PER_PAGE, 1, MAX_PER_PAGE)),
    );
    if let Some(after) = opt_str(args, "starting_after") {
        p.insert("starting_after".into(), json!(after));
    }
    Value::Object(p)
}

fn search_body(query: Value, args: &Value) -> Value {
    json!({"query": query, "pagination": search_pagination(args)})
}

// ---------------------------------------------------------------- response mappers

/// `toCursorList` from `lookups.ts`.
fn cursor_list(body: &Value, items_key: &str) -> Value {
    let items = body[items_key].as_array().cloned().unwrap_or_default();
    let next = body["pages"]["next"]["starting_after"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from);
    let total = body["total_count"]
        .as_u64()
        .map(|t| json!(t))
        .unwrap_or_else(|| json!(items.len()));
    json!({
        "items": items,
        "total_count": total,
        "has_more": next.is_some(),
        "next_starting_after": next,
    })
}

/// `toPagedList` from `lookups.ts`.
fn paged_list(body: &Value, items_key: &str) -> Value {
    let items = body[items_key].as_array().cloned().unwrap_or_default();
    let page = body["pages"]["page"].as_i64();
    let total_pages = body["pages"]["total_pages"].as_i64();
    let next = match (page, total_pages) {
        (Some(p), Some(t)) if p < t => Some(p + 1),
        _ => None,
    };
    let total = body["total_count"]
        .as_u64()
        .map(|t| json!(t))
        .unwrap_or_else(|| json!(items.len()));
    json!({
        "items": items,
        "total_count": total,
        "has_more": next.is_some(),
        "next_page": next,
    })
}

fn admin(admin: &Value) -> Value {
    json!({
        "id": admin["id"],
        "name": admin["name"].clone(),
        "email": admin["email"].clone(),
        "job_title": admin["job_title"].clone(),
        "away_mode_enabled": admin["away_mode_enabled"] == Value::Bool(true),
        "has_inbox_seat": admin["has_inbox_seat"] == Value::Bool(true),
        "team_ids": admin["team_ids"].clone(),
    })
}

// ---------------------------------------------------------------- adapter

pub struct Intercom {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Intercom {
    fn default() -> Self {
        // `REGION_BASE_URLS['intercom']` — the real US base URL of the piece.
        Self::with_api_base("https://api.intercom.io")
    }
}

impl Intercom {
    /// `Intercom::with_region("eu.intercom")` -> `https://api.eu.intercom.io`,
    /// mirroring `getIntercomBaseUrl` of the piece (which throws for anything
    /// but the three documented region values).
    pub fn with_region(region: &str) -> Result<Self> {
        match region_base_url(region.trim()) {
            Some(base) => Ok(Self::with_api_base(base)),
            None => bail!("unsupported Intercom region `{region}`; pick US, EU or AU"),
        }
    }

    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], mutating: bool| {
            // Intercom's OAuth apps declare no scopes (`scope: []`).
            tool(SLUG, slug, title, desc, props, req, &[], mutating)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Intercom".into(),
            description: LocalizedText::new(
                "Intercom-Kundenservice: Kontakte, Firmen, Unterhaltungen und Tickets suchen, lesen und bearbeiten.",
                "Intercom customer support: search, read and write contacts, companies, conversations and tickets.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    // `{region}` is the piece's own connection property
                    // (`intercom` | `eu.intercom` | `au.intercom`); the hub does
                    // not substitute it, so it is passed through verbatim.
                    authorize_url: "https://app.{region}.com/oauth".into(),
                    token_url: "https://api.{region}.io/auth/eagle/token".into(),
                    // `scope: []` in `src/lib/auth.ts`.
                    default_scopes: Vec::new(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // "Access Token" custom auth: the same `Authorization: Bearer`
                // header as the OAuth token.
                AuthScheme::Bearer,
            ],
            tools: vec![
                // ------------------------------------------------------ read
                t(
                    "INTERCOM_LIST_ADMINS",
                    ("Admins auflisten", "List admins"),
                    (
                        "Listet die Admins (Teammates) des Workspace mit ID, Name, E-Mail und Team-IDs.",
                        "Lists the admins (teammates) in the workspace with ID, name, email and team IDs.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_LIST_TEAMS",
                    ("Teams auflisten", "List teams"),
                    (
                        "Listet die Teams des Workspace mit ID, Name und Admin-IDs.",
                        "Lists the teams in the workspace with ID, name and member admin IDs.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_LIST_TAGS",
                    ("Tags auflisten", "List tags"),
                    (
                        "Listet alle Tags des Workspace; liefert die Tag-IDs zum Taggen von Kontakten und Unterhaltungen.",
                        "List all tags in the workspace to discover tag IDs before tagging contacts or conversations.",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_LIST_TICKET_TYPES",
                    ("Ticket-Typen auflisten", "List ticket types"),
                    (
                        "Listet die Ticket-Typen mit ihren Attributen (Name, Datentyp, Pflicht, Listenoptionen).",
                        "Lists the ticket types with their attributes (name, data type, required, list options).",
                    ),
                    &[],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_LIST_DATA_ATTRIBUTES",
                    ("Datenattribute auflisten", "List data attributes"),
                    (
                        "Listet die Standard- und eigenen Attribute für Kontakte, Firmen oder Unterhaltungen.",
                        "Lists the standard and custom attributes for contacts, companies or conversations.",
                    ),
                    &[
                        (
                            "model",
                            json!({"type": "string", "description": "Objekt: contact, company oder conversation.", "enum": ["contact", "company", "conversation"]}),
                        ),
                        ("include_archived", flag("Archivierte Attribute mitliefern.", false)),
                    ],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_LIST_COMPANIES",
                    ("Firmen auflisten", "List companies"),
                    (
                        "Listet Firmen seitenweise, optional nur mit einem Tag oder in einem Segment.",
                        "Lists companies one page at a time, optionally only those with a tag or in a segment.",
                    ),
                    &[
                        ("tag_id", s("Nur Firmen mit diesem Tag (alternativ zu segment_id).")),
                        ("segment_id", s("Nur Firmen in diesem Segment (alternativ zu tag_id).")),
                        ("page", page_prop()),
                        ("per_page", per_page_prop()),
                    ],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_FIND_COMPANY",
                    ("Firma suchen", "Find company"),
                    (
                        "Sucht eine Firma nach Name oder nach der selbst vergebenen Company ID; found=false, wenn nichts passt.",
                        "Finds a company by name or by your own Company ID; found=false when nothing matches.",
                    ),
                    &[
                        ("search_field", choice("Suchfeld.", &["name", "company_id"])),
                        ("search_value", s("Suchwert")),
                    ],
                    &["search_field", "search_value"],
                    false,
                ),
                t(
                    "INTERCOM_GET_COMPANY",
                    ("Firma abrufen", "Get company"),
                    (
                        "Ruft eine Firma über ihre Intercom-ID ab.",
                        "Gets one company by its Intercom ID.",
                    ),
                    &[("company_id", id_prop("Intercom-ID der Firma."))],
                    &["company_id"],
                    false,
                ),
                t(
                    "INTERCOM_GET_CONTACT",
                    ("Kontakt abrufen", "Get contact"),
                    (
                        "Ruft einen Kontakt (User oder Lead) über seine Intercom-ID ab; für E-Mail-Lookups SEARCH_CONTACTS nutzen.",
                        "Gets one contact (user or lead) by Intercom ID; use SEARCH_CONTACTS to look up by email.",
                    ),
                    &[("contact_id", id_prop("Intercom-ID des Kontakts."))],
                    &["contact_id"],
                    false,
                ),
                t(
                    "INTERCOM_LIST_CONTACT_NOTES",
                    ("Kontaktnotizen auflisten", "List contact notes"),
                    (
                        "Listet die internen Notizen eines Kontakts, neueste zuerst, seitenweise.",
                        "Lists the internal notes on one contact, newest first, one page at a time.",
                    ),
                    &[
                        ("contact_id", id_prop("Intercom-ID des Kontakts.")),
                        ("page", page_prop()),
                        ("per_page", per_page_prop()),
                    ],
                    &["contact_id"],
                    false,
                ),
                t(
                    "INTERCOM_SEARCH_CONTACTS",
                    ("Kontakte suchen", "Search contacts"),
                    (
                        "Findet Kontakte nach E-Mail, Name, Rolle, Tag, Segment, Aktualisierungszeit oder eigenem Attribut.",
                        "Finds contacts by email, name, role, tag, segment, update time or a custom attribute.",
                    ),
                    &[
                        ("email", s("E-Mail ist genau.")),
                        ("name_contains", s("Name enthält.")),
                        ("role", choice("Rolle.", &["user", "lead"])),
                        ("tag_id", s("Tag-ID aus INTERCOM_LIST_TAGS.")),
                        ("segment_id", s("Segment-ID.")),
                        ("updated_after", date_prop("Aktualisiert nach (UTC).")),
                        ("updated_before", date_prop("Aktualisiert vor (UTC).")),
                        ("custom_attribute", s("Name eines eigenen Attributs; benötigt custom_value.")),
                        ("custom_value", s("Wert des eigenen Attributs.")),
                        (
                            "advanced_query",
                            obj("Optionaler Intercom-Filter, z. B. {\"field\": \"name\", \"operator\": \"~\", \"value\": \"Ann\"}; wird mit AND verknüpft."),
                        ),
                        ("per_page", per_page_prop()),
                        ("starting_after", starting_after_prop()),
                    ],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_GET_CONVERSATION",
                    ("Unterhaltung abrufen", "Get conversation"),
                    (
                        "Ruft eine Unterhaltung mit Nachrichten, Teilen, Zuständigkeit und Status ab.",
                        "Retrieves one conversation with its messages, parts, assignee and state.",
                    ),
                    &[("conversation_id", id_prop("Intercom-ID der Unterhaltung."))],
                    &["conversation_id"],
                    false,
                ),
                t(
                    "INTERCOM_FIND_CONVERSATION",
                    ("Unterhaltung suchen", "Find conversation"),
                    (
                        "Sucht die erste passende Unterhaltung nach ID, Betreff, Nachricht, Absender, Zuständigkeit oder Tag.",
                        "Searches for the first matching conversation by ID, subject, body, author, assignee or tag.",
                    ),
                    &[
                        (
                            "search_field",
                            json!({"type": "string", "description": "Suchfeld.", "enum": [
                                "id", "source.subject", "source.body", "source.author.email",
                                "admin_assignee_id", "team_assignee_id", "tag_ids"]}),
                        ),
                        (
                            "match_type",
                            json!({"type": "string", "description": "Trefferart; IDs nur mit equals.", "enum": ["contains", "equals", "starts_with"]}),
                        ),
                        ("search_term", s("Suchbegriff.")),
                        ("state", choice("Optionaler Statusfilter.", &["open", "closed", "snoozed"])),
                        ("updated_after", date_prop("Aktualisiert nach (UTC).")),
                        ("updated_before", date_prop("Aktualisiert vor (UTC).")),
                    ],
                    &["search_field", "match_type", "search_term"],
                    false,
                ),
                t(
                    "INTERCOM_SEARCH_CONVERSATIONS",
                    ("Unterhaltungen durchsuchen", "Search conversations"),
                    (
                        "Findet Unterhaltungen nach Status, Zuständigkeit, Kontakt, Tag, Text oder Datum, seitenweise.",
                        "Finds conversations by state, assignee, contact, tag, text or date, one page at a time.",
                    ),
                    &[
                        ("state", choice("Status.", &["open", "closed", "snoozed"])),
                        ("admin_assignee_id", s("Zuständige Admin-ID.")),
                        ("team_assignee_id", s("Zuständige Team-ID.")),
                        ("contact_id", s("Kontakt-ID.")),
                        ("tag_id", s("Tag-ID aus INTERCOM_LIST_TAGS.")),
                        ("author_email", s("E-Mail des Absenders.")),
                        ("body_contains", s("Wort in der ersten Nachricht.")),
                        ("subject_contains", s("Wort im Betreff.")),
                        ("created_after", date_prop("Erstellt nach (UTC).")),
                        ("created_before", date_prop("Erstellt vor (UTC).")),
                        ("updated_after", date_prop("Aktualisiert nach (UTC).")),
                        ("updated_before", date_prop("Aktualisiert vor (UTC).")),
                        ("advanced_query", obj("Optionaler Intercom-Filter, mit AND verknüpft.")),
                        ("per_page", per_page_prop()),
                        ("starting_after", starting_after_prop()),
                    ],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_GET_TICKET",
                    ("Ticket abrufen", "Get ticket"),
                    (
                        "Ruft ein Ticket über seine API-ID ab (nicht die Inbox-Ticketnummer).",
                        "Gets one ticket by its API ID (not the inbox ticket number shown to teammates).",
                    ),
                    &[("ticket_id", id_prop("Ticket-ID aus Intercom."))],
                    &["ticket_id"],
                    false,
                ),
                t(
                    "INTERCOM_SEARCH_TICKETS",
                    ("Tickets durchsuchen", "Search tickets"),
                    (
                        "Findet Tickets nach Typ, Status, Kontakt, Zuständigkeit, Titel oder Datum, seitenweise.",
                        "Finds tickets by type, state, contact, assignee, title or date, one page at a time.",
                    ),
                    &[
                        ("ticket_type_id", s("Ticket-Typ-ID aus INTERCOM_LIST_TICKET_TYPES.")),
                        (
                            "state",
                            choice("Ticket-Status.", &["submitted", "in_progress", "waiting_on_customer", "resolved"]),
                        ),
                        ("open", choice("Offen oder geschlossen.", &["open", "closed"])),
                        ("contact_id", s("Kontakt-ID.")),
                        ("admin_assignee_id", s("Zuständige Admin-ID.")),
                        ("title_contains", s("Wort im Titel.")),
                        ("created_after", date_prop("Erstellt nach (UTC).")),
                        ("created_before", date_prop("Erstellt vor (UTC).")),
                        ("updated_after", date_prop("Aktualisiert nach (UTC).")),
                        ("updated_before", date_prop("Aktualisiert vor (UTC).")),
                        ("advanced_query", obj("Optionaler Intercom-Filter, mit AND verknüpft.")),
                        ("per_page", per_page_prop()),
                        ("starting_after", starting_after_prop()),
                    ],
                    &[],
                    false,
                ),
                t(
                    "INTERCOM_SEARCH_ARTICLES",
                    ("Help-Center-Artikel suchen", "Search Help Center articles"),
                    (
                        "Sucht Help-Center-Artikel nach einem Stichwort und liefert Titel, Beschreibung, URL und Status.",
                        "Searches Help Center articles for a phrase and returns title, description, URL and state.",
                    ),
                    &[
                        ("phrase", s("Suchbegriff.")),
                        ("state", choice("Artikelstatus; Standard published.", &["published", "draft", "all"])),
                        ("help_center_id", s("Nur bei mehreren Help Centern nötig.")),
                    ],
                    &["phrase"],
                    false,
                ),
                // ------------------------------------------------------ write
                t(
                    "INTERCOM_CREATE_USER",
                    ("User anlegen", "Create user"),
                    (
                        "Legt einen neuen Kontakt mit Rolle user an (legt bei jedem Aufruf erneut an).",
                        "Creates a new user-role contact (always creates a record, so retries may duplicate).",
                    ),
                    &[
                        ("email", s("E-Mail des Kontakts.")),
                        ("name", s("Vollständiger Name.")),
                        ("external_id", s("Eigene User-ID.")),
                        ("created_at", date_prop("Registrierungszeitpunkt (UTC).")),
                        ("custom_attributes", obj("Eigene Attribute als Name/Wert-Objekt.")),
                    ],
                    &["email"],
                    true,
                ),
                t(
                    "INTERCOM_UPDATE_CONTACT",
                    ("Kontakt aktualisieren", "Update contact"),
                    (
                        "Aktualisiert Rolle, E-Mail, Name, Telefon, externe ID, Eigentümer, Abonnement oder eigene Attribute eines Kontakts.",
                        "Updates role, email, name, phone, external ID, owner, email subscription or custom attributes of one contact.",
                    ),
                    &[
                        ("contact_id", id_prop("Intercom-ID des Kontakts.")),
                        ("role", choice("Rolle.", &["user", "lead"])),
                        ("email", s("Neue E-Mail.")),
                        ("name", s("Neuer Name.")),
                        ("phone", s("Neue Telefonnummer.")),
                        ("external_id", s("Neue externe ID.")),
                        ("owner_id", id_prop("Admin-ID des Eigentümers.")),
                        ("unsubscribed", choice("E-Mail-Abonnement.", &["subscribed", "unsubscribed"])),
                        ("custom_attributes", obj("Eigene Attribute; nur die aufgeführten ändern sich.")),
                    ],
                    &["contact_id"],
                    true,
                ),
                t(
                    "INTERCOM_CREATE_OR_UPDATE_COMPANY",
                    ("Firma anlegen oder aktualisieren", "Create or update company"),
                    (
                        "Legt eine Firma an oder aktualisiert die Firma mit der angegebenen Company ID.",
                        "Creates a company, or updates the company that carries the given company_id.",
                    ),
                    &[
                        ("company_id", s("Selbst vergebene Company ID; danach nicht mehr änderbar.")),
                        ("name", s("Name der Firma.")),
                        ("plan", s("Plan, z. B. enterprise.")),
                        ("size", json!({"type": "integer", "description": "Anzahl der Mitarbeiter.", "minimum": 0})),
                        ("website", s("Website der Firma.")),
                        ("industry", s("Branche.")),
                        ("monthly_spend", json!({"type": "integer", "description": "Monatlicher Umsatz.", "minimum": 0})),
                        ("remote_created_at", date_prop("Erstellzeitpunkt bei Ihnen (UTC).")),
                        ("custom_attributes", obj("Eigene Attribute als Name/Wert-Objekt.")),
                    ],
                    &[],
                    true,
                ),
                t(
                    "INTERCOM_CREATE_CONVERSATION",
                    ("Unterhaltung starten", "Create conversation"),
                    (
                        "Startet eine neue Unterhaltung mit einer ersten Nachricht eines Kontakts.",
                        "Starts a new conversation from an existing contact with an initial message.",
                    ),
                    &[
                        ("contact_id", id_prop("Intercom-ID des Kontakts.")),
                        ("contact_type", choice("Rolle des Kontakts.", &["user", "lead"])),
                        ("body", s("Text der ersten Nachricht.")),
                    ],
                    &["contact_id", "contact_type", "body"],
                    true,
                ),
                t(
                    "INTERCOM_REPLY_TO_CONVERSATION",
                    ("Unterhaltung beantworten", "Reply to conversation"),
                    (
                        "Postet eine für den Kunden sichtbare Admin-Antwort in eine Unterhaltung.",
                        "Posts a customer-visible admin reply to an existing conversation.",
                    ),
                    &[
                        ("conversation_id", id_prop("Intercom-ID der Unterhaltung.")),
                        ("admin_id", id_prop("Admin-ID aus INTERCOM_LIST_ADMINS.")),
                        ("body", s("Antworttext.")),
                    ],
                    &["conversation_id", "admin_id", "body"],
                    true,
                ),
                t(
                    "INTERCOM_ADD_NOTE_TO_CONVERSATION",
                    ("Notiz zur Unterhaltung hinzufügen", "Add note to conversation"),
                    (
                        "Hängt eine interne, für den Kunden unsichtbare Admin-Notiz an eine Unterhaltung an.",
                        "Appends an internal admin-only note to a conversation.",
                    ),
                    &[
                        ("conversation_id", id_prop("Intercom-ID der Unterhaltung.")),
                        ("admin_id", id_prop("Admin-ID aus INTERCOM_LIST_ADMINS.")),
                        ("body", s("Notiztext.")),
                    ],
                    &["conversation_id", "admin_id", "body"],
                    true,
                ),
                t(
                    "INTERCOM_ADD_NOTE_TO_CONTACT",
                    ("Notiz zum Kontakt hinzufügen", "Add note to contact"),
                    (
                        "Hängt eine interne Notiz an einen Kontakt an.",
                        "Adds an internal note to a contact.",
                    ),
                    &[
                        ("contact_id", id_prop("Intercom-ID des Kontakts.")),
                        ("body", s("Notiztext.")),
                    ],
                    &["contact_id", "body"],
                    true,
                ),
                t(
                    "INTERCOM_SET_CONVERSATION_STATE",
                    ("Unterhaltung schließen, öffnen oder snoozen", "Close, reopen or snooze conversation"),
                    (
                        "Schließt, öffnet oder vertagt eine Unterhaltung, optional mit Abschlussnachricht.",
                        "Closes, reopens or snoozes a conversation, optionally with a closing message.",
                    ),
                    &[
                        ("conversation_id", id_prop("Intercom-ID der Unterhaltung.")),
                        ("state", choice("Neuer Status.", &["close", "open", "snoozed"])),
                        ("admin_id", id_prop("Handelnde Admin-ID aus INTERCOM_LIST_ADMINS.")),
                        ("snoozed_until", date_prop("Pflicht bei snoozed; Zeitpunkt in der Zukunft.")),
                        ("closing_message", s("Optionale Abschlussnachricht, nur beim Schließen.")),
                    ],
                    &["conversation_id", "state", "admin_id"],
                    true,
                ),
                t(
                    "INTERCOM_ASSIGN_CONVERSATION",
                    ("Unterhaltung zuweisen", "Assign conversation"),
                    (
                        "Weist eine Unterhaltung einem Admin oder Team zu, optional mit Nachricht.",
                        "Assigns a conversation to an admin or team, optionally with a message body.",
                    ),
                    &[
                        ("conversation_id", id_prop("Intercom-ID der Unterhaltung.")),
                        ("admin_id", id_prop("Admin-ID aus INTERCOM_LIST_ADMINS.")),
                        ("assignee_id", id_prop("Admin- oder Team-ID der Zielzuständigkeit.")),
                        ("body", s("Optionale Nachricht.")),
                    ],
                    &["conversation_id", "admin_id", "assignee_id"],
                    true,
                ),
                t(
                    "INTERCOM_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Sendet eine ausgehende Nachricht eines Admins an einen Kontakt (E-Mail oder In-App-Chat).",
                        "Sends a new outbound admin message to a contact (email or in-app chat).",
                    ),
                    &[
                        (
                            "message_type",
                            choice("Nachrichtentyp.", &["email", "inapp"]),
                        ),
                        ("admin_id", id_prop("Absendende Admin-ID aus INTERCOM_LIST_ADMINS.")),
                        ("contact_id", id_prop("Empfänger-Kontakt-ID.")),
                        ("contact_role", choice("Rolle des Empfängers (siehe INTERCOM_GET_CONTACT).", &["user", "lead"])),
                        ("subject", s("Betreff; nur bei message_type=email.")),
                        ("template", choice("E-Mail-Stil; nur bei message_type=email.", &["personal", "plain"])),
                        ("body", s("Nachrichtentext.")),
                        (
                            "create_conversation_without_contact_reply",
                            flag("Inbox-Unterhaltung eröffnen, ohne dass der Kontakt antwortet.", false),
                        ),
                    ],
                    &["message_type", "admin_id", "contact_id", "contact_role", "body"],
                    true,
                ),
                t(
                    "INTERCOM_TAG_CONTACT",
                    ("Kontakt taggen", "Tag contact"),
                    (
                        "Hängt ein vorhandenes Tag an einen Kontakt an.",
                        "Attaches an existing tag to a contact.",
                    ),
                    &[
                        ("contact_id", id_prop("Intercom-ID des Kontakts.")),
                        ("tag_id", id_prop("Tag-ID aus INTERCOM_LIST_TAGS.")),
                    ],
                    &["contact_id", "tag_id"],
                    true,
                ),
                t(
                    "INTERCOM_TAG_CONVERSATION",
                    ("Unterhaltung taggen", "Tag conversation"),
                    (
                        "Hängt ein vorhandenes Tag an eine Unterhaltung an.",
                        "Attaches an existing tag to a conversation.",
                    ),
                    &[
                        ("conversation_id", id_prop("Intercom-ID der Unterhaltung.")),
                        ("tag_id", id_prop("Tag-ID aus INTERCOM_LIST_TAGS.")),
                        ("admin_id", id_prop("Admin-ID aus INTERCOM_LIST_ADMINS.")),
                    ],
                    &["conversation_id", "tag_id"],
                    true,
                ),
            ],
        };
        Intercom {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    /// Every Intercom request carries `Intercom-Version` and `Accept`, like
    /// `intercomRequest`/`sendMeRequest` in `src/lib/common/client.ts`.
    fn req(&self, method: &str, url: String) -> HttpRequest {
        HttpRequest::new(method, url)
            .header("intercom-version", API_VERSION)
            .header("accept", "application/json")
    }
}

impl ToolkitAdapter for Intercom {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/list-admins.ts
            "INTERCOM_LIST_ADMINS" => self.req("GET", self.url("/admins")),
            // AP: src/lib/actions/list-teams.ts
            "INTERCOM_LIST_TEAMS" => self.req("GET", self.url("/teams")),
            // AP: src/lib/actions/list-all-tags.ts (client.tags.list -> GET /tags)
            "INTERCOM_LIST_TAGS" => self.req("GET", self.url("/tags")),
            // AP: src/lib/actions/list-ticket-types.ts + src/lib/common/props.ts
            // (`listTicketTypes` -> GET /ticket_types)
            "INTERCOM_LIST_TICKET_TYPES" => self.req("GET", self.url("/ticket_types")),
            // AP: src/lib/actions/list-data-attributes.ts
            "INTERCOM_LIST_DATA_ATTRIBUTES" => {
                let include_archived = args.get("include_archived") == Some(&json!(true));
                self.req(
                    "GET",
                    self.url(&format!(
                        "/data_attributes{}",
                        query(&[
                            ("model", opt_str(args, "model").map(String::from)),
                            ("include_archived", include_archived.then(|| "true".into())),
                        ])
                    )),
                )
            }
            // AP: src/lib/common/lookups.ts (`listCompanies`) + src/lib/actions/list-companies.ts
            "INTERCOM_LIST_COMPANIES" => {
                let tag_id = opt_str(args, "tag_id").map(String::from);
                let segment_id = opt_str(args, "segment_id").map(String::from);
                if tag_id.is_some() && segment_id.is_some() {
                    bail!("filter by `tag_id` or `segment_id`, not both");
                }
                let page = args.get("page").and_then(Value::as_u64).unwrap_or(1).max(1);
                let per_page = opt_u64(args, "per_page", DEFAULT_PER_PAGE, 1, MAX_PER_PAGE);
                if tag_id.is_some() || segment_id.is_some() {
                    self.req(
                        "GET",
                        self.url(&format!(
                            "/companies{}",
                            query(&[
                                ("tag_id", tag_id),
                                ("segment_id", segment_id),
                                ("page", Some(page.to_string())),
                                ("per_page", Some(per_page.to_string())),
                            ])
                        )),
                    )
                } else {
                    self.req(
                        "POST",
                        self.url(&format!(
                            "/companies/list{}",
                            query(&[
                                ("page", Some(page.to_string())),
                                ("per_page", Some(per_page.to_string())),
                                ("order", Some("desc".into())),
                            ])
                        )),
                    )
                }
            }
            // AP: src/lib/common/lookups.ts (`findCompanyBy`) + src/lib/actions/find-company.ts
            "INTERCOM_FIND_COMPANY" => {
                let field = match req_str(args, "search_field")? {
                    "company_id" => "company_id",
                    _ => "name",
                };
                let value = req_str(args, "search_value")?.trim();
                self.req(
                    "GET",
                    self.url(&format!(
                        "/companies{}",
                        query(&[("per_page", Some("1".into())), (field, Some(value.to_string()))])
                    )),
                )
            }
            // AP: src/lib/actions/get-company.ts
            "INTERCOM_GET_COMPANY" => self.req(
                "GET",
                self.url(&format!("/companies/{}", path_segment(args, "company_id")?)),
            ),
            // AP: src/lib/common/lookups.ts (`resolveContact`) + src/lib/actions/get-contact.ts
            "INTERCOM_GET_CONTACT" => self.req(
                "GET",
                self.url(&format!("/contacts/{}", path_segment(args, "contact_id")?)),
            ),
            // AP: src/lib/actions/list-contact-notes.ts
            "INTERCOM_LIST_CONTACT_NOTES" => self.req(
                "GET",
                self.url(&format!(
                    "/contacts/{}/notes{}",
                    path_segment(args, "contact_id")?,
                    query(&[
                        (
                            "page",
                            Some(args.get("page").and_then(Value::as_u64).unwrap_or(1).max(1).to_string()),
                        ),
                        (
                            "per_page",
                            Some(opt_u64(args, "per_page", DEFAULT_PER_PAGE, 1, MAX_PER_PAGE).to_string()),
                        ),
                    ])
                )),
            ),
            // AP: src/lib/common/lookups.ts (`searchContacts`) + src/lib/actions/search-contacts.ts
            "INTERCOM_SEARCH_CONTACTS" => {
                let custom = opt_str(args, "custom_attribute").map(str::trim);
                if custom.is_some() && opt_str(args, "custom_value").is_none() {
                    bail!("`custom_value` is required together with `custom_attribute`");
                }
                let filters: Vec<Value> = [
                    text_filter("email", "=", args, "email"),
                    text_filter("name", "~", args, "name_contains"),
                    opt_filter("role", "=", args, "role"),
                    text_filter("tag_id", "=", args, "tag_id"),
                    text_filter("segment_id", "=", args, "segment_id"),
                    date_filter("updated_at", args, "updated_after")?,
                    date_filter_before("updated_at", args, "updated_before")?,
                    custom.map(|name| {
                        filt(
                            &format!("custom_attributes.{name}"),
                            "=",
                            json!(req_str(args, "custom_value").unwrap_or_default()),
                        )
                    }),
                ]
                .into_iter()
                .flatten()
                .collect();
                let query = search_query(
                    filters,
                    json!({"field": "role", "operator": "IN", "value": ["user", "lead"]}),
                    args,
                );
                self.req("POST", self.url("/contacts/search"))
                    .json(&search_body(query, args))
            }
            // AP: src/lib/actions/get-conversation.ts (client.conversations.find
            // -> GET /conversations/{conversation_id})
            "INTERCOM_GET_CONVERSATION" => self.req(
                "GET",
                self.url(&format!(
                    "/conversations/{}",
                    path_segment(args, "conversation_id")?
                )),
            ),
            // AP: src/lib/actions/find-conversation.ts (client.conversations.search
            // -> POST /conversations/search)
            "INTERCOM_FIND_CONVERSATION" => {
                let field = req_str(args, "search_field")?;
                let operator = match req_str(args, "match_type")? {
                    "contains" => "~",
                    "starts_with" => "^",
                    _ => "=",
                };
                if operator != "="
                    && matches!(
                        field,
                        "id" | "admin_assignee_id" | "team_assignee_id" | "tag_ids"
                    )
                {
                    bail!("`{field}` can only be matched with equals");
                }
                let term = req_str(args, "search_term")?.trim();
                let mut filters = vec![filt(field, operator, json!(term))];
                filters.extend(opt_filter("state", "=", args, "state"));
                filters.extend(date_filter("updated_at", args, "updated_after")?);
                filters.extend(date_filter_before("updated_at", args, "updated_before")?);
                self.req("POST", self.url("/conversations/search"))
                    .json(&json!({"query": {"operator": "AND", "value": filters}}))
            }
            // AP: src/lib/common/lookups.ts (`searchRecords`) + src/lib/actions/search-conversations.ts
            "INTERCOM_SEARCH_CONVERSATIONS" => {
                let filters: Vec<Value> = [
                    opt_filter("state", "=", args, "state"),
                    text_filter("admin_assignee_id", "=", args, "admin_assignee_id"),
                    text_filter("team_assignee_id", "=", args, "team_assignee_id"),
                    text_filter("contact_ids", "=", args, "contact_id"),
                    text_filter("tag_ids", "=", args, "tag_id"),
                    text_filter("source.author.email", "=", args, "author_email"),
                    text_filter("source.body", "~", args, "body_contains"),
                    text_filter("source.subject", "~", args, "subject_contains"),
                    date_filter("created_at", args, "created_after")?,
                    date_filter_before("created_at", args, "created_before")?,
                    date_filter("updated_at", args, "updated_after")?,
                    date_filter_before("updated_at", args, "updated_before")?,
                ]
                .into_iter()
                .flatten()
                .collect();
                let query = search_query(filters, json!({"field": "created_at", "operator": ">", "value": 0}), args);
                self.req("POST", self.url("/conversations/search"))
                    .json(&search_body(query, args))
            }
            // AP: src/lib/actions/get-ticket.ts
            "INTERCOM_GET_TICKET" => self.req(
                "GET",
                self.url(&format!("/tickets/{}", path_segment(args, "ticket_id")?)),
            ),
            // AP: src/lib/common/lookups.ts (`searchRecords`) + src/lib/actions/search-tickets.ts
            "INTERCOM_SEARCH_TICKETS" => {
                let filters: Vec<Value> = [
                    text_filter("ticket_type_id", "=", args, "ticket_type_id"),
                    opt_filter("state", "=", args, "state"),
                    opt_str(args, "open").map(|o| filt("open", "=", json!(o == "open"))),
                    text_filter("contact_ids", "=", args, "contact_id"),
                    text_filter("admin_assignee_id", "=", args, "admin_assignee_id"),
                    text_filter("title", "~", args, "title_contains"),
                    date_filter("created_at", args, "created_after")?,
                    date_filter_before("created_at", args, "created_before")?,
                    date_filter("updated_at", args, "updated_after")?,
                    date_filter_before("updated_at", args, "updated_before")?,
                ]
                .into_iter()
                .flatten()
                .collect();
                let query = search_query(filters, json!({"field": "created_at", "operator": ">", "value": 0}), args);
                self.req("POST", self.url("/tickets/search"))
                    .json(&search_body(query, args))
            }
            // AP: src/lib/actions/search-articles.ts
            "INTERCOM_SEARCH_ARTICLES" => self.req(
                "GET",
                self.url(&format!(
                    "/articles/search{}",
                    query(&[
                        ("phrase", Some(req_str(args, "phrase")?.trim().to_string())),
                        ("state", Some(opt_str(args, "state").unwrap_or("published").to_string())),
                        ("help_center_id", opt_str(args, "help_center_id").map(String::from)),
                    ])
                )),
            ),
            // AP: src/lib/actions/create-user.ts (client.contacts.create -> POST /contacts)
            "INTERCOM_CREATE_USER" => {
                let email = req_str(args, "email")?.trim();
                let mut body = Map::new();
                body.insert("role".into(), json!("user"));
                body.insert("email".into(), json!(email));
                if let Some(v) = opt_str(args, "name") {
                    body.insert("name".into(), json!(v));
                }
                if let Some(v) = opt_str(args, "external_id") {
                    body.insert("external_id".into(), json!(v));
                }
                if let Some(v) = opt_str(args, "created_at") {
                    body.insert("signed_up_at".into(), json!(unix_seconds("created_at", v)?));
                }
                if let Some(v) = args.get("custom_attributes").filter(|v| v.is_object()) {
                    body.insert("custom_attributes".into(), v.clone());
                }
                self.req("POST", self.url("/contacts")).json(&Value::Object(body))
            }
            // AP: src/lib/actions/update-contact.ts
            "INTERCOM_UPDATE_CONTACT" => {
                let contact_id = path_segment(args, "contact_id")?;
                let mut body = Map::new();
                if let Some(v) = opt_str(args, "role") {
                    body.insert("role".into(), json!(v));
                }
                for key in ["email", "name", "phone", "external_id"] {
                    if let Some(v) = opt_str(args, key) {
                        body.insert(key.into(), json!(v.trim()));
                    }
                }
                if let Some(v) = opt_str(args, "owner_id") {
                    let owner: i64 = v
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("`owner_id` must be a numeric admin ID"))?;
                    body.insert("owner_id".into(), json!(owner));
                }
                if let Some(v) = opt_str(args, "unsubscribed") {
                    body.insert("unsubscribed_from_emails".into(), json!(v == "unsubscribed"));
                }
                if let Some(v) = args.get("custom_attributes").filter(|v| v.is_object()) {
                    body.insert("custom_attributes".into(), v.clone());
                }
                if body.is_empty() {
                    bail!("nothing to update: set at least one field");
                }
                self.req("PUT", self.url(&format!("/contacts/{contact_id}")))
                    .json(&Value::Object(body))
            }
            // AP: src/lib/actions/create-update-company.ts
            // (client.companies.createOrUpdate -> POST /companies)
            "INTERCOM_CREATE_OR_UPDATE_COMPANY" => {
                let mut body = Map::new();
                for key in ["company_id", "name", "plan", "website", "industry"] {
                    if let Some(v) = opt_str(args, key) {
                        body.insert(key.into(), json!(v));
                    }
                }
                for key in ["size", "monthly_spend"] {
                    if let Some(v) = args.get(key).and_then(Value::as_i64) {
                        body.insert(key.into(), json!(v));
                    }
                }
                if let Some(v) = opt_str(args, "remote_created_at") {
                    body.insert("remote_created_at".into(), json!(unix_seconds("remote_created_at", v)?));
                }
                if let Some(v) = args.get("custom_attributes").filter(|v| v.is_object()) {
                    body.insert("custom_attributes".into(), v.clone());
                }
                self.req("POST", self.url("/companies")).json(&Value::Object(body))
            }
            // AP: src/lib/actions/create-conversation.ts (client.conversations.create
            // -> POST /conversations)
            "INTERCOM_CREATE_CONVERSATION" => {
                let contact_id = path_segment(args, "contact_id")?;
                let contact_type = req_str(args, "contact_type")?;
                if !matches!(contact_type, "user" | "lead") {
                    bail!("`contact_type` must be user or lead");
                }
                let body = json!({
                    "body": req_str(args, "body")?,
                    "from": {"type": contact_type, "id": contact_id},
                });
                self.req("POST", self.url("/conversations")).json(&body)
            }
            // AP: src/lib/actions/reply-to-conversation.ts (client.conversations.reply
            // -> POST /conversations/{conversation_id}/reply)
            "INTERCOM_REPLY_TO_CONVERSATION" => {
                let conversation_id = path_segment(args, "conversation_id")?;
                let body = json!({
                    "type": "admin",
                    "message_type": "comment",
                    "body": req_str(args, "body")?,
                    "admin_id": req_str(args, "admin_id")?,
                });
                self.req(
                    "POST",
                    self.url(&format!("/conversations/{conversation_id}/reply")),
                )
                .json(&body)
            }
            // AP: src/lib/actions/add-note-to-conversation.ts (client.conversations.reply
            // with message_type "note")
            "INTERCOM_ADD_NOTE_TO_CONVERSATION" => {
                let conversation_id = path_segment(args, "conversation_id")?;
                let body = json!({
                    "type": "admin",
                    "message_type": "note",
                    "body": req_str(args, "body")?,
                    "admin_id": req_str(args, "admin_id")?,
                });
                self.req(
                    "POST",
                    self.url(&format!("/conversations/{conversation_id}/reply")),
                )
                .json(&body)
            }
            // AP: src/lib/actions/add-note-to-user.ts (client.notes.create -> POST /notes)
            "INTERCOM_ADD_NOTE_TO_CONTACT" => {
                let contact_id = path_segment(args, "contact_id")?;
                let body = json!({"contact_id": contact_id, "body": req_str(args, "body")?});
                self.req("POST", self.url("/notes")).json(&body)
            }
            // AP: src/lib/actions/set-conversation-state.ts
            "INTERCOM_SET_CONVERSATION_STATE" => {
                let conversation_id = path_segment(args, "conversation_id")?;
                let state = req_str(args, "state")?;
                let admin_id = req_str(args, "admin_id")?;
                let closing_message = opt_str(args, "closing_message").map(str::trim);
                if closing_message.is_some() && state != "close" {
                    bail!("a closing message can only be sent when closing the conversation");
                }
                let mut body = Map::new();
                body.insert("message_type".into(), json!(state));
                body.insert("admin_id".into(), json!(admin_id));
                match state {
                    "close" => {
                        body.insert("type".into(), json!("admin"));
                        if let Some(msg) = closing_message {
                            body.insert("body".into(), json!(msg));
                        }
                    }
                    "open" => {}
                    "snoozed" => {
                        let until = opt_str(args, "snoozed_until")
                            .ok_or_else(|| anyhow::anyhow!("`snoozed_until` is required for snoozed"))?;
                        body.insert("snoozed_until".into(), json!(unix_seconds("snoozed_until", until)?));
                    }
                    other => bail!("`state` must be close, open or snoozed, got `{other}`"),
                }
                self.req(
                    "POST",
                    self.url(&format!("/conversations/{conversation_id}/parts")),
                )
                .json(&Value::Object(body))
            }
            // AP: src/lib/actions/assign-conversation-to-admin.ts
            // (client.conversations.manage -> POST /conversations/{conversation_id}/manage)
            "INTERCOM_ASSIGN_CONVERSATION" => {
                let conversation_id = path_segment(args, "conversation_id")?;
                let mut body = Map::new();
                body.insert("type".into(), json!("admin"));
                body.insert("message_type".into(), json!("assignment"));
                body.insert("admin_id".into(), json!(req_str(args, "admin_id")?));
                body.insert("assignee_id".into(), json!(req_str(args, "assignee_id")?));
                if let Some(v) = opt_str(args, "body") {
                    body.insert("body".into(), json!(v));
                }
                self.req(
                    "POST",
                    self.url(&format!("/conversations/{conversation_id}/manage")),
                )
                .json(&Value::Object(body))
            }
            // AP: src/lib/actions/send-message.action.ts (client.messages.create
            // -> POST /messages)
            "INTERCOM_SEND_MESSAGE" => {
                let message_type = req_str(args, "message_type")?;
                if !matches!(message_type, "email" | "inapp") {
                    bail!("`message_type` must be email or inapp");
                }
                let contact_role = req_str(args, "contact_role")?;
                if !matches!(contact_role, "user" | "lead") {
                    bail!("`contact_role` must be user or lead");
                }
                let contact_id = path_segment(args, "contact_id")?;
                let mut body = Map::new();
                body.insert("message_type".into(), json!(message_type));
                body.insert(
                    "from".into(),
                    json!({"id": req_str(args, "admin_id")?, "type": "admin"}),
                );
                body.insert(
                    "to".into(),
                    json!({"id": contact_id, "type": contact_role}),
                );
                if message_type == "email" {
                    body.insert(
                        "subject".into(),
                        json!(req_str(args, "subject")?),
                    );
                    body.insert(
                        "template".into(),
                        json!(opt_str(args, "template").unwrap_or("personal")),
                    );
                }
                body.insert("body".into(), json!(req_str(args, "body")?));
                body.insert(
                    "create_conversation_without_contact_reply".into(),
                    json!(args.get("create_conversation_without_contact_reply") == Some(&json!(true))),
                );
                self.req("POST", self.url("/messages")).json(&Value::Object(body))
            }
            // AP: src/lib/actions/add-remove-tag-on-contact.ts (client.tags.tagContact
            // -> POST /contacts/{contact_id}/tags)
            "INTERCOM_TAG_CONTACT" => {
                let contact_id = path_segment(args, "contact_id")?;
                let tag_id = path_segment(args, "tag_id")?;
                self.req("POST", self.url(&format!("/contacts/{contact_id}/tags")))
                    .json(&json!({"id": tag_id}))
            }
            // AP: src/lib/actions/add-remove-tag-on-conversation.ts
            // (client.tags.tagConversation -> POST /conversations/{conversation_id}/tags)
            "INTERCOM_TAG_CONVERSATION" => {
                let conversation_id = path_segment(args, "conversation_id")?;
                let tag_id = path_segment(args, "tag_id")?;
                let mut body = Map::new();
                body.insert("id".into(), json!(tag_id));
                if let Some(v) = opt_str(args, "admin_id") {
                    body.insert("admin_id".into(), json!(v));
                }
                self.req(
                    "POST",
                    self.url(&format!("/conversations/{conversation_id}/tags")),
                )
                .json(&Value::Object(body))
            }
            other => anyhow::bail!("unknown intercom tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "INTERCOM_LIST_ADMINS" => {
                let items: Vec<Value> = v["admins"]
                    .as_array()
                    .map(|a| a.iter().map(admin).collect())
                    .unwrap_or_default();
                json!({"items": items})
            }
            "INTERCOM_LIST_TEAMS" => {
                let items: Vec<Value> = v["teams"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| json!({"id": t["id"], "name": t["name"], "admin_ids": t["admin_ids"]}))
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"items": items})
            }
            "INTERCOM_LIST_TICKET_TYPES" => {
                let items: Vec<Value> = v["data"]
                    .as_array()
                    .or_else(|| v["ticket_types"].as_array())
                    .map(|a| {
                        a.iter()
                            .map(|t| {
                                let attributes: Vec<Value> = t["ticket_type_attributes"]["data"]
                                    .as_array()
                                    .map(|attrs| {
                                        attrs
                                            .iter()
                                            .map(|a| {
                                                json!({
                                                    "name": a["name"],
                                                    "data_type": a["data_type"],
                                                    "required_to_create": a["required_to_create"] == Value::Bool(true),
                                                    "visible_to_contacts": a["visible_to_contacts"] == Value::Bool(true),
                                                    "archived": a["archived"] == Value::Bool(true),
                                                    "options": a["input_options"]["list_options"].clone(),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                json!({
                                    "id": t["id"],
                                    "name": t["name"],
                                    "description": t["description"],
                                    "category": t["category"],
                                    "archived": t["archived"] == Value::Bool(true),
                                    "attributes": attributes,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"items": items})
            }
            "INTERCOM_LIST_DATA_ATTRIBUTES" => {
                let items: Vec<Value> = v["data"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|d| json!({
                                "name": d["name"],
                                "full_name": d["full_name"],
                                "label": d["label"],
                                "data_type": d["data_type"],
                                "model": d["model"],
                                "custom": d["custom"] == Value::Bool(true),
                                "api_writable": d["api_writable"] == Value::Bool(true),
                                "options": d["options"].clone(),
                            }))
                            .collect()
                    })
                    .unwrap_or_default();
                json!({"items": items})
            }
            "INTERCOM_LIST_COMPANIES" => paged_list(&v, "data"),
            "INTERCOM_LIST_CONTACT_NOTES" => paged_list(&v, "data"),
            "INTERCOM_FIND_COMPANY" => {
                let company = match v.get("type").and_then(Value::as_str) {
                    Some("company") => v.clone(),
                    _ => v["data"][0].clone(),
                };
                json!({"found": !company.is_null() && company != json!({}), "company": company})
            }
            "INTERCOM_SEARCH_CONTACTS" => cursor_list(&v, "data"),
            // `find-conversation.ts` returns `{found, conversation}`; the SDK
            // normalises the result list to `data`, the raw REST call uses
            // `conversations`.
            "INTERCOM_FIND_CONVERSATION" => {
                let items = v["data"]
                    .as_array()
                    .or_else(|| v["conversations"].as_array())
                    .cloned()
                    .unwrap_or_default();
                json!({
                    "found": !items.is_empty(),
                    "conversation": items.first().cloned().unwrap_or_else(|| json!({})),
                })
            }
            "INTERCOM_SEARCH_CONVERSATIONS" => cursor_list(&v, "conversations"),
            "INTERCOM_SEARCH_TICKETS" => cursor_list(&v, "tickets"),
            "INTERCOM_SEARCH_ARTICLES" => {
                let items = v["data"]["articles"].as_array().cloned().unwrap_or_default();
                json!({
                    "items": items,
                    "total_count": v["total_count"].as_u64().map(|t| json!(t)).unwrap_or_else(|| json!(items.len())),
                })
            }
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolkits::ToolkitAdapter;

    fn ic() -> Intercom {
        Intercom::with_api_base("https://ic.test")
    }

    #[test]
    fn regions_map_to_base_urls() {
        assert_eq!(
            Intercom::with_region("eu.intercom").unwrap().api_base,
            "https://api.eu.intercom.io"
        );
        assert_eq!(
            Intercom::default().api_base,
            "https://api.intercom.io"
        );
        assert!(Intercom::with_region("de.intercom").is_err());
    }

    #[test]
    fn version_header_and_query_encoding() {
        let r = ic()
            .build_request(
                "INTERCOM_LIST_DATA_ATTRIBUTES",
                &json!({"model": "contact", "include_archived": true}),
            )
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://ic.test/data_attributes?model=contact&include_archived=true"
        );
        assert_eq!(r.headers.get("intercom-version").unwrap(), "2.11");
    }

    #[test]
    fn search_bodies_and_path_guards() {
        let r = ic()
            .build_request(
                "INTERCOM_SEARCH_CONTACTS",
                &json!({"email": "a@b.de", "per_page": 5}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["query"]["field"], "email");
        assert_eq!(body["query"]["value"], "a@b.de");
        assert_eq!(body["pagination"]["per_page"], 5);

        assert!(ic()
            .build_request("INTERCOM_GET_CONTACT", &json!({"contact_id": "../me"}))
            .is_err());
        assert!(ic()
            .build_request(
                "INTERCOM_FIND_CONVERSATION",
                &json!({"search_field": "id", "match_type": "contains", "search_term": "1"})
            )
            .is_err());
        assert!(ic()
            .build_request(
                "INTERCOM_LIST_COMPANIES",
                &json!({"tag_id": "1", "segment_id": "2"})
            )
            .is_err());
    }

    #[test]
    fn dates_become_unix_seconds() {
        assert_eq!(unix_seconds("updated_after", "2026-01-01").unwrap(), 1_767_225_600);
        assert!(unix_seconds("updated_after", "not-a-date").is_err());
    }
}