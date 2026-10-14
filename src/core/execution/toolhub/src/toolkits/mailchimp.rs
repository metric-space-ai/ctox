//! Mailchimp toolkit (Marketing API 3.0). Action set modelled on Activepieces
//! `piece-mailchimp` (MIT).
//!
//! Auth: OAuth2 exactly like the piece (`https://login.mailchimp.com/oauth2/authorize`
//! / `https://login.mailchimp.com/oauth2/token`, no scopes, no PKCE). The token travels
//! in `Authorization: Bearer <token>`, which is what both the piece's `fetch` calls and
//! the `@mailchimp/mailchimp_marketing` SDK send once `setConfig({accessToken})` is used.
//!
//! Base URL: the API lives on the account's data centre,
//! `https://<dc>.api.mailchimp.com/3.0`, where `dc` is the `dc` field of
//! `GET https://login.mailchimp.com/oauth2/metadata` (see
//! `mailchimpCommon.getMailChimpServerPrefix`, `src/lib/common/index.ts`). The adapter is
//! therefore built with `Mailchimp::with_api_base("https://<dc>.api.mailchimp.com/3.0")`;
//! `Default` only carries a documented placeholder so that the catalogue can be dumped
//! without an account.
//!
//! SDK mapping: the piece talks to `@mailchimp/mailchimp_marketing`; each action's request
//! arm names the SDK method and the Mailchimp REST endpoint that method wraps (e.g.
//! `lists.setListMember` -> `PUT /lists/{list_id}/members/{subscriber_hash}`).
//! Members are addressed by the MD5 hash of the lower-cased address, exactly like
//! `mailchimpCommon.getMD5EmailHash`, so the MD5 is computed here (RFC 1321, no new
//! dependency). Mailchimp OAuth tokens carry no scopes, so every tool's `scopes` is empty.
//!
//! `MAILCHIMP_CREATE_CAMPAIGN` and `MAILCHIMP_SET_CAMPAIGN_CONTENT` are the two calls the
//! piece's `create_campaign` action makes (`campaigns.create` + `campaigns.setContent`),
//! split because one tool builds exactly one request. For the same reason
//! `MAILCHIMP_FIND_SUBSCRIBER` covers the action's single-list branch and makes `list_id`
//! mandatory: the "search every list" branch fans out over `GET /lists` plus one call per
//! audience.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "mailchimp";

/// Documented default data centre; real accounts must pass their own `dc`
/// (from `GET https://login.mailchimp.com/oauth2/metadata`) to `with_api_base`.
const DEFAULT_API_BASE: &str = "https://us1.api.mailchimp.com/3.0";

// ---------------------------------------------------------------- schema fragments

fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

fn obj(desc: &str) -> Value {
    json!({"type": "object", "description": desc})
}

fn fields(desc: &str) -> Value {
    s(desc)
}

const STATUSES: &[&str] = &[
    "subscribed",
    "unsubscribed",
    "cleaned",
    "pending",
    "transactional",
];

// ---------------------------------------------------------------- MD5 (RFC 1321)

/// Per-round shift amounts.
const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// Per-round additive constants: floor(2^32 * abs(sin(i + 1))).
const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// Hex MD5 digest, as Mailchimp's `subscriber_hash`.
fn md5_hex(input: &str) -> String {
    let msg = input.as_bytes();
    let bit_len = (msg.len() as u64).wrapping_mul(8);
    let mut data = msg.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_le_bytes());

    let (mut h0, mut h1, mut h2, mut h3) =
        (0x6745_2301u32, 0xefcd_ab89u32, 0x98ba_dcfeu32, 0x1032_5476u32);
    for chunk in data.chunks(64) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (h0, h1, h2, h3);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = a
                .wrapping_add(f)
                .wrapping_add(MD5_K[i])
                .wrapping_add(m[g]);
            let prev_d = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(MD5_S[i]));
            a = prev_d;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
    }
    // The digest is the little-endian byte representation of each state word.
    [h0, h1, h2, h3]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `mailchimpCommon.getMD5EmailHash`: MD5 over the lower-cased address.
fn email_hash(email: &str) -> String {
    md5_hex(&email.to_lowercase())
}

fn flag_on(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn enum_value<'a>(args: &'a Value, key: &str, allowed: &[&str]) -> Result<&'a str> {
    let v = req_str(args, key)?;
    if !allowed.contains(&v) {
        bail!("`{key}` must be one of {}", allowed.join(", "));
    }
    Ok(v)
}

/// Comma-separated prop as the query value Mailchimp expects (`fields`, `exclude_fields`).
fn csv(args: &Value, key: &str) -> Option<String> {
    opt_str(args, key).map(|v| v.trim().to_string())
}

fn put(m: &mut Map<String, Value>, key: &str, value: Value) {
    m.insert(key.into(), value);
}

fn list_of(v: &Value, key: &str) -> Vec<Value> {
    v[key].as_array().cloned().unwrap_or_default()
}

// ---------------------------------------------------------------- adapter

