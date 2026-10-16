//! WhatsApp Business toolkit (Meta Graph API, Cloud API v23.0).
//! Action set modelled on Activepieces `piece-whatsapp` (MIT).
//!
//! Auth: the piece uses a CustomAuth made of a permanent System User access
//! token (`Authorization: Bearer …`), the WhatsApp Business Account id (WABA)
//! and an optional app secret for webhook signature checks. The token is
//! injected by the hub (`AuthScheme::Bearer`); the WABA id becomes a plain
//! `business_account_id` argument on the tools that address the account itself
//! (`/{waba-id}/phone_numbers`, `/{waba-id}/message_templates`). The app secret
//! only guards webhook triggers, which are not ported.
//!
//! Every tool is exactly one Graph call: the send tools post to
//! `/{phone-number-id}/messages` (envelope from `whatsappClient.sendMessage` in
//! `src/lib/common/client.ts`), the rest use the plain object paths. The
//! `fields` query strings are taken verbatim from the actions; query values are
//! percent-encoded.
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "whatsapp";

/// Permission levels Meta documents for System User tokens (piece auth help).
const MESSAGING: &str = "whatsapp_business_messaging";
const MANAGEMENT: &str = "whatsapp_business_management";

/// `supportedMediaTypes` (src/lib/common/utils.ts).
const MEDIA_TYPES: [&str; 5] = ["image", "audio", "document", "sticker", "video"];
/// `mediaTypeSupportsCaption`; these are also the interactive media header types.
const MEDIA_TYPES_WITH_CAPTION: [&str; 3] = ["image", "video", "document"];

/// Query strings copied from the actions.
const PHONE_LIST_FIELDS: &str =
    "id,verified_name,display_phone_number,quality_rating,code_verification_status,platform_type";
const PHONE_DETAIL_FIELDS: &str = "id,verified_name,display_phone_number,quality_rating,\
code_verification_status,name_status,status,throughput,platform_type";
const PROFILE_FIELDS: &str = "about,address,description,email,profile_picture_url,websites,vertical";
const TEMPLATE_LIST_FIELDS: &str = "id,name,language,category,status,components,quality_score";
const TEMPLATE_LOOKUP_FIELDS: &str = "id,name,language";

const TEMPLATE_STATUSES: [&str; 5] = ["APPROVED", "PENDING", "REJECTED", "PAUSED", "DISABLED"];
const TEMPLATE_CATEGORIES: [&str; 3] = ["MARKETING", "UTILITY", "AUTHENTICATION"];
/// `businessVertical` dropdown (src/lib/common/props.ts).
const VERTICALS: [&str; 21] = [
    "OTHER",
    "AUTO",
    "BEAUTY",
    "APPAREL",
    "EDU",
    "ENTERTAIN",
    "EVENT_PLAN",
    "FINANCE",
    "GROCERY",
    "GOVT",
    "HOTEL",
    "HEALTH",
    "NONPROFIT",
    "PROF_SERVICES",
    "RETAIL",
    "TRAVEL",
    "RESTAURANT",
    "ALCOHOL",
    "ONLINE_GAMBLING",
    "PHYSICAL_GAMBLING",
    "OTC_DRUGS",
];

// ---------------------------------------------------------------- schema props

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn flag(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc, "default": false})
}

fn decimal(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}

/// Array-of-object property (Activepieces `Property.Array` with sub-props);
/// a JSON string holding the array is accepted as well.
fn record_array(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "object"}, "description": desc})
}

/// Free-form JSON property (Activepieces `Property.Json`).
fn json_value(desc: &str) -> Value {
    json!({"description": desc, "oneOf": [{"type": "array"}, {"type": "object"}, {"type": "string"}]})
}

fn prop_phone_number_id() -> (&'static str, Value) {
    (
        "phone_number_id",
        s("ID der Business-Telefonnummer, aus WHATSAPP_LIST_PHONE_NUMBERS"),
    )
}

fn prop_business_account_id() -> (&'static str, Value) {
    (
        "business_account_id",
        s("WhatsApp Business Account ID (WABA, Meta \"Business Account ID\")"),
    )
}

fn prop_to() -> (&'static str, Value) {
    (
        "to",
        s("Empfängernummer im internationalen Format, nur Ziffern (z. B. 491521234567)"),
    )
}

fn prop_reply_to() -> (&'static str, Value) {
    (
        "reply_to_message_id",
        s("Optionale WhatsApp-Nachrichten-ID (wamid), auf die geantwortet wird"),
    )
}

fn prop_message_id() -> (&'static str, Value) {
    (
        "message_id",
        s("WhatsApp-Nachrichten-ID (wamid), z. B. aus dem New-Incoming-Message-Trigger"),
    )
}

fn prop_media_id() -> (&'static str, Value) {
    ("media_id", s("Medien-ID aus WHATSAPP_UPLOAD-Medien oder eingehenden Medienachrichten"))
}

fn prop_footer() -> (&'static str, Value) {
    ("footer", s("Optionaler Fußtext, max. 60 Zeichen"))
}

fn prop_components() -> (&'static str, Value) {
    (
        "components",
        json_value("Komponenten-Array nach Meta-Dokumentation; Platzhalter als [[1]] statt {{1}} schreiben, wird konvertiert. Beispiel: [{\"type\":\"BODY\",\"text\":\"Hallo [[1]]\",\"example\":{\"body_text\":[[\"Sam\"]]}}]"),
    )
}

fn prop_category(required: bool) -> (&'static str, Value) {
    let desc = if required {
        "Kategorie, nach der Meta prüft und abrechnet (MARKETING, UTILITY, AUTHENTICATION)"
    } else {
        "Neue Kategorie; leer lassen, um die aktuelle zu behalten"
    };
    ("category", choice(desc, &TEMPLATE_CATEGORIES))
}

// ------------------------------------------------------- input helpers (piece)

/// Meta object ids are numeric strings (WABA, phone number, media, template).
fn graph_id<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = path_segment(args, key)?;
    if !v.bytes().all(|b| b.is_ascii_digit()) {
        bail!("`{key}` must be a numeric Meta object id");
    }
    Ok(v)
}

/// Recipient phone number: international format, digits only (optional `+`).
fn recipient(args: &Value) -> Result<String> {
    let v = req_str(args, "to")?.trim();
    let digits = v.strip_prefix('+').unwrap_or(v);
    if digits.len() < 3
        || digits.len() > 20
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        bail!("`to` must be a phone number in international format, digits only (for example 491521234567)");
    }
    Ok(v.to_string())
}

