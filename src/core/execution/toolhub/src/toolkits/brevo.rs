//! Brevo toolkit (REST v3, formerly Sendinblue). Action set modelled on
//! Activepieces `piece-sendinblue` (MIT).
//!
//! Auth: the piece uses a single `PieceAuth.SecretText` API key that its shared
//! `apiCall` helper sends verbatim as an `api-key` request header (no prefix)
//! against `https://api.brevo.com/v3` — replicated here with
//! `AuthScheme::ApiKey { header_name: "api-key", prefix: "" }`. Brevo API keys
//! carry no scopes; like the Linear port the `scopes` of each tool carry the
//! permission level the call needs (`read`, `write`).
//!
//! Every request arm below is the `brevoCommon.apiCall` of one TS action
//! (`method`, `resourceUri`, `query`, `body`), with `compactBody` mirrored by
//! simply not inserting absent values. Identifiers that the TS interpolates
//! through `encodeURIComponent` (emails, phone numbers, contact/company ids)
//! are percent-encoded here; numeric ids the TS interpolates raw are checked
//! to be digits only.
//!
//! Deviations (kept minimal and documented):
//! - `create_or_update_contact` ports the upsert `POST /contacts`; the TS
//!   action's trailing `GET /contacts/{email}` echo is folded into
//!   `{success: true}` because an adapter is one request/one response.
//! - `create_or_update_email_template` returns `{success: true}` when Brevo
//!   answers the update branch with an empty body (the TS echoes the input id).
//! - The 404 -> `found:false` mapping of the Find/Get actions is implemented
//!   in `parse_response`; through the hub a 404 surfaces as a provider error.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "brevo";

const READ: &str = "read";
const WRITE: &str = "write";

/// StaticDropdown values of the identifier-typed contact actions.
const IDENTIFIER_TYPES: &[&str] = &[
    "email_id",
    "phone_id",
    "contact_id",
    "ext_id",
    "whatsapp_id",
    "landline_number_id",
];

const CAMPAIGN_STATS: &[&str] = &[
    "globalStats",
    "linksStats",
    "statsByDomain",
    "statsByDevice",
    "statsByBrowser",
];

const ASC_DESC: &[&str] = &["asc", "desc"];

// ---------------------------------------------------------------- schema helpers

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn flag_d(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn num(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

fn obj_p(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn datetime(desc: &str) -> Value {
    json!({"type": "string", "description": desc, "format": "date-time"})
}

/// Contact id list; Activepieces fills these from a dropdown of list ids.
fn id_list(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}

/// `{email, name?}` rows: recipients of the transactional email.
fn people_prop(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {
        "type": "object",
        "properties": {
            "email": {"type": "string", "description": "E-Mail-Adresse / email address"},
            "name": {"type": "string", "description": "Anzeigename / display name"}},
        "required": ["email"]}})
}

/// `{url, name}` rows: attachments of the transactional email.
fn attachment_prop(desc: &str) -> Value {
    json!({"type": "array", "description": desc, "items": {
        "type": "object",
        "properties": {
            "url": {"type": "string", "description": "Öffentliche URL der Datei / public URL of the file"},
            "name": {"type": "string", "description": "Dateiname / file name"}},
        "required": ["url"]}})
}

fn offset_p() -> (&'static str, Value) {
    (
        "offset",
        int("Index of the first record to return. Default 0.", 0, 1_000_000, 0),
    )
}

// ---------------------------------------------------------------- id helpers

/// Whole-number path id (campaign, list, template, process ids): digits only.
fn numeric_id(args: &Value, key: &str) -> Result<u64> {
    match args.get(key) {
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| anyhow!("`{key}` must be a positive whole number")),
        Some(Value::String(st)) => st
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("`{key}` must be a whole number")),
        _ => bail!("missing argument `{key}`"),
    }
}

/// Optional whole number; present in the schema but unset stays unset.
fn opt_num(args: &Value, key: &str, min: u64, max: u64) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => {
            let n = numeric_id(args, key)?;
            if n < min || n > max {
                bail!("`{key}` must be between {min} and {max}");
            }
            Ok(Some(n))
        }
    }
}

/// Same, rendered for a query pair.
fn opt_num_sent(args: &Value, key: &str, min: u64, max: u64) -> Result<Option<String>> {
    Ok(opt_num(args, key, min, max)?.map(|n| n.to_string()))
}

/// List of whole-number ids; strings are coerced like the TS `Number(...)` does.
fn id_vec(args: &Value, key: &str) -> Vec<u64> {
    match args.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::Number(n) => n.as_u64(),
                Value::String(st) => st.trim().parse().ok(),
                _ => None,
            })
            .collect(),
        Some(v) => v.as_u64().into_iter().collect(),
        _ => Vec::new(),
    }
}

/// String list that also flattens non-strings via their JSON text (linked deals).
fn str_vec_all(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::String(st) => {
                    let t = st.trim();
                    (!t.is_empty()).then(|| t.to_string())
                }
                other => (!other.is_null()).then(|| other.to_string()),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    match args.get(key) {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::String(st)) => match st.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// `identifier_type` with the Activepieces default `email_id`.
fn identifier_type(args: &Value) -> Result<String> {
    let v = opt_str(args, "identifier_type").unwrap_or("email_id");
    if IDENTIFIER_TYPES.contains(&v) {
        Ok(v.to_string())
    } else {
        bail!(
            "`identifier_type` must be one of {}",
            IDENTIFIER_TYPES.join(", ")
        )
    }
}

fn enum_arg<'a>(args: &'a Value, key: &str, allowed: &[&str]) -> Result<Option<&'a str>> {
    match opt_str(args, key) {
        Some(v) if allowed.contains(&v) => Ok(Some(v)),
        Some(_) => bail!("`{key}` must be one of {}", allowed.join(", ")),
        None => Ok(None),
    }
}

/// Percent-encode a value that the TS interpolates via `encodeURIComponent`.
fn encoded_segment(args: &Value, key: &str) -> Result<String> {
    let v = req_str(args, key)?;
    if v.len() > 200 || v.contains('/') || v.contains('?') || v.contains('#') {
        bail!("invalid argument `{key}`");
    }
    Ok(url::form_urlencoded::byte_serialize(v.as_bytes()).collect())
}

fn checked_sort(args: &Value) -> Result<Option<&str>> {
    enum_arg(args, "sort", ASC_DESC)
}

// ---------------------------------------------------------------- body helpers

type Body = Map<String, Value>;

fn put(m: &mut Body, key: &str, value: Value) {
    m.insert(key.into(), value);
}

fn put_str(m: &mut Body, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_str(args, key) {
        m.insert(field.into(), json!(v));
    }
}

fn put_bool(m: &mut Body, args: &Value, key: &str, field: &str) {
    if let Some(v) = opt_bool(args, key) {
        m.insert(field.into(), json!(v));
    }
}

/// Object prop, skipped when absent or empty (`brevoCommon.isEmptyObject`).
fn put_object(m: &mut Body, args: &Value, key: &str, field: &str) {
    if let Some(Value::Object(o)) = args.get(key) {
        if !o.is_empty() {
            m.insert(field.into(), json!(o));
        }
    }
}

fn put_ids(m: &mut Body, args: &Value, key: &str, field: &str) {
    let ids = id_vec(args, key);
    if !ids.is_empty() {
        m.insert(field.into(), json!(ids));
    }
}