pub struct Mailchimp {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Mailchimp {
    fn default() -> Self {
        Self::with_api_base(DEFAULT_API_BASE)
    }
}

#[allow(clippy::too_many_lines)]
impl Mailchimp {
    pub fn with_api_base(api_base: &str) -> Self {
        // Mailchimp OAuth tokens are account-wide; the API defines no scopes.
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, &[], m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Mailchimp".into(),
            description: LocalizedText::new(
                "Audiences, Abonnenten, Tags und Kampagnen in Mailchimp lesen und bearbeiten.",
                "Read and change audiences, subscribers, tags and campaigns in Mailchimp.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://login.mailchimp.com/oauth2/authorize".into(),
                    token_url: "https://login.mailchimp.com/oauth2/token".into(),
                    // `scope: []` in the piece's `PieceAuth.OAuth2`.
                    default_scopes: Vec::new(),
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: false,
                    extra_authorize_params: Vec::new(),
                }),
                // The piece sends `Authorization: Bearer <access_token>`.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "MAILCHIMP_LIST_AUDIENCES",
                    ("Audiences auflisten", "List audiences"),
                    (
                        "Listet die Audiences (Listen) des Kontos; leer = alle.",
                        "Lists the account's audiences (lists); empty = all of them.",
                    ),
                    &[("count", int("Anzahl pro Seite (max. 1000)", 1, 1000, 1000))],
                    &[],
                    false,
                ),
                t(
                    "MAILCHIMP_LIST_CAMPAIGNS",
                    ("Kampagnen auflisten", "List campaigns"),
                    (
                        "Listet Kampagnen, optional nur Entwürfe (status save).",
                        "Lists campaigns, optionally only drafts (status save).",
                    ),
                    &[
                        ("count", int("Anzahl pro Seite (max. 1000)", 1, 1000, 1000)),
                        ("status", s("Statusfilter, z. B. save oder sent")),
                    ],
                    &[],
                    false,
                ),
                t(
                    "MAILCHIMP_LIST_STORES",
                    ("Shops auflisten", "List stores"),
                    (
                        "Listet die verbundenen E-Commerce-Shops des Kontos.",
                        "Lists the connected e-commerce stores of the account.",
                    ),
                    &[("count", int("Anzahl pro Seite (max. 1000)", 1, 1000, 1000))],
                    &[],
                    false,
                ),
                t(
                    "MAILCHIMP_FIND_SUBSCRIBER",
                    ("Abonnenten suchen", "Find subscriber"),
                    (
                        "Liest ein Abonnement per E-Mail (Merge-Felder, Interessen, Statistik).",
                        "Reads one subscriber by email (merge fields, interests, stats).",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        ("include_fields", str_list("Nur diese Felder; leer = alle")),
                        ("exclude_fields", str_list("Diese Felder weglassen")),
                    ],
                    &["list_id", "email"],
                    false,
                ),
                t(
                    "MAILCHIMP_FIND_TAG",
                    ("Tags suchen", "Find tags"),
                    (
                        "Sucht Tags einer Audience, optional nach Namen.",
                        "Searches the tags of an audience, optionally by name.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("name", s("Tag-Name; leer = alle Tags")),
                    ],
                    &["list_id"],
                    false,
                ),
                t(
                    "MAILCHIMP_FIND_CAMPAIGN",
                    ("Kampagnen durchsuchen", "Search campaigns"),
                    (
                        "Volltextsuche über alle Kampagnen des Kontos.",
                        "Full-text search over all campaigns of the account.",
                    ),
                    &[
                        ("query", s("Suchbegriff, z. B. \"Newsletter\" oder \"spring sale\"")),
                        ("fields", fields("Kommagetrennte Feldliste, die zurückgegeben wird")),
                        ("exclude_fields", fields("Kommagetrennte Feldliste, die ausgelassen wird")),
                    ],
                    &["query"],
                    false,
                ),
                t(
                    "MAILCHIMP_FIND_CUSTOMER",
                    ("Kunde suchen", "Find customer"),
                    (
                        "Sucht E-Commerce-Kunden eines Shops per E-Mail-Adresse.",
                        "Searches the customers of an e-commerce store by email address.",
                    ),
                    &[
                        ("store_id", s("Shop-ID (aus MAILCHIMP_LIST_STORES)")),
                        ("email_address", s("E-Mail des Kunden")),
                        ("count", int("Anzahl der Datensätze (max. 1000)", 1, 1000, 10)),
                        ("offset", int("Anzahl der übersprungenen Datensätze", 0, 1_000_000, 0)),
                        ("fields", fields("Kommagetrennte Feldliste, die zurückgegeben wird")),
                        ("exclude_fields", fields("Kommagetrennte Feldliste, die ausgelassen wird")),
                    ],
                    &["store_id", "email_address"],
                    false,
                ),
                t(
                    "MAILCHIMP_GET_CAMPAIGN_REPORT",
                    ("Kampagnen-Report lesen", "Get campaign report"),
                    (
                        "Liest den Report einer versandten Kampagne (Opens, Klicks, Bounces).",
                        "Reads the report of a sent campaign (opens, clicks, bounces).",
                    ),
                    &[
                        ("campaign_id", s("Kampagnen-ID (aus MAILCHIMP_LIST_CAMPAIGNS)")),
                        ("fields", fields("Kommagetrennte Feldliste, z. B. opens.unique_opens")),
                        ("exclude_fields", fields("Kommagetrennte Feldliste, z. B. timeseries,_links")),
                    ],
                    &["campaign_id"],
                    false,
                ),
                t(
                    "MAILCHIMP_ADD_MEMBER_TO_LIST",
                    ("Abonnent hinzufügen oder ändern", "Add or update subscriber"),
                    (
                        "Legt ein Abonnement an oder aktualisiert es (Upsert); braucht --confirm.",
                        "Creates or updates a subscriber (upsert); requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email_address", s("E-Mail des Abonnenten")),
                        (
                            "status_if_new",
                            choice("Status, falls der Kontakt neu ist", STATUSES),
                        ),
                        ("email_type", choice("Format der E-Mails", &["html", "text"])),
                        ("status", choice("Status bestehender Abonnenten", STATUSES)),
                        ("first_name", s("Vorname (Merge-Feld FNAME)")),
                        ("last_name", s("Nachname (Merge-Feld LNAME)")),
                        ("language", s("Sprache, z. B. en, es, fr")),
                        ("vip", flag("VIP-Status", false)),
                        ("skip_merge_validation", flag("Pflicht-Merge-Felder nicht prüfen", false)),
                    ],
                    &["list_id", "email_address", "status_if_new"],
                    true,
                ),
                t(
                    "MAILCHIMP_UPDATE_MEMBER_IN_LIST",
                    ("Abo-Status ändern", "Update member status"),
                    (
                        "Ändert den Abo-Status eines bestehenden Abonnenten; braucht --confirm.",
                        "Changes the subscription status of an existing member; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        ("status", choice("Neuer Abo-Status", STATUSES)),
                    ],
                    &["list_id", "email", "status"],
                    true,
                ),
                t(
                    "MAILCHIMP_ARCHIVE_SUBSCRIBER",
                    ("Abonnent archivieren", "Archive subscriber"),
                    (
                        "Archiviert ein Abonnement (DELETE des Members); braucht --confirm.",
                        "Archives a subscription (member delete); requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        (
                            "subscriber_hash",
                            s("E-Mail, MD5-Hash der E-Mail oder contact_id"),
                        ),
                    ],
                    &["list_id", "subscriber_hash"],
                    true,
                ),
                t(
                    "MAILCHIMP_ADD_SUBSCRIBER_TO_TAG",
                    ("Abonnenten taggen", "Add subscriber to tag"),
                    (
                        "Aktiviert ein Tag für einen Abonnenten; braucht --confirm.",
                        "Activates a tag on a subscriber; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        ("tag_name", s("Name des Tags; wird bei Bedarf angelegt")),
                    ],
                    &["list_id", "email", "tag_name"],
                    true,
                ),
                t(
                    "MAILCHIMP_REMOVE_SUBSCRIBER_FROM_TAG",
                    ("Tag entfernen", "Remove subscriber from tag"),
                    (
                        "Deaktiviert ein Tag für einen Abonnenten; braucht --confirm.",
                        "Deactivates a tag on a subscriber; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        ("tag_name", s("Name des Tags")),
                    ],
                    &["list_id", "email", "tag_name"],
                    true,
                ),
                t(
                    "MAILCHIMP_ADD_NOTE_TO_SUBSCRIBER",
                    ("Notiz an Abonnent", "Add note to subscriber"),
                    (
                        "Hängt eine interne Notiz an einen Abonnenten; braucht --confirm.",
                        "Appends an internal note to a subscriber; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        ("note", s("Text der Notiz")),
                    ],
                    &["list_id", "email", "note"],
                    true,
                ),
                t(
                    "MAILCHIMP_CREATE_CUSTOM_EVENT",
                    ("Eigenes Ereignis protokollieren", "Create custom event"),
                    (
                        "Protokolliert ein benanntes Ereignis für einen Abonnenten; braucht --confirm.",
                        "Logs a named event for a subscriber; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email", s("E-Mail des Abonnenten")),
                        (
                            "name",
                            s("Ereignisname, klein geschrieben, z. B. purchased_item"),
                        ),
                        ("properties", obj("Optionale Schlüssel-Wert-Daten zum Ereignis")),
                    ],
                    &["list_id", "email", "name"],
                    true,
                ),
                t(
                    "MAILCHIMP_UNSUBSCRIBE_EMAIL",
                    ("E-Mail abbestellen", "Unsubscribe email"),
                    (
                        "Setzt eine E-Mail-Adresse per Batch-Upsert auf \"unsubscribed\"; braucht --confirm.",
                        "Sets an email address to \"unsubscribed\" via batch upsert; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("email_address", s("E-Mail-Adresse, die abbestellt wird")),
                        ("skip_merge_validation", flag("Pflicht-Merge-Felder nicht prüfen", false)),
                        ("skip_duplicate_check", flag("Duplikate im Request ignorieren", false)),
                        ("update_existing", flag("Bestehende Abonnenten mit ändern", true)),
                    ],
                    &["list_id", "email_address"],
                    true,
                ),
                t(
                    "MAILCHIMP_CREATE_AUDIENCE",
                    ("Audience anlegen", "Create audience"),
                    (
                        "Legt eine neue Audience (Liste) mit Pflichtkontaktadresse an; braucht --confirm.",
                        "Creates a new audience (list) with the required contact address; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name der Liste")),
                        ("company", s("Firma für die Kontaktangaben")),
                        ("address1", s("Straße und Hausnummer")),
                        ("address2", s("Zusatz zur Adresse (optional)")),
                        ("city", s("Stadt")),
                        ("state", s("Bundesland / Region")),
                        ("zip", s("Postleitzahl")),
                        ("country", s("Land")),
                        ("phone", s("Telefonnummer (optional)")),
                        (
                            "permission_reminder",
                            s("Erinnerungstext, z. B. \"You are receiving this email because you signed up.\""),
                        ),
                        ("from_name", s("Standard-Absendername für Kampagnen")),
                        ("from_email", s("Standard-Absenderadresse für Kampagnen")),
                        ("subject", s("Standard-Betreffzeile für Kampagnen")),
                        ("language", s("Standardsprache für Kampagnen, z. B. en")),
                        ("email_type_option", flag("HTML und Text anbieten", true)),
                        ("use_archive_bar", flag("Archive-Bar verwenden", false)),
                        ("notify_on_subscribe", s("E-Mail für Abonnement-Benachrichtigungen")),
                        ("notify_on_unsubscribe", s("E-Mail für Abmeldungs-Benachrichtigungen")),
                        ("double_optin", flag("Double-Opt-in verlangen", false)),
                        ("marketing_permissions", flag("Marketing-Permissions (GDPR)", false)),
                    ],
                    &[
                        "name",
                        "company",
                        "address1",
                        "city",
                        "country",
                        "permission_reminder",
                        "from_name",
                        "from_email",
                        "subject",
                        "language",
                    ],
                    true,
                ),
                t(
                    "MAILCHIMP_CREATE_TAG",
                    ("Tag anlegen", "Create tag"),
                    (
                        "Legt ein Tag als leeres statisches Segment an; braucht --confirm.",
                        "Creates a tag as an empty static segment; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("name", s("Name des Tags")),
                    ],
                    &["list_id", "name"],
                    true,
                ),
                t(
                    "MAILCHIMP_CREATE_CAMPAIGN",
                    ("Kampagne anlegen", "Create campaign"),
                    (
                        "Legt eine Entwurfskampagne an; Inhalt folgt mit MAILCHIMP_SET_CAMPAIGN_CONTENT; braucht --confirm.",
                        "Creates a draft campaign; set its content with MAILCHIMP_SET_CAMPAIGN_CONTENT; requires --confirm.",
                    ),
                    &[
                        (
                            "campaign_type",
                            choice("Kampagnentyp", &["regular", "plaintext", "ab", "rss", "variate"]),
                        ),
                        ("list_id", s("Audience-ID (aus MAILCHIMP_LIST_AUDIENCES)")),
                        ("subject_line", s("Betreffzeile der Kampagne")),
                        ("title", s("Titel der Kampagne")),
                        ("from_name", s("Absendername")),
                        ("from_email", s("Absenderadresse")),
                        ("reply_to", s("Antwortadresse")),
                        ("to_name", s("Empfängername, z. B. *|FNAME|*; leer = von Mailchimp gesetzt")),
                    ],
                    &[
                        "campaign_type",
                        "list_id",
                        "subject_line",
                        "title",
                        "from_name",
                        "from_email",
                        "reply_to",
                    ],
                    true,
                ),
                t(
                    "MAILCHIMP_SET_CAMPAIGN_CONTENT",
                    ("Kampagneninhalt setzen", "Set campaign content"),
                    (
                        "Setzt Inhalt einer Kampagne aus Template, URL oder HTML; braucht --confirm.",
                        "Sets the content of a campaign from a template, URL or HTML; requires --confirm.",
                    ),
                    &[
                        ("campaign_id", s("Kampagnen-ID aus MAILCHIMP_CREATE_CAMPAIGN")),
                        ("template_id", s("Template-ID (für content_type template)")),
                        ("content_url", s("URL des Inhalts (für content_type url)")),
                        ("html_content", s("HTML-Inhalt (für content_type html)")),
                    ],
                    &["campaign_id"],
                    true,
                ),
                t(
                    "MAILCHIMP_SEND_CAMPAIGN",
                    ("Kampagne senden", "Send campaign"),
                    (
                        "Sendet eine Entwurfskampagne sofort; nicht rückgängig zu machen; braucht --confirm.",
                        "Sends a draft campaign immediately; there is no undo; requires --confirm.",
                    ),
                    &[("campaign_id", s("Kampagnen-ID eines Entwurfs (status save)"))],
                    &["campaign_id"],
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
}