/// WhatsApp message id (`wamid.…`); travels in the body, so only basic hygiene.
fn message_ref<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    let v = req_str(args, key)?;
    header_safe(key, v)?;
    if v.len() > 256 {
        bail!("`{key}` is too long for a WhatsApp message id");
    }
    Ok(v)
}

fn max_len(key: &str, value: &str, max: usize) -> Result<()> {
    let chars = value.chars().count();
    if chars > max {
        bail!("`{key}` must be at most {max} characters (got {chars}).");
    }
    Ok(())
}

fn req_max_len<'a>(args: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    let v = req_str(args, key)?;
    max_len(key, v, max)?;
    Ok(v)
}

fn opt_max_len<'a>(args: &'a Value, key: &str, max: usize) -> Result<Option<&'a str>> {
    match opt_str(args, key) {
        Some(v) => {
            max_len(key, v, max)?;
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

fn req_number(args: &Value, key: &str) -> Result<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing or non-numeric argument `{key}`"))
}

/// `inputUtils.assertUnique`.
fn unique(label: &str, values: &[&str]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for v in values {
        if !seen.insert(*v) {
            bail!("{label} must be unique.");
        }
    }
    Ok(())
}

/// `inputUtils.asRecords`: objects from an array (or a JSON string holding one).
fn records(args: &Value, key: &str) -> Vec<Map<String, Value>> {
    let parsed = match args.get(key) {
        Some(Value::String(text)) => serde_json::from_str::<Value>(text.trim()).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    };
    match parsed {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|i| i.as_object().cloned())
            .collect(),
        _ => Vec::new(),
    }
}

/// `inputUtils.asStrings`: non-empty strings from an array (or JSON string array).
fn strings(args: &Value, key: &str) -> Vec<String> {
    let parsed = match args.get(key) {
        Some(Value::String(text)) => serde_json::from_str::<Value>(text.trim()).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    };
    match parsed {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|i| i.as_str().map(String::from))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// `inputUtils.requiredString`.
fn required_string(entry: &Map<String, Value>, key: &str, label: &str, max: usize) -> Result<String> {
    let value = entry
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow!("{label} is required on every entry."))?;
    let chars = value.chars().count();
    if chars > max {
        bail!("{label} must be at most {max} characters (got {chars}).");
    }
    Ok(value.to_string())
}

/// `inputUtils.optionalString`.
fn optional_string(entry: &Map<String, Value>, key: &str, label: &str, max: usize) -> Result<Option<String>> {
    match entry.get(key).and_then(Value::as_str).filter(|v| !v.is_empty()) {
        Some(value) => {
            let chars = value.chars().count();
            if chars > max {
                bail!("{label} must be at most {max} characters (got {chars}).");
            }
            Ok(Some(value.to_string()))
        }
        None => Ok(None),
    }
}

/// Buttons of an interactive button message (piece validation rules).
fn button_entries(args: &Value) -> Result<Vec<(String, String)>> {
    let entries: Vec<(String, String)> = records(args, "buttons")
        .into_iter()
        .map(|entry| {
            Ok((
                required_string(&entry, "id", "Button ID", 256)?,
                required_string(&entry, "title", "Button title", 20)?,
            ))
        })
        .collect::<Result<_>>()?;
    if entries.is_empty() || entries.len() > 3 {
        bail!("Interactive button messages need between 1 and 3 buttons.");
    }
    let ids: Vec<&str> = entries.iter().map(|(id, _)| id.as_str()).collect();
    let titles: Vec<&str> = entries.iter().map(|(_, title)| title.as_str()).collect();
    unique("Button IDs", &ids)?;
    unique("Button titles", &titles)?;
    Ok(entries)
}

/// Rows of an interactive list message (piece validation rules).
fn row_entries(args: &Value) -> Result<Vec<(String, String, Option<String>)>> {
    let entries: Vec<(String, String, Option<String>)> = records(args, "rows")
        .into_iter()
        .map(|entry| {
            Ok((
                required_string(&entry, "id", "Row ID", 200)?,
                required_string(&entry, "title", "Row title", 24)?,
                optional_string(&entry, "description", "Row description", 72)?,
            ))
        })
        .collect::<Result<_>>()?;
    if entries.is_empty() || entries.len() > 10 {
        bail!("Interactive list messages need between 1 and 10 rows.");
    }
    let ids: Vec<&str> = entries.iter().map(|(id, _, _)| id.as_str()).collect();
    unique("Row IDs", &ids)?;
    Ok(entries)
}

/// `whatsappClient.buildInteractiveHeader`: optional header above the body.
fn interactive_header(args: &Value) -> Result<Option<Value>> {
    let Some(kind) = opt_str(args, "header_type") else {
        return Ok(None);
    };
    match kind {
        "none" => Ok(None),
        "text" => {
            let text = opt_str(args, "header_text")
                .ok_or_else(|| anyhow!("Header Text is required when the header type is text."))?;
            Ok(Some(json!({"type": "text", "text": text})))
        }
        media if MEDIA_TYPES_WITH_CAPTION.contains(&media) => {
            let link = opt_str(args, "header_media_url").ok_or_else(|| {
                anyhow!("Header Media URL is required when the header type is {media}.")
            })?;
            let mut header = Map::new();
            header.insert("type".into(), json!(media));
            header.insert(media.into(), json!({"link": link}));
            Ok(Some(Value::Object(header)))
        }
        other => bail!("`header_type` must be none, text, image, video or document, got `{other}`"),
    }
}

/// Property.Json `components`: array/object or a JSON string holding one,
/// normalized through `whatsappClient.normalizeTemplateComponents`.
fn template_components(args: &Value, key: &str) -> Result<Value> {
    let raw = match args.get(key) {
        Some(Value::String(text)) if !text.trim().is_empty() => serde_json::from_str::<Value>(text.trim())
            .map_err(|e| anyhow!("`{key}` must be valid JSON: {e}"))?,
        Some(v) if v.is_array() || v.is_object() => v.clone(),
        Some(_) => bail!("`{key}` must be a components array as documented by Meta"),
        None => return Err(anyhow!("missing argument `{key}`")),
    };
    Ok(normalize_placeholders(&raw))
}