/// Recipient rows of the transactional email (`{email, name?}`); plain
/// strings are accepted as addresses without a name.
fn recipients(args: &Value, key: &str) -> Vec<Value> {
    match args.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::Object(o) => {
                    let email = o
                        .get("email")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())?;
                    let mut m = Map::new();
                    m.insert("email".into(), json!(email));
                    if let Some(name) = o.get("name").and_then(Value::as_str).filter(|n| !n.is_empty())
                    {
                        m.insert("name".into(), json!(name));
                    }
                    Some(Value::Object(m))
                }
                Value::String(st) => {
                    let t = st.trim();
                    (!t.is_empty()).then(|| json!({"email": t}))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Attachment rows (`{url, name}`) of the transactional email.
fn attachments(args: &Value) -> Vec<Value> {
    match args.get("attachments") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| {
                let o = v.as_object()?;
                let url = o.get("url").and_then(Value::as_str).filter(|s| !s.is_empty())?;
                let name = o.get("name").and_then(Value::as_str).filter(|s| !s.is_empty())?;
                Some(json!({"url": url, "name": name}))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `sender` object shared by the campaign and template actions.
fn sender_object(args: &Value) -> Result<Value> {
    let mut sender = Map::new();
    sender.insert("email".into(), json!(req_str(args, "sender_email")?));
    if let Some(name) = opt_str(args, "sender_name") {
        sender.insert("name".into(), json!(name));
    }
    Ok(Value::Object(sender))
}

/// Inline import rows: `{email, attributes?}` objects with a non-empty email.
fn import_rows(args: &Value) -> Vec<Value> {
    match args.get("json_body") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|row| {
                let o = row.as_object()?;
                let email = o
                    .get("email")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?;
                let mut m = Map::new();
                m.insert("email".into(), json!(email));
                if let Some(attrs) = o.get("attributes").and_then(Value::as_object) {
                    if !attrs.is_empty() {
                        m.insert("attributes".into(), json!(attrs));
                    }
                }
                Some(Value::Object(m))
            })
            .collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- adapter

pub struct Brevo {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Brevo {
    fn default() -> Self {
        Self::with_api_base("https://api.brevo.com/v3")
    }
}

#[allow(clippy::too_many_lines)]
impl Brevo {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Brevo".into(),
            description: LocalizedText::new(
                "Brevo (ehemals Sendinblue): Kontakte, Kampagnen, Templates und CRM-Daten lesen und bearbeiten.",
                "Brevo (formerly Sendinblue): read and change contacts, campaigns, templates and CRM data.",
            ),
            auth_schemes: vec![AuthScheme::ApiKey {
                header_name: "api-key".into(),
                prefix: String::new(),
            }],
            tools: vec![
                // ------------------------------------------------------------- read
                t(
                    "BREVO_GET_ACCOUNT_INFO",
                    ("Kontoinformationen lesen", "Get account info"),
                    (
                        "Liest Kontoinhaber, Tarif und Restguthaben.",
                        "Reads the account holder, plan and remaining credits.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_FIND_CONTACT",
                    ("Kontakt finden", "Find contact"),
                    (
                        "Sucht einen Kontakt per E-Mail, Telefon, ID oder External ID.",
                        "Looks a contact up by email, phone, id or external id.",
                    ),
                    &[
                        ("identifier", s("E-Mail, Nummer oder ID des Kontakts")),
                        (
                            "identifier_type",
                            choice("Woran der Wert erkannt wird", IDENTIFIER_TYPES),
                        ),
                    ],
                    &["identifier"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_CONTACTS",
                    ("Kontakte auflisten", "List contacts"),
                    (
                        "Listet Kontakte, optional nach Liste, Segment oder Änderungsdatum.",
                        "Lists contacts, optionally by list, segment or modification date.",
                    ),
                    &[
                        ("list_ids", id_list("Nur Kontakte dieser Listen (List-IDs)")),
                        ("segment_id", s("Nur Kontakte dieses Segments")),
                        ("created_since", datetime("Nur Kontakte, ab diesem Zeitpunkt erstellt")),
                        ("modified_since", datetime("Nur Kontakte, ab diesem Zeitpunkt geändert")),
                        ("limit", int("Kontakte pro Seite, Standard 50", 1, 1000, 50)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_CONTACT_CAMPAIGN_STATS",
                    ("Kontakt-Kampagnenstatistik", "Get contact campaign stats"),
                    (
                        "Liest Öffnungs-, Klick- und Zustellstatistiken eines Kontakts.",
                        "Reads a contact's open, click and delivery statistics.",
                    ),
                    &[
                        ("identifier", s("E-Mail oder numerische Kontakt-ID")),
                        ("start_date", s("Startdatum YYYY-MM-DD (nur mit Enddatum)")),
                        ("end_date", s("Enddatum YYYY-MM-DD (max. 90 Tage Spanne)")),
                    ],
                    &["identifier"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_CONTACT_ATTRIBUTES",
                    ("Kontaktattribute auflisten", "List contact attributes"),
                    (
                        "Listet alle im Konto konfigurierten Kontaktattribute.",
                        "Lists every contact attribute configured in the account.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_CONTACT_LISTS",
                    ("Kontaktlisten auflisten", "List contact lists"),
                    (
                        "Listet die Kontaktlisten mit ID, Ordner und Abonnentenzahl.",
                        "Lists the contact lists with id, folder and subscriber count.",
                    ),
                    &[
                        ("limit", int("Listen pro Seite, Standard 10", 1, 50, 10)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_CONTACT_LIST",
                    ("Kontaktliste lesen", "Get contact list"),
                    (
                        "Liest eine Kontaktliste per ID, optional mit Kampagnenstatistik.",
                        "Reads one contact list by id, optionally with campaign statistics.",
                    ),
                    &[
                        ("list_id", num("ID der Liste")),
                        ("start_date", s("Startdatum, z. B. 2026-01-01 (nur mit Enddatum)")),
                        ("end_date", s("Enddatum, z. B. 2026-01-31")),
                    ],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_COMPANIES",
                    ("Firmen auflisten", "List companies"),
                    (
                        "Listet CRM-Firmen mit Sortierung und Filtern.",
                        "Lists CRM companies with sorting and filters.",
                    ),
                    &[
                        ("page", num("1-basierte Seitennummer")),
                        ("limit", int("Firmen pro Seite, Standard 50", 1, 100, 50)),
                        ("sort", choice("Sortierung", ASC_DESC)),
                        ("sort_by", s("Feld zum Sortieren, z. B. createdAt oder name")),
                        ("filters", s("Roh-JSON-Filter nach Brevo-Doku, z. B. {\"attributes.name\":\"Acme\"}")),
                        ("linked_contacts_ids", id_list("Nur Firmen mit Verknüpfung zu diesen Kontakt-IDs")),
                        ("linked_deals_ids", id_list("Nur Firmen mit Verknüpfung zu diesen Deal-IDs")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_COMPANY",
                    ("Firma lesen", "Get company"),
                    (
                        "Liest eine CRM-Firma per ID.",
                        "Reads one CRM company by id.",
                    ),
                    &[("company_id", s("Firmen-ID, z. B. 61a5cd07ca1347c82306ad06"))],
                    &["company_id"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_CRM_NOTES",
                    ("CRM-Notizen auflisten", "List CRM notes"),
                    (
                        "Listet CRM-Notizen, optional nach Entität oder Zeitraum.",
                        "Lists CRM notes, optionally by entity or date range.",
                    ),
                    &[
                        ("entity", choice("Nur Notizen dieses Entitätstyps", &["companies", "deals", "contacts"])),
                        ("entity_ids", s("Kommagetrennte Entitäts-IDs")),
                        ("date_from", num("Von, Unix-Zeitstempel in Millisekunden")),
                        ("date_to", num("Bis, Unix-Zeitstempel in Millisekunden")),
                        ("limit", int("Notizen pro Seite, Standard 50", 1, 1000, 50)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_EMAIL_CAMPAIGNS",
                    ("E-Mail-Kampagnen auflisten", "List email campaigns"),
                    (
                        "Listet E-Mail-Kampagnen mit Status- und Datumsfiltern.",
                        "Lists email campaigns with status and date filters.",
                    ),
                    &[
                        ("type", choice("Nur dieser Kampagnentyp", &["classic", "trigger"])),
                        (
                            "status",
                            choice("Nur dieser Status", &["suspended", "archive", "sent", "queued", "draft", "inProcess", "inReview"]),
                        ),
                        ("start_date", datetime("Von (nur bei Status sent oder leer)")),
                        ("end_date", datetime("Bis (nur bei Status sent oder leer)")),
                        ("limit", int("Kampagnen pro Seite, Standard 50", 1, 100, 50)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                        ("exclude_html_content", flag("HTML-Inhalt weglassen")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_EMAIL_CAMPAIGN",
                    ("E-Mail-Kampagne lesen", "Get email campaign"),
                    (
                        "Liest eine E-Mail-Kampagne mit Statistiken.",
                        "Reads one email campaign including its statistics.",
                    ),
                    &[
                        ("campaign_id", num("ID der Kampagne")),
                        ("statistics", choice("Statistikteil, leer = voller Report", CAMPAIGN_STATS)),
                    ],
                    &["campaign_id"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_EMAIL_TEMPLATES",
                    ("E-Mail-Templates auflisten", "List email templates"),
                    (
                        "Listet die E-Mail-Templates des Kontos.",
                        "Lists the email templates of the account.",
                    ),
                    &[
                        ("template_status", flag("Nur aktive (true) oder inaktive (false) Templates")),
                        ("limit", int("Templates pro Seite, Standard 50", 1, 1000, 50)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_EMAIL_TEMPLATE",
                    ("E-Mail-Template lesen", "Get email template"),
                    (
                        "Liest ein E-Mail-Template per ID.",
                        "Reads one email template by id.",
                    ),
                    &[("template_id", s("ID des Templates, z. B. 12"))],
                    &["template_id"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_SMS_CAMPAIGNS",
                    ("SMS-Kampagnen auflisten", "List SMS campaigns"),
                    (
                        "Listet SMS-Kampagnen mit Status- und Datumsfiltern.",
                        "Lists SMS campaigns with status and date filters.",
                    ),
                    &[
                        (
                            "status",
                            choice("Nur dieser Status", &["sent", "draft", "queued", "suspended", "inProcess", "archive"]),
                        ),
                        ("start_date", datetime("Von (nur gesendete Kampagnen)")),
                        ("end_date", datetime("Bis (nur gesendete Kampagnen)")),
                        ("limit", int("Kampagnen pro Seite, Standard 500", 1, 1000, 500)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Erstellungsdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_SMS_CAMPAIGN",
                    ("SMS-Kampagne lesen", "Get SMS campaign"),
                    (
                        "Liest eine SMS-Kampagne mit Statistiken.",
                        "Reads one SMS campaign including its statistics.",
                    ),
                    &[("campaign_id", num("ID der Kampagne"))],
                    &["campaign_id"],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_SENDERS",
                    ("Absender auflisten", "List senders"),
                    (
                        "Listet verifizierte Absender, optional nach IP oder Domain.",
                        "Lists verified senders, optionally by IP or domain.",
                    ),
                    &[
                        ("ip", s("Nur Absender dieser dedizierten IP")),
                        ("domain", s("Nur Absender dieser Domain")),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_SENDER_DOMAINS",
                    ("Absender-Domains auflisten", "List sender domains"),
                    (
                        "Listet Absender-Domains mit Authentifizierungsstatus.",
                        "Lists sender domains with their authentication status.",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_LIST_TRANSACTIONAL_EMAIL_EVENTS",
                    ("Transaktions-E-Mail-Events auflisten", "List transactional email events"),
                    (
                        "Listet Transaktions-E-Mail-Events (zugestellt, geöffnet, gebounct …).",
                        "Lists transactional email events (delivered, opened, bounced, …).",
                    ),
                    &[
                        ("days", int("Letzte N Tage, 1-90; nicht mit Start-/Enddatum", 1, 90, 30)),
                        ("start_date", s("Startdatum YYYY-MM-DD (nur mit Enddatum)")),
                        ("end_date", s("Enddatum YYYY-MM-DD (max. 90 Tage)")),
                        ("email", s("Nur Events dieser Empfängeradresse")),
                        (
                            "event",
                            choice("Nur dieser Eventtyp",
                                &["bounces", "hardBounces", "softBounces", "delivered", "spam", "requests",
                                  "opened", "clicks", "invalid", "deferred", "blocked", "unsubscribed",
                                  "error", "loadedByProxy"]),
                        ),
                        ("tags", s("Kommagetrennte Tags zum Filtern")),
                        ("message_id", s("Nur Events dieser Message-ID")),
                        ("template_id", num("Nur Events dieses Template-IDs")),
                        ("limit", int("Events pro Seite, Standard 50", 1, 5000, 50)),
                        offset_p(),
                        ("sort", choice("Sortierung nach Eventdatum", ASC_DESC)),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "BREVO_GET_IMPORT_PROCESS",
                    ("Importprozess lesen", "Get import process"),
                    (
                        "Liest den Status eines Hintergrundprozesses wie eines Imports.",
                        "Reads the status of a background process such as an import.",
                    ),
                    &[("process_id", num("Prozess-ID aus BREVO_IMPORT_CONTACTS"))],
                    &["process_id"],
                    &[READ],
                    false,
                ),
                // ------------------------------------------------------------ write
                t(
                    "BREVO_CREATE_OR_UPDATE_CONTACT",
                    ("Kontakt anlegen oder aktualisieren", "Create or update contact"),
                    (
                        "Upsert eines Kontakts per E-Mail; braucht --confirm.",
                        "Upserts a contact keyed on email; requires --confirm.",
                    ),
                    &[
                        ("email", s("E-Mail, an der der Kontakt erkannt wird")),
                        ("ext_id", s("Eigene ID für diesen Kontakt")),
                        ("attributes", obj_p("Attribute aus dem Brevo-Konto, z. B. FIRSTNAME")),
                        ("list_ids", id_list("Listen, zu denen der Kontakt kommt")),
                        ("email_blacklisted", flag("E-Mail-Abmeldung setzen (true)")),
                        ("sms_blacklisted", flag("SMS-Abmeldung setzen (true)")),
                        ("blocked_sender_addresses", str_list("Absender, deren Transaktionsmails dieser Kontakt nicht bekommt")),
                    ],
                    &["email"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_UPDATE_CONTACT",
                    ("Kontakt ändern", "Update contact"),
                    (
                        "Ändert einen bestehenden Kontakt per ID oder E-Mail; braucht --confirm.",
                        "Changes an existing contact by id or email; requires --confirm.",
                    ),
                    &[
                        ("identifier", s("Wert, über den der Kontakt gefunden wird")),
                        ("identifier_type", choice("Woran der Wert erkannt wird", IDENTIFIER_TYPES)),
                        ("attributes", obj_p("Attribute in GROSSBUCHSTABEN, z. B. {\"FNAME\":\"Elly\"}")),
                        ("ext_id", s("Neue externe ID setzen")),
                        ("email_blacklisted", flag("E-Mail-Blacklist (true/false); leer = unverändert")),
                        ("sms_blacklisted", flag("SMS-Blacklist (true/false); leer = unverändert")),
                        ("list_ids", id_list("Listen, zu denen der Kontakt kommt")),
                        ("unlink_list_ids", id_list("Listen, aus denen der Kontakt entfernt wird")),
                        ("blocked_sender_addresses", str_list("Absender, deren Transaktionsmails dieser Kontakt nicht bekommt")),
                    ],
                    &["identifier"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_UNSUBSCRIBE_CONTACT",
                    ("Kontakt abmelden", "Unsubscribe contact"),
                    (
                        "Setzt E-Mail-/SMS-Blacklist und optional Listen ab; braucht --confirm.",
                        "Blacklists email/SMS and optionally removes lists; requires --confirm.",
                    ),
                    &[
                        ("email", s("E-Mail, an der der Kontakt erkannt wird")),
                        ("unsubscribe_email", flag_d("Von E-Mail abmelden", true)),
                        ("unsubscribe_sms", flag("Von SMS abmelden")),
                        ("unlink_list_ids", id_list("Listen, aus denen der Kontakt entfernt wird")),
                    ],
                    &["email"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_DELETE_CONTACT",
                    ("Kontakt löschen", "Delete contact"),
                    (
                        "Löscht einen Kontakt endgültig; braucht --confirm.",
                        "Permanently deletes a contact; requires --confirm.",
                    ),
                    &[
                        ("identifier", s("Wert, über den der Kontakt gefunden wird")),
                        ("identifier_type", choice("Woran der Wert erkannt wird", IDENTIFIER_TYPES)),
                    ],
                    &["identifier"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_CONTACT_LIST",
                    ("Kontaktliste anlegen", "Create contact list"),
                    (
                        "Legt eine Kontaktliste in einem Ordner an; braucht --confirm.",
                        "Creates a contact list in a folder; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name der Liste")),
                        ("folder_id", num("ID des Ordners (Standardordner meist 1)")),
                    ],
                    &["name", "folder_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_COMPANY",
                    ("Firma anlegen", "Create company"),
                    (
                        "Legt eine CRM-Firma an; braucht --confirm.",
                        "Creates a CRM company; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name der Firma")),
                        ("attributes", obj_p("Firmenattribute wie domain, industry, phone_number, address")),
                        ("country_code", num("Ländervorwahl für Telefonattribute, z. B. 49")),
                        ("linked_contacts_ids", id_list("Kontakt-IDs, die verknüpft werden")),
                        ("linked_deals_ids", id_list("Deal-IDs, die verknüpft werden")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_DELETE_COMPANY",
                    ("Firma löschen", "Delete company"),
                    (
                        "Löscht eine CRM-Firma endgültig; braucht --confirm.",
                        "Permanently deletes a CRM company; requires --confirm.",
                    ),
                    &[("company_id", s("Firmen-ID, z. B. 61a5cd07ca1347c82306ad06"))],
                    &["company_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_EVENT",
                    ("Event tracken", "Create event"),
                    (
                        "Zeichnet ein Custom-Event für einen Kontakt auf; braucht --confirm.",
                        "Records a custom event for a contact; requires --confirm.",
                    ),
                    &[
                        ("event_name", s("Name des Events, z. B. order_completed")),
                        ("event_date", datetime("Zeitpunkt; leer = jetzt")),
                        ("event_properties", obj_p("Eventdaten, z. B. Bestellnummer und Summe")),
                        ("email", s("E-Mail des Kontakts")),
                        ("contact_properties", obj_p("Kontaktattribute, die aktualisiert werden, z. B. FIRSTNAME")),
                    ],
                    &["event_name", "email"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_IMPORT_CONTACTS",
                    ("Kontakte importieren", "Import contacts"),
                    (
                        "Startet einen Bulk-Import aus Datei-URL oder Zeilen; braucht --confirm.",
                        "Starts a bulk import from a file URL or rows; requires --confirm.",
                    ),
                    &[
                        ("file_url", s("Öffentliche URL einer .txt-, .csv- oder .json-Datei")),
                        ("json_body", json!({"type": "array", "description": "Kontaktzeilen statt Datei, Objekte mit email und optional attributes",
                            "items": {"type": "object", "properties": {
                                "email": {"type": "string", "description": "E-Mail-Adresse"},
                                "attributes": {"type": "object", "description": "Attribute dieser Zeile"}},
                                "required": ["email"]}})),
                        ("list_ids", id_list("Bestehende Listen, in die importiert wird")),
                        ("new_list_name", s("Neue Liste mit diesem Namen anlegen (braucht Ordner-ID)")),
                        ("new_list_folder_id", num("Ordner der neuen Liste")),
                        ("notify_url", s("Webhook-URL, die Brevo nach Abschluss aufruft")),
                        ("email_blacklist", flag_d("Importierte E-Mails auf die Blacklist setzen", false)),
                        ("sms_blacklist", flag_d("Importierte SMS-Nummern auf die Blacklist setzen", false)),
                        ("update_existing_contacts", flag_d("Bestehende Kontakte aktualisieren", true)),
                        ("empty_contacts_attributes", flag_d("Fehlende Attribute bestehender Kontakte leeren", false)),
                    ],
                    &[],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_SEND_TRANSACTIONAL_EMAIL",
                    ("Transaktions-E-Mail senden", "Send transactional email"),
                    (
                        "Sendet eine einzelne Transaktions-E-Mail; braucht --confirm.",
                        "Sends a single transactional email; requires --confirm.",
                    ),
                    &[
                        ("to", people_prop("Empfänger")),
                        ("cc", people_prop("Cc-Empfänger")),
                        ("bcc", people_prop("Bcc-Empfänger")),
                        ("reply_to", s("Antwortadresse; leer = Absender")),
                        ("sender_email", s("Verifizierter Absender (Pflicht ohne Template)")),
                        ("sender_name", s("Anzeigename des Absenders")),
                        ("template_id", num("ID eines gespeicherten Templates statt Inhalt")),
                        ("params", obj_p("Platzhalterwerte für das Template")),
                        ("subject", s("Betreff (Pflicht ohne Template)")),
                        ("html_content", s("HTML-Inhalt (Pflicht ohne Template)")),
                        ("text_content", s("Textversion für Plaintext-Postfächer")),
                        ("attachments", attachment_prop("Anhänge per URL")),
                        ("tags", str_list("Labels zum Filtern im Reporting")),
                        ("scheduled_at", datetime("Sendezeitpunkt; leer = sofort")),
                        ("sandbox", flag("Sandbox: prüfen, aber nichts zustellen")),
                    ],
                    &["to"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_SEND_TRANSACTIONAL_SMS",
                    ("Transaktions-SMS senden", "Send transactional SMS"),
                    (
                        "Sendet eine einzelne Transaktions-SMS; braucht --confirm.",
                        "Sends a single transactional SMS; requires --confirm.",
                    ),
                    &[
                        ("sender", s("Absender, max. 11 alphanumerisch oder 15 Ziffern")),
                        ("recipient", s("Empfänger mit Ländervorwahl, ohne + oder führende Nullen")),
                        ("content", s("Nachrichtentext; ab 160 Zeichen mehrere SMS")),
                        ("type", choice("Transactional für Auftrags- und Kontonachrichten", &["transactional", "marketing"])),
                        ("tag", s("Label zum Filtern im Reporting")),
                        ("web_url", s("Webhook-URL für Zustellberichte")),
                        ("unicode_enabled", flag("Als Unicode senden (Emoji; 70 Zeichen/Teil)")),
                        ("organisation_prefix", s("Markenname vor der Nachricht")),
                    ],
                    &["sender", "recipient", "content"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_EMAIL_CAMPAIGN",
                    ("E-Mail-Kampagne anlegen", "Create email campaign"),
                    (
                        "Legt eine E-Mail-Kampagne (Entwurf oder geplant) an; braucht --confirm.",
                        "Creates an email campaign (draft or scheduled); requires --confirm.",
                    ),
                    &[
                        ("name", s("Interner Kampagnenname, nur im Dashboard sichtbar")),
                        ("sender_email", s("Verifizierter Absender (oder Sender-ID)")),
                        ("sender_name", s("Anzeigename des Absenders")),
                        ("sender_id", num("ID eines bestehenden Absenders statt E-Mail")),
                        ("subject", s("Betreff (Pflicht ohne Template)")),
                        ("template_id", num("Template-ID, deren Inhalt kopiert wird")),
                        ("html_content", s("HTML-Inhalt (statt Template oder HTML-URL)")),
                        ("html_url", s("URL, von der Brevo den HTML-Inhalt lädt")),
                        ("list_ids", id_list("Empfängerlisten")),
                        ("exclusion_list_ids", id_list("Ausgeschlossene Listen")),
                        ("scheduled_at", datetime("Sendezeitpunkt; leer = Entwurf")),
                        ("reply_to", s("Antwortadresse")),
                        ("preview_text", s("Preheader-Text neben dem Betreff")),
                        ("to_field", s("Personalisierung des An-Felds, z. B. {{contact.FIRSTNAME}}")),
                        ("params", obj_p("Personalisierungsvariablen bei Template-Nutzung")),
                        ("footer", s("HTML-Fußzeile, typischerweise Abmeldelink")),
                        ("header", s("HTML-Kopfzeile")),
                        ("utm_campaign", s("Wert für den utm_campaign-Trackingparameter")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_UPDATE_EMAIL_CAMPAIGN",
                    ("E-Mail-Kampagne ändern", "Update email campaign"),
                    (
                        "Ändert nur die übergebenen Felder einer Kampagne; braucht --confirm.",
                        "Changes only the campaign fields you pass; requires --confirm.",
                    ),
                    &[
                        ("campaign_id", num("ID der Kampagne")),
                        ("name", s("Neuer interner Name")),
                        ("sender_email", s("Verifizierter Absender (oder Sender-ID)")),
                        ("sender_name", s("Anzeigename des Absenders")),
                        ("sender_id", num("ID eines bestehenden Absenders statt E-Mail")),
                        ("subject", s("Neuer Betreff")),
                        ("html_content", s("HTML-Inhalt (statt HTML-URL)")),
                        ("html_url", s("URL, von der Brevo den HTML-Inhalt lädt")),
                        ("list_ids", id_list("Empfängerlisten (ersetzt alle)")),
                        ("exclusion_list_ids", id_list("Ausgeschlossene Listen (ersetzt alle)")),
                        ("scheduled_at", datetime("Sendezeitpunkt; leer = unverändert")),
                        ("reply_to", s("Antwortadresse")),
                        ("preview_text", s("Preheader-Text neben dem Betreff")),
                        ("to_field", s("Personalisierung des An-Felds, z. B. {{contact.FIRSTNAME}}")),
                        ("params", obj_p("Personalisierungsvariablen bei Template-Nutzung")),
                        ("footer", s("HTML-Fußzeile, typischerweise Abmeldelink")),
                        ("header", s("HTML-Kopfzeile")),
                        ("utm_campaign", s("Wert für den utm_campaign-Trackingparameter")),
                    ],
                    &["campaign_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_SEND_EMAIL_CAMPAIGN_NOW",
                    ("E-Mail-Kampagne jetzt senden", "Send email campaign now"),
                    (
                        "Sendet einen Entwurf sofort an alle Empfängerlisten; braucht --confirm.",
                        "Sends a draft to all recipient lists now; requires --confirm.",
                    ),
                    &[("campaign_id", num("ID des Entwurfs"))],
                    &["campaign_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_SMS_CAMPAIGN",
                    ("SMS-Kampagne anlegen", "Create SMS campaign"),
                    (
                        "Legt eine SMS-Kampagne (Entwurf oder geplant) an; braucht --confirm.",
                        "Creates an SMS campaign (draft or scheduled); requires --confirm.",
                    ),
                    &[
                        ("name", s("Interner Kampagnenname")),
                        ("sender", s("Absender, max. 11 alphanumerisch oder 15 Ziffern")),
                        ("content", s("Nachrichtentext; 160 Zeichen pro Segment")),
                        ("list_ids", id_list("Empfängerlisten")),
                        ("exclusion_list_ids", id_list("Ausgeschlossene Listen")),
                        ("segment_ids", id_list("Segment-IDs als zusätzliche Zielgruppe")),
                        ("scheduled_at", datetime("Sendezeitpunkt; leer = Entwurf")),
                        ("unicode_enabled", flag_d("Als Unicode senden", false)),
                        ("organisation_prefix", s("Markenname vor der Nachricht")),
                        ("unsubscribe_instruction", s("Abmeldehinweis, muss STOP enthalten")),
                    ],
                    &["name", "sender", "content"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_DELETE_SMS_CAMPAIGN",
                    ("SMS-Kampagne löschen", "Delete SMS campaign"),
                    (
                        "Löscht eine SMS-Kampagne endgültig; braucht --confirm.",
                        "Permanently deletes an SMS campaign; requires --confirm.",
                    ),
                    &[("campaign_id", num("ID der Kampagne"))],
                    &["campaign_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_CREATE_OR_UPDATE_EMAIL_TEMPLATE",
                    ("E-Mail-Template anlegen oder ändern", "Create or update email template"),
                    (
                        "Legt ein Template an oder ändert es per ID; braucht --confirm.",
                        "Creates a template or changes one by id; requires --confirm.",
                    ),
                    &[
                        ("template_id", num("Zum Ändern angeben; leer = neues Template")),
                        ("template_name", s("Name (Pflicht beim Anlegen)")),
                        ("subject", s("Betreff (Pflicht beim Anlegen)")),
                        ("sender_email", s("Verifizierter Absender (Pflicht beim Anlegen)")),
                        ("sender_name", s("Anzeigename des Absenders")),
                        ("html_content", s("HTML-Inhalt, mind. 10 Zeichen (oder HTML-URL)")),
                        ("html_url", s("URL, von der Brevo den HTML-Inhalt lädt")),
                        ("reply_to", s("Antwortadresse")),
                        ("to_field", s("Personalisierung des An-Felds, z. B. {{contact.FIRSTNAME}}")),
                        ("is_active", flag("Template aktiv setzen; leer = unverändert")),
                        ("tag", s("Tag")),
                        ("attachment_url", s("URL des Anhangs")),
                    ],
                    &[],
                    &[WRITE],
                    true,
                ),
                t(
                    "BREVO_DELETE_EMAIL_TEMPLATE",
                    ("E-Mail-Template löschen", "Delete email template"),
                    (
                        "Löscht ein (inaktives) E-Mail-Template; braucht --confirm.",
                        "Deletes an (inactive) email template; requires --confirm.",
                    ),
                    &[("template_id", num("ID des Templates"))],
                    &["template_id"],
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

    /// Request with percent-encoded query pairs (`apiCall` queryParams).
    fn req(&self, method: &str, path: &str, q: &[(&str, Option<String>)]) -> HttpRequest {
        HttpRequest::new(method, format!("{}{}", self.url(path), query(q)))
    }

    /// Contact identifier path + `identifierType` query, shared by three actions.
    fn contact_path(&self, args: &Value, suffix: &str) -> Result<String> {
        Ok(format!(
            "/contacts/{}{}",
            encoded_segment(args, "identifier")?,
            suffix
        ))
    }

    /// Campaign/template body shared by create and update email campaign.
    fn email_campaign_body(args: &Value) -> Result<Body> {
        let sender_id = opt_num(args, "sender_id", 1, u64::MAX)?;
        let sender_email = opt_str(args, "sender_email");
        if sender_id.is_some() && sender_email.is_some() {
            bail!("provide `sender_id` or `sender_email`, not both");
        }
        let html = opt_str(args, "html_content");
        let html_url = opt_str(args, "html_url");
        if html.is_some() && html_url.is_some() {
            bail!("provide `html_content` or `html_url`, not both");
        }
        let list_ids = id_vec(args, "list_ids");
        let exclusion = id_vec(args, "exclusion_list_ids");
        let mut b = Body::new();
        put_str(&mut b, args, "name", "name");
        if let Some(id) = sender_id {
            put(&mut b, "sender", json!({"id": id}));
        } else if sender_email.is_some() {
            put(&mut b, "sender", sender_object(args)?);
        }
        put_str(&mut b, args, "subject", "subject");
        put_str(&mut b, args, "html_content", "htmlContent");
        put_str(&mut b, args, "html_url", "htmlUrl");
        if !list_ids.is_empty() || !exclusion.is_empty() {
            let mut r = Body::new();
            put_ids(&mut r, args, "list_ids", "listIds");
            put_ids(&mut r, args, "exclusion_list_ids", "exclusionListIds");
            put(&mut b, "recipients", Value::Object(r));
        }
        put_str(&mut b, args, "scheduled_at", "scheduledAt");
        put_str(&mut b, args, "reply_to", "replyTo");
        put_str(&mut b, args, "preview_text", "previewText");
        put_str(&mut b, args, "to_field", "toField");
        put_object(&mut b, args, "params", "params");
        put_str(&mut b, args, "footer", "footer");
        put_str(&mut b, args, "header", "header");
        put_str(&mut b, args, "utm_campaign", "utmCampaign");
        Ok(b)
    }
}

#[allow(clippy::too_many_lines)]
impl ToolkitAdapter for Brevo {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/get-account-info.ts
            "BREVO_GET_ACCOUNT_INFO" => HttpRequest::new("GET", self.url("/account")),
            // AP: src/lib/actions/find-contact.ts
            "BREVO_FIND_CONTACT" => self.req(
                "GET",
                &self.contact_path(args, "")?,
                &[("identifierType", Some(identifier_type(args)?))],
            ),
            // AP: src/lib/actions/list-contacts.ts
            "BREVO_LIST_CONTACTS" => {
                let list_ids = id_vec(args, "list_ids");
                self.req(
                    "GET",
                    "/contacts",
                    &[
                        (
                            "listIds",
                            (!list_ids.is_empty()).then(|| {
                                list_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
                            }),
                        ),
                        ("segmentId", opt_num_sent(args, "segment_id", 1, u64::MAX)?),
                        ("createdSince", opt_str(args, "created_since").map(String::from)),
                        ("modifiedSince", opt_str(args, "modified_since").map(String::from)),
                        ("limit", Some(opt_u64(args, "limit", 50, 1, 1000).to_string())),
                        ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                        ("sort", checked_sort(args)?.map(String::from)),
                    ],
                )
            }
            // AP: src/lib/actions/get-contact-campaign-stats.ts
            "BREVO_GET_CONTACT_CAMPAIGN_STATS" => {
                let start = opt_str(args, "start_date");
                let end = opt_str(args, "end_date");
                if start.is_some() != end.is_some() {
                    bail!("provide both `start_date` and `end_date`, or neither");
                }
                self.req(
                    "GET",
                    &format!("/contacts/{}/campaignStats", encoded_segment(args, "identifier")?),
                    &[
                        ("startDate", start.map(String::from)),
                        ("endDate", end.map(String::from)),
                    ],
                )
            }
            // AP: src/lib/actions/list-contact-attributes.ts
            "BREVO_LIST_CONTACT_ATTRIBUTES" => HttpRequest::new("GET", self.url("/contacts/attributes")),
            // AP: src/lib/actions/list-contact-lists.ts
            "BREVO_LIST_CONTACT_LISTS" => self.req(
                "GET",
                "/contacts/lists",
                &[
                    ("limit", Some(opt_u64(args, "limit", 10, 1, 50).to_string())),
                    ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                    ("sort", checked_sort(args)?.map(String::from)),
                ],
            ),
            // AP: src/lib/actions/get-contact-list.ts
            "BREVO_GET_CONTACT_LIST" => self.req(
                "GET",
                &format!("/contacts/lists/{}", numeric_id(args, "list_id")?),
                &[
                    ("startDate", opt_str(args, "start_date").map(String::from)),
                    ("endDate", opt_str(args, "end_date").map(String::from)),
                ],
            ),
            // AP: src/lib/actions/list-companies.ts
            "BREVO_LIST_COMPANIES" => {
                let contacts = id_vec(args, "linked_contacts_ids");
                let deals = str_vec_all(args, "linked_deals_ids");
                self.req(
                    "GET",
                    "/companies",
                    &[
                        ("page", opt_num_sent(args, "page", 1, u64::MAX)?),
                        ("limit", Some(opt_u64(args, "limit", 50, 1, 100).to_string())),
                        ("sort", checked_sort(args)?.map(String::from)),
                        ("sortBy", opt_str(args, "sort_by").map(String::from)),
                        ("filters", opt_str(args, "filters").map(String::from)),
                        (
                            "linkedContactsIds",
                            (!contacts.is_empty())
                                .then(|| contacts.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")),
                        ),
                        (
                            "linkedDealsIds",
                            (!deals.is_empty()).then(|| deals.join(",")),
                        ),
                    ],
                )
            }
            // AP: src/lib/actions/get-company.ts
            "BREVO_GET_COMPANY" => HttpRequest::new(
                "GET",
                self.url(&format!("/companies/{}", path_segment(args, "company_id")?)),
            ),
            // AP: src/lib/actions/list-crm-notes.ts
            "BREVO_LIST_CRM_NOTES" => self.req(
                "GET",
                "/crm/notes",
                &[
                    (
                        "entity",
                        enum_arg(args, "entity", &["companies", "deals", "contacts"])?.map(String::from),
                    ),
                    ("entityIds", opt_str(args, "entity_ids").map(String::from)),
                    ("dateFrom", opt_num_sent(args, "date_from", 0, u64::MAX)?),
                    ("dateTo", opt_num_sent(args, "date_to", 0, u64::MAX)?),
                    ("limit", Some(opt_u64(args, "limit", 50, 1, 1000).to_string())),
                    ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                    ("sort", checked_sort(args)?.map(String::from)),
                ],
            ),
            // AP: src/lib/actions/list-email-campaigns.ts
            "BREVO_LIST_EMAIL_CAMPAIGNS" => self.req(
                "GET",
                "/emailCampaigns",
                &[
                    ("type", enum_arg(args, "type", &["classic", "trigger"])?.map(String::from)),
                    (
                        "status",
                        enum_arg(args, "status", &["suspended", "archive", "sent", "queued", "draft", "inProcess", "inReview"])?
                            .map(String::from),
                    ),
                    ("startDate", opt_str(args, "start_date").map(String::from)),
                    ("endDate", opt_str(args, "end_date").map(String::from)),
                    ("limit", Some(opt_u64(args, "limit", 50, 1, 100).to_string())),
                    ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                    ("sort", checked_sort(args)?.map(String::from)),
                    ("excludeHtmlContent", opt_bool(args, "exclude_html_content").map(|b| b.to_string())),
                ],
            ),
            // AP: src/lib/actions/list-email-templates.ts
            "BREVO_LIST_EMAIL_TEMPLATES" => self.req(
                "GET",
                "/smtp/templates",
                &[
                    ("templateStatus", opt_bool(args, "template_status").map(|b| b.to_string())),
                    ("limit", Some(opt_u64(args, "limit", 50, 1, 1000).to_string())),
                    ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                    ("sort", checked_sort(args)?.map(String::from)),
                ],
            ),
            // AP: src/lib/actions/get-email-template.ts
            "BREVO_GET_EMAIL_TEMPLATE" => HttpRequest::new(
                "GET",
                self.url(&format!("/smtp/templates/{}", encoded_segment(args, "template_id")?)),
            ),
            // AP: src/lib/actions/get-email-campaign.ts
            "BREVO_GET_EMAIL_CAMPAIGN" => self.req(
                "GET",
                &format!("/emailCampaigns/{}", numeric_id(args, "campaign_id")?),
                &[(
                    "statistics",
                    enum_arg(args, "statistics", CAMPAIGN_STATS)?.map(String::from),
                )],
            ),
            // AP: src/lib/actions/list-sms-campaigns.ts
            "BREVO_LIST_SMS_CAMPAIGNS" => self.req(
                "GET",
                "/smsCampaigns",
                &[
                    (
                        "status",
                        enum_arg(args, "status", &["sent", "draft", "queued", "suspended", "inProcess", "archive"])?
                            .map(String::from),
                    ),
                    ("startDate", opt_str(args, "start_date").map(String::from)),
                    ("endDate", opt_str(args, "end_date").map(String::from)),
                    ("limit", Some(opt_u64(args, "limit", 500, 1, 1000).to_string())),
                    ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                    ("sort", checked_sort(args)?.map(String::from)),
                ],
            ),
            // AP: src/lib/actions/get-sms-campaign.ts
            "BREVO_GET_SMS_CAMPAIGN" => HttpRequest::new(
                "GET",
                self.url(&format!("/smsCampaigns/{}", numeric_id(args, "campaign_id")?)),
            ),
            // AP: src/lib/actions/list-senders.ts
            "BREVO_LIST_SENDERS" => self.req(
                "GET",
                "/senders",
                &[
                    ("ip", opt_str(args, "ip").map(String::from)),
                    ("domain", opt_str(args, "domain").map(String::from)),
                ],
            ),
            // AP: src/lib/actions/list-sender-domains.ts
            "BREVO_LIST_SENDER_DOMAINS" => HttpRequest::new("GET", self.url("/senders/domains")),
            // AP: src/lib/actions/list-transactional-email-events.ts
            "BREVO_LIST_TRANSACTIONAL_EMAIL_EVENTS" => {
                let days = opt_num_sent(args, "days", 1, 90)?;
                let start = opt_str(args, "start_date");
                let end = opt_str(args, "end_date");
                if days.is_some() && (start.is_some() || end.is_some()) {
                    bail!("provide either `days`, or `start_date` and `end_date` together, not both");
                }
                self.req(
                    "GET",
                    "/smtp/statistics/events",
                    &[
                        ("limit", Some(opt_u64(args, "limit", 50, 1, 5000).to_string())),
                        ("offset", Some(opt_u64(args, "offset", 0, 0, u64::MAX).to_string())),
                        ("startDate", start.map(String::from)),
                        ("endDate", end.map(String::from)),
                        ("days", days),
                        ("email", opt_str(args, "email").map(String::from)),
                        (
                            "event",
                            enum_arg(args, "event",
                                &["bounces", "hardBounces", "softBounces", "delivered", "spam", "requests",
                                  "opened", "clicks", "invalid", "deferred", "blocked", "unsubscribed",
                                  "error", "loadedByProxy"])?
                                .map(String::from),
                        ),
                        ("tags", opt_str(args, "tags").map(String::from)),
                        ("messageId", opt_str(args, "message_id").map(String::from)),
                        ("templateId", opt_num_sent(args, "template_id", 1, u64::MAX)?),
                        ("sort", checked_sort(args)?.map(String::from)),
                    ],
                )
            }
            // AP: src/lib/actions/get-import-process.ts
            "BREVO_GET_IMPORT_PROCESS" => HttpRequest::new(
                "GET",
                self.url(&format!("/processes/{}", numeric_id(args, "process_id")?)),
            ),
            // AP: src/lib/actions/create-or-update-contact.ts
            "BREVO_CREATE_OR_UPDATE_CONTACT" => {
                let mut b = Body::new();
                put(&mut b, "email", json!(req_str(args, "email")?.trim()));
                put_str(&mut b, args, "ext_id", "ext_id");
                put_object(&mut b, args, "attributes", "attributes");
                // contactFlags.blacklistFlag: only an explicit true is sent.
                if opt_bool(args, "email_blacklisted") == Some(true) {
                    put(&mut b, "emailBlacklisted", json!(true));
                }
                if opt_bool(args, "sms_blacklisted") == Some(true) {
                    put(&mut b, "smsBlacklisted", json!(true));
                }
                put_ids(&mut b, args, "list_ids", "listIds");
                let blocked = str_vec_all(args, "blocked_sender_addresses");
                if !blocked.is_empty() {
                    put(&mut b, "smtpBlacklistSender", json!(blocked));
                }
                put(&mut b, "updateEnabled", json!(true));
                HttpRequest::new("POST", self.url("/contacts")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/update-contact.ts
            "BREVO_UPDATE_CONTACT" => {
                let mut b = Body::new();
                put_str(&mut b, args, "ext_id", "ext_id");
                put_object(&mut b, args, "attributes", "attributes");
                put_bool(&mut b, args, "email_blacklisted", "emailBlacklisted");
                put_bool(&mut b, args, "sms_blacklisted", "smsBlacklisted");
                put_ids(&mut b, args, "list_ids", "listIds");
                put_ids(&mut b, args, "unlink_list_ids", "unlinkListIds");
                let blocked = str_vec_all(args, "blocked_sender_addresses");
                if !blocked.is_empty() {
                    put(&mut b, "smtpBlacklistSender", json!(blocked));
                }
                self.req(
                    "PUT",
                    &self.contact_path(args, "")?,
                    &[("identifierType", Some(identifier_type(args)?))],
                )
                .json(&Value::Object(b))
            }
            // AP: src/lib/actions/unsubscribe-contact.ts
            "BREVO_UNSUBSCRIBE_CONTACT" => {
                let email = opt_bool(args, "unsubscribe_email").unwrap_or(true);
                let sms = opt_bool(args, "unsubscribe_sms").unwrap_or(false);
                let unlink = id_vec(args, "unlink_list_ids");
                // contactFlags.unsubscribeBody guard.
                if !email && !sms && unlink.is_empty() {
                    bail!("tick `unsubscribe_email` or `unsubscribe_sms`, or pass `unlink_list_ids`");
                }
                let mut b = Body::new();
                if email {
                    put(&mut b, "emailBlacklisted", json!(true));
                }
                if sms {
                    put(&mut b, "smsBlacklisted", json!(true));
                }
                if !unlink.is_empty() {
                    put(&mut b, "unlinkListIds", json!(unlink));
                }
                self.req(
                    "PUT",
                    &format!("/contacts/{}", encoded_segment(args, "email")?),
                    &[],
                )
                .json(&Value::Object(b))
            }
            // AP: src/lib/actions/delete-contact.ts
            "BREVO_DELETE_CONTACT" => self.req(
                "DELETE",
                &self.contact_path(args, "")?,
                &[("identifierType", Some(identifier_type(args)?))],
            ),
            // AP: src/lib/actions/create-contact-list.ts
            "BREVO_CREATE_CONTACT_LIST" => HttpRequest::new("POST", self.url("/contacts/lists")).json(&json!({
                "name": req_str(args, "name")?.trim(),
                "folderId": numeric_id(args, "folder_id")?,
            })),
            // AP: src/lib/actions/create-company.ts
            "BREVO_CREATE_COMPANY" => {
                let mut b = Body::new();
                put(&mut b, "name", json!(req_str(args, "name")?.trim()));
                put_object(&mut b, args, "attributes", "attributes");
                if let Some(code) = opt_num(args, "country_code", 1, 9999)? {
                    put(&mut b, "countryCode", json!(code));
                }
                put_ids(&mut b, args, "linked_contacts_ids", "linkedContactsIds");
                let deals = str_vec_all(args, "linked_deals_ids");
                if !deals.is_empty() {
                    put(&mut b, "linkedDealsIds", json!(deals));
                }
                HttpRequest::new("POST", self.url("/companies")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/delete-company.ts
            "BREVO_DELETE_COMPANY" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/companies/{}", path_segment(args, "company_id")?)),
            ),
            // AP: src/lib/actions/create-event.ts
            "BREVO_CREATE_EVENT" => {
                let mut b = Body::new();
                put(&mut b, "event_name", json!(req_str(args, "event_name")?.trim()));
                put(
                    &mut b,
                    "identifiers",
                    json!({"email_id": req_str(args, "email")?.trim()}),
                );
                put_str(&mut b, args, "event_date", "event_date");
                put_object(&mut b, args, "event_properties", "event_properties");
                put_object(&mut b, args, "contact_properties", "contact_properties");
                HttpRequest::new("POST", self.url("/events")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/import-contacts.ts
            "BREVO_IMPORT_CONTACTS" => {
                let rows = import_rows(args);
                let file_url = opt_str(args, "file_url");
                if file_url.is_none() && rows.is_empty() {
                    bail!("provide `file_url` or at least one `json_body` row with an email");
                }
                let list_ids = id_vec(args, "list_ids");
                let new_list = match (opt_str(args, "new_list_name"), opt_num(args, "new_list_folder_id", 1, u64::MAX)?) {
                    (Some(name), Some(folder)) => Some(json!({"listName": name, "folderId": folder})),
                    (Some(_), None) => bail!("`new_list_name` needs `new_list_folder_id`"),
                    (None, Some(_)) => bail!("`new_list_folder_id` needs `new_list_name`"),
                    (None, None) => None,
                };
                if list_ids.is_empty() && new_list.is_none() {
                    bail!("provide `list_ids`, or `new_list_name` with `new_list_folder_id`, to import into");
                }
                let mut b = Body::new();
                if let Some(url) = file_url {
                    put(&mut b, "fileUrl", json!(url));
                }
                if !rows.is_empty() {
                    put(&mut b, "jsonBody", json!(rows));
                }
                if !list_ids.is_empty() {
                    put(&mut b, "listIds", json!(list_ids));
                }
                if let Some(nl) = new_list {
                    put(&mut b, "newList", nl);
                }
                put_str(&mut b, args, "notify_url", "notifyUrl");
                put_bool(&mut b, args, "email_blacklist", "emailBlacklist");
                put_bool(&mut b, args, "sms_blacklist", "smsBlacklist");
                put_bool(&mut b, args, "update_existing_contacts", "updateExistingContacts");
                put_bool(&mut b, args, "empty_contacts_attributes", "emptyContactsAttributes");
                HttpRequest::new("POST", self.url("/contacts/import")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/send-transactional-email.ts
            "BREVO_SEND_TRANSACTIONAL_EMAIL" => {
                let to = recipients(args, "to");
                if to.is_empty() {
                    bail!("at least one valid recipient email is required in `to`");
                }
                let template = opt_num(args, "template_id", 1, u64::MAX)?;
                let subject = opt_str(args, "subject");
                let html = opt_str(args, "html_content");
                let sender_email = opt_str(args, "sender_email");
                if template.is_none() && (subject.is_none() || html.is_none()) {
                    bail!("provide `template_id`, or both `subject` and `html_content`");
                }
                if template.is_none() && sender_email.is_none() {
                    bail!("`sender_email` is required when no template is selected");
                }
                let mut b = Body::new();
                put(&mut b, "to", json!(to));
                if sender_email.is_some() {
                    put(&mut b, "sender", sender_object(args)?);
                }
                if let Some(id) = template {
                    put(&mut b, "templateId", json!(id));
                }
                put_str(&mut b, args, "subject", "subject");
                put_str(&mut b, args, "html_content", "htmlContent");
                put_str(&mut b, args, "text_content", "textContent");
                put_object(&mut b, args, "params", "params");
                let cc = recipients(args, "cc");
                if !cc.is_empty() {
                    put(&mut b, "cc", json!(cc));
                }
                let bcc = recipients(args, "bcc");
                if !bcc.is_empty() {
                    put(&mut b, "bcc", json!(bcc));
                }
                if let Some(reply) = opt_str(args, "reply_to") {
                    put(&mut b, "replyTo", json!({"email": reply}));
                }
                let files = attachments(args);
                if !files.is_empty() {
                    put(&mut b, "attachment", json!(files));
                }
                let tags = str_vec_all(args, "tags");
                if !tags.is_empty() {
                    put(&mut b, "tags", json!(tags));
                }
                put_str(&mut b, args, "scheduled_at", "scheduledAt");
                if opt_bool(args, "sandbox") == Some(true) {
                    put(&mut b, "headers", json!({"X-Sib-Sandbox": "drop"}));
                }
                HttpRequest::new("POST", self.url("/smtp/email")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/send-transactional-sms.ts
            "BREVO_SEND_TRANSACTIONAL_SMS" => {
                let mut b = Body::new();
                put(&mut b, "sender", json!(req_str(args, "sender")?.trim()));
                put(&mut b, "recipient", json!(req_str(args, "recipient")?.trim()));
                put(&mut b, "content", json!(req_str(args, "content")?));
                let kind = opt_str(args, "type").unwrap_or("transactional");
                if !["transactional", "marketing"].contains(&kind) {
                    bail!("`type` must be transactional or marketing");
                }
                put(&mut b, "type", json!(kind));
                put_str(&mut b, args, "tag", "tag");
                put_str(&mut b, args, "web_url", "webUrl");
                put_bool(&mut b, args, "unicode_enabled", "unicodeEnabled");
                put_str(&mut b, args, "organisation_prefix", "organisationPrefix");
                HttpRequest::new("POST", self.url("/transactionalSMS/send")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/create-email-campaign.ts
            "BREVO_CREATE_EMAIL_CAMPAIGN" => {
                let template = opt_num(args, "template_id", 1, u64::MAX)?;
                let html = opt_str(args, "html_content");
                let html_url = opt_str(args, "html_url");
                let sources = [template.is_some(), html.is_some(), html_url.is_some()]
                    .into_iter()
                    .filter(|b| *b)
                    .count();
                if sources == 0 {
                    bail!("provide `template_id`, `html_content` or `html_url`");
                }
                if sources > 1 {
                    bail!("provide only one of `template_id`, `html_content` or `html_url`");
                }
                // Create branch (unlike update) also demands a sender.
                if opt_num(args, "sender_id", 1, u64::MAX)?.is_none()
                    && opt_str(args, "sender_email").is_none()
                {
                    bail!("provide `sender_id` or `sender_email`");
                }
                if template.is_none() && opt_str(args, "subject").is_none() {
                    bail!("`subject` is required unless `template_id` is provided");
                }
                let mut b = Self::email_campaign_body(args)?;
                if let Some(id) = template {
                    put(&mut b, "templateId", json!(id));
                }
                HttpRequest::new("POST", self.url("/emailCampaigns")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/update-email-campaign.ts
            "BREVO_UPDATE_EMAIL_CAMPAIGN" => {
                let b = Self::email_campaign_body(args)?;
                self.req(
                    "PUT",
                    &format!("/emailCampaigns/{}", numeric_id(args, "campaign_id")?),
                    &[],
                )
                .json(&Value::Object(b))
            }
            // AP: src/lib/actions/send-email-campaign-now.ts
            "BREVO_SEND_EMAIL_CAMPAIGN_NOW" => HttpRequest::new(
                "POST",
                self.url(&format!("/emailCampaigns/{}/sendNow", numeric_id(args, "campaign_id")?)),
            ),
            // AP: src/lib/actions/create-sms-campaign.ts
            "BREVO_CREATE_SMS_CAMPAIGN" => {
                let mut b = Body::new();
                put(&mut b, "name", json!(req_str(args, "name")?.trim()));
                put(&mut b, "sender", json!(req_str(args, "sender")?.trim()));
                put(&mut b, "content", json!(req_str(args, "content")?));
                let list_ids = id_vec(args, "list_ids");
                let exclusion = id_vec(args, "exclusion_list_ids");
                let segments = id_vec(args, "segment_ids");
                if !list_ids.is_empty() || !exclusion.is_empty() || !segments.is_empty() {
                    let mut r = Body::new();
                    put_ids(&mut r, args, "list_ids", "listIds");
                    put_ids(&mut r, args, "exclusion_list_ids", "exclusionListIds");
                    put_ids(&mut r, args, "segment_ids", "segmentIds");
                    put(&mut b, "recipients", Value::Object(r));
                }
                put_str(&mut b, args, "scheduled_at", "scheduledAt");
                put_bool(&mut b, args, "unicode_enabled", "unicodeEnabled");
                put_str(&mut b, args, "organisation_prefix", "organisationPrefix");
                put_str(&mut b, args, "unsubscribe_instruction", "unsubscribeInstruction");
                HttpRequest::new("POST", self.url("/smsCampaigns")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/delete-sms-campaign.ts
            "BREVO_DELETE_SMS_CAMPAIGN" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/smsCampaigns/{}", numeric_id(args, "campaign_id")?)),
            ),
            // AP: src/lib/actions/create-or-update-email-template.ts
            "BREVO_CREATE_OR_UPDATE_EMAIL_TEMPLATE" => {
                let html = opt_str(args, "html_content");
                let html_url = opt_str(args, "html_url");
                if html.is_some() && html_url.is_some() {
                    bail!("provide `html_content` or `html_url`, not both");
                }
                let template_id = opt_num(args, "template_id", 1, u64::MAX)?;
                if template_id.is_none() {
                    if opt_str(args, "template_name").is_none() {
                        bail!("`template_name` is required when creating a new template");
                    }
                    if opt_str(args, "subject").is_none() {
                        bail!("`subject` is required when creating a new template");
                    }
                    if opt_str(args, "sender_email").is_none() {
                        bail!("`sender_email` is required when creating a new template");
                    }
                    if html.is_none() && html_url.is_none() {
                        bail!("provide `html_content` or `html_url` when creating a new template");
                    }
                    if html.is_some_and(|h| h.chars().count() < 10) {
                        bail!("`html_content` must be at least 10 characters");
                    }
                }
                let mut b = Body::new();
                put_str(&mut b, args, "template_name", "templateName");
                put_str(&mut b, args, "subject", "subject");
                if opt_str(args, "sender_email").is_some() {
                    put(&mut b, "sender", sender_object(args)?);
                }
                put_str(&mut b, args, "html_content", "htmlContent");
                put_str(&mut b, args, "html_url", "htmlUrl");
                put_str(&mut b, args, "reply_to", "replyTo");
                put_str(&mut b, args, "to_field", "toField");
                put_bool(&mut b, args, "is_active", "isActive");
                put_str(&mut b, args, "tag", "tag");
                put_str(&mut b, args, "attachment_url", "attachmentUrl");
                match template_id {
                    Some(id) => self
                        .req("PUT", &format!("/smtp/templates/{id}"), &[])
                        .json(&Value::Object(b)),
                    None => HttpRequest::new("POST", self.url("/smtp/templates")).json(&Value::Object(b)),
                }
            }
            // AP: src/lib/actions/delete-email-template.ts
            "BREVO_DELETE_EMAIL_TEMPLATE" => HttpRequest::new(
                "DELETE",
                self.url(&format!("/smtp/templates/{}", numeric_id(args, "template_id")?)),
            ),
            other => bail!("unknown Brevo tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        // The Find/Get actions of the piece turn a 404 into {found: false}.
        if response.status == 404 {
            const FOUND_TOOLS: &[&str] = &[
                "BREVO_FIND_CONTACT",
                "BREVO_GET_COMPANY",
                "BREVO_GET_CONTACT_LIST",
                "BREVO_GET_EMAIL_CAMPAIGN",
                "BREVO_GET_EMAIL_TEMPLATE",
                "BREVO_GET_SMS_CAMPAIGN",
            ];
            if FOUND_TOOLS.contains(&tool) {
                return Ok(json!({"found": false, "data": {}}));
            }
        }
        Ok(match tool {
            "BREVO_FIND_CONTACT"
            | "BREVO_GET_COMPANY"
            | "BREVO_GET_CONTACT_LIST"
            | "BREVO_GET_EMAIL_CAMPAIGN"
            | "BREVO_GET_EMAIL_TEMPLATE"
            | "BREVO_GET_SMS_CAMPAIGN" => json!({"found": true, "data": v}),
            "BREVO_GET_IMPORT_PROCESS" => {
                // get-import-process.ts: terminal statuses flip `finished`.
                const TERMINAL: &[&str] = &["completed", "failed", "cancelled"];
                let status = v["status"].as_str().unwrap_or_default();
                json!({
                    "id": v["id"],
                    "status": v["status"],
                    "finished": TERMINAL.contains(&status),
                    "name": v["name"],
                    "export_url": v["export_url"],
                    "info": v["info"],
                })
            }
            // Brevo answers the write arms with an empty body.
            "BREVO_CREATE_EVENT"
            | "BREVO_UNSUBSCRIBE_CONTACT"
            | "BREVO_UPDATE_CONTACT"
            | "BREVO_CREATE_OR_UPDATE_CONTACT"
            | "BREVO_DELETE_COMPANY"
            | "BREVO_DELETE_CONTACT"
            | "BREVO_DELETE_EMAIL_TEMPLATE"
            | "BREVO_DELETE_SMS_CAMPAIGN"
            | "BREVO_SEND_EMAIL_CAMPAIGN_NOW"
            | "BREVO_UPDATE_EMAIL_CAMPAIGN" => json!({"success": true}),
            // Update branch echoes nothing; the TS returns the input id.
            "BREVO_CREATE_OR_UPDATE_EMAIL_TEMPLATE" if v.is_null() => json!({"success": true}),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> Brevo {
        Brevo::with_api_base("https://api.brevo.test/v3")
    }

    #[test]
    fn paths_queries_and_guards() {
        let a = b();
        let r = a
            .build_request("BREVO_FIND_CONTACT", &json!({"identifier": "a+b@x.de"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://api.brevo.test/v3/contacts/a%2Bb%40x.de?identifierType=email_id"
        );
        assert!(a
            .build_request("BREVO_FIND_CONTACT", &json!({"identifier": "x", "identifier_type": "nope"}))
            .is_err());

        let r = a
            .build_request(
                "BREVO_LIST_CONTACTS",
                &json!({"list_ids": ["2", 7], "limit": 5, "sort": "desc", "modified_since": "2026-06-01T00:00:00Z"}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://api.brevo.test/v3/contacts?listIds=2%2C7&modifiedSince=2026-06-01T00%3A00%3A00Z&limit=5&offset=0&sort=desc"
        );

        // Date pairs must come together; days excludes dates.
        assert!(a
            .build_request("BREVO_GET_CONTACT_CAMPAIGN_STATS", &json!({"identifier": "a@b.de", "start_date": "2026-06-01"}))
            .is_err());
        assert!(a
            .build_request(
                "BREVO_LIST_TRANSACTIONAL_EMAIL_EVENTS",
                &json!({"days": 7, "end_date": "2026-06-01"})
            )
            .is_err());
        assert!(a
            .build_request("BREVO_LIST_TRANSACTIONAL_EMAIL_EVENTS", &json!({"days": 91}))
            .is_err());
        assert!(a.build_request("BREVO_GET_EMAIL_CAMPAIGN", &json!({"campaign_id": "../x"})).is_err());
        assert!(a.build_request("BREVO_GET_SMS_CAMPAIGN", &json!({"campaign_id": 7})).is_ok());
        assert!(a.build_request("BREVO_GET_COMPANY", &json!({"company_id": "a/b"})).is_err());
    }

    #[test]
    fn write_bodies_mirror_compact_body() {
        let a = b();
        let r = a
            .build_request(
                "BREVO_CREATE_EMAIL_CAMPAIGN",
                &json!({"name": "N", "sender_email": "news@acme.de", "sender_id": 3,
                        "subject": "S", "html_content": "<html></html>"}),
            )
            .unwrap_err();
        assert!(r.to_string().contains("not both"), "{r}");

        let r = a
            .build_request(
                "BREVO_CREATE_EMAIL_CAMPAIGN",
                &json!({"name": "N", "sender_id": 3, "template_id": 12, "html_content": "<html></html>"}),
            )
            .unwrap_err();
        assert!(r.to_string().contains("only one of"), "{r}");

        let r = a
            .build_request("BREVO_CREATE_EMAIL_CAMPAIGN", &json!({"name": "N", "sender_id": 3}))
            .unwrap_err();
        assert!(r.to_string().contains("template_id"), "{r}");

        let r = a
            .build_request(
                "BREVO_CREATE_EMAIL_CAMPAIGN",
                &json!({"name": "N", "subject": "S", "html_content": "<html></html>"}),
            )
            .unwrap_err();
        assert!(r.to_string().contains("sender_id"), "{r}");

        let r = a
            .build_request(
                "BREVO_CREATE_EMAIL_CAMPAIGN",
                &json!({"name": "N", "sender_id": 3, "subject": "S", "html_content": "<html></html>",
                        "list_ids": [2, 5]}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.brevo.test/v3/emailCampaigns");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["sender"], json!({"id": 3}));
        assert_eq!(body["recipients"]["listIds"], json!([2, 5]));
        assert_eq!(body["htmlContent"], "<html></html>");
        assert!(body.get("scheduledAt").is_none());

        let r = a
            .build_request(
                "BREVO_SEND_TRANSACTIONAL_EMAIL",
                &json!({"to": [{"email": "jane@example.com", "name": "Jane"}, "x@y.de"],
                        "subject": "Hi", "html_content": "<p>Hi</p>", "sender_email": "news@acme.de",
                        "sandbox": true, "tags": ["welcome"]}),
            )
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["to"], json!([{"email": "jane@example.com", "name": "Jane"}, {"email": "x@y.de"}]));
        assert_eq!(body["headers"], json!({"X-Sib-Sandbox": "drop"}));

        // Unsubscribe guard from contactFlags.unsubscribeBody.
        assert!(a.build_request("BREVO_UNSUBSCRIBE_CONTACT", &json!({"email": "a@b.de",
            "unsubscribe_email": false})).is_err());

        let r = a
            .build_request("BREVO_CREATE_OR_UPDATE_CONTACT", &json!({"email": "a@b.de"}))
            .unwrap();
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["updateEnabled"], json!(true));
        assert!(body.get("listIds").is_none());
    }

    #[test]
    fn not_found_maps_to_found_false() {
        let a = b();
        let resp = HttpResponse {
            status: 404,
            body: serde_json::to_vec(&json!({"code": "document_not_found", "message": "Contact not found"})).unwrap(),
            session_id: None,
        };
        let out = a.parse_response("BREVO_FIND_CONTACT", &resp).unwrap();
        assert_eq!(out["found"], json!(false));
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"id": 12, "email": "a@b.de"})).unwrap(),
            session_id: None,
        };
        let out = a.parse_response("BREVO_FIND_CONTACT", &resp).unwrap();
        assert_eq!(out["found"], json!(true));
        assert_eq!(out["data"]["id"], json!(12));
        let resp = HttpResponse { status: 204, body: Vec::new(), session_id: None };
        let out = a.parse_response("BREVO_DELETE_CONTACT", &resp).unwrap();
        assert_eq!(out["success"], json!(true));
    }
}