impl ToolkitAdapter for Mailchimp {
    fn connection_fields(&self) -> Vec<ConnectionField> {
        vec![ConnectionField::new(
            "datacenter",
            ("Mailchimp-Rechenzentrum", "Mailchimp data center"),
            (
                "Kürzel wie „us21“. Es steht am Ende des API-Keys (…-us21) und wird dann automatisch erkannt.",
                "Code such as \"us21\". It is the suffix of your API key (…-us21) and is detected automatically.",
            ),
            "us21",
            true,
        )]
    }
    fn connection_api_base(&self, params: &std::collections::BTreeMap<String, String>) -> Result<Option<String>> {
        params
            .get("datacenter")
            .map(|v| {
                let dc = v.trim().to_ascii_lowercase();
                let ok = dc.len() <= 8
                    && dc.len() > 2
                    && dc.bytes().take(2).all(|b| b.is_ascii_lowercase())
                    && dc.bytes().skip(2).all(|b| b.is_ascii_digit());
                if !ok {
                    anyhow::bail!("invalid `datacenter`: expected a code like `us21`");
                }
                Ok(format!("https://{dc}.api.mailchimp.com/3.0"))
            })
            .transpose()
    }
    fn default_api_base(&self) -> Option<String> {
        Some(self.api_base.clone())
    }
    fn derive_connection_params(
        &self,
        credential: Option<&str>,
        _token_extra: &std::collections::BTreeMap<String, String>,
    ) -> std::collections::BTreeMap<String, String> {
        credential
            .and_then(|c| c.rsplit_once('-'))
            .map(|(_, dc)| dc.to_string())
            .filter(|dc| self.connection_api_base(&[("datacenter".to_string(), dc.clone())].into()).is_ok())
            .map(|dc| [("datacenter".to_string(), dc)].into())
            .unwrap_or_default()
    }
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/common/index.ts — `lists.getAllLists` -> GET /lists
            "MAILCHIMP_LIST_AUDIENCES" => HttpRequest::new(
                "GET",
                self.url("/lists").with_query(query(&[
                    ("fields", Some("lists.id,lists.name,total_items".into())),
                    ("count", Some(count_query(args))),
                ])),
            ),
            // AP: src/lib/common/index.ts — `campaigns.list` -> GET /campaigns
            "MAILCHIMP_LIST_CAMPAIGNS" => HttpRequest::new(
                "GET",
                self.url("/campaigns").with_query(query(&[
                    (
                        "fields",
                        Some(
                            "campaigns.id,campaigns.settings.title,campaigns.status,total_items"
                                .into(),
                        ),
                    ),
                    ("count", Some(count_query(args))),
                    ("status", opt_str(args, "status").map(String::from)),
                ])),
            ),
            // AP: src/lib/common/index.ts — `ecommerce.stores` -> GET /ecommerce/stores
            "MAILCHIMP_LIST_STORES" => HttpRequest::new(
                "GET",
                self.url("/ecommerce/stores").with_query(query(&[
                    ("fields", Some("stores.id,stores.name,total_items".into())),
                    ("count", Some(count_query(args))),
                ])),
            ),
            // AP: src/lib/actions/find-subscriber.ts — `lists.getListMember`
            //     -> GET /lists/{list_id}/members/{subscriber_hash}
            "MAILCHIMP_FIND_SUBSCRIBER" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/lists/{list}/members/{hash}")).with_query(query(&[
                        ("fields", joined(args, "include_fields")),
                        ("exclude_fields", joined(args, "exclude_fields")),
                    ])),
                )
            }
            // AP: src/lib/actions/find-tag.ts — `lists.tagSearch`
            //     -> GET /lists/{list_id}/tag-search
            "MAILCHIMP_FIND_TAG" => {
                let list = path_segment(args, "list_id")?;
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/lists/{list}/tag-search")).with_query(query(&[(
                        "name",
                        opt_str(args, "name").map(String::from),
                    )])),
                )
            }
            // AP: src/lib/actions/find-campaign.ts — `searchCampaigns.search`
            //     -> GET /search-campaigns
            "MAILCHIMP_FIND_CAMPAIGN" => HttpRequest::new(
                "GET",
                self.url("/search-campaigns").with_query(query(&[
                    ("query", Some(req_str(args, "query")?.trim().to_string())),
                    ("fields", csv(args, "fields")),
                    ("exclude_fields", csv(args, "exclude_fields")),
                ])),
            ),
            // AP: src/lib/actions/find-customer.ts — `ecommerce.getAllStoreCustomers`
            //     -> GET /ecommerce/stores/{store_id}/customers
            "MAILCHIMP_FIND_CUSTOMER" => {
                let store = path_segment(args, "store_id")?;
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/ecommerce/stores/{store}/customers")).with_query(
                        query(&[
                            (
                                "email_address",
                                Some(req_str(args, "email_address")?.trim().to_string()),
                            ),
                            ("count", Some(opt_u64(args, "count", 10, 1, 1000).to_string())),
                            ("offset", Some(opt_u64(args, "offset", 0, 0, 1_000_000).to_string())),
                            ("fields", csv(args, "fields")),
                            ("exclude_fields", csv(args, "exclude_fields")),
                        ]),
                    ),
                )
            }
            // AP: src/lib/actions/get-campaign-report.ts — `reports.getCampaignReport`
            //     -> GET /reports/{campaign_id}
            "MAILCHIMP_GET_CAMPAIGN_REPORT" => {
                let campaign = path_segment(args, "campaign_id")?;
                HttpRequest::new(
                    "GET",
                    self.url(&format!("/reports/{campaign}")).with_query(query(&[
                        ("fields", csv(args, "fields")),
                        ("exclude_fields", csv(args, "exclude_fields")),
                    ])),
                )
            }
            // AP: src/lib/actions/add-member-to-list.ts — `lists.setListMember`
            //     -> PUT /lists/{list_id}/members/{subscriber_hash}
            "MAILCHIMP_ADD_MEMBER_TO_LIST" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email_address")?);
                let mut b = Map::new();
                put(
                    &mut b,
                    "email_address",
                    json!(req_str(args, "email_address")?.trim()),
                );
                put(&mut b, "status_if_new", json!(enum_value(args, "status_if_new", STATUSES)?));
                if let Some(v) = opt_str(args, "email_type") {
                    if !["html", "text"].contains(&v) {
                        bail!("`email_type` must be html or text");
                    }
                    put(&mut b, "email_type", json!(v));
                }
                if opt_str(args, "status").is_some() {
                    put(&mut b, "status", json!(enum_value(args, "status", STATUSES)?));
                }
                let mut merge = Map::new();
                if let Some(v) = opt_str(args, "first_name") {
                    merge.insert("FNAME".into(), json!(v));
                }
                if let Some(v) = opt_str(args, "last_name") {
                    merge.insert("LNAME".into(), json!(v));
                }
                if !merge.is_empty() {
                    put(&mut b, "merge_fields", Value::Object(merge));
                }
                if let Some(v) = opt_str(args, "language") {
                    put(&mut b, "language", json!(v));
                }
                if let Some(v) = args.get("vip").and_then(Value::as_bool) {
                    put(&mut b, "vip", json!(v));
                }
                HttpRequest::new(
                    "PUT",
                    self.url(&format!("/lists/{list}/members/{hash}")).with_query(query(&[(
                        "skip_merge_validation",
                        flag_on(args, "skip_merge_validation").then(|| "true".to_string()),
                    )])),
                )
                .json(&Value::Object(b))
            }
            // AP: src/lib/actions/update-subscriber-status.ts — `lists.updateListMember`
            //     -> PATCH /lists/{list_id}/members/{subscriber_hash}
            "MAILCHIMP_UPDATE_MEMBER_IN_LIST" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                HttpRequest::new(
                    "PATCH",
                    self.url(&format!("/lists/{list}/members/{hash}")),
                )
                .json(&json!({"status": enum_value(args, "status", STATUSES)?}))
            }
            // AP: src/lib/actions/archive-subscriber.ts — `lists.deleteListMember`
            //     -> DELETE /lists/{list_id}/members/{subscriber_hash}
            "MAILCHIMP_ARCHIVE_SUBSCRIBER" => {
                let list = path_segment(args, "list_id")?;
                let raw = req_str(args, "subscriber_hash")?.trim();
                // The action hashes only when the input looks like an email address.
                let hash = if raw.contains('@') {
                    email_hash(raw)
                } else {
                    path_segment(args, "subscriber_hash")?.to_string()
                };
                HttpRequest::new("DELETE", self.url(&format!("/lists/{list}/members/{hash}")))
            }
            // AP: src/lib/actions/add-subscriber-to-tag.ts — `lists.updateListMemberTags`
            //     -> POST /lists/{list_id}/members/{subscriber_hash}/tags
            "MAILCHIMP_ADD_SUBSCRIBER_TO_TAG" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                let tag = req_str(args, "tag_name")?.trim();
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/lists/{list}/members/{hash}/tags")),
                )
                .json(&json!({"tags": [{"name": tag, "status": "active"}]}))
            }
            // AP: src/lib/actions/remove-subscriber-from-tag.ts (explicit fetch)
            //     -> POST /lists/{list_id}/members/{subscriber_hash}/tags
            "MAILCHIMP_REMOVE_SUBSCRIBER_FROM_TAG" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                let tag = req_str(args, "tag_name")?.trim();
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/lists/{list}/members/{hash}/tags")),
                )
                .json(&json!({"tags": [{"name": tag, "status": "inactive"}]}))
            }
            // AP: src/lib/actions/add-note-to-subscriber.ts (explicit fetch)
            //     -> POST /lists/{list_id}/members/{subscriber_hash}/notes
            "MAILCHIMP_ADD_NOTE_TO_SUBSCRIBER" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/lists/{list}/members/{hash}/notes")),
                )
                .json(&json!({"note": req_str(args, "note")?}))
            }
            // AP: src/lib/actions/create-custom-event.ts — `lists.createListMemberEvent`
            //     -> POST /lists/{list_id}/members/{subscriber_hash}/events
            "MAILCHIMP_CREATE_CUSTOM_EVENT" => {
                let list = path_segment(args, "list_id")?;
                let hash = email_hash(req_str(args, "email")?);
                let mut b = Map::new();
                put(&mut b, "name", json!(req_str(args, "name")?.trim()));
                if let Some(p) = args.get("properties") {
                    if !p.is_object() {
                        bail!("`properties` must be an object");
                    }
                    put(&mut b, "properties", p.clone());
                }
                HttpRequest::new(
                    "POST",
                    self.url(&format!("/lists/{list}/members/{hash}/events")),
                )
                .json(&Value::Object(b))
            }
            // AP: src/lib/actions/unsubscribe-email.ts — `lists.batchListMembers`
            //     -> POST /lists/{list_id}
            "MAILCHIMP_UNSUBSCRIBE_EMAIL" => {
                let list = path_segment(args, "list_id")?;
                let email = req_str(args, "email_address")?.trim();
                HttpRequest::new("POST", self.url(&format!("/lists/{list}")).with_query(
                    query(&[
                        (
                            "skip_merge_validation",
                            flag_on(args, "skip_merge_validation").then(|| "true".to_string()),
                        ),
                        (
                            "skip_duplicate_check",
                            flag_on(args, "skip_duplicate_check").then(|| "true".to_string()),
                        ),
                    ]),
                ))
                .json(&json!({
                    "members": [{"email_address": email, "status": "unsubscribed"}],
                    "update_existing": flag_on(args, "update_existing"),
                }))
            }
            // AP: src/lib/actions/create-audience.ts — `lists.createList` -> POST /lists
            "MAILCHIMP_CREATE_AUDIENCE" => {
                let mut contact = Map::new();
                for key in [
                    "company",
                    "address1",
                    "address2",
                    "city",
                    "state",
                    "zip",
                    "country",
                    "phone",
                ] {
                    put(&mut contact, key, json!(opt_str(args, key).unwrap_or("")));
                }
                let mut defaults = Map::new();
                for key in ["from_name", "from_email", "subject", "language"] {
                    put(&mut defaults, key, json!(req_str(args, key)?.trim()));
                }
                let mut b = Map::new();
                put(&mut b, "name", json!(req_str(args, "name")?.trim()));
                put(&mut b, "contact", Value::Object(contact));
                put(
                    &mut b,
                    "permission_reminder",
                    json!(req_str(args, "permission_reminder")?),
                );
                put(&mut b, "campaign_defaults", Value::Object(defaults));
                for key in [
                    "email_type_option",
                    "use_archive_bar",
                    "double_optin",
                    "marketing_permissions",
                ] {
                    put(&mut b, key, json!(flag_on(args, key)));
                }
                for key in ["notify_on_subscribe", "notify_on_unsubscribe"] {
                    put(&mut b, key, json!(opt_str(args, key).unwrap_or("")));
                }
                HttpRequest::new("POST", self.url("/lists")).json(&Value::Object(b))
            }
            // AP: src/lib/actions/create-tag.ts — `lists.createSegment`
            //     -> POST /lists/{list_id}/segments
            "MAILCHIMP_CREATE_TAG" => {
                let list = path_segment(args, "list_id")?;
                HttpRequest::new("POST", self.url(&format!("/lists/{list}/segments")))
                    .json(&json!({"name": req_str(args, "name")?.trim(), "static_segment": []}))
            }
            // AP: src/lib/actions/create-campaign.ts — `campaigns.create` -> POST /campaigns
            "MAILCHIMP_CREATE_CAMPAIGN" => {
                let mut settings = Map::new();
                for key in [
                    "subject_line",
                    "title",
                    "from_name",
                    "from_email",
                    "reply_to",
                    "to_name",
                ] {
                    if key == "to_name" {
                        put(&mut settings, key, json!(opt_str(args, key).unwrap_or("")));
                    } else {
                        put(&mut settings, key, json!(req_str(args, key)?.trim()));
                    }
                }
                HttpRequest::new("POST", self.url("/campaigns")).json(&json!({
                    "type": enum_value(args, "campaign_type", &["regular", "plaintext", "ab", "rss", "variate"])?,
                    "recipients": {"list_id": path_segment(args, "list_id")?},
                    "settings": Value::Object(settings),
                }))
            }
            // AP: src/lib/actions/create-campaign.ts — `campaigns.setContent`
            //     -> PUT /campaigns/{campaign_id}/content
            "MAILCHIMP_SET_CAMPAIGN_CONTENT" => {
                let campaign = path_segment(args, "campaign_id")?;
                let mut b = Map::new();
                if let Some(v) = opt_str(args, "template_id") {
                    put(&mut b, "template", json!({"id": v}));
                }
                if let Some(v) = opt_str(args, "content_url") {
                    put(&mut b, "url", json!(v));
                }
                if let Some(v) = opt_str(args, "html_content") {
                    put(&mut b, "html", json!(v));
                }
                if b.is_empty() {
                    bail!("pass at least one of `template_id`, `content_url` or `html_content`");
                }
                HttpRequest::new("PUT", self.url(&format!("/campaigns/{campaign}/content")))
                    .json(&Value::Object(b))
            }
            // AP: src/lib/actions/send-campaign.ts — `campaigns.send`
            //     -> POST /campaigns/{campaign_id}/actions/send
            "MAILCHIMP_SEND_CAMPAIGN" => {
                let campaign = path_segment(args, "campaign_id")?;
                HttpRequest::new("POST", self.url(&format!("/campaigns/{campaign}/actions/send")))
            }
            other => bail!("unknown Mailchimp tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let body = response.json()?;
        Ok(match tool {
            "MAILCHIMP_LIST_AUDIENCES" => json!({
                "success": true,
                "lists": list_of(&body, "lists"),
                "total_items": body["total_items"],
                "count": list_of(&body, "lists").len(),
                "_links": body["_links"],
            }),
            "MAILCHIMP_LIST_CAMPAIGNS" => json!({
                "success": true,
                "campaigns": list_of(&body, "campaigns"),
                "total_items": body["total_items"],
                "count": list_of(&body, "campaigns").len(),
                "_links": body["_links"],
            }),
            "MAILCHIMP_LIST_STORES" => json!({
                "success": true,
                "stores": list_of(&body, "stores"),
                "total_items": body["total_items"],
                "count": list_of(&body, "stores").len(),
                "_links": body["_links"],
            }),
            // `find_subscriber` list branch of the action.
            "MAILCHIMP_FIND_SUBSCRIBER" => {
                let email = body["email_address"].clone();
                let primary = json!({
                    "id": body["id"],
                    "email_address": email,
                    "status": body["status"],
                    "list_id": body["list_id"],
                    "merge_fields": body["merge_fields"],
                    "member_rating": body["member_rating"],
                    "vip": body["vip"],
                    "email_type": body["email_type"],
                    "language": body["language"],
                    "timestamp_signup": body["timestamp_signup"],
                    "last_changed": body["last_changed"],
                });
                json!({
                    "success": true,
                    "found": true,
                    "match_type": "exact",
                    "subscribers": [{
                        "id": body["id"],
                        "email_address": body["email_address"],
                        "unique_email_id": body["unique_email_id"],
                        "email_type": body["email_type"],
                        "status": body["status"],
                        "merge_fields": body["merge_fields"],
                        "interests": body["interests"],
                        "stats": body["stats"],
                        "ip_signup": body["ip_signup"],
                        "timestamp_signup": body["timestamp_signup"],
                        "ip_opt": body["ip_opt"],
                        "timestamp_opt": body["timestamp_opt"],
                        "member_rating": body["member_rating"],
                        "last_changed": body["last_changed"],
                        "language": body["language"],
                        "vip": body["vip"],
                        "email_client": body["email_client"],
                        "location": body["location"],
                        "tags_count": body["tags_count"],
                        "list_id": body["list_id"],
                    }],
                    "primary_subscriber": primary,
                })
            }
            "MAILCHIMP_FIND_TAG" => {
                let tags = list_of(&body, "tags");
                json!({
                    "success": true,
                    "tags": tags,
                    "total_items": body["total_items"],
                    "found_tags": tags.len(),
                })
            }
            "MAILCHIMP_FIND_CAMPAIGN" => json!({
                "success": true,
                "results": list_of(&body, "results"),
                "total_items": body["total_items"],
                "count": list_of(&body, "results").len(),
                "_links": body["_links"],
            }),
            "MAILCHIMP_FIND_CUSTOMER" => {
                let customers = list_of(&body, "customers");
                json!({
                    "success": true,
                    "store_id": body["store_id"],
                    "customers": customers,
                    "total_items": body["total_items"],
                    "found_customers": customers.len(),
                    "_links": body["_links"],
                })
            }
            "MAILCHIMP_GET_CAMPAIGN_REPORT" => {
                let sent = body["emails_sent"].as_f64().unwrap_or(0.0);
                let rate = |num: f64, den: f64| {
                    if den > 0.0 {
                        num / den * 100.0
                    } else {
                        0.0
                    }
                };
                let hard = body["bounces"]["hard_bounces"].as_f64().unwrap_or(0.0);
                let soft = body["bounces"]["soft_bounces"].as_f64().unwrap_or(0.0);
                // Counts are echoed as integers, like the report itself.
                let count = |k: &str| json!(body["bounces"][k].as_i64().unwrap_or(0));
                json!({
                    "success": true,
                    "report": body.clone(),
                    "summary": {
                        "campaign_title": body["campaign_title"],
                        "subject_line": body["subject_line"],
                        "emails_sent": body["emails_sent"],
                        "unique_opens": body["opens"]["unique_opens"],
                        "unique_clicks": body["clicks"]["unique_clicks"],
                        "open_rate": body["opens"]["open_rate"],
                        "click_rate": body["clicks"]["click_rate"],
                        "bounce_rate": rate(hard + soft, sent),
                        "unsub_rate": rate(
                            body["unsubscribed"].as_f64().unwrap_or(0.0), sent),
                        "abuse_rate": rate(body["abuse_reports"].as_f64().unwrap_or(0.0), sent),
                    },
                    "performance_metrics": {
                        "opens": {
                            "total": body["opens"]["opens_total"],
                            "unique": body["opens"]["unique_opens"],
                            "rate": body["opens"]["open_rate"],
                            "last_open": body["opens"]["last_open"],
                        },
                        "clicks": {
                            "total": body["clicks"]["clicks_total"],
                            "unique": body["clicks"]["unique_clicks"],
                            "unique_subscribers": body["clicks"]["unique_subscriber_clicks"],
                            "rate": body["clicks"]["click_rate"],
                            "last_click": body["clicks"]["last_click"],
                        },
                        "bounces": {
                            "hard": count("hard_bounces"),
                            "soft": count("soft_bounces"),
                            "syntax_errors": count("syntax_errors"),
                            "total": json!(body["bounces"]["hard_bounces"].as_i64().unwrap_or(0)
                                + body["bounces"]["soft_bounces"].as_i64().unwrap_or(0)
                                + body["bounces"]["syntax_errors"].as_i64().unwrap_or(0)),
                        },
                        "engagement": {
                            "forwards": body["forwards"]["forwards_count"],
                            "facebook_likes": body["facebook_likes"]["facebook_likes"],
                        },
                    },
                    "industry_comparison": body["industry_stats"],
                    "list_performance": body["list_stats"],
                    "ecommerce_data": body["ecommerce"],
                    "share_report": body["share_report"],
                    "timeseries": body["timeseries"],
                })
            }
            "MAILCHIMP_ADD_MEMBER_TO_LIST" => json!({
                "success": true,
                "id": body["id"],
                "email_address": body["email_address"],
                "unique_email_id": body["unique_email_id"],
                "contact_id": body["contact_id"],
                "full_name": body["full_name"],
                "web_id": body["web_id"],
                "email_type": body["email_type"],
                "status": body["status"],
                "merge_fields": body["merge_fields"],
                "interests": body["interests"],
                "stats": body["stats"],
                "ip_signup": body["ip_signup"],
                "timestamp_signup": body["timestamp_signup"],
                "ip_opt": body["ip_opt"],
                "timestamp_opt": body["timestamp_opt"],
                "member_rating": body["member_rating"],
                "last_changed": body["last_changed"],
                "language": body["language"],
                "vip": body["vip"],
                "email_client": body["email_client"],
                "location": body["location"],
                "marketing_permissions": body["marketing_permissions"],
                "source": body["source"],
                "tags_count": body["tags_count"],
                "tags": body["tags"],
                "list_id": body["list_id"],
                "_links": body["_links"],
            }),
            "MAILCHIMP_UPDATE_MEMBER_IN_LIST" => body,
            // `lists.deleteListMember` answers 204 without a body.
            "MAILCHIMP_ARCHIVE_SUBSCRIBER" => json!({"success": true}),
            "MAILCHIMP_ADD_SUBSCRIBER_TO_TAG" | "MAILCHIMP_REMOVE_SUBSCRIBER_FROM_TAG" => {
                json!({"success": true, "data": body})
            }
            "MAILCHIMP_ADD_NOTE_TO_SUBSCRIBER" => json!({
                "success": true,
                "note_id": body["id"],
                "note_content": body["note"],
                "created_at": body["created_at"],
                "_links": body["_links"],
            }),
            "MAILCHIMP_CREATE_CUSTOM_EVENT" => json!({
                "success": true,
                "list_id": body["list_id"],
                "email": body["email"],
                "name": body["name"],
                "properties": body["properties"],
                "occurred_at": body["occurred_at"],
            }),
            "MAILCHIMP_UNSUBSCRIBE_EMAIL" => json!({
                "success": true,
                "new_members": body["new_members"],
                "updated_members": body["updated_members"],
                "errors": body["errors"],
                "total_created": body["total_created"],
                "total_updated": body["total_updated"],
                "error_count": body["error_count"],
                "_links": body["_links"],
            }),
            "MAILCHIMP_CREATE_AUDIENCE" => json!({
                "success": true,
                "id": body["id"],
                "web_id": body["web_id"],
                "name": body["name"],
                "contact": body["contact"],
                "permission_reminder": body["permission_reminder"],
                "use_archive_bar": body["use_archive_bar"],
                "campaign_defaults": body["campaign_defaults"],
                "notify_on_subscribe": body["notify_on_subscribe"],
                "notify_on_unsubscribe": body["notify_on_unsubscribe"],
                "date_created": body["date_created"],
                "list_rating": body["list_rating"],
                "email_type_option": body["email_type_option"],
                "subscribe_url_short": body["subscribe_url_short"],
                "subscribe_url_long": body["subscribe_url_long"],
                "beamer_address": body["beamer_address"],
                "visibility": body["visibility"],
                "double_optin": body["double_optin"],
                "has_welcome": body["has_welcome"],
                "marketing_permissions": body["marketing_permissions"],
                "modules": body["modules"],
                "stats": body["stats"],
                "_links": body["_links"],
            }),
            "MAILCHIMP_CREATE_TAG" => json!({
                "success": true,
                "tag": body,
            }),
            "MAILCHIMP_CREATE_CAMPAIGN" => json!({
                "success": true,
                "campaign_id": body["id"],
                "campaign_web_id": body["web_id"],
                "campaign_title": body["settings"]["title"],
                "campaign_subject": body["settings"]["subject_line"],
                "campaign_status": body["status"],
                "campaign_type": body["type"],
                "created_at": body["create_time"],
                "archive_url": body["archive_url"],
                "_links": body["_links"],
            }),
            "MAILCHIMP_SET_CAMPAIGN_CONTENT" => json!({
                "success": true,
                "content": body,
            }),
            // `campaigns.send` answers 204 without a body.
            "MAILCHIMP_SEND_CAMPAIGN" => json!({
                "success": true,
                "sent": true,
            }),
            other => bail!("unknown Mailchimp tool `{other}`"),
        })
    }
}