/// `[[name]]` -> `{{name}}` in every string, recursively.
fn normalize_placeholders(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(replace_placeholders(s)),
        Value::Array(items) => Value::Array(items.iter().map(normalize_placeholders).collect()),
        Value::Object(obj) => Value::Object(
            obj.iter()
                .map(|(k, val)| (k.clone(), normalize_placeholders(val)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn replace_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("]]") {
            Some(end)
                if !after[..end].is_empty()
                    && after[..end]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                out.push_str("{{");
                out.push_str(&after[..end]);
                out.push_str("}}");
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `{ type: 'text', text }` parameter list of Send Template.
fn text_parameters(values: &[String]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|text| json!({"type": "text", "text": text}))
            .collect(),
    )
}

// ---------------------------------------------------------------- adapter

pub struct WhatsApp {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for WhatsApp {
    fn default() -> Self {
        // WHATSAPP_API_BASE (src/lib/common/utils.ts).
        Self::with_api_base("https://graph.facebook.com/v23.0")
    }
}

#[allow(clippy::too_many_lines)]
impl WhatsApp {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "WhatsApp Business".into(),
            description: LocalizedText::new(
                "WhatsApp Business Cloud API: Nachrichten, Vorlagen und Business-Profile über die eigene Nummer lesen und senden.",
                "WhatsApp Business Cloud API: read and send messages, templates and business profiles from your own number.",
            ),
            // System User access token, sent as `Authorization: Bearer …`.
            auth_schemes: vec![AuthScheme::Bearer],
            tools: vec![
                t(
                    "WHATSAPP_GET_PHONE_NUMBER",
                    ("Telefonnummer lesen", "Get phone number"),
                    (
                        "Liest Details und Status einer Business-Telefonnummer.",
                        "Returns the details and health of one business phone number.",
                    ),
                    &[prop_phone_number_id()],
                    &["phone_number_id"],
                    &[MANAGEMENT],
                    false,
                ),
                t(
                    "WHATSAPP_LIST_PHONE_NUMBERS",
                    ("Telefonnummern auflisten", "List phone numbers"),
                    (
                        "Listet die Telefonnummern des Business-Kontos; eine Graph-Seite pro Aufruf.",
                        "Lists the phone numbers registered on the business account; one Graph page per call.",
                    ),
                    &[
                        prop_business_account_id(),
                        ("after", s("Cursor der nächsten Seite (next_after)")),
                    ],
                    &["business_account_id"],
                    &[MANAGEMENT],
                    false,
                ),
                t(
                    "WHATSAPP_GET_BUSINESS_PROFILE",
                    ("Business-Profil lesen", "Get business profile"),
                    (
                        "Liest das öffentliche Business-Profil einer Telefonnummer.",
                        "Returns the public business profile shown for a phone number.",
                    ),
                    &[prop_phone_number_id()],
                    &["phone_number_id"],
                    &[MANAGEMENT],
                    false,
                ),
                t(
                    "WHATSAPP_LIST_MESSAGE_TEMPLATES",
                    ("Nachrichtenvorlagen auflisten", "List message templates"),
                    (
                        "Listet die Nachrichtenvorlagen des Business-Kontos, optional nach Prüfstatus gefiltert.",
                        "Lists the message templates on the business account, optionally filtered by review status.",
                    ),
                    &[
                        prop_business_account_id(),
                        ("status", choice("Nur Vorlagen in diesem Prüfstatus; leer = alle", &TEMPLATE_STATUSES)),
                        ("limit", int("Maximale Anzahl Vorlagen. Standard 100.", 1, 200, 100)),
                        ("after", s("Cursor der nächsten Seite (next_after)")),
                    ],
                    &["business_account_id"],
                    &[MANAGEMENT],
                    false,
                ),
                t(
                    "WHATSAPP_GET_TEMPLATE",
                    ("Nachrichtenvorlage lesen", "Get message template"),
                    (
                        "Liest ID, Namen und Sprache einer Nachrichtenvorlage.",
                        "Reads the id, name and language of a message template.",
                    ),
                    &[("template_id", s("Vorlagen-ID, z. B. aus WHATSAPP_LIST_MESSAGE_TEMPLATES"))],
                    &["template_id"],
                    &[MANAGEMENT],
                    false,
                ),
                t(
                    "WHATSAPP_GET_MEDIA_URL",
                    ("Medien-URL ermitteln", "Get media URL"),
                    (
                        "Löst eine Medien-ID in Download-URL, MIME-Typ, Größe und SHA-256 auf; die URL läuft nach fünf Minuten ab.",
                        "Resolves a media ID to its download URL, MIME type, size and SHA-256; the URL expires after five minutes.",
                    ),
                    &[prop_media_id()],
                    &["media_id"],
                    &[MESSAGING],
                    false,
                ),
                t(
                    "WHATSAPP_SEND_MESSAGE",
                    ("Nachricht senden", "Send message"),
                    (
                        "Sendet eine Textnachricht über WhatsApp; braucht --confirm.",
                        "Sends a plain text message through WhatsApp; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("text", s("Nachrichtentext, bis zu 4096 Zeichen")),
                    ],
                    &["phone_number_id", "to", "text"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_MEDIA",
                    ("Medien senden", "Send media"),
                    (
                        "Sendet Bild, Video, Audio, Dokument oder Sticker über eine öffentliche URL; braucht --confirm.",
                        "Sends an image, video, audio, document or sticker by public URL; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("type", choice("Medientyp der Datei an der URL", &MEDIA_TYPES)),
                        ("media", s("Öffentliche URL der Datei")),
                        ("caption", s("Text unter dem Medium; ignoriert bei Audio und Sticker")),
                        ("filename", s("Nur Dokumente: Dateiname für den Empfänger")),
                    ],
                    &["phone_number_id", "to", "type", "media"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_TEMPLATE",
                    ("Vorlage senden", "Send template"),
                    (
                        "Sendet eine genehmigte Vorlage über Namen und positionale Parameter; braucht --confirm.",
                        "Sends an approved template by name with positional parameters; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("template_name", s("Exakter Vorlagenname, z. B. hello_world")),
                        ("language_code", s("Sprache und Locale der Vorlage, z. B. en_US")),
                        ("header_parameters", str_list("Werte für Text-Header-Platzhalter, in Reihenfolge")),
                        ("body_parameters", str_list("Werte für die Body-Platzhalter {{1}}, {{2}}, … in Reihenfolge")),
                        ("button_url_parameters", str_list("Werte für den dynamischen URL-Suffix des ersten URL-Buttons")),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "template_name", "language_code"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_INTERACTIVE_BUTTONS",
                    ("Interaktive Schaltflächen senden", "Send interactive buttons"),
                    (
                        "Sendet eine Nachricht mit bis zu drei Schnellantwort-Schaltflächen; braucht --confirm.",
                        "Sends a message with up to three quick-reply buttons; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("body", s("Haupttext der Nachricht, max. 1024 Zeichen")),
                        ("buttons", record_array("1-3 Schaltflächen als Objekte {\"id\": …, \"title\": …}; Titel einmalig, max. 20 Zeichen, IDs max. 256 Zeichen")),
                        ("header_type", choice("Optionaler Kopfbereich über dem Text", &["none", "text", "image", "video", "document"])),
                        ("header_text", s("Pflicht bei header_type=text, max. 60 Zeichen")),
                        ("header_media_url", s("Öffentliche URL, pflicht bei header_type=image/video/document")),
                        prop_footer(),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "body", "buttons"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_INTERACTIVE_LIST",
                    ("Interaktive Liste senden", "Send interactive list"),
                    (
                        "Sendet eine Nachricht, die ein Menü mit bis zu zehn Zeilen öffnet; braucht --confirm.",
                        "Sends a message that opens a menu of up to ten selectable rows; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("body", s("Haupttext der Nachricht, max. 4096 Zeichen")),
                        ("button_text", s("Text der Schaltfläche, die die Liste öffnet, max. 20 Zeichen")),
                        ("section_title", s("Titel über den Zeilen, max. 24 Zeichen")),
                        ("rows", record_array("1-10 Zeilen als Objekte {\"id\": …, \"title\": …, \"description\": …}; IDs einmalig, max. 200 Zeichen, Titel max. 24, Beschreibung max. 72")),
                        ("header_text", s("Optionaler Kopftext, max. 60 Zeichen")),
                        prop_footer(),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "body", "button_text", "section_title", "rows"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_INTERACTIVE_CTA_URL",
                    ("Interaktive CTA-URL senden", "Send interactive CTA URL"),
                    (
                        "Sendet eine Nachricht mit einer Schaltfläche, die einen Link öffnet; braucht --confirm.",
                        "Sends a message with a single button that opens a web link; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("body", s("Haupttext der Nachricht, max. 1024 Zeichen")),
                        ("button_text", s("Beschriftung der Schaltfläche, max. 20 Zeichen")),
                        ("url", s("Link, den die Schaltfläche öffnet")),
                        ("header_type", choice("Optionaler Kopfbereich über dem Text", &["none", "text", "image", "video", "document"])),
                        ("header_text", s("Pflicht bei header_type=text, max. 60 Zeichen")),
                        ("header_media_url", s("Öffentliche URL, pflicht bei header_type=image/video/document")),
                        prop_footer(),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "body", "button_text", "url"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_LOCATION",
                    ("Standort senden", "Send location"),
                    (
                        "Sendet eine Kartenposition für Breiten- und Längengrad; braucht --confirm.",
                        "Sends a map pin for a latitude and longitude; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("latitude", decimal("Dezimalgrad, z. B. 31.9539")),
                        ("longitude", decimal("Dezimalgrad, z. B. 35.9106")),
                        ("name", s("Titel unter der Kartenposition, z. B. Ladenname")),
                        ("address", s("Straßenadresse; nur sichtbar, wenn auch ein Name gesetzt ist")),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "latitude", "longitude"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_CONTACT",
                    ("Kontakt senden", "Send contact"),
                    (
                        "Sendet eine Visitenkarte, die der Empfänger speichern kann; braucht --confirm.",
                        "Sends a contact card the recipient can save; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        ("formatted_name", s("Angezeigter Name auf der Karte")),
                        ("first_name", s("Vorname")),
                        ("last_name", s("Nachname")),
                        ("phones", record_array("Nummern als Objekte {\"phone\": \"+49…\", \"type\": \"CELL|MAIN|WORK|HOME|IPHONE\"}")),
                        ("emails", record_array("E-Mails als Objekte {\"email\": …, \"type\": \"WORK|HOME\"}")),
                        ("company", s("Firma")),
                        ("job_title", s("Position")),
                        ("website", s("Website, wird als Arbeits-URL hinterlegt")),
                        prop_reply_to(),
                    ],
                    &["phone_number_id", "to", "formatted_name"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_SEND_REACTION",
                    ("Reaktion senden", "Send reaction"),
                    (
                        "Reagiert mit einem Emoji auf eine Nachricht; ohne Emoji wird die Reaktion entfernt; braucht --confirm.",
                        "Reacts to a message with an emoji; without an emoji the reaction is removed; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_to(),
                        prop_message_id(),
                        ("emoji", s("Ein einzelnes Emoji wie 👍; leer lassen, um die Reaktion zu entfernen")),
                    ],
                    &["phone_number_id", "to", "message_id"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_MARK_MESSAGE_AS_READ",
                    ("Nachricht als gelesen markieren", "Mark message as read"),
                    (
                        "Zeigt dem Absender blaue Häkchen, optional mit Tipp-Indikator; braucht --confirm.",
                        "Shows the sender blue ticks, optionally with a typing indicator; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        prop_message_id(),
                        ("show_typing_indicator", flag("„schreibt …“ anzeigen, bis geantwortet wird oder 25 Sekunden vergehen")),
                    ],
                    &["phone_number_id", "message_id"],
                    &[MESSAGING],
                    true,
                ),
                t(
                    "WHATSAPP_CREATE_MESSAGE_TEMPLATE",
                    ("Nachrichtenvorlage anlegen", "Create message template"),
                    (
                        "Reicht eine neue Nachrichtenvorlage zur Meta-Prüfung ein; braucht --confirm.",
                        "Submits a new message template for Meta review; requires --confirm.",
                    ),
                    &[
                        prop_business_account_id(),
                        ("name", s("Kleinbuchstaben, Ziffern und Unterstriche, z. B. order_update")),
                        ("language", s("Sprach- und Locale-Code, z. B. en_US")),
                        prop_category(true),
                        prop_components(),
                    ],
                    &["business_account_id", "name", "language", "category", "components"],
                    &[MANAGEMENT],
                    true,
                ),
                t(
                    "WHATSAPP_EDIT_MESSAGE_TEMPLATE",
                    ("Nachrichtenvorlage bearbeiten", "Edit message template"),
                    (
                        "Ersetzt alle Komponenten einer Vorlage und reicht sie erneut ein; braucht --confirm.",
                        "Replaces all components of a template and resubmits it for review; requires --confirm.",
                    ),
                    &[
                        ("message_template_id", s("Vorlagen-ID aus WHATSAPP_LIST_MESSAGE_TEMPLATES")),
                        prop_components(),
                        prop_category(false),
                    ],
                    &["message_template_id", "components"],
                    &[MANAGEMENT],
                    true,
                ),
                t(
                    "WHATSAPP_DELETE_MESSAGE_TEMPLATE",
                    ("Nachrichtenvorlage löschen", "Delete message template"),
                    (
                        "Löscht eine Nachrichtenvorlage aus dem Business-Konto; braucht --confirm.",
                        "Deletes a message template from the business account; requires --confirm.",
                    ),
                    &[
                        prop_business_account_id(),
                        ("template_id", s("Vorlagen-ID (hsm_id)")),
                        ("template_name", s("Exakter Vorlagenname, aus WHATSAPP_GET_TEMPLATE")),
                    ],
                    &["business_account_id", "template_id", "template_name"],
                    &[MANAGEMENT],
                    true,
                ),
                t(
                    "WHATSAPP_UPDATE_BUSINESS_PROFILE",
                    ("Business-Profil aktualisieren", "Update business profile"),
                    (
                        "Aktualisiert das öffentliche Business-Profil; leere Felder bleiben unverändert; braucht --confirm.",
                        "Updates the public business profile; empty fields are left unchanged; requires --confirm.",
                    ),
                    &[
                        prop_phone_number_id(),
                        ("about", s("Kurze Statuszeile, max. 139 Zeichen")),
                        ("address", s("Adresse")),
                        ("description", s("Beschreibung")),
                        ("email", s("E-Mail")),
                        ("websites", str_list("Bis zu zwei URLs")),
                        ("vertical", choice("Branche des Business-Profils", &VERTICALS)),
                    ],
                    &["phone_number_id"],
                    &[MANAGEMENT],
                    true,
                ),
                t(
                    "WHATSAPP_DELETE_MEDIA",
                    ("Medien löschen", "Delete media"),
                    (
                        "Löscht eine hochgeladene Mediendatei dauerhaft; braucht --confirm.",
                        "Permanently deletes an uploaded media file; requires --confirm.",
                    ),
                    &[prop_media_id()],
                    &["media_id"],
                    &[MESSAGING],
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

    /// `whatsappClient.sendMessage`: POST /{phone_number_id}/messages with the
    /// shared envelope (`messaging_product`, `recipient_type`, `to`, optional
    /// reply context) plus the per-type payload.
    fn send_message(&self, args: &Value, payload: Value) -> Result<HttpRequest> {
        let id = graph_id(args, "phone_number_id")?;
        let mut body = Map::new();
        body.insert("messaging_product".into(), json!("whatsapp"));
        body.insert("recipient_type".into(), json!("individual"));
        body.insert("to".into(), json!(recipient(args)?));
        if args.get("reply_to_message_id").and_then(Value::as_str).is_some() {
            body.insert(
                "context".into(),
                json!({"message_id": message_ref(args, "reply_to_message_id")?}),
            );
        }
        let payload = payload
            .as_object()
            .ok_or_else(|| anyhow!("message payload must be an object"))?;
        for (k, v) in payload {
            body.insert(k.clone(), v.clone());
        }
        Ok(HttpRequest::new("POST", self.url(&format!("/{id}/messages")))
            .json(&Value::Object(body)))
    }
}

#[allow(clippy::too_many_lines)]
impl ToolkitAdapter for WhatsApp {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        match tool {
            // AP: src/lib/actions/send-message.ts
            "WHATSAPP_SEND_MESSAGE" => {
                let text = req_str(args, "text")?;
                self.send_message(args, json!({"type": "text", "text": {"body": text}}))
            }
            // AP: src/lib/actions/send-media.ts
            "WHATSAPP_SEND_MEDIA" => {
                let kind = req_str(args, "type")?;
                if !MEDIA_TYPES.contains(&kind) {
                    bail!("`type` must be one of {}", MEDIA_TYPES.join(", "));
                }
                let mut media = Map::new();
                media.insert("link".into(), json!(req_str(args, "media")?));
                if let Some(caption) = opt_str(args, "caption") {
                    if MEDIA_TYPES_WITH_CAPTION.contains(&kind) {
                        media.insert("caption".into(), json!(caption));
                    }
                }
                if let Some(filename) = opt_str(args, "filename") {
                    if kind == "document" {
                        media.insert("filename".into(), json!(filename));
                    }
                }
                let mut payload = Map::new();
                payload.insert("type".into(), json!(kind));
                payload.insert(kind.into(), Value::Object(media));
                self.send_message(args, Value::Object(payload))
            }
            // AP: src/lib/actions/send-template.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_TEMPLATE" => {
                let header = strings(args, "header_parameters");
                let body = strings(args, "body_parameters");
                let button = strings(args, "button_url_parameters");
                let mut components = Vec::new();
                if !header.is_empty() {
                    components.push(json!({"type": "header", "parameters": text_parameters(&header)}));
                }
                if !body.is_empty() {
                    components.push(json!({"type": "body", "parameters": text_parameters(&body)}));
                }
                if !button.is_empty() {
                    components.push(
                        json!({"type": "button", "sub_type": "url", "index": 0,
                               "parameters": text_parameters(&button)}),
                    );
                }
                let mut template = Map::new();
                template.insert("name".into(), json!(req_str(args, "template_name")?));
                template.insert(
                    "language".into(),
                    json!({"code": req_str(args, "language_code")?}),
                );
                if !components.is_empty() {
                    template.insert("components".into(), json!(components));
                }
                self.send_message(args, json!({"type": "template", "template": template}))
            }
            // AP: src/lib/actions/send-interactive-buttons.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_INTERACTIVE_BUTTONS" => {
                let body = req_max_len(args, "body", 1024)?;
                let footer = opt_max_len(args, "footer", 60)?;
                max_len("header_text", opt_str(args, "header_text").unwrap_or(""), 60)?;
                let buttons = button_entries(args)?;
                let header = interactive_header(args)?;
                let mut interactive = Map::new();
                interactive.insert("type".into(), json!("button"));
                if let Some(header) = header {
                    interactive.insert("header".into(), header);
                }
                interactive.insert("body".into(), json!({"text": body}));
                if let Some(footer) = footer {
                    interactive.insert("footer".into(), json!({"text": footer}));
                }
                interactive.insert(
                    "action".into(),
                    json!({"buttons": buttons
                        .iter()
                        .map(|(id, title)| json!({"type": "reply", "reply": {"id": id, "title": title}}))
                        .collect::<Vec<_>>()}),
                );
                self.send_message(args, json!({"type": "interactive", "interactive": interactive}))
            }
            // AP: src/lib/actions/send-interactive-list.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_INTERACTIVE_LIST" => {
                let body = req_max_len(args, "body", 4096)?;
                let button_text = req_max_len(args, "button_text", 20)?;
                let section_title = req_max_len(args, "section_title", 24)?;
                let header_text = opt_max_len(args, "header_text", 60)?;
                let footer = opt_max_len(args, "footer", 60)?;
                let rows = row_entries(args)?;
                let mut interactive = Map::new();
                interactive.insert("type".into(), json!("list"));
                if let Some(header_text) = header_text {
                    interactive.insert("header".into(), json!({"type": "text", "text": header_text}));
                }
                interactive.insert("body".into(), json!({"text": body}));
                if let Some(footer) = footer {
                    interactive.insert("footer".into(), json!({"text": footer}));
                }
                interactive.insert(
                    "action".into(),
                    json!({
                        "button": button_text,
                        "sections": [{"title": section_title, "rows": rows
                            .iter()
                            .map(|(id, title, description)| match description {
                                Some(description) => json!({"id": id, "title": title, "description": description}),
                                None => json!({"id": id, "title": title}),
                            })
                            .collect::<Vec<_>>()}],
                    }),
                );
                self.send_message(args, json!({"type": "interactive", "interactive": interactive}))
            }
            // AP: src/lib/actions/send-interactive-cta-url.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_INTERACTIVE_CTA_URL" => {
                let body = req_max_len(args, "body", 1024)?;
                let button_text = req_max_len(args, "button_text", 20)?;
                let url = req_str(args, "url")?;
                let footer = opt_max_len(args, "footer", 60)?;
                max_len("header_text", opt_str(args, "header_text").unwrap_or(""), 60)?;
                let header = interactive_header(args)?;
                let mut interactive = Map::new();
                interactive.insert("type".into(), json!("cta_url"));
                if let Some(header) = header {
                    interactive.insert("header".into(), header);
                }
                interactive.insert("body".into(), json!({"text": body}));
                if let Some(footer) = footer {
                    interactive.insert("footer".into(), json!({"text": footer}));
                }
                interactive.insert(
                    "action".into(),
                    json!({"name": "cta_url", "parameters": {"display_text": button_text, "url": url}}),
                );
                self.send_message(args, json!({"type": "interactive", "interactive": interactive}))
            }
            // AP: src/lib/actions/send-location.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_LOCATION" => {
                let mut location = Map::new();
                location.insert("latitude".into(), json!(req_number(args, "latitude")?.to_string()));
                location.insert("longitude".into(), json!(req_number(args, "longitude")?.to_string()));
                if let Some(name) = opt_str(args, "name") {
                    location.insert("name".into(), json!(name));
                }
                if let Some(address) = opt_str(args, "address") {
                    location.insert("address".into(), json!(address));
                }
                self.send_message(args, json!({"type": "location", "location": location}))
            }
            // AP: src/lib/actions/send-contact.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_CONTACT" => {
                let mut name = Map::new();
                name.insert("formatted_name".into(), json!(req_str(args, "formatted_name")?));
                if let Some(first_name) = opt_str(args, "first_name") {
                    name.insert("first_name".into(), json!(first_name));
                }
                if let Some(last_name) = opt_str(args, "last_name") {
                    name.insert("last_name".into(), json!(last_name));
                }
                let phones: Vec<Value> = records(args, "phones")
                    .into_iter()
                    .map(|entry| {
                        let phone = required_string(&entry, "phone", "Phone", usize::MAX)?;
                        Ok(match optional_string(&entry, "type", "Type", usize::MAX)? {
                            Some(kind) => json!({"phone": phone, "type": kind}),
                            None => json!({"phone": phone}),
                        })
                    })
                    .collect::<Result<_>>()?;
                let emails: Vec<Value> = records(args, "emails")
                    .into_iter()
                    .map(|entry| {
                        let email = required_string(&entry, "email", "Email", usize::MAX)?;
                        Ok(match optional_string(&entry, "type", "Type", usize::MAX)? {
                            Some(kind) => json!({"email": email, "type": kind}),
                            None => json!({"email": email}),
                        })
                    })
                    .collect::<Result<_>>()?;
                let mut contact = Map::new();
                contact.insert("name".into(), Value::Object(name));
                if !phones.is_empty() {
                    contact.insert("phones".into(), json!(phones));
                }
                if !emails.is_empty() {
                    contact.insert("emails".into(), json!(emails));
                }
                let company = opt_str(args, "company");
                let job_title = opt_str(args, "job_title");
                if company.is_some() || job_title.is_some() {
                    let mut org = Map::new();
                    if let Some(company) = company {
                        org.insert("company".into(), json!(company));
                    }
                    if let Some(title) = job_title {
                        org.insert("title".into(), json!(title));
                    }
                    contact.insert("org".into(), Value::Object(org));
                }
                if let Some(website) = opt_str(args, "website") {
                    contact.insert("urls".into(), json!([{"url": website, "type": "WORK"}]));
                }
                self.send_message(args, json!({"type": "contacts", "contacts": [contact]}))
            }
            // AP: src/lib/actions/send-reaction.ts (whatsappClient.sendMessage)
            "WHATSAPP_SEND_REACTION" => {
                let message_id = message_ref(args, "message_id")?;
                let emoji = opt_str(args, "emoji").unwrap_or("");
                self.send_message(
                    args,
                    json!({"type": "reaction", "reaction": {"message_id": message_id, "emoji": emoji}}),
                )
            }
            // AP: src/lib/actions/mark-message-as-read.ts (whatsappClient.request)
            "WHATSAPP_MARK_MESSAGE_AS_READ" => {
                let id = graph_id(args, "phone_number_id")?;
                let mut body = Map::new();
                body.insert("messaging_product".into(), json!("whatsapp"));
                body.insert("status".into(), json!("read"));
                body.insert("message_id".into(), json!(message_ref(args, "message_id")?));
                if args
                    .get("show_typing_indicator")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    body.insert("typing_indicator".into(), json!({"type": "text"}));
                }
                Ok(HttpRequest::new("POST", self.url(&format!("/{id}/messages")))
                    .json(&Value::Object(body)))
            }
            // AP: src/lib/actions/get-media-url.ts
            "WHATSAPP_GET_MEDIA_URL" => {
                let id = graph_id(args, "media_id")?;
                Ok(HttpRequest::new("GET", self.url(&format!("/{id}"))))
            }
            // AP: src/lib/actions/delete-media.ts
            "WHATSAPP_DELETE_MEDIA" => {
                let id = graph_id(args, "media_id")?;
                Ok(HttpRequest::new("DELETE", self.url(&format!("/{id}"))))
            }
            // AP: src/lib/actions/list-phone-numbers.ts
            "WHATSAPP_LIST_PHONE_NUMBERS" => {
                let waba = graph_id(args, "business_account_id")?;
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/{waba}/phone_numbers{}",
                        query(&[
                            ("fields", Some(PHONE_LIST_FIELDS.to_string())),
                            ("limit", Some("100".into())),
                            ("after", opt_str(args, "after").map(String::from)),
                        ])
                    )),
                ))
            }
            // AP: src/lib/actions/get-phone-number.ts
            "WHATSAPP_GET_PHONE_NUMBER" => {
                let id = graph_id(args, "phone_number_id")?;
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/{id}{}",
                        query(&[("fields", Some(PHONE_DETAIL_FIELDS.to_string()))])
                    )),
                ))
            }
            // AP: src/lib/actions/get-business-profile.ts
            "WHATSAPP_GET_BUSINESS_PROFILE" => {
                let id = graph_id(args, "phone_number_id")?;
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/{id}/whatsapp_business_profile{}",
                        query(&[("fields", Some(PROFILE_FIELDS.to_string()))])
                    )),
                ))
            }
            // AP: src/lib/actions/list-message-templates.ts
            "WHATSAPP_LIST_MESSAGE_TEMPLATES" => {
                let waba = graph_id(args, "business_account_id")?;
                let status = match opt_str(args, "status") {
                    Some(status) if TEMPLATE_STATUSES.contains(&status) => Some(status.to_string()),
                    Some(_) => bail!(
                        "`status` must be one of {}",
                        TEMPLATE_STATUSES.join(", ")
                    ),
                    None => None,
                };
                let limit = opt_u64(args, "limit", 100, 1, 200);
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/{waba}/message_templates{}",
                        query(&[
                            ("fields", Some(TEMPLATE_LIST_FIELDS.to_string())),
                            ("limit", Some(limit.to_string())),
                            ("status", status),
                            ("after", opt_str(args, "after").map(String::from)),
                        ])
                    )),
                ))
            }
            // AP: src/lib/actions/send-from-template.ts (template lookup, also
            // src/lib/common/utils.ts message_template_fields)
            "WHATSAPP_GET_TEMPLATE" => {
                let id = graph_id(args, "template_id")?;
                Ok(HttpRequest::new(
                    "GET",
                    self.url(&format!(
                        "/{id}{}",
                        query(&[("fields", Some(TEMPLATE_LOOKUP_FIELDS.to_string()))])
                    )),
                ))
            }
            // AP: src/lib/actions/create-message-template.ts
            "WHATSAPP_CREATE_MESSAGE_TEMPLATE" => {
                let waba = graph_id(args, "business_account_id")?;
                let category = req_str(args, "category")?;
                if !TEMPLATE_CATEGORIES.contains(&category) {
                    bail!(
                        "`category` must be one of {}",
                        TEMPLATE_CATEGORIES.join(", ")
                    );
                }
                let body = json!({
                    "name": req_str(args, "name")?,
                    "language": req_str(args, "language")?,
                    "category": category,
                    "components": template_components(args, "components")?,
                });
                Ok(HttpRequest::new("POST", self.url(&format!("/{waba}/message_templates"))).json(&body))
            }
            // AP: src/lib/actions/edit-message-template.ts
            "WHATSAPP_EDIT_MESSAGE_TEMPLATE" => {
                let id = graph_id(args, "message_template_id")?;
                let mut body = Map::new();
                body.insert(
                    "components".into(),
                    template_components(args, "components")?,
                );
                if let Some(category) = opt_str(args, "category") {
                    if !TEMPLATE_CATEGORIES.contains(&category) {
                        bail!(
                            "`category` must be one of {}",
                            TEMPLATE_CATEGORIES.join(", ")
                        );
                    }
                    body.insert("category".into(), json!(category));
                }
                Ok(HttpRequest::new("POST", self.url(&format!("/{id}"))).json(&Value::Object(body)))
            }
            // AP: src/lib/actions/delete-message-template.ts (delete call; the
            // id/name lookup is WHATSAPP_GET_TEMPLATE)
            "WHATSAPP_DELETE_MESSAGE_TEMPLATE" => {
                let waba = graph_id(args, "business_account_id")?;
                Ok(HttpRequest::new(
                    "DELETE",
                    self.url(&format!(
                        "/{waba}/message_templates{}",
                        query(&[
                            ("hsm_id", Some(graph_id(args, "template_id")?.to_string())),
                            ("name", Some(req_str(args, "template_name")?.to_string())),
                        ])
                    )),
                ))
            }
            // AP: src/lib/actions/update-business-profile.ts
            "WHATSAPP_UPDATE_BUSINESS_PROFILE" => {
                let id = graph_id(args, "phone_number_id")?;
                let mut body = Map::new();
                body.insert("messaging_product".into(), json!("whatsapp"));
                for (key, field) in [
                    ("about", "about"),
                    ("address", "address"),
                    ("description", "description"),
                    ("email", "email"),
                ] {
                    if let Some(v) = opt_str(args, key) {
                        body.insert(field.into(), json!(v));
                    }
                }
                let websites = strings(args, "websites");
                if !websites.is_empty() {
                    body.insert("websites".into(), json!(websites));
                }
                if let Some(vertical) = opt_str(args, "vertical") {
                    if !VERTICALS.contains(&vertical) {
                        bail!("`vertical` must be one of {}", VERTICALS.join(", "));
                    }
                    body.insert("vertical".into(), json!(vertical));
                }
                Ok(HttpRequest::new(
                    "POST",
                    self.url(&format!("/{id}/whatsapp_business_profile")),
                )
                .json(&Value::Object(body)))
            }
            other => bail!("unknown WhatsApp tool `{other}`"),
        }
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        graph_error(&v)?;
        Ok(match tool {
            "WHATSAPP_SEND_MESSAGE"
            | "WHATSAPP_SEND_MEDIA"
            | "WHATSAPP_SEND_TEMPLATE"
            | "WHATSAPP_SEND_INTERACTIVE_BUTTONS"
            | "WHATSAPP_SEND_INTERACTIVE_LIST"
            | "WHATSAPP_SEND_INTERACTIVE_CTA_URL"
            | "WHATSAPP_SEND_LOCATION"
            | "WHATSAPP_SEND_CONTACT"
            | "WHATSAPP_SEND_REACTION" => message_send(&v),
            "WHATSAPP_MARK_MESSAGE_AS_READ"
            | "WHATSAPP_DELETE_MEDIA"
            | "WHATSAPP_EDIT_MESSAGE_TEMPLATE"
            | "WHATSAPP_DELETE_MESSAGE_TEMPLATE"
            | "WHATSAPP_UPDATE_BUSINESS_PROFILE" => confirmed(&v, tool)?,
            "WHATSAPP_GET_MEDIA_URL"
            | "WHATSAPP_GET_PHONE_NUMBER"
            | "WHATSAPP_GET_TEMPLATE"
            | "WHATSAPP_CREATE_MESSAGE_TEMPLATE" => v,
            "WHATSAPP_LIST_PHONE_NUMBERS" => graph_page(&v, "phone_numbers"),
            "WHATSAPP_LIST_MESSAGE_TEMPLATES" => graph_page(&v, "templates"),
            "WHATSAPP_GET_BUSINESS_PROFILE" => v["data"]
                .as_array()
                .and_then(|data| data.first().cloned())
                .ok_or_else(|| anyhow!("WhatsApp returned no business profile"))?,
            other => bail!("unknown WhatsApp tool `{other}`"),
        })
    }
}

