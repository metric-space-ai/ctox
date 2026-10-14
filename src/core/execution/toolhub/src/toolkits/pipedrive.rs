//! Pipedrive toolkit (REST API v1/v2). Action set modelled on Activepieces
//! `piece-pipedrive` (MIT), file by file.
//!
//! Auth: OAuth2 exactly like `src/lib/auth.ts` — authorize/token at
//! `oauth.pipedrive.com`, `Authorization: Bearer <token>` on every call.
//!
//! Base URL: the piece calls `${api_domain}/api/...`, where `api_domain` is
//! returned by the OAuth2 token response (per-account, e.g.
//! `https://api.pipedrive.com` or a company subdomain). Pass the connection's
//! `api_domain` via [`Pipedrive::with_api_base`]; the default is
//! `https://api.pipedrive.com`, the same fallback the piece uses.
//!
//! Single-request note: the piece resolves and renames custom fields with a
//! second call (`GET /api/v1/<object>Fields`); this adapter sends the main
//! request only, so records keep custom fields keyed by their field hash and
//! write tools accept standard fields only.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "pipedrive";

const BASE: &str = "base";
const CONTACTS: &str = "contacts:full";
const USERS: &str = "users:read";
const DEALS: &str = "deals:full";
const ACTIVITIES: &str = "activities:full";
const LEADS: &str = "leads:full";
const PRODUCTS: &str = "products:full";

/// `DEAL_OPTIONAL_FIELDS` from `src/lib/common/constants.ts`.
const DEAL_OPTIONAL_FIELDS: &[&str] = &[
    "next_activity_id",
    "last_activity_id",
    "first_won_time",
    "products_count",
    "files_count",
    "notes_count",
    "followers_count",
    "email_messages_count",
    "activities_count",
    "done_activities_count",
    "undone_activities_count",
    "participants_count",
    "last_incoming_mail_time",
    "last_outgoing_mail_time",
    "smart_bcc_email",
];

/// `PERSON_OPTIONAL_FIELDS` from `src/lib/common/constants.ts`.
const PERSON_OPTIONAL_FIELDS: &[&str] = &[
    "next_activity_id",
    "last_activity_id",
    "open_deals_count",
    "related_open_deals_count",
    "closed_deals_count",
    "related_closed_deals_count",
    "participant_open_deals_count",
    "participant_closed_deals_count",
    "email_messages_count",
    "activities_count",
    "done_activities_count",
    "undone_activities_count",
    "files_count",
    "notes_count",
    "followers_count",
    "related_won_deals_count",
    "lost_deals_count",
    "related_lost_deals_count",
    "last_incoming_mail_time",
    "last_outgoing_mail_time",
    "won_deals_count",
];

/// `ORGANIZATION_OPTIONAL_FIELDS` from `src/lib/common/constants.ts`.
const ORGANIZATION_OPTIONAL_FIELDS: &[&str] = &[
    "next_activity_id",
    "last_activity_id",
    "open_deals_count",
    "related_open_deals_count",
    "closed_deals_count",
    "related_closed_deals_count",
    "email_messages_count",
    "people_count",
    "activities_count",
    "done_activities_count",
    "undone_activities_count",
    "files_count",
    "notes_count",
    "followers_count",
    "won_deals_count",
    "related_won_deals_count",
    "lost_deals_count",
    "related_lost_deals_count",
];

fn joined(fields: &[&str]) -> String {
    fields.join(",")
}

// ---------------------------------------------------------------- schema helpers

fn int_id(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn number(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}

fn boolean(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn choice_i(desc: &str, options: &[i64]) -> Value {
    json!({"type": "integer", "description": desc, "enum": options})
}

fn date(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "pattern": "^\\d{4}-\\d{2}-\\d{2}"})
}

fn datetime(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "format": "date-time"})
}

fn int_list(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": desc})
}

/// `pipedriveAtomic.paginationProps()`: limit (1-500, default 50) + cursor.
fn page_props(extra: Vec<(&'static str, Value)>) -> Vec<(&'static str, Value)> {
    let mut v = vec![
        (
            "limit",
            int("Maximale Trefferzahl in dieser Seite (1-500), Standard 50. / Maximum items in this page (1-500), default 50.", 1, 500, 50),
        ),
        (
            "cursor",
            s("next_cursor aus dem vorherigen Aufruf für die nächste Seite. / The next_cursor from the previous call, for the next page."),
        ),
    ];
    v.extend(extra);
    v
}

/// `pipedriveAtomic.sortDirectionProp()`.
fn sort_direction(desc: &str) -> Value {
    choice(desc, &["asc", "desc"])
}

/// Search input of the v2 `/search` endpoints (`assertSearchTerm`).
fn term_prop() -> Value {
    s("Suchtext, mindestens 2 Zeichen (1 bei exakter Suche). / Search term, at least 2 characters (1 with exact match).")
}

// ---------------------------------------------------------------- argument helpers

/// Numeric id for a path segment or a JSON body field (`Property.Number`).
fn id_part(args: &Value, key: &str) -> Result<String> {
    let raw = match args.get(key) {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Null) | None => bail!("missing argument `{key}`"),
        Some(_) => bail!("`{key}` must be a numeric id"),
    };
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        bail!("`{key}` must be a numeric id");
    }
    Ok(raw)
}

fn opt_id(args: &Value, key: &str) -> Result<Option<String>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => Ok(Some(id_part(args, key)?)),
    }
}

/// JSON number for a body field; numeric strings are accepted and normalized.
fn opt_num(args: &Value, key: &str) -> Result<Option<Value>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v @ Value::Number(_)) => Ok(Some(v.clone())),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
                bail!("`{key}` must be a number");
            }
            Ok(Some(json!(t.parse::<u64>()?)))
        }
        Some(_) => bail!("`{key}` must be a number"),
    }
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

/// Array of numbers for body fields such as `label_ids`.
fn num_vec(args: &Value, key: &str) -> Result<Vec<Value>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.is_empty() => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| match v {
                Value::Number(_) => Ok(v.clone()),
                Value::String(s)
                    if !s.trim().is_empty() && s.trim().bytes().all(|b| b.is_ascii_digit()) =>
                {
                    Ok(json!(s.trim().parse::<u64>()?))
                }
                _ => bail!("`{key}` must be numbers"),
            })
            .collect(),
        Some(_) => bail!("`{key}` must be an array of numbers"),
    }
}