/// `count` prop of the dropdown helpers (the piece always asks for 1000).
fn count_query(args: &Value) -> String {
    opt_u64(args, "count", 1000, 1, 1000).to_string()
}

/// Array props (`find_subscriber`) are joined with commas, like the action does.
fn joined(args: &Value, key: &str) -> Option<String> {
    let v = str_vec(args, key);
    (!v.is_empty()).then(|| v.join(","))
}

/// `self.url(path).with_query(q)` keeps the match arms readable.
trait WithQuery {
    fn with_query(self, q: String) -> Self;
}

impl WithQuery for String {
    fn with_query(mut self, q: String) -> Self {
        if !q.is_empty() {
            self.push_str(&q);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a() -> Mailchimp {
        Mailchimp::with_api_base("https://us21.api.mailchimp.test/3.0")
    }

    #[test]
    fn md5_matches_known_vectors() {
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex("The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(md5_hex(&"x".repeat(100)), "aed563ecafb4bcc5654c597a421547b2");
        // `mailchimpCommon.getMD5EmailHash` lower-cases first.
        assert_eq!(email_hash("Test@Example.com"), "55502f40dc8b7c769880b10874abc9d0");
    }

    #[test]
    fn builds_the_documented_endpoints() {
        let m = a();
        let r = m
            .build_request("MAILCHIMP_FIND_SUBSCRIBER", &json!({"list_id": "ab12", "email": "a@b.com"}))
            .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.url,
            "https://us21.api.mailchimp.test/3.0/lists/ab12/members/357a20e8c56e69d6f9734d23ef9517e8"
        );

        let r = m
            .build_request(
                "MAILCHIMP_ADD_MEMBER_TO_LIST",
                &json!({"list_id": "ab12", "email_address": "a@b.com",
                        "status_if_new": "subscribed", "skip_merge_validation": true}),
            )
            .unwrap();
        assert_eq!(r.method, "PUT");
        assert!(r.url.ends_with(
            "/lists/ab12/members/357a20e8c56e69d6f9734d23ef9517e8?skip_merge_validation=true"
        ));
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["status_if_new"], "subscribed");

        let r = m
            .build_request(
                "MAILCHIMP_UNSUBSCRIBE_EMAIL",
                &json!({"list_id": "ab12", "email_address": "a@b.com", "skip_duplicate_check": true}),
            )
            .unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://us21.api.mailchimp.test/3.0/lists/ab12?skip_duplicate_check=true");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["members"][0]["status"], "unsubscribed");

        let r = m
            .build_request("MAILCHIMP_SEND_CAMPAIGN", &json!({"campaign_id": "c1"}))
            .unwrap();
        assert_eq!(
            r.url,
            "https://us21.api.mailchimp.test/3.0/campaigns/c1/actions/send"
        );
    }

    #[test]
    fn validation_and_query_encoding() {
        let m = a();
        assert!(m
            .build_request("MAILCHIMP_FIND_SUBSCRIBER", &json!({"list_id": "../etc", "email": "a@b.com"}))
            .is_err());
        assert!(m
            .build_request(
                "MAILCHIMP_ADD_MEMBER_TO_LIST",
                &json!({"list_id": "ab12", "email_address": "a@b.com", "status_if_new": "nope"})
            )
            .is_err());
        assert!(m
            .build_request("MAILCHIMP_SET_CAMPAIGN_CONTENT", &json!({"campaign_id": "c1"}))
            .is_err());

        // Query values are percent-encoded (commas included).
        let r = m
            .build_request("MAILCHIMP_FIND_TAG", &json!({"list_id": "ab12", "name": "vip, a b"}))
            .unwrap();
        assert!(r.url.ends_with("/lists/ab12/tag-search?name=vip%2C+a+b"), "{}", r.url);

        // An email is hashed, an existing hash or contact_id is passed through.
        let r = m
            .build_request(
                "MAILCHIMP_ARCHIVE_SUBSCRIBER",
                &json!({"list_id": "ab12", "subscriber_hash": "357a20e8c56e69d6f9734d23ef9517e8"}),
            )
            .unwrap();
        assert!(r
            .url
            .ends_with("/lists/ab12/members/357a20e8c56e69d6f9734d23ef9517e8"));
        assert!(m
            .build_request(
                "MAILCHIMP_ARCHIVE_SUBSCRIBER",
                &json!({"list_id": "ab12", "subscriber_hash": "x/y"})
            )
            .is_err());
    }
}