/// Meta Graph errors can arrive as `{"error": {…}}` even on HTTP 200.
fn graph_error(v: &Value) -> Result<()> {
    if let Some(message) = v["error"]["message"].as_str() {
        bail!("WhatsApp API error: {message}");
    }
    Ok(())
}

/// Message-send envelope plus the first message id / WhatsApp id as a shortcut.
fn message_send(v: &Value) -> Value {
    let mut out = v.clone();
    out["message_id"] = v["messages"][0]["id"].clone();
    out["message_status"] = v["messages"][0]["message_status"].clone();
    out["wa_id"] = v["contacts"][0]["wa_id"].clone();
    out
}

/// `{success: true}` style responses; anything else is an error.
fn confirmed(v: &Value, tool: &str) -> Result<Value> {
    if v["success"] != json!(true) {
        bail!("WhatsApp did not confirm the change ({tool}).");
    }
    Ok(v.clone())
}

/// Graph data page -> `{<field>: […], count, next_after}` like the piece's
/// list actions (which flatten every page into one array).
fn graph_page(v: &Value, field: &str) -> Value {
    let items = v["data"].as_array().cloned().unwrap_or_default();
    let next_after = if v["paging"]["next"].is_null() {
        Value::Null
    } else {
        v["paging"]["cursors"]["after"].clone()
    };
    json!({field: items, "count": items.len(), "next_after": next_after})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_normalization() {
        assert_eq!(replace_placeholders("Hallo [[1]]!"), "Hallo {{1}}!");
        assert_eq!(replace_placeholders("[[order_id]]"), "{{order_id}}");
        assert_eq!(replace_placeholders("[[ nope ]]"), "[[ nope ]]");
        assert_eq!(replace_placeholders("plain"), "plain");
        let normalized = normalize_placeholders(&json!([{"type": "BODY", "text": "Hi [[1]]"}]));
        assert_eq!(normalized[0]["text"], json!("Hi {{1}}"));
    }

    #[test]
    fn ids_and_recipients_are_validated() {
        let w = WhatsApp::with_api_base("https://graph.facebook.com/test");
        assert!(w
            .build_request("WHATSAPP_GET_MEDIA_URL", &json!({"media_id": "../oauth"}))
            .is_err());
        assert!(w
            .build_request("WHATSAPP_GET_MEDIA_URL", &json!({"media_id": "26;rm"}))
            .is_err());
        assert!(w
            .build_request(
                "WHATSAPP_SEND_MESSAGE",
                &json!({"phone_number_id": "123", "to": "49 152", "text": "hi"})
            )
            .is_err());
        assert!(w
            .build_request(
                "WHATSAPP_SEND_MESSAGE",
                &json!({"phone_number_id": "123", "to": "49152", "text": ""})
            )
            .is_err());
        assert!(w
            .build_request(
                "WHATSAPP_SEND_INTERACTIVE_BUTTONS",
                &json!({"phone_number_id": "123", "to": "49152", "body": "b",
                        "buttons": [{"id": "a", "title": "Yes"}, {"id": "b", "title": "Yes"}]})
            )
            .is_err());
        assert!(w
            .build_request(
                "WHATSAPP_SEND_INTERACTIVE_BUTTONS",
                &json!({"phone_number_id": "123", "to": "49152", "body": "b",
                        "buttons": [{"id": "a", "title": "Yes"}, {"id": "b", "title": "No"},
                                    {"id": "c", "title": "Maybe"}, {"id": "d", "title": "No"}]})
            )
            .is_err());
        assert!(w
            .build_request(
                "WHATSAPP_SEND_INTERACTIVE_CTA_URL",
                &json!({"phone_number_id": "123", "to": "49152", "body": "b",
                        "button_text": "Open", "url": "https://x.test", "header_type": "text"})
            )
            .is_err());
        // `fields` values are percent-encoded in the query string.
        let r = w
            .build_request("WHATSAPP_GET_PHONE_NUMBER", &json!({"phone_number_id": "123"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert!(r.url.contains("/123?fields=id%2Cverified_name"), "{}", r.url);
    }
}