fn put_str(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_num(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if let Some(v) = opt_num(args, key)? {
        m.insert(field.into(), v);
    }
    Ok(())
}

fn put_bool(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_bool(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_ymd(m: &mut Map<String, Value>, args: &Value, key: &str, field: &str) -> Result<()> {
    if let Some(v) = ymd(args, key)? {
        m.insert(field.into(), json!(v));
    }
    Ok(())
}

/// Dropdown value check (the TS StaticDropdowns can only send these).
fn one_of(args: &Value, key: &str, allowed: &[&str]) -> Result<Option<String>> {
    match opt_str(args, key) {
        None => Ok(None),
        Some(v) if allowed.contains(&v) => Ok(Some(v.to_string())),
        Some(_) => bail!("`{key}` must be one of: {}", allowed.join(", ")),
    }
}

fn require_one_of(args: &Value, key: &str, allowed: &[&str]) -> Result<String> {
    one_of(args, key, allowed)?.ok_or_else(|| anyhow!("missing argument `{key}`"))
}

fn sort_dir(args: &Value) -> Result<Option<String>> {
    one_of(args, "sort_direction", &["asc", "desc"])
}

fn limit_arg(args: &Value) -> String {
    opt_u64(args, "limit", 50, 1, 500).to_string()
}

fn is_date(s: &str) -> bool {
    s.len() == 10
        && s.bytes()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-'
}

/// `pipedriveAtomic.toRfc3339`: a date becomes midnight UTC, a full ISO
/// date-time is normalized to `…Z` (fractional seconds dropped, like the
/// piece does). The piece converts any offset to UTC, so only UTC input
/// (with `Z` or none) is accepted here.
fn rfc3339(args: &Value, key: &str) -> Result<Option<String>> {
    let Some(raw) = opt_str(args, key) else {
        return Ok(None);
    };
    let v = raw.trim();
    let err = || {
        anyhow!("`{key}` must be a UTC date-time (YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ), got \"{raw}\"")
    };
    if !v.is_ascii() {
        return Err(err());
    }
    if is_date(v) {
        return Ok(Some(format!("{v}T00:00:00Z")));
    }
    let Some(rest) = v.get(10..).and_then(|r| r.strip_prefix(['T', 't', ' '])) else {
        return Err(err());
    };
    let rest = rest.strip_suffix('Z').unwrap_or(rest);
    let (hms, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let ok_hms = hms.len() == 8
        && hms.as_bytes()[2] == b':'
        && hms.as_bytes()[5] == b':'
        && hms
            .bytes()
            .enumerate()
            .all(|(i, b)| matches!(i, 2 | 5) || b.is_ascii_digit());
    let ok_frac = frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()));
    if !ok_hms || !ok_frac {
        return Err(err());
    }
    Ok(Some(format!("{}T{hms}Z", &v[..10])))
}

/// `dayjs(v).format("YYYY-MM-DD")`: the calendar date of a date or date-time.
fn ymd(args: &Value, key: &str) -> Result<Option<String>> {
    let Some(raw) = opt_str(args, key) else {
        return Ok(None);
    };
    let v = raw.trim();
    if v.len() >= 10 && v.is_ascii() && is_date(&v[..10]) {
        return Ok(Some(v[..10].to_string()));
    }
    bail!("`{key}` must be a date in YYYY-MM-DD format, got \"{raw}\"")
}

/// `pipedriveAtomic.assertSearchTerm`.
fn search_term(args: &Value) -> Result<String> {
    let exact = opt_bool(args, "exact_match").unwrap_or(false);
    let trimmed = req_str(args, "term")?.trim();
    let min = if exact { 1 } else { 2 };
    if trimmed.chars().count() < min {
        bail!("`term` must contain at least {min} characters");
    }
    Ok(trimmed.to_string())
}

fn fields_arg(args: &Value, allowed: &[&str]) -> Result<Option<String>> {
    let fields = str_vec(args, "fields");
    if fields.is_empty() {
        return Ok(None);
    }
    for f in &fields {
        if !allowed.contains(&f.as_str()) {
            bail!("`fields` must be one of: {}", allowed.join(", "));
        }
    }
    Ok(Some(fields.join(",")))
}

/// Lead UUID path segment (`encodeURIComponent(leadId.trim())`).
fn lead_ref<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    path_segment(args, key)
}

// ---------------------------------------------------------------- adapter

pub struct Pipedrive {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Pipedrive {
    fn default() -> Self {
        Self::with_api_base("https://api.pipedrive.com")
    }
}

#[allow(clippy::too_many_lines)]
impl Pipedrive {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Pipedrive".into(),
            description: LocalizedText::new(
                "Pipedrive-CRM: Deals, Kontakte, Organisationen, Aktivitäten, Leads, Notizen und Produkte lesen und bearbeiten.",
                "Pipedrive CRM: read and change deals, persons, organizations, activities, leads, notes and products.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://oauth.pipedrive.com/oauth/authorize".into(),
                    token_url: "https://oauth.pipedrive.com/oauth/token".into(),
                    // Scopes of the Activepieces piece (`src/lib/auth.ts`).
                    default_scopes: [
                        BASE,
                        "admin",
                        CONTACTS,
                        USERS,
                        DEALS,
                        ACTIVITIES,
                        LEADS,
                        PRODUCTS,
                        "webhooks:full",
                    ]
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: true,
                    extra_authorize_params: Vec::new(),
                }),
                // Personal API token: Pipedrive accepts it in the `x-api-token` header.
                AuthScheme::ApiKey {
                    header_name: "x-api-token".into(),
                    prefix: String::new(),
                },
            ],
            tools: vec![
                t(
                    "PIPEDRIVE_GET_CURRENT_USER",
                    ("Angemeldeten Nutzer lesen", "Get current user"),
                    (
                        "Liest den Nutzer hinter der Verbindung (ID, E-Mail, Zeitzone, Firma).",
                        "Reads the user behind the connection (ID, email, timezone, company).",
                    ),
                    &[],
                    &[],
                    &[BASE, USERS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_USERS",
                    ("Nutzer auflisten", "List users"),
                    (
                        "Listet die Nutzer des Unternehmens, um Owner- oder Nutzer-IDs aufzulösen.",
                        "Lists the company's users to resolve owner or user IDs.",
                    ),
                    &[],
                    &[],
                    &[BASE, USERS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_PIPELINES",
                    ("Pipelines auflisten", "List pipelines"),
                    (
                        "Listet eine Seite der Vertriebspipelines mit Sortierung und Cursor.",
                        "Lists one page of the sales pipelines with sorting and cursor.",
                    ),
                    &page_props(vec![
                        ("sort_by", choice("Sortierung, Standard ID", &["id", "update_time", "add_time"])),
                        ("sort_direction", sort_direction("Sortierrichtung, Standard aufsteigend")),
                    ]),
                    &[],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_STAGES",
                    ("Phasen auflisten", "List stages"),
                    (
                        "Listet Deal-Phasen, optional nur die einer Pipeline.",
                        "Lists deal stages, optionally only those of one pipeline.",
                    ),
                    &page_props(vec![
                        ("pipeline_id", int_id("Nur Phasen dieser Pipeline (aus PIPEDRIVE_LIST_PIPELINES)")),
                        ("sort_by", choice("Sortierung, Standard ID", &["id", "update_time", "add_time", "order_nr"])),
                        ("sort_direction", sort_direction("Sortierrichtung, Standard aufsteigend")),
                    ]),
                    &[],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_GET_DEAL",
                    ("Deal lesen", "Get deal"),
                    (
                        "Liest einen Deal per ID inklusive Aktivitäts-, Produkt- und Notiz-Zählern.",
                        "Reads one deal by ID including activity, product and note counters.",
                    ),
                    &[("deal_id", int_id("Numerische Deal-ID (aus PIPEDRIVE_SEARCH_DEALS oder PIPEDRIVE_LIST_DEALS)"))],
                    &["deal_id"],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_DEALS",
                    ("Deals auflisten", "List deals"),
                    (
                        "Listet eine Seite von Deals, gefiltert nach Owner, Person, Organisation, Pipeline, Phase, Status oder Änderungszeit.",
                        "Lists one page of deals filtered by owner, person, organization, pipeline, stage, status or update time.",
                    ),
                    &page_props(vec![
                        ("owner_id", int_id("Nur Deals dieses Nutzers (aus PIPEDRIVE_LIST_USERS)")),
                        ("person_id", int_id("Nur Deals dieser Person")),
                        ("organization_id", int_id("Nur Deals dieser Organisation")),
                        ("pipeline_id", int_id("Nur Deals dieser Pipeline")),
                        ("stage_id", int_id("Nur Deals dieser Phase")),
                        ("status", choice("Nur Deals mit diesem Status", &["open", "won", "lost", "deleted"])),
                        ("updated_since", datetime("Nur Deals, die ab diesem UTC-Zeitpunkt geändert wurden")),
                        ("updated_until", datetime("Nur Deals, die vor diesem UTC-Zeitpunkt geändert wurden")),
                        ("sort_by", choice("Sortierung, Standard ID", &["id", "update_time", "add_time"])),
                        ("sort_direction", sort_direction("Sortierrichtung, Standard aufsteigend")),
                    ]),
                    &[],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_SEARCH_DEALS",
                    ("Deals durchsuchen", "Search deals"),
                    (
                        "Volltextsuche über Deal-Titel, Notizen und benutzerdefinierte Felder, nach Relevanz sortiert.",
                        "Full-text search over deal titles, notes and custom fields, ranked by relevance.",
                    ),
                    &page_props(vec![
                        ("term", term_prop()),
                        ("fields", str_list("Felder: title, notes, custom_fields; leer = alle")),
                        ("exact_match", boolean("Nur exakte Treffer des ganzen Begriffs")),
                        ("person_id", int_id("Nur Deals dieser Person")),
                        ("organization_id", int_id("Nur Deals dieser Organisation")),
                        ("status", choice("Nur Deals mit diesem Status", &["open", "won", "lost"])),
                    ]),
                    &["term"],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_DEAL_PRODUCTS",
                    ("Deal-Produkte auflisten", "List deal products"),
                    (
                        "Listet die Produktpositionen eines Deals; die Zeilen-ID braucht PIPEDRIVE_REMOVE_PRODUCT_FROM_DEAL nicht der Produkt-ID.",
                        "Lists the product lines of a deal; the row id (not the product id) is what the remove tool needs.",
                    ),
                    &page_props(vec![
                        ("deal_id", int_id("Numerische Deal-ID")),
                        ("sort_by", choice("Sortierung, Standard ID", &["id", "add_time", "update_time", "order_nr"])),
                        ("sort_direction", sort_direction("Sortierrichtung, Standard aufsteigend")),
                    ]),
                    &["deal_id"],
                    &[BASE, DEALS],
                    false,
                ),
                t(
                    "PIPEDRIVE_GET_PERSON",
                    ("Person lesen", "Get person"),
                    (
                        "Liest eine Person (Kontakt) per ID inklusive E-Mails, Telefonen und Zählern.",
                        "Reads one person (contact) by ID including emails, phones and counters.",
                    ),
                    &[("person_id", int_id("Numerische Personen-ID (aus PIPEDRIVE_SEARCH_PERSONS)"))],
                    &["person_id"],
                    &[BASE, CONTACTS],
                    false,
                ),
                t(
                    "PIPEDRIVE_SEARCH_PERSONS",
                    ("Personen durchsuchen", "Search persons"),
                    (
                        "Durchsucht Personen nach Name, E-Mail, Telefon, Notizen oder Custom-Feldern.",
                        "Searches persons by name, email, phone, notes or custom fields.",
                    ),
                    &page_props(vec![
                        ("term", term_prop()),
                        ("fields", str_list("Felder: name, email, phone, notes, custom_fields; leer = alle")),
                        ("exact_match", boolean("Nur exakte Treffer des ganzen Begriffs")),
                        ("organization_id", int_id("Nur Personen dieser Organisation")),
                    ]),
                    &["term"],
                    &[BASE, CONTACTS],
                    false,
                ),
                t(
                    "PIPEDRIVE_GET_ORGANIZATION",
                    ("Organisation lesen", "Get organization"),
                    (
                        "Liest eine Organisation per ID inklusive Personen- und Deal-Zählern.",
                        "Reads one organization by ID including people and deal counters.",
                    ),
                    &[("organization_id", int_id("Numerische Organisations-ID (aus PIPEDRIVE_SEARCH_ORGANIZATIONS)"))],
                    &["organization_id"],
                    &[BASE, CONTACTS],
                    false,
                ),
                t(
                    "PIPEDRIVE_SEARCH_ORGANIZATIONS",
                    ("Organisationen durchsuchen", "Search organizations"),
                    (
                        "Durchsucht Organisationen nach Name, Adresse, Notizen oder Custom-Feldern.",
                        "Searches organizations by name, address, notes or custom fields.",
                    ),
                    &page_props(vec![
                        ("term", term_prop()),
                        ("fields", str_list("Felder: name, address, notes, custom_fields; leer = alle")),
                        ("exact_match", boolean("Nur exakte Treffer des ganzen Begriffs")),
                    ]),
                    &["term"],
                    &[BASE, CONTACTS],
                    false,
                ),
                t(
                    "PIPEDRIVE_LIST_ACTIVITIES",
                    ("Aktivitäten auflisten", "List activities"),
                    (
                        "Listet eine Seite von Aktivitäten, gefiltert nach Deal, Lead, Person, Organisation, Owner oder Status.",
                        "Lists one page of activities filtered by deal, lead, person, organization, owner or done state.",
                    ),
                    &page_props(vec![
                        ("deal_id", int_id("Nur Aktivitäten dieses Deals")),
                        ("lead_id", s("Nur Aktivitäten dieses Leads (UUID)")),
                        ("person_id", int_id("Nur Aktivitäten dieser Person")),
                        ("organization_id", int_id("Nur Aktivitäten dieser Organisation")),
                        ("owner_id", int_id("Nur Aktivitäten dieses Nutzers")),
                        ("done", boolean("Nur erledigte (true) oder offene (false) Aktivitäten")),
                        ("updated_since", datetime("Nur Aktivitäten, die ab diesem UTC-Zeitpunkt geändert wurden")),
                        ("updated_until", datetime("Nur Aktivitäten, die vor diesem UTC-Zeitpunkt geändert wurden")),
                        ("sort_by", choice("Sortierung, Standard ID", &["id", "update_time", "add_time", "due_date"])),
                        ("sort_direction", sort_direction("Sortierrichtung, Standard aufsteigend")),
                    ]),
                    &[],
                    &[BASE, ACTIVITIES],
                    false,
                ),
                t(
                    "PIPEDRIVE_GET_LEAD",
                    ("Lead lesen", "Get lead"),
                    (
                        "Liest einen Lead per UUID inklusive Wert, Owner und Verknüpfungen.",
                        "Reads one lead by UUID including value, owner and links.",
                    ),
                    &[("lead_id", s("Lead-UUID (aus PIPEDRIVE_FIND_LEAD)"))],
                    &["lead_id"],
                    &[BASE, LEADS],
                    false,
                ),
                t(
                    "PIPEDRIVE_FIND_LEAD",
                    ("Leads suchen", "Find leads"),
                    (
                        "Durchsucht Leads nach Titel; liefert die Treffer einer Seite (100 pro Seite).",
                        "Searches leads by title and returns the matches of one page (100 per page).",
                    ),
                    &[
                        ("term", s("Zu suchender Titel")),
                        ("exact_match", boolean("Nur vollständige, case-insensitive Titel-Treffer")),
                        ("cursor", s("next_cursor aus dem vorherigen Aufruf für die nächste Seite")),
                    ],
                    &["term"],
                    &[BASE, LEADS],
                    false,
                ),
                t(
                    "PIPEDRIVE_FIND_NOTES",
                    ("Notizen suchen", "Find notes"),
                    (
                        "Listet die Notizen eines Deals, Leads, Kontakts oder einer Organisation, neueste zuerst.",
                        "Lists the notes of one deal, lead, person or organization, newest first.",
                    ),
                    &[
                        ("object_type", choice("Nach welchem Objekt suchen", &["deal_id", "lead_id", "person_id", "org_id"])),
                        ("object_id", s("ID des Objekts (Lead-IDs sind UUIDs, sonst numerisch)")),
                    ],
                    &["object_type", "object_id"],
                    &[BASE],
                    false,
                ),
                t(
                    "PIPEDRIVE_FIND_PRODUCTS",
                    ("Produkte suchen", "Find products"),
                    (
                        "Sucht Produkte exakt nach Name oder Produktcode.",
                        "Finds products by exact name or product code.",
                    ),
                    &[
                        ("field", choice("Feld zum Suchen", &["name", "code"])),
                        ("field_value", s("Exakter Wert des Felds")),
                    ],
                    &["field", "field_value"],
                    &[BASE, PRODUCTS],
                    false,
                ),
                t(
                    "PIPEDRIVE_CREATE_DEAL",
                    ("Deal anlegen", "Create deal"),
                    (
                        "Legt einen neuen Deal mit Wert, Phase und Verknüpfungen an; braucht --confirm.",
                        "Creates a new deal with value, stage and links; requires --confirm.",
                    ),
                    &[
                        ("title", s("Titel des Deals")),
                        ("value", number("Deal-Wert")),
                        ("currency", s("Währungscode, z. B. USD oder EUR")),
                        ("pipeline_id", int_id("Pipeline-ID")),
                        ("stage_id", int_id("Phasen-ID")),
                        ("status", choice("Status", &["open", "won", "lost", "deleted"])),
                        ("probability", number("Wahrscheinlichkeit in Prozent")),
                        ("expected_close_date", date("Erwartetes Abschlussdatum, YYYY-MM-DD")),
                        ("creation_time", datetime("Erstellungszeit (UTC), wird als add_time gesetzt")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("person_id", int_id("Verknüpfte Personen-ID")),
                        ("organization_id", int_id("Verknüpfte Organisations-ID")),
                        ("label_ids", int_list("Label-IDs")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                    ],
                    &["title"],
                    &[BASE, DEALS],
                    true,
                ),
                t(
                    "PIPEDRIVE_UPDATE_DEAL",
                    ("Deal ändern", "Update deal"),
                    (
                        "Ändert nur die übergebenen Felder eines Deals; braucht --confirm.",
                        "Changes only the deal fields you pass; requires --confirm.",
                    ),
                    &[
                        ("deal_id", int_id("Numerische Deal-ID")),
                        ("title", s("Neuer Titel")),
                        ("value", number("Neuer Deal-Wert")),
                        ("currency", s("Währungscode, z. B. USD oder EUR")),
                        ("pipeline_id", int_id("Pipeline-ID")),
                        ("stage_id", int_id("Phasen-ID")),
                        ("status", choice("Status", &["open", "won", "lost", "deleted"])),
                        ("probability", number("Wahrscheinlichkeit in Prozent")),
                        ("expected_close_date", date("Erwartetes Abschlussdatum, YYYY-MM-DD")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("person_id", int_id("Verknüpfte Personen-ID")),
                        ("organization_id", int_id("Verknüpfte Organisations-ID")),
                        ("label_ids", int_list("Label-IDs (ersetzt alle)")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                    ],
                    &["deal_id"],
                    &[BASE, DEALS],
                    true,
                ),
                t(
                    "PIPEDRIVE_DELETE_DEAL",
                    ("Deal löschen", "Delete deal"),
                    (
                        "Löscht einen Deal (Soft-Delete, 30 Tage wiederherstellbar); braucht --confirm.",
                        "Deletes a deal (soft delete, restorable for 30 days); requires --confirm.",
                    ),
                    &[("deal_id", int_id("Numerische ID des zu löschenden Deals"))],
                    &["deal_id"],
                    &[BASE, DEALS],
                    true,
                ),
                t(
                    "PIPEDRIVE_CREATE_PERSON",
                    ("Person anlegen", "Create person"),
                    (
                        "Legt eine neue Person mit E-Mails, Telefonen und Labels an; braucht --confirm.",
                        "Creates a new person with emails, phones and labels; requires --confirm.",
                    ),
                    &[
                        ("name", s("Voller Name; entweder Name ODER Vor-/Nachname")),
                        ("first_name", s("Vorname (nur zusammen mit Nachname)")),
                        ("last_name", s("Nachname (nur zusammen mit Vorname)")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("organization_id", int_id("Organisations-ID")),
                        ("email", str_list("E-Mail-Adressen; die erste ist primär")),
                        ("phone", str_list("Telefonnummern; die erste ist primär")),
                        ("label_ids", int_list("Label-IDs")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                        ("marketing_status", choice("Marketing-Status", &["no_consent", "unsubscribed", "subscribed", "archived"])),
                    ],
                    &[],
                    &[BASE, CONTACTS],
                    true,
                ),
                t(
                    "PIPEDRIVE_UPDATE_PERSON",
                    ("Person ändern", "Update person"),
                    (
                        "Ändert nur die übergebenen Felder einer Person; braucht --confirm.",
                        "Changes only the person fields you pass; requires --confirm.",
                    ),
                    &[
                        ("person_id", int_id("Numerische Personen-ID")),
                        ("name", s("Voller Name")),
                        ("first_name", s("Vorname (nur zusammen mit Nachname)")),
                        ("last_name", s("Nachname (nur zusammen mit Vorname)")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("organization_id", int_id("Organisations-ID")),
                        ("email", str_list("E-Mail-Adressen (ersetzt alle); die erste ist primär")),
                        ("phone", str_list("Telefonnummern (ersetzt alle); die erste ist primär")),
                        ("label_ids", int_list("Label-IDs (ersetzt alle)")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                        ("marketing_status", choice("Marketing-Status", &["no_consent", "unsubscribed", "subscribed", "archived"])),
                    ],
                    &["person_id"],
                    &[BASE, CONTACTS],
                    true,
                ),
                t(
                    "PIPEDRIVE_CREATE_ORGANIZATION",
                    ("Organisation anlegen", "Create organization"),
                    (
                        "Legt eine neue Organisation mit Adresse und Labels an; braucht --confirm.",
                        "Creates a new organization with address and labels; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name der Organisation")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("address", s("Postadresse als Text")),
                        ("label_ids", int_list("Label-IDs")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                    ],
                    &["name"],
                    &[BASE, CONTACTS],
                    true,
                ),
                t(
                    "PIPEDRIVE_UPDATE_ORGANIZATION",
                    ("Organisation ändern", "Update organization"),
                    (
                        "Ändert nur die übergebenen Felder einer Organisation; braucht --confirm.",
                        "Changes only the organization fields you pass; requires --confirm.",
                    ),
                    &[
                        ("organization_id", int_id("Numerische Organisations-ID")),
                        ("name", s("Neuer Name")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("address", s("Postadresse als Text")),
                        ("label_ids", int_list("Label-IDs (ersetzt alle)")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                    ],
                    &["organization_id"],
                    &[BASE, CONTACTS],
                    true,
                ),
                t(
                    "PIPEDRIVE_CREATE_LEAD",
                    ("Lead anlegen", "Create lead"),
                    (
                        "Legt einen Lead an; eine Person oder Organisation ist nötig; braucht --confirm.",
                        "Creates a lead; a person or organization is required; needs --confirm.",
                    ),
                    &[
                        ("title", s("Titel des Leads")),
                        ("person_id", int_id("Verknüpfte Personen-ID")),
                        ("organization_id", int_id("Verknüpfte Organisations-ID")),
                        ("value", number("Lead-Wert (nur mit Währung)")),
                        ("currency", s("Währungscode, Pflicht wenn Wert gesetzt")),
                        ("expected_close_date", date("Erwartetes Abschlussdatum, YYYY-MM-DD")),
                        ("owner_id", int_id("Owner-Nutzer-ID")),
                        ("channel", int_id("Kanal-ID (Kanalfeld des Mandanten)")),
                        ("label_ids", str_list("Label-IDs (Lead-Labels sind Strings)")),
                        ("visible_to", choice_i("1 = Owner & Follower, 3 = ganzes Unternehmen", &[1, 3])),
                    ],
                    &["title"],
                    &[BASE, LEADS],
                    true,
                ),
                t(
                    "PIPEDRIVE_CREATE_ACTIVITY",
                    ("Aktivität anlegen", "Create activity"),
                    (
                        "Legt eine Aktivität (Anruf, Meeting, Aufgabe) an; braucht --confirm.",
                        "Creates an activity (call, meeting, task); requires --confirm.",
                    ),
                    &[
                        ("subject", s("Betreff")),
                        ("type", s("Aktivitätstyp (key_string, siehe /activityTypes)")),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD")),
                        ("due_time", s("Fälligkeitszeit, HH:MM")),
                        ("duration", s("Dauer, HH:MM")),
                        ("is_done", boolean("Als erledigt markieren")),
                        ("busy", choice("free = frei, busy = belegt", &["free", "busy"])),
                        ("note", s("Notiz zur Aktivität")),
                        ("public_description", s("Öffentliche Beschreibung")),
                        ("assign_to", int_id("Nutzer-ID, dem die Aktivität zugewiesen wird")),
                        ("deal_id", int_id("Verknüpfte Deal-ID")),
                        ("person_id", int_id("Teilnehmer-Personen-ID")),
                        ("organization_id", int_id("Verknüpfte Organisations-ID")),
                        ("lead_id", s("Verknüpfte Lead-UUID")),
                    ],
                    &["subject"],
                    &[BASE, ACTIVITIES],
                    true,
                ),
                t(
                    "PIPEDRIVE_UPDATE_ACTIVITY",
                    ("Aktivität ändern", "Update activity"),
                    (
                        "Ändert nur die übergebenen Felder einer Aktivität; braucht --confirm.",
                        "Changes only the activity fields you pass; requires --confirm.",
                    ),
                    &[
                        ("activity_id", int_id("Numerische Aktivitäts-ID")),
                        ("subject", s("Neuer Betreff")),
                        ("type", s("Aktivitätstyp (key_string)")),
                        ("due_date", date("Fälligkeitsdatum, YYYY-MM-DD")),
                        ("due_time", s("Fälligkeitszeit, HH:MM")),
                        ("duration", s("Dauer, HH:MM")),
                        ("is_done", boolean("Als erledigt markieren")),
                        ("busy", choice("free = frei, busy = belegt", &["free", "busy"])),
                        ("note", s("Notiz zur Aktivität")),
                        ("public_description", s("Öffentliche Beschreibung")),
                        ("assign_to", int_id("Nutzer-ID, dem die Aktivität zugewiesen wird")),
                        ("deal_id", int_id("Verknüpfte Deal-ID")),
                        ("person_id", int_id("Teilnehmer-Personen-ID")),
                        ("organization_id", int_id("Verknüpfte Organisations-ID")),
                        ("lead_id", s("Verknüpfte Lead-UUID")),
                    ],
                    &["activity_id"],
                    &[BASE, ACTIVITIES],
                    true,
                ),
                t(
                    "PIPEDRIVE_CREATE_NOTE",
                    ("Notiz anlegen", "Create note"),
                    (
                        "Legt eine Notiz zu Deal, Person, Organisation oder Lead an; braucht --confirm.",
                        "Creates a note on a deal, person, organization or lead; requires --confirm.",
                    ),
                    &[
                        ("content", s("Notiztext, HTML erlaubt")),
                        ("deal_id", int_id("An diesen Deal hängen")),
                        ("pinned_to_deal", boolean("Notiz im Deal anpinnen")),
                        ("person_id", int_id("An diese Person hängen")),
                        ("pinned_to_person", boolean("Notiz an der Person anpinnen")),
                        ("organization_id", int_id("An diese Organisation hängen")),
                        ("pinned_to_organization", boolean("Notiz an der Organisation anpinnen")),
                        ("lead_id", s("An diesen Lead hängen (UUID)")),
                        ("pinned_to_lead", boolean("Notiz am Lead anpinnen")),
                    ],
                    &["content"],
                    &[BASE],
                    true,
                ),
                t(
                    "PIPEDRIVE_UPDATE_NOTE",
                    ("Notiz ändern", "Update note"),
                    (
                        "Ändert Text, Verknüpfung, Autor oder Pin-Status einer Notiz; braucht --confirm.",
                        "Changes a note's text, links, author or pinned flags; requires --confirm.",
                    ),
                    &[
                        ("note_id", int_id("Numerische Notiz-ID")),
                        ("content", s("Neuer Notiztext, HTML erlaubt")),
                        ("deal_id", int_id("An diesen Deal hängen")),
                        ("person_id", int_id("An diese Person hängen")),
                        ("organization_id", int_id("An diese Organisation hängen")),
                        ("lead_id", s("An diesen Lead hängen (UUID)")),
                        ("user_id", int_id("Autor-Nutzer-ID")),
                        ("pinned_to_deal", choice_i("1 = anpinnen, 0 = lösen", &[1, 0])),
                        ("pinned_to_person", choice_i("1 = anpinnen, 0 = lösen", &[1, 0])),
                        ("pinned_to_organization", choice_i("1 = anpinnen, 0 = lösen", &[1, 0])),
                        ("pinned_to_lead", choice_i("1 = anpinnen, 0 = lösen", &[1, 0])),
                    ],
                    &["note_id"],
                    &[BASE],
                    true,
                ),
                t(
                    "PIPEDRIVE_ADD_PRODUCT_TO_DEAL",
                    ("Produkt zum Deal hinzufügen", "Add product to deal"),
                    (
                        "Hängt eine Produktposition mit Preis und Menge an einen Deal; braucht --confirm.",
                        "Attaches a product line with price and quantity to a deal; requires --confirm.",
                    ),
                    &[
                        ("deal_id", int_id("Numerische Deal-ID")),
                        ("product_id", int_id("Produkt-ID (aus PIPEDRIVE_FIND_PRODUCTS)")),
                        ("item_price", number("Preis pro Einheit")),
                        ("quantity", number("Menge")),
                        ("discount", number("Rabatt")),
                        ("discount_type", choice("Rabattart", &["percentage", "amount"])),
                        ("tax", number("Steuer in Prozent")),
                        ("tax_method", choice("Steuerart", &["exclusive", "inclusive", "none"])),
                        ("comments", s("Kommentar zur Position")),
                        ("is_enabled", boolean("Position aktiv (Standard true)")),
                    ],
                    &["deal_id", "product_id", "item_price", "quantity"],
                    &[BASE, DEALS, PRODUCTS],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }
}

/// `#[allow]`-free request building; every arm mirrors one Activepieces `run()`.
#[allow(clippy::too_many_lines)]
fn request(tool: &str, url: impl Fn(&str) -> String, args: &Value) -> Result<HttpRequest> {
    Ok(match tool {
        // AP: src/lib/actions/get-current-user.ts
        "PIPEDRIVE_GET_CURRENT_USER" => HttpRequest::new("GET", url("/v1/users/me")),
        // AP: src/lib/actions/list-users.ts
        "PIPEDRIVE_LIST_USERS" => HttpRequest::new("GET", url("/v1/users")),
        // AP: src/lib/actions/list-pipelines.ts
        "PIPEDRIVE_LIST_PIPELINES" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/pipelines{}",
                query(&[
                    ("sort_by", one_of(args, "sort_by", &["id", "update_time", "add_time"])?),
                    ("sort_direction", sort_dir(args)?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/list-stages.ts
        "PIPEDRIVE_LIST_STAGES" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/stages{}",
                query(&[
                    ("pipeline_id", opt_id(args, "pipeline_id")?),
                    ("sort_by", one_of(args, "sort_by", &["id", "update_time", "add_time", "order_nr"])?),
                    ("sort_direction", sort_dir(args)?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/get-deal.ts
        "PIPEDRIVE_GET_DEAL" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/deals/{}{}",
                id_part(args, "deal_id")?,
                query(&[("include_fields", Some(joined(DEAL_OPTIONAL_FIELDS)))])
            )),
        ),
        // AP: src/lib/actions/list-deals.ts
        "PIPEDRIVE_LIST_DEALS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/deals{}",
                query(&[
                    ("owner_id", opt_id(args, "owner_id")?),
                    ("person_id", opt_id(args, "person_id")?),
                    ("org_id", opt_id(args, "organization_id")?),
                    ("pipeline_id", opt_id(args, "pipeline_id")?),
                    ("stage_id", opt_id(args, "stage_id")?),
                    ("status", one_of(args, "status", &["open", "won", "lost", "deleted"])?),
                    ("updated_since", rfc3339(args, "updated_since")?),
                    ("updated_until", rfc3339(args, "updated_until")?),
                    ("sort_by", one_of(args, "sort_by", &["id", "update_time", "add_time"])?),
                    ("sort_direction", sort_dir(args)?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/search-deals.ts
        "PIPEDRIVE_SEARCH_DEALS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/deals/search{}",
                query(&[
                    ("term", Some(search_term(args)?)),
                    ("fields", fields_arg(args, &["title", "notes", "custom_fields"])?),
                    ("exact_match", opt_bool(args, "exact_match").map(|b| b.to_string())),
                    ("person_id", opt_id(args, "person_id")?),
                    ("organization_id", opt_id(args, "organization_id")?),
                    ("status", one_of(args, "status", &["open", "won", "lost"])?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/list-deal-products.ts
        "PIPEDRIVE_LIST_DEAL_PRODUCTS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/deals/{}/products{}",
                id_part(args, "deal_id")?,
                query(&[
                    ("sort_by", one_of(args, "sort_by", &["id", "add_time", "update_time", "order_nr"])?),
                    ("sort_direction", sort_dir(args)?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/get-person.ts
        "PIPEDRIVE_GET_PERSON" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/persons/{}{}",
                id_part(args, "person_id")?,
                query(&[("include_fields", Some(joined(PERSON_OPTIONAL_FIELDS)))])
            )),
        ),
        // AP: src/lib/actions/search-persons.ts
        "PIPEDRIVE_SEARCH_PERSONS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/persons/search{}",
                query(&[
                    ("term", Some(search_term(args)?)),
                    (
                        "fields",
                        fields_arg(args, &["name", "email", "phone", "notes", "custom_fields"])?
                    ),
                    ("exact_match", opt_bool(args, "exact_match").map(|b| b.to_string())),
                    ("organization_id", opt_id(args, "organization_id")?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/get-organization.ts
        "PIPEDRIVE_GET_ORGANIZATION" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/organizations/{}{}",
                id_part(args, "organization_id")?,
                query(&[("include_fields", Some(joined(ORGANIZATION_OPTIONAL_FIELDS)))])
            )),
        ),
        // AP: src/lib/actions/search-organizations.ts
        "PIPEDRIVE_SEARCH_ORGANIZATIONS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/organizations/search{}",
                query(&[
                    ("term", Some(search_term(args)?)),
                    ("fields", fields_arg(args, &["name", "address", "notes", "custom_fields"])?),
                    ("exact_match", opt_bool(args, "exact_match").map(|b| b.to_string())),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/list-activities.ts
        "PIPEDRIVE_LIST_ACTIVITIES" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/activities{}",
                query(&[
                    ("deal_id", opt_id(args, "deal_id")?),
                    ("lead_id", opt_str(args, "lead_id").map(str::trim).filter(|v| !v.is_empty()).map(String::from)),
                    ("person_id", opt_id(args, "person_id")?),
                    ("org_id", opt_id(args, "organization_id")?),
                    ("owner_id", opt_id(args, "owner_id")?),
                    ("done", opt_bool(args, "done").map(|b| b.to_string())),
                    ("updated_since", rfc3339(args, "updated_since")?),
                    ("updated_until", rfc3339(args, "updated_until")?),
                    ("sort_by", one_of(args, "sort_by", &["id", "update_time", "add_time", "due_date"])?),
                    ("sort_direction", sort_dir(args)?),
                    ("limit", Some(limit_arg(args))),
                    ("cursor", opt_str(args, "cursor").map(String::from)),
                ])
            )),
        ),
        // AP: src/lib/actions/get-lead.ts
        "PIPEDRIVE_GET_LEAD" => HttpRequest::new(
            "GET",
            url(&format!("/v1/leads/{}", lead_ref(args, "lead_id")?)),
        ),
        // AP: src/lib/actions/find-leads.ts
        "PIPEDRIVE_FIND_LEAD" => {
            let exact = opt_bool(args, "exact_match").filter(|b| *b).map(|b| b.to_string());
            HttpRequest::new(
                "GET",
                url(&format!(
                    "/v2/leads/search{}",
                    query(&[
                        ("term", Some(req_str(args, "term")?.trim().to_string())),
                        ("fields", Some("title".into())),
                        ("limit", Some("100".into())),
                        ("exact_match", exact),
                        ("cursor", opt_str(args, "cursor").map(String::from)),
                    ])
                )),
            )
        }
        // AP: src/lib/actions/find-notes.ts
        "PIPEDRIVE_FIND_NOTES" => {
            let object_type =
                require_one_of(args, "object_type", &["deal_id", "lead_id", "person_id", "org_id"])?;
            let object_id = req_str(args, "object_id")?.trim().to_string();
            HttpRequest::new(
                "GET",
                url(&format!(
                    "/v1/notes{}",
                    query(&[
                        ("sort", Some("update_time DESC".into())),
                        (object_type.as_str(), Some(object_id)),
                        // `pipedrivePaginatedV1ApiCall` pages with start/limit;
                        // this adapter returns the first page.
                        ("start", Some("0".into())),
                        ("limit", Some("500".into())),
                    ])
                )),
            )
        }
        // AP: src/lib/actions/find-products.ts
        "PIPEDRIVE_FIND_PRODUCTS" => HttpRequest::new(
            "GET",
            url(&format!(
                "/v2/products/search{}",
                query(&[
                    ("term", Some(req_str(args, "field_value")?.trim().to_string())),
                    ("fields", Some(require_one_of(args, "field", &["name", "code"])?)),
                    ("exact_match", Some("true".into())),
                ])
            )),
        ),
        // AP: src/lib/actions/create-deal.ts
        "PIPEDRIVE_CREATE_DEAL" => {
            let mut body = Map::new();
            put_str(&mut body, args, "title", "title");
            if !body.contains_key("title") {
                bail!("missing argument `title`");
            }
            put_num(&mut body, args, "value", "value")?;
            put_str(&mut body, args, "currency", "currency");
            put_num(&mut body, args, "pipeline_id", "pipeline_id")?;
            put_num(&mut body, args, "stage_id", "stage_id")?;
            put_str(&mut body, args, "status", "status");
            put_num(&mut body, args, "probability", "probability")?;
            if let Some(v) = rfc3339(args, "creation_time")? {
                // dayjs(creationTime).format('YYYY-MM-DDTHH:mm:ss[Z]')
                body.insert("add_time".into(), json!(v));
            }
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_num(&mut body, args, "owner_id", "owner_id")?;
            put_num(&mut body, args, "organization_id", "org_id")?;
            put_num(&mut body, args, "person_id", "person_id")?;
            put_labels(&mut body, args)?;
            put_ymd(&mut body, args, "expected_close_date", "expected_close_date")?;
            HttpRequest::new("POST", url("/v2/deals")).json(&Value::Object(body))
        }
        // AP: src/lib/actions/update-deal.ts
        "PIPEDRIVE_UPDATE_DEAL" => {
            let mut body = Map::new();
            put_str(&mut body, args, "title", "title");
            put_num(&mut body, args, "value", "value")?;
            put_str(&mut body, args, "currency", "currency");
            put_num(&mut body, args, "pipeline_id", "pipeline_id")?;
            put_num(&mut body, args, "stage_id", "stage_id")?;
            put_str(&mut body, args, "status", "status");
            put_num(&mut body, args, "probability", "probability")?;
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_num(&mut body, args, "owner_id", "owner_id")?;
            put_num(&mut body, args, "organization_id", "org_id")?;
            put_num(&mut body, args, "person_id", "person_id")?;
            put_labels(&mut body, args)?;
            put_ymd(&mut body, args, "expected_close_date", "expected_close_date")?;
            if body.is_empty() {
                bail!("nothing to update: pass at least one deal field");
            }
            HttpRequest::new("PATCH", url(&format!("/v2/deals/{}", id_part(args, "deal_id")?)))
                .json(&Value::Object(body))
        }
        // AP: src/lib/actions/delete-deal.ts
        "PIPEDRIVE_DELETE_DEAL" => {
            HttpRequest::new("DELETE", url(&format!("/v2/deals/{}", id_part(args, "deal_id")?)))
        }
        // AP: src/lib/actions/create-person.ts
        "PIPEDRIVE_CREATE_PERSON" | "PIPEDRIVE_UPDATE_PERSON" => {
            let creating = tool == "PIPEDRIVE_CREATE_PERSON";
            let name = non_blank(args, "name");
            let first = non_blank(args, "first_name");
            let last = non_blank(args, "last_name");
            if creating && name.is_some() && (first.is_some() || last.is_some()) {
                bail!("Provide either Name OR First Name/Last Name, not both.");
            }
            if creating && name.is_none() && first.is_none() && last.is_none() {
                bail!("Provide Name or at least one of First Name / Last Name.");
            }
            if first.is_some() != last.is_some() {
                bail!("If First Name is provided, Last Name must be provided as well.");
            }
            let mut body = Map::new();
            put_str(&mut body, args, "name", "name");
            put_str(&mut body, args, "first_name", "first_name");
            put_str(&mut body, args, "last_name", "last_name");
            put_num(&mut body, args, "owner_id", "owner_id")?;
            put_num(&mut body, args, "organization_id", "org_id")?;
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_str(&mut body, args, "marketing_status", "marketing_status");
            put_contact_list(&mut body, args, "email", "emails")?;
            put_contact_list(&mut body, args, "phone", "phones")?;
            put_labels(&mut body, args)?;
            let path = if creating {
                "/v2/persons".to_string()
            } else {
                format!("/v2/persons/{}", id_part(args, "person_id")?)
            };
            HttpRequest::new(if creating { "POST" } else { "PATCH" }, url(&path))
                .json(&Value::Object(body))
        }
        // AP: src/lib/actions/create-organization.ts
        "PIPEDRIVE_CREATE_ORGANIZATION" => {
            let mut body = Map::new();
            put_str(&mut body, args, "name", "name");
            if !body.contains_key("name") {
                bail!("missing argument `name`");
            }
            put_num(&mut body, args, "owner_id", "owner_id")?;
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_address(&mut body, args);
            put_labels(&mut body, args)?;
            HttpRequest::new("POST", url("/v2/organizations")).json(&Value::Object(body))
        }
        // AP: src/lib/actions/update-organization.ts
        "PIPEDRIVE_UPDATE_ORGANIZATION" => {
            let mut body = Map::new();
            put_str(&mut body, args, "name", "name");
            put_num(&mut body, args, "owner_id", "owner_id")?;
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_address(&mut body, args);
            put_labels(&mut body, args)?;
            if body.is_empty() {
                bail!("nothing to update: pass at least one organization field");
            }
            HttpRequest::new(
                "PATCH",
                url(&format!("/v2/organizations/{}", id_part(args, "organization_id")?)),
            )
            .json(&Value::Object(body))
        }
        // AP: src/lib/actions/create-lead.ts
        "PIPEDRIVE_CREATE_LEAD" => {
            let person = opt_num(args, "person_id")?;
            let org = opt_num(args, "organization_id")?;
            if person.is_none() && org.is_none() {
                bail!("Provide a person or an organization: a lead needs at least one of them.");
            }
            let mut body = Map::new();
            put_str(&mut body, args, "title", "title");
            if !body.contains_key("title") {
                bail!("missing argument `title`");
            }
            put_num(&mut body, args, "owner_id", "owner_id")?;
            if let Some(v) = org {
                body.insert("organization_id".into(), v);
            }
            if let Some(v) = person {
                body.insert("person_id".into(), v);
            }
            put_num(&mut body, args, "channel", "channel")?;
            put_num(&mut body, args, "visible_to", "visible_to")?;
            put_ymd(&mut body, args, "expected_close_date", "expected_close_date")?;
            put_lead_labels(&mut body, args)?;
            if let Some(amount) = args.get("value").and_then(Value::as_f64) {
                let Some(currency) = opt_str(args, "currency") else {
                    bail!("`currency` is required when `value` is provided");
                };
                body.insert("value".into(), json!({"amount": amount, "currency": currency}));
            }
            HttpRequest::new("POST", url("/v1/leads")).json(&Value::Object(body))
        }
        // AP: src/lib/actions/create-activity.ts
        "PIPEDRIVE_CREATE_ACTIVITY" | "PIPEDRIVE_UPDATE_ACTIVITY" => {
            let creating = tool == "PIPEDRIVE_CREATE_ACTIVITY";
            let mut body = Map::new();
            put_str(&mut body, args, "subject", "subject");
            if creating && !body.contains_key("subject") {
                bail!("missing argument `subject`");
            }
            put_num(&mut body, args, "organization_id", "org_id")?;
            put_num(&mut body, args, "deal_id", "deal_id")?;
            if let Some(v) = opt_str(args, "lead_id").map(str::trim).filter(|v| !v.is_empty()) {
                body.insert("lead_id".into(), json!(v));
            }
            put_str(&mut body, args, "public_description", "public_description");
            put_str(&mut body, args, "type", "type");
            put_num(&mut body, args, "assign_to", "owner_id")?;
            put_str(&mut body, args, "due_time", "due_time");
            put_str(&mut body, args, "duration", "duration");
            put_bool(&mut body, args, "is_done", "done");
            put_str(&mut body, args, "note", "note");
            if let Some(person) = opt_num(args, "person_id")? {
                body.insert(
                    "participants".into(),
                    json!([{"person_id": person, "primary": true}]),
                );
            }
            if let Some(busy) = one_of(args, "busy", &["free", "busy"])? {
                body.insert("busy".into(), json!(busy == "busy"));
            }
            put_ymd(&mut body, args, "due_date", "due_date")?;
            if !creating && body.is_empty() {
                bail!("nothing to update: pass at least one activity field");
            }
            let path = if creating {
                "/v2/activities".to_string()
            } else {
                format!("/v2/activities/{}", id_part(args, "activity_id")?)
            };
            HttpRequest::new(if creating { "POST" } else { "PATCH" }, url(&path))
                .json(&Value::Object(body))
        }
        // AP: src/lib/actions/create-note.ts
        "PIPEDRIVE_CREATE_NOTE" => {
            let mut body = Map::new();
            put_str(&mut body, args, "content", "content");
            if !body.contains_key("content") {
                bail!("missing argument `content`");
            }
            let deal = opt_num(args, "deal_id")?;
            let person = opt_num(args, "person_id")?;
            let org = opt_num(args, "organization_id")?;
            let lead = opt_str(args, "lead_id").map(str::trim).filter(|v| !v.is_empty());
            if deal.is_none() && person.is_none() && org.is_none() && lead.is_none() {
                bail!("Provide at least one of deal_id, person_id, organization_id or lead_id.");
            }
            // The piece always sends the four pin flags (0 when unset).
            let pin = |key: &str| i64::from(opt_bool(args, key).unwrap_or(false));
            body.insert("pinned_to_deal_flag".into(), json!(pin("pinned_to_deal")));
            body.insert("pinned_to_person_flag".into(), json!(pin("pinned_to_person")));
            body.insert(
                "pinned_to_organization_flag".into(),
                json!(pin("pinned_to_organization")),
            );
            body.insert("pinned_to_lead_flag".into(), json!(pin("pinned_to_lead")));
            if let Some(v) = lead {
                body.insert("lead_id".into(), json!(v));
            }
            if let Some(v) = person {
                body.insert("person_id".into(), v);
            }
            if let Some(v) = org {
                body.insert("org_id".into(), v);
            }
            if let Some(v) = deal {
                body.insert("deal_id".into(), v);
            }
            HttpRequest::new("POST", url("/v1/notes")).json(&Value::Object(body))
        }
        // AP: src/lib/actions/update-note.ts
        "PIPEDRIVE_UPDATE_NOTE" => {
            let mut body = Map::new();
            put_str(&mut body, args, "content", "content");
            put_num(&mut body, args, "deal_id", "deal_id")?;
            if let Some(v) = opt_str(args, "lead_id").map(str::trim).filter(|v| !v.is_empty()) {
                body.insert("lead_id".into(), json!(v));
            }
            put_num(&mut body, args, "person_id", "person_id")?;
            put_num(&mut body, args, "organization_id", "org_id")?;
            put_num(&mut body, args, "user_id", "user_id")?;
            for (key, field) in [
                ("pinned_to_deal", "pinned_to_deal_flag"),
                ("pinned_to_lead", "pinned_to_lead_flag"),
                ("pinned_to_person", "pinned_to_person_flag"),
                ("pinned_to_organization", "pinned_to_organization_flag"),
            ] {
                if let Some(v) = pinned_flag(args, key)? {
                    body.insert(field.into(), json!(v));
                }
            }
            if body.is_empty() {
                bail!("Set at least one field to update on the note.");
            }
            HttpRequest::new("PUT", url(&format!("/v1/notes/{}", id_part(args, "note_id")?)))
                .json(&Value::Object(body))
        }
        // AP: src/lib/actions/add-product-to-deal.ts
        "PIPEDRIVE_ADD_PRODUCT_TO_DEAL" => {
            let mut body = Map::new();
            put_num(&mut body, args, "product_id", "product_id")?;
            if !body.contains_key("product_id") {
                bail!("missing argument `product_id`");
            }
            put_num(&mut body, args, "item_price", "item_price")?;
            if !body.contains_key("item_price") {
                bail!("missing argument `item_price`");
            }
            put_num(&mut body, args, "quantity", "quantity")?;
            if !body.contains_key("quantity") {
                bail!("missing argument `quantity`");
            }
            put_str(&mut body, args, "discount_type", "discount_type");
            put_num(&mut body, args, "discount", "discount")?;
            // comments ?? ''
            body.insert(
                "comments".into(),
                json!(opt_str(args, "comments").unwrap_or("")),
            );
            put_bool(&mut body, args, "is_enabled", "is_enabled");
            put_num(&mut body, args, "tax", "tax")?;
            put_str(&mut body, args, "tax_method", "tax_method");
            HttpRequest::new(
                "POST",
                url(&format!("/v2/deals/{}/products", id_part(args, "deal_id")?)),
            )
            .json(&Value::Object(body))
        }
        other => bail!("unknown Pipedrive tool `{other}`"),
    })
}

/// `label_ids` body field (the piece reads `labelIds` props).
fn put_labels(m: &mut Map<String, Value>, args: &Value) -> Result<()> {
    let ids = num_vec(args, "label_ids")?;
    if !ids.is_empty() {
        m.insert("label_ids".into(), json!(ids));
    }
    Ok(())
}

/// Leads carry their labels in `label_ids` too, but as strings in the piece.
fn put_lead_labels(m: &mut Map<String, Value>, args: &Value) -> Result<()> {
    let ids = str_vec(args, "label_ids");
    if !ids.is_empty() {
        m.insert("label_ids".into(), json!(ids));
    }
    Ok(())
}

/// Emails/phones become `{value, label: "work", primary}` objects, first primary.
fn put_contact_list(
    m: &mut Map<String, Value>,
    args: &Value,
    key: &str,
    field: &str,
) -> Result<()> {
    let values = str_vec(args, key);
    if values.is_empty() {
        return Ok(());
    }
    let items: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, value)| json!({"value": value, "label": "work", "primary": i == 0}))
        .collect();
    m.insert(field.into(), json!(items));
    Ok(())
}

/// The address is sent as `{value: "<text>"}` (create/update organization).
fn put_address(m: &mut Map<String, Value>, args: &Value) {
    if let Some(address) = opt_str(args, "address") {
        m.insert("address".into(), json!({"value": address}));
    }
}

/// `pinnedFlagProp` sends 1 (pin) or 0 (unpin).
fn pinned_flag(args: &Value, key: &str) -> Result<Option<i64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) if n.as_i64() == Some(1) => Ok(Some(1)),
        Some(Value::Number(n)) if n.as_i64() == Some(0) => Ok(Some(0)),
        Some(_) => bail!("`{key}` must be 1 (pin) or 0 (unpin)"),
    }
}

fn non_blank<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

impl ToolkitAdapter for Pipedrive {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        let base = self.api_base.clone();
        request(tool, |resource| format!("{base}/api{resource}"), args)
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = envelope(response)?;
        Ok(match tool {
            "PIPEDRIVE_GET_CURRENT_USER"
            | "PIPEDRIVE_GET_DEAL"
            | "PIPEDRIVE_GET_PERSON"
            | "PIPEDRIVE_GET_ORGANIZATION"
            | "PIPEDRIVE_GET_LEAD" => record(&v)?,
            "PIPEDRIVE_LIST_USERS" | "PIPEDRIVE_FIND_NOTES" => plain_list(&v),
            "PIPEDRIVE_LIST_PIPELINES"
            | "PIPEDRIVE_LIST_STAGES"
            | "PIPEDRIVE_LIST_DEALS"
            | "PIPEDRIVE_LIST_DEAL_PRODUCTS"
            | "PIPEDRIVE_LIST_ACTIVITIES" => cursor_page(&v),
            "PIPEDRIVE_SEARCH_DEALS"
            | "PIPEDRIVE_SEARCH_PERSONS"
            | "PIPEDRIVE_SEARCH_ORGANIZATIONS"
            | "PIPEDRIVE_FIND_LEAD"
            | "PIPEDRIVE_FIND_PRODUCTS" => search_page(&v),
            "PIPEDRIVE_CREATE_DEAL"
            | "PIPEDRIVE_UPDATE_DEAL"
            | "PIPEDRIVE_CREATE_PERSON"
            | "PIPEDRIVE_UPDATE_PERSON"
            | "PIPEDRIVE_CREATE_ORGANIZATION"
            | "PIPEDRIVE_UPDATE_ORGANIZATION"
            | "PIPEDRIVE_CREATE_LEAD"
            | "PIPEDRIVE_CREATE_ACTIVITY"
            | "PIPEDRIVE_UPDATE_ACTIVITY"
            | "PIPEDRIVE_CREATE_NOTE"
            | "PIPEDRIVE_UPDATE_NOTE"
            | "PIPEDRIVE_ADD_PRODUCT_TO_DEAL"
            | "PIPEDRIVE_DELETE_DEAL" => record(&v)?,
            other => bail!("unknown Pipedrive tool `{other}`"),
        })
    }
}

/// Pipedrive envelope: `{success, data, additional_data}`; `success: false` is
/// an API error even on HTTP 200.
fn envelope(response: &HttpResponse) -> Result<Value> {
    let v = response.json()?;
    if v.get("success") == Some(&json!(false)) {
        let msg = v["error"].as_str().unwrap_or("Pipedrive reported a failure");
        bail!("Pipedrive API error: {msg}");
    }
    Ok(v)
}

fn record(v: &Value) -> Result<Value> {
    match v.get("data") {
        Some(d) if d.is_object() => Ok(d.clone()),
        _ => bail!("Pipedrive returned no data record"),
    }
}

fn plain_list(v: &Value) -> Value {
    let data = v["data"].as_array().cloned().unwrap_or_default();
    json!({
        "found": !data.is_empty(),
        "data": data,
        "more_items": v["additional_data"]["pagination"]["more_items_in_collection"]
            .as_bool()
            .unwrap_or(false),
    })
}

fn cursor_page(v: &Value) -> Value {
    let data = v["data"].as_array().cloned().unwrap_or_default();
    json!({
        "found": !data.is_empty(),
        "data": data,
        "next_cursor": v["additional_data"]["next_cursor"],
    })
}

/// `data.items[].item` of the v2 search endpoints; `result_score` is kept.
fn search_page(v: &Value) -> Value {
    let data: Vec<Value> = v["data"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let mut item = entry["item"].clone();
            if let (Some(o), Some(score)) = (item.as_object_mut(), entry.get("result_score")) {
                o.insert("result_score".into(), score.clone());
            }
            item
        })
        .collect();
    json!({
        "found": !data.is_empty(),
        "data": data,
        "next_cursor": v["additional_data"]["next_cursor"],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_of(tool: &str, args: Value) -> Result<HttpRequest> {
        let a = Pipedrive::with_api_base("https://api.pipedrive.test");
        a.build_request(tool, &args)
    }

    #[test]
    fn ids_and_dates_are_validated() {
        assert!(request_of("PIPEDRIVE_GET_DEAL", json!({"deal_id": "../me"})).is_err());
        assert!(request_of("PIPEDRIVE_GET_DEAL", json!({"deal_id": "12x"})).is_err());
        assert!(request_of("PIPEDRIVE_GET_LEAD", json!({"lead_id": "a/b"})).is_err());
        assert!(request_of("PIPEDRIVE_UPDATE_DEAL", json!({"deal_id": 3})).is_err());
        assert!(request_of("PIPEDRIVE_LIST_DEALS", json!({"status": "bogus"})).is_err());
        assert!(request_of(
            "PIPEDRIVE_LIST_DEALS",
            json!({"updated_since": "yesterday"})
        )
        .is_err());
        assert!(request_of(
            "PIPEDRIVE_SEARCH_DEALS",
            json!({"term": "a"})
        )
        .is_err());
        assert!(request_of("PIPEDRIVE_SEARCH_DEALS", json!({"term": "a", "exact_match": true})).is_ok());
    }

    #[test]
    fn rfc3339_and_ymd_normalization() {
        let a = Value::Null;
        assert_eq!(
            rfc3339(&json!({"v": "2026-09-01"}), "v").unwrap(),
            Some("2026-09-01T00:00:00Z".to_string())
        );
        assert_eq!(
            rfc3339(&json!({"v": "2026-09-01T10:30:00.500Z"}), "v").unwrap(),
            Some("2026-09-01T10:30:00Z".to_string())
        );
        assert_eq!(
            rfc3339(&json!({"v": "2026-09-01T10:30:00"}), "v").unwrap(),
            Some("2026-09-01T10:30:00Z".to_string())
        );
        assert!(rfc3339(&json!({"v": "2026-09-01T10:30:00+02:00"}), "v").is_err());
        assert!(rfc3339(&a, "v").unwrap().is_none());
        assert_eq!(
            ymd(&json!({"v": "2026-09-01T10:30:00Z"}), "v").unwrap(),
            Some("2026-09-01".to_string())
        );
        assert!(ymd(&json!({"v": "01.09.2026"}), "v").is_err());
    }

    #[test]
    fn person_name_rules_match_the_piece() {
        assert!(request_of("PIPEDRIVE_CREATE_PERSON", json!({"name": "Ada", "first_name": "A"})).is_err());
        assert!(request_of("PIPEDRIVE_CREATE_PERSON", json!({})).is_err());
        assert!(request_of("PIPEDRIVE_CREATE_PERSON", json!({"first_name": "Ada"})).is_err());
        assert!(request_of(
            "PIPEDRIVE_CREATE_PERSON",
            json!({"first_name": "Ada", "last_name": "Lovelace"})
        )
        .is_ok());
        assert!(request_of("PIPEDRIVE_UPDATE_PERSON", json!({"person_id": 1})).is_ok());
    }

    #[test]
    fn note_and_lead_associations_are_required() {
        assert!(request_of("PIPEDRIVE_CREATE_NOTE", json!({"content": "hi"})).is_err());
        assert!(request_of("PIPEDRIVE_CREATE_LEAD", json!({"title": "T"})).is_err());
        assert!(request_of(
            "PIPEDRIVE_CREATE_LEAD",
            json!({"title": "T", "person_id": 1, "value": 5})
        )
        .is_err());
        assert!(request_of("PIPEDRIVE_UPDATE_NOTE", json!({"note_id": 1})).is_err());
        let r = request_of(
            "PIPEDRIVE_CREATE_NOTE",
            json!({"content": "hi", "deal_id": 3}),
        )
        .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["pinned_to_deal_flag"], json!(0));
        assert_eq!(body["pinned_to_lead_flag"], json!(0));
    }
}